//! Common-array native/ripsolve comparison. Benchmark dependencies only.
use lp_ip_craft::*;
use lp_ip_craft_native::{NativeOptions, NativeSolver};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn floats(v: &Value, negative: bool) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| {
            x.as_f64().unwrap_or(if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            })
        })
        .collect()
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 4,
        "solvercmp native|ripsolve fixture.json seconds [policy]"
    );
    let payload: Value =
        serde_json::from_slice(&std::fs::read(Path::new(&args[2])).unwrap()).unwrap();
    let v = payload.get("model").unwrap_or(&payload);
    let seconds: f64 = args[3].parse().unwrap();
    let policy = args.get(4).map_or("default", String::as_str);
    let objective = floats(&v["objective"], false);
    let n = objective.len();
    let lower = floats(&v["lower"], true);
    let upper = floats(&v["upper"], false);
    let rl = floats(&v["row_lower"], true);
    let ru = floats(&v["row_upper"], false);
    let m = rl.len();
    let kinds: Vec<_> = v["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| match k.as_str().unwrap() {
            "integer" => VariableKind::Integer,
            "binary" => VariableKind::Binary,
            _ => VariableKind::Continuous,
        })
        .collect();
    let indices: Vec<_> = v["indices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    let ptr: Vec<_> = v["indptr"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    let values = floats(&v["values"], false);
    let sense = if v["sense"] == "maximize" {
        Sense::Maximize
    } else {
        Sense::Minimize
    };
    let offset = v["objective_offset"].as_f64().unwrap_or(0.);
    let model = Model::new(ModelData {
        sense,
        objective: objective.clone(),
        objective_offset: offset,
        variables: Bounds::new(lower.clone(), upper.clone()),
        kinds: kinds.clone(),
        rows: Bounds::new(rl.clone(), ru.clone()),
        matrix: CsrMatrix::new(m, n, ptr.clone(), indices.clone(), values.clone()),
    })
    .unwrap();
    let node_cap: u64 = std::env::var("LPIP_COMPARE_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100000);
    let pivot_cap: u64 = std::env::var("LPIP_COMPARE_PIVOTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000000);
    let began = Instant::now();
    let result = if args[1] == "native" {
        let enhanced = policy != "default" && !policy.starts_with("branch-");
        let settings = NativeOptions {
            time_limit_seconds: Some(seconds),
            node_limit: Some(node_cap),
            iteration_limit: Some(pivot_cap),
            decompose: false,
            mip_diagnostics: true,
            mip_trace_limit: 8,
            mip_presolve: enhanced,
            coefficient_tightening: enhanced,
            dual_presolve: enhanced,
            gmi_cuts: matches!(policy, "gmi" | "combined" | "heuristics"),
            mir_cuts: matches!(policy, "mir" | "combined" | "heuristics"),
            mip_cut_rounds: 16,
            root_heuristics: policy == "heuristics",
            branching_score: if policy == "branch-product" {
                "product".into()
            } else {
                "weighted_sum".into()
            },
            pseudocost_min_step: if policy.contains("clamped") { 0.01 } else { 0. },
            branch_trace_limit: 8,
            ..Default::default()
        };
        match NativeSolver::new(settings).solve(
            &model,
            &Options::default(),
            None,
            &CancellationToken::default(),
        ) {
            Ok(r) => {
                json!({"status":r.status.as_str(),"objective":r.evaluation.as_ref().map(|e|e.objective),"bound":r.best_bound,"candidate":r.candidate,"nodes":r.nodes,"pivots":r.iterations,"metadata":r.backend.effective_options,"message":r.message})
            }
            Err(e) => json!({"status":"error","error":e.to_string()}),
        }
    } else {
        use ripsolve::{
            Builder,
            model::{Sense as Rs, VarType},
            search,
        };
        let mut b = Builder::new(if sense == Sense::Minimize {
            Rs::Minimize
        } else {
            Rs::Maximize
        });
        for j in 0..n {
            b.column(
                &format!("x{j}"),
                lower[j],
                upper[j],
                if kinds[j].is_integral() {
                    VarType::Integer
                } else {
                    VarType::Continuous
                },
            );
        }
        b.objective(
            &objective
                .iter()
                .enumerate()
                .map(|(j, &c)| (j, c))
                .collect::<Vec<_>>(),
        );
        for i in 0..m {
            b.range(
                &(ptr[i]..ptr[i + 1])
                    .map(|k| (indices[k], values[k]))
                    .collect::<Vec<_>>(),
                rl[i],
                ru[i],
            );
        }
        let problem = b.build();
        let mut settings = search::Options {
            threads: 1,
            time_limit: Some(Duration::from_secs_f64(seconds)),
            gap_tolerance: 0.,
            ..Default::default()
        };
        if std::env::var("LPIP_COMPARE_NODES").is_ok() {
            settings.max_nodes = node_cap as usize;
        }
        match policy {
            "serial" => settings.cdcl = false,
            "no-presolve" => settings.presolve = false,
            "no-cuts" => settings.cut_rounds = 0,
            "no-restart" => settings.restart_share = 0,
            "bare" => {
                settings.presolve = false;
                settings.cut_rounds = 0;
                settings.heuristic_frequency = 0;
                settings.improvement_frequency = 0;
                settings.cdcl = false;
                settings.aggregate = false;
                settings.node_propagation = false;
            }
            _ => {}
        }
        let r = search::solve(&problem, settings);
        let direction = 1.;
        json!({"status":format!("{:?}",r.status),"objective":r.objective.map(|v|direction*v+offset),"bound":if r.bound.is_finite(){Some(direction*r.bound+offset)}else{None},"candidate":r.x,"nodes":r.nodes,"root_bound":if r.root_bound.is_finite(){Some(direction*r.root_bound+offset)}else{None},"root_bound_after_cuts":if r.root_bound_after_cuts.is_finite(){Some(direction*r.root_bound_after_cuts+offset)}else{None},"cuts_added":r.cuts_added,"reference_version":"0.4.0"})
    };
    println!(
        "{}",
        json!({"solver":args[1],"policy":policy,"n":n,"m":m,"nnz":values.len(),"seconds":began.elapsed().as_secs_f64(),"result":result})
    );
}
