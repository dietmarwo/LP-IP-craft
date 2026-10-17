mod extended;
mod fixtures;
mod imports;
use lp_ip_craft::*;
use serde_json::{Value, json};
use std::time::Instant;

fn exact_binary(model: &Model) -> f64 {
    // Exhaustive Gray-code enumeration; all objective/column coefficients
    // in these fixtures are integers and exactly representable as i64.
    let n = model.n();
    assert!(n <= 20 && model.kinds().iter().all(|k| *k == VariableKind::Binary));
    let mut columns = vec![vec![]; n];
    for i in 0..model.m() {
        for k in model.matrix().pattern.indptr[i]..model.matrix().pattern.indptr[i + 1] {
            let j = model.matrix().pattern.indices[k];
            let value = model.matrix().values[k];
            assert_eq!(value, value as i64 as f64);
            columns[j].push((i, value as i64));
        }
    }
    let mut sums = vec![0i64; model.m()];
    let mut objective = 0i64;
    let mut previous = 0usize;
    let mut best = i64::MIN;
    for bit in 0..(1usize << n) {
        let gray = bit ^ (bit >> 1);
        if bit > 0 {
            let j = (gray ^ previous).trailing_zeros() as usize;
            let sign = if (gray >> j) & 1 == 1 { 1 } else { -1 };
            let cost = model.objective()[j];
            assert_eq!(cost, cost as i64 as f64);
            objective += sign * cost as i64;
            for &(i, a) in &columns[j] {
                sums[i] += sign * a;
            }
        }
        if objective > best
            && sums.iter().enumerate().all(|(i, &s)| {
                s as f64 >= model.rows().lower[i] && s as f64 <= model.rows().upper[i]
            })
        {
            best = objective;
        }
        previous = gray;
    }
    assert!(best != i64::MIN);
    best as f64 + model.objective_offset()
}

fn fingerprint(model: &Model) -> String {
    let mut h = 0xcbf29ce484222325u64;
    let mut feed = |v: u64| {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    feed(model.n() as u64);
    feed(model.m() as u64);
    feed(model.nnz() as u64);
    feed(if model.sense() == Sense::Minimize {
        0
    } else {
        1
    });
    feed(model.objective_offset().to_bits());
    for a in [
        model.objective(),
        &model.variables().lower,
        &model.variables().upper,
        &model.rows().lower,
        &model.rows().upper,
        &model.matrix().values,
    ] {
        for &v in a {
            feed(v.to_bits());
        }
    }
    for k in model.kinds() {
        feed(match k {
            VariableKind::Continuous => 0,
            VariableKind::Integer => 1,
            VariableKind::Binary => 2,
        });
    }
    for &v in &model.matrix().pattern.indptr {
        feed(v as u64);
    }
    for &v in &model.matrix().pattern.indices {
        feed(v as u64);
    }
    format!("{h:016x}")
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let name = args.get(1).expect("fixture name").as_str();
    let size: usize = args.get(2).unwrap().parse().unwrap();
    let warm = args.get(3).is_none_or(|s| s != "cold");
    let workers: usize = args.get(4).unwrap().parse().unwrap();
    let repeats: usize = args.get(5).unwrap().parse().unwrap();
    assert!(repeats > 0);
    let is_family = matches!(
        name,
        "packing"
            | "network"
            | "irregular"
            | "selection"
            | "packing-wide"
            | "packing-permuted"
            | "long-row-mip"
            | "integer-knapsack"
            | "multi-knapsack"
            | "graph-mip"
            | "mixed-overload"
            | "aspect-wide"
            | "aspect-tall"
            | "singleton-lp"
            | "inferred-mip"
    );
    let began = Instant::now();
    let (model, expected) = match name {
        "singleton-lp" => {let(m,e)=extended::singleton_lp(size);(m,Some(e))},
        "inferred-mip" => {let(m,e)=extended::inferred_mip(size);(m,Some(e))},
        "aspect-wide" | "aspect-tall" => {let (m,e)=extended::aspect(size,name=="aspect-wide");(m,Some(e))},
        "mixed-overload" => {
            let (m, e) = extended::mixed(size);
            (m, Some(e))
        }
        "packing-wide" | "packing-permuted" => {
            let (m, e) = extended::packing(size, name == "packing-permuted");
            (m, Some(e))
        }
        "long-row-mip" | "integer-knapsack" => {
            let (m, e) = extended::knapsack(size, name == "integer-knapsack");
            (m, Some(e))
        }
        "multi-knapsack" | "graph-mip" => {
            let m = extended::binary(size, name == "graph-mip");
            let e = exact_binary(&m);
            (m, Some(e))
        }
        "packing" => {
            let (m, e) = fixtures::packing(size);
            (m, Some(e))
        }
        "network" => {
            let (m, e) = fixtures::network(size);
            (m, Some(e))
        }
        "irregular" => {
            let (m, e) = fixtures::irregular(size);
            (m, Some(e))
        }
        "selection" => {
            let (m, e) = fixtures::selection(size);
            (m, Some(e))
        }
        "fixed" | "pivot" | "knapsack" | "partition" | "nodecopy" | "redundant" | "blocks" => {
            let m = fixtures::legacy(name);
            let e = if matches!(name, "knapsack" | "partition" | "nodecopy") {
                Some(exact_binary(&m))
            } else {
                None
            };
            (m, e)
        }
        _ if name.starts_with("json=") => {
            let (m, e) = extended::json_file(&name[5..]);
            (m, Some(e))
        }
        _ => imports::fixture(name),
    };
    let construction = began.elapsed().as_secs_f64();
    let hash = fingerprint(&model);
    let mip = model.is_mip();
    let mut runs = vec![];
    for run in 0..=repeats {
        #[cfg(feature = "backend_native")]
        let mut solver = lp_ip_craft_native::NativeSolver::new(lp_ip_craft_native::NativeOptions {
            time_limit_seconds: Some(std::env::var("LPIP_TIME_LIMIT").map_or(30., |v| v.parse().unwrap())),
            node_limit: Some(std::env::var("LPIP_NODE_LIMIT").map_or(100_000, |v| v.parse().unwrap())),
            iteration_limit: Some(std::env::var("LPIP_ITERATION_LIMIT").map_or(1_000_000, |v| v.parse().unwrap())),
            component_threads: workers,
            decompose: !is_family && std::env::var_os("LPIP_BENCH_NO_DECOMPOSE").is_none(),
            presolve: std::env::var("LPIP_PRESOLVE").map_or(true,|v|v!="off"),
            node_reoptimization: warm,
            reduced_cost_fixing: args.get(12).is_none_or(|s| s != "no_fix"),
            strong_branching: args.get(11).is_some_and(|s| s == "strong"),
            basis_update: match args.get(6).map(String::as_str).unwrap_or("auto") {
                "eta" => lp_ip_craft_native::BasisUpdate::Eta,
                "ft" => lp_ip_craft_native::BasisUpdate::ForrestTomlin,
                "auto" => lp_ip_craft_native::BasisUpdate::Auto,
                _ => panic!("basis method must be auto, eta, or ft"),
            },
            refactor_interval: args.get(7).map_or(256, |s| s.parse().unwrap()),
            adaptive_refactor: args.get(10).is_none_or(|s| s != "fixed"),
            factor_pivot_search: args.get(8).map_or(4, |s| s.parse().unwrap()),
            factor_ordering: match args.get(9).map(String::as_str).unwrap_or("auto") {
                "auto" => lp_ip_craft_native::FactorOrdering::Auto,
                "natural" => lp_ip_craft_native::FactorOrdering::Natural,
                "min_degree" => lp_ip_craft_native::FactorOrdering::MinDegree,
                _ => panic!("ordering must be auto/natural/min_degree"),
            },
            ..Default::default()
        });
        #[cfg(all(feature="backend_native",feature="evolved"))]
        {
            solver.settings.incumbent_heuristic = std::env::var("LPIP_HEURISTIC").map_or(true, |v| v != "off");
            solver.settings.strong_branching |= std::env::var("LPIP_STRONG_BRANCHING").is_ok_and(|v| v == "on");
            solver.settings.singleton_presolve = std::env::var("LPIP_SINGLETON").map_or(true, |v| v != "off");
            solver.settings.root_propagation = std::env::var("LPIP_ROOT_PROPAGATION").map_or(true, |v| v != "off");
            solver.settings.root_cuts = std::env::var_os("LPIP_ROOT_CUTS").is_some();
            solver.settings.factor_replay = std::env::var_os("LPIP_FACTOR_REPLAY").is_some();
            solver.settings.adaptive_pricing = std::env::var("LPIP_PRICING").map_or(true, |v| v != "row");
            solver.settings.devex = std::env::var_os("LPIP_DEVEX").is_some();
            solver.settings.avoid_dominant_components = std::env::var("LPIP_COMPONENT_SPLIT").map_or(true, |v| v != "force");
        }
        #[cfg(all(feature="backend_native",feature="search-diagnostics"))]
        {
            solver.settings.rational_certification = std::env::var("LPIP_RATIONAL_CERTIFICATION").map_or(true, |v| v != "off");
            solver.settings.reliability_branching = std::env::var_os("LPIP_RELIABILITY").is_some();
            solver.settings.feasibility_jump = std::env::var_os("LPIP_FEASIBILITY_JUMP").is_some();
            solver.settings.feasibility_jump_steps = std::env::var("LPIP_JUMP_STEPS").map_or(2000, |v| v.parse().unwrap());
            solver.settings.root_separation = std::env::var_os("LPIP_ROOT_SEPARATION").is_some();
            solver.settings.cover_lifting = std::env::var("LPIP_COVER_LIFTING").map_or(true, |v| v != "off");
            solver.settings.mixed_propagation = std::env::var_os("LPIP_MIXED_PROPAGATION").is_some();
            solver.settings.mip_diagnostics = std::env::var_os("LPIP_MIP_DIAGNOSTICS").is_some();
            solver.settings.mip_trace_limit = std::env::var("LPIP_MIP_TRACE_LIMIT").map_or(256, |v| v.parse().unwrap());
        }
        #[cfg(feature = "backend_highs")]
        let mut solver = lp_ip_craft_highs::HighsSolver::new(lp_ip_craft_highs::HighsOptions {
            time_limit_seconds: Some(std::env::var("LPIP_TIME_LIMIT").map_or(30., |v| v.parse().unwrap())),
            node_limit: mip.then(|| std::env::var("LPIP_NODE_LIMIT").map_or(100_000, |v| v.parse().unwrap())),
            iteration_limit: (!mip).then(|| std::env::var("LPIP_ITERATION_LIMIT").map_or(1_000_000, |v| v.parse().unwrap())),
            ..Default::default()
        });
        #[cfg(feature = "kernel-profile")]
        lp_ip_craft_native::profile::reset();
        let t = Instant::now();
        let result = solver.solve(
            &model,
            &Options::default(),
            None,
            &CancellationToken::default(),
        );
        let seconds = t.elapsed().as_secs_f64();
        let mut record = json!({"run":run,"seconds":seconds});
        match result {
            Ok(r) => {
                let mut accepted = r.status == Status::Optimal;
                let mut checks = Value::Null;
                if let Some(x) = &r.candidate {
                    let e = check::evaluate(&model, x, &Tolerances::default()).unwrap();
                    accepted &= e.feasible_candidate;
                    if let Some(reference) = expected {
                        accepted &=
                            (e.objective - reference).abs() <= 1e-6 * (1. + reference.abs());
                    }
                    checks = json!({"objective":e.objective,"primal":e.primal_violation,"integrality":e.integrality_violation,"feasible":e.feasible_candidate});
                    if !mip && r.status == Status::Optimal {
                        if let Some(d) = &r.duals {
                            let k = check::lp_residuals(&model, x, d).unwrap();
                            let scale = 1.
                                + model
                                    .objective()
                                    .iter()
                                    .fold(0.0f64, |a, &b| a.max(b.abs()));
                            accepted &= k.primal <= 1e-7
                                && k.stationarity <= 1e-7 * scale
                                && k.dual_sign <= 1e-7 * scale
                                && k.duality_gap <= 1e-7;
                            checks["stationarity"] = json!(k.stationarity);
                            checks["duality_gap"] = json!(k.duality_gap);
                            checks["dual_sign"] = json!(k.dual_sign);
                            checks["complementarity"] = json!(k.complementarity);
                        } else {
                            accepted = false;
                        }
                    }
                } else {
                    accepted = false;
                }
                record["status"] = json!(r.status.as_str());
                record["accepted"] = json!(accepted);
                record["checks"] = checks;
                record["message"] = json!(r.message);
                record["nodes"] = json!(r.nodes);
                record["iterations"] = json!(r.iterations);
                record["best_bound"] = json!(r.best_bound);
                record["backend_version"] = json!(r.backend.version);
                record["effective_options"] = json!(r.backend.effective_options);
                record["phases"] = json!({"total_seconds":r.backend.total_seconds,"conversion_seconds":r.backend.conversion_seconds,"solve_seconds":r.backend.solve_seconds,"check_seconds":r.backend.check_seconds});
            }
            Err(e) => {
                record["status"] = json!("error");
                record["accepted"] = json!(false);
                record["message"] = json!(format!("{e:?}"));
            }
        }
        #[cfg(feature = "kernel-profile")]
        {
            record["kernel_profile"] = json!(lp_ip_craft_native::profile::snapshot());
        }
        runs.push(record);
    }
    let rss = std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|s| s.starts_with("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u64>()
        .unwrap();
    println!(
        "{}",
        json!({"fixture":name,"size":size,"warm":warm,"worker_cap":workers,"family":is_family,"n":model.n(),"m":model.m(),"nnz":model.nnz(),"mip":mip,"model_fingerprint":hash,"construction_seconds":construction,"expected":expected,"peak_rss_kib":rss,"runs":runs})
    );
}
