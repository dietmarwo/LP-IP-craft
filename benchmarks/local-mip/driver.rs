//! Run a single model in a fresh process. Stdout is a JSON-lines event stream.
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, Error, LpIpSolver, Model, ModelData, Options, Sense,
    VariableKind, check,
};
use lp_ip_craft_native::{NativeOptions, NativeSolver};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    time::Instant,
};

fn numeric(v: &Value, field: &str, null: f64) -> Vec<f64> {
    v[field]
        .as_array()
        .expect(field)
        .iter()
        .map(|x| {
            if x.is_null() {
                null
            } else {
                x.as_f64().expect(field)
            }
        })
        .collect()
}

fn index(v: &Value, field: &str) -> Vec<usize> {
    v[field]
        .as_array()
        .expect(field)
        .iter()
        .map(|x| x.as_u64().expect(field) as usize)
        .collect()
}

fn load(path: &str) -> Model {
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let v = v.get("model").unwrap_or(&v);
    let lower = numeric(v, "lower", f64::NEG_INFINITY);
    let row_lower = numeric(v, "row_lower", f64::NEG_INFINITY);
    let matrix = CsrMatrix::new(
        row_lower.len(),
        lower.len(),
        index(v, "indptr"),
        index(v, "indices"),
        numeric(v, "values", f64::NAN),
    );
    Model::new(ModelData {
        sense: match v["sense"].as_str() {
            Some("maximize") => Sense::Maximize,
            Some("minimize") => Sense::Minimize,
            _ => panic!("unknown objective sense"),
        },
        objective: numeric(v, "objective", f64::NAN),
        objective_offset: v["objective_offset"].as_f64().unwrap_or(0.),
        variables: Bounds::new(lower, numeric(v, "upper", f64::INFINITY)),
        kinds: v["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| match x.as_str() {
                Some("binary") => VariableKind::Binary,
                Some("integer") => VariableKind::Integer,
                Some("continuous") => VariableKind::Continuous,
                _ => panic!("unknown variable kind"),
            })
            .collect(),
        rows: Bounds::new(row_lower, numeric(v, "row_upper", f64::INFINITY)),
        matrix,
    })
    .unwrap()
}

fn emit(v: Value) {
    let mut out = io::stdout().lock();
    writeln!(out, "{v}").unwrap();
    out.flush().unwrap();
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() == 8,
        "local-mip-pilot MODEL ARM SECONDS|none NODES PIVOTS SEED MODE"
    );
    let model = load(&args[1]);
    let arm = args[2].as_str();
    assert!(matches!(arm, "stock" | "history" | "A" | "B" | "C" | "D"));
    let history = arm == "history";
    let mut settings = NativeOptions {
        time_limit_seconds: (args[3] != "none").then(|| args[3].parse().unwrap()),
        node_limit: Some(args[4].parse().unwrap()),
        iteration_limit: Some(args[5].parse().unwrap()),
        decompose: false,
        component_threads: 1,
        tree_threads: 1,
        packing_workers: 1,
        root_heuristics: arm != "stock",
        feasibility_jump: matches!(arm, "A" | "B" | "C" | "D"),
        gmi_cuts: history,
        mir_cuts: history,
        dual_presolve: history,
        mip_cut_rounds: if history { 16 } else { 8 },
        ..Default::default()
    };
    // The stock comparison can compile the same driver against the baseline.
    let _seed: u64 = args[6].parse().unwrap();
    let mode = args[7].as_str();
    assert!(matches!(mode, "trace" | "plain" | "root"));
    assert!(
        mode != "plain" || !matches!(arm, "C" | "D"),
        "matched controls require the observed root hook"
    );
    let options = Options::default();
    let cancel = CancellationToken::default();
    let began = Instant::now();
    #[cfg(feature = "local-mip-pilot")]
    let result = {
        use lp_ip_craft_native::local_mip_bench::{self, Controls};
        let effort = (model.nnz().max(1) as u64)
            .saturating_mul(64)
            .min(2_000_000);
        if matches!(arm, "B" | "D") {
            settings.local_mip_effort = effort;
            settings.local_mip_seed = _seed;
        }
        settings.mip_diagnostics = mode == "plain";
        let mut solver = NativeSolver::new(settings);
        if mode == "plain" {
            solver.solve(&model, &options, None, &cancel)
        } else {
            local_mip_bench::solve(
                &mut solver,
                &model,
                &options,
                &cancel,
                Controls {
                    matched_fj_effort: if arm == "C" { effort } else { 0 },
                    // Keep FJ enabled in RENS children for both matched arms.
                    // The replacement is limited to the main root hook.
                    replace_root_fj: arm == "D",
                    stop_at_root: mode == "root",
                },
                |event| {
                    emit(
                        json!({"kind":event.kind, "source":event.source, "phase":event.phase,
                        "seconds":event.seconds, "objective":event.objective, "bound":event.bound,
                        "nodes":event.nodes,"pivots":event.pivots, "stage_nodes":event.stage_nodes,
                        "stage_pivots":event.stage_pivots,"stage_seconds":event.stage_seconds}),
                    );
                },
            )
        }
    };
    #[cfg(not(feature = "local-mip-pilot"))]
    let result = {
        assert!(matches!(arm, "stock" | "history" | "A") && mode == "plain");
        settings.mip_diagnostics = true;
        NativeSolver::new(settings).solve(&model, &options, None, &cancel)
    };
    let seconds = began.elapsed().as_secs_f64();
    let rss_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|line| line.starts_with("VmHWM:"))?
                .split_whitespace()
                .nth(1)?
                .parse::<u64>()
                .ok()
        });
    match result {
        Ok(r) => {
            let evaluation = r
                .candidate
                .as_ref()
                .map(|x| check::evaluate(&model, x, &options.tolerances));
            let checked = evaluation.as_ref().and_then(|r| r.as_ref().ok());
            emit(
                json!({"kind":"result", "status":r.status.as_str(), "message":r.message,
                "objective":r.evaluation.map(|e|e.objective), "bound":r.best_bound,
                "candidate":r.candidate, "checked":checked.map(|e|e.feasible_candidate),
                "checked_objective":checked.map(|e|e.objective), "rows_violation":checked.map(|e|e.primal_violation),
                "integrality_violation":checked.map(|e|e.integrality_violation), "nodes":r.nodes,
                "pivots":r.iterations, "metadata":r.backend.effective_options,
                "seconds":seconds, "rss_kib":rss_kib}),
            );
        }
        Err(e) => emit(
            json!({"kind":"result", "status":if matches!(e, Error::Unsupported(_)) { "unsupported" } else { "error" },
            "message":e.to_string(), "seconds":seconds,"rss_kib":rss_kib}),
        ),
    }
}
