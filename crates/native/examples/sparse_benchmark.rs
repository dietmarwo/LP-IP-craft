//! Deterministic connected fixtures with independent known objective values.
use lp_ip_craft::*;
use lp_ip_craft_native::*;
use std::time::Instant;

fn csr(n: usize, rows: &[Vec<(usize, f64)>]) -> CsrMatrix {
    let mut indptr = vec![0];
    let mut indices = vec![];
    let mut values = vec![];
    for row in rows {
        let mut row = row.clone();
        row.sort_by_key(|e| e.0);
        for (j, v) in row {
            indices.push(j);
            values.push(v);
        }
        indptr.push(indices.len());
    }
    CsrMatrix::new(rows.len(), n, indptr, indices, values)
}
pub fn packing(n: usize) -> (Model, f64) {
    planted(n, false)
}
pub fn irregular(n: usize) -> (Model, f64) {
    planted(n, true)
}
fn planted(n: usize, irregular: bool) -> (Model, f64) {
    assert!(n >= 8);
    let rows: Vec<Vec<_>> = (0..n)
        .map(|i| vec![(i, 2.), ((i + 1) % n, 1.), ((i + 7) % n, 1.)])
        .collect();
    let rows = if irregular {
        let mut seed = 51921u64;
        (0..n)
            .map(|i| {
                let mut row = std::collections::BTreeMap::from([(i, 8.), ((i + 1) % n, 1.)]);
                for _ in 0..4 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let j = ((seed >> 32) as usize) % n;
                    if j != i {
                        row.insert(j, 1.);
                    }
                }
                row.into_iter().collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    } else {
        rows
    };
    let rhs: Vec<_> = rows.iter().map(|r| r.iter().map(|e| e.1).sum()).collect();
    let mut objective = vec![0.; n];
    for (i, row) in rows.iter().enumerate() {
        let y = (1 + i % 5) as f64;
        for &(j, a) in row {
            objective[j] += y * a;
        }
    }
    // x=1 is feasible, y_i=1+i%5 is dual feasible, with identical objectives.
    let expected = objective.iter().sum();
    let m = Model::new(ModelData {
        sense: Sense::Maximize,
        objective,
        objective_offset: 0.,
        variables: Bounds::new(vec![0.; n], vec![2.; n]),
        kinds: vec![VariableKind::Continuous; n],
        matrix: csr(n, &rows),
        rows: Bounds::new(vec![f64::NEG_INFINITY; n], rhs),
    })
    .unwrap();
    (m, expected)
}
pub fn network(nodes: usize) -> (Model, f64) {
    assert!(nodes >= 3);
    let mut rows = vec![vec![]; nodes];
    let mut objective = vec![];
    for step in 1..=2 {
        for i in 0..nodes - step {
            let j = objective.len();
            rows[i].push((j, 1.));
            rows[i + step].push((j, -1.));
            objective.push(if step == 1 { 1. } else { 3. });
        }
    }
    let n = objective.len();
    let mut balance = vec![0.; nodes];
    balance[0] = 1.;
    balance[nodes - 1] = -1.;
    // Unit flow on consecutive arcs, potentials -i, nonnegative skip-arc RC.
    (
        Model::new(ModelData {
            sense: Sense::Minimize,
            objective,
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![2.; n]),
            kinds: vec![VariableKind::Continuous; n],
            matrix: csr(n, &rows),
            rows: Bounds::fixed(balance),
        })
        .unwrap(),
        (nodes - 1) as f64,
    )
}
pub fn selection(n: usize) -> (Model, f64) {
    let weights: Vec<_> = (0..n).map(|i| 1 + (i * 7 + 3) % 5).collect();
    let profits: Vec<_> = (0..n).map(|i| 1 + (i * 11 + 7) % 17).collect();
    let capacity = weights.iter().sum::<usize>() / 3;
    let mut rows: Vec<Vec<_>> = (0..n).map(|i| vec![(i, 1.), ((i + 1) % n, 1.)]).collect();
    rows.push(
        weights
            .iter()
            .enumerate()
            .map(|(i, &w)| (i, w as f64))
            .collect(),
    );
    let mut upper = vec![1.5; n];
    upper.push(capacity as f64 + 0.5);
    // Independent integer dynamic program: first bit, previous bit, capacity.
    let mut expected = 0i64;
    for first in 0..=1 {
        let mut dp = vec![[-1i64; 2]; capacity + 1];
        if first * weights[0] > capacity {
            continue;
        }
        dp[first * weights[0]][first] = (first * profits[0]) as i64;
        for i in 1..n {
            let mut next = vec![[-1; 2]; capacity + 1];
            for (w, states) in dp.iter().enumerate() {
                for (prev, &value) in states.iter().enumerate() {
                    if value < 0 {
                        continue;
                    }
                    next[w][0] = next[w][0].max(value);
                    if prev == 0 && !(i == n - 1 && first == 1) && w + weights[i] <= capacity {
                        next[w + weights[i]][1] =
                            next[w + weights[i]][1].max(value + profits[i] as i64);
                    }
                }
            }
            dp = next;
        }
        expected = expected.max(*dp.iter().flat_map(|r| r.iter()).max().unwrap());
    }
    (
        Model::new(ModelData {
            sense: Sense::Maximize,
            objective: profits.iter().map(|&p| p as f64).collect(),
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![1.; n]),
            kinds: vec![VariableKind::Binary; n],
            matrix: csr(n, &rows),
            rows: Bounds::new(vec![f64::NEG_INFINITY; n + 1], upper),
        })
        .unwrap(),
        expected as f64,
    )
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let family = args.get(1).map(String::as_str).unwrap_or("packing");
    let size = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(128);
    let sparse = args.get(3).is_none_or(|s| s == "sparse");
    let warm = args.get(4).is_none_or(|s| s != "cold");
    let (model, expected) = match family {
        "packing" => packing(size),
        "irregular" => irregular(size),
        "network" => network(size),
        "selection" => selection(size),
        _ => panic!("unknown family"),
    };
    let settings = NativeOptions {
        sparse,
        decompose: false,
        component_threads: 1,
        node_reoptimization: warm,
        time_limit_seconds: Some(30.),
        ..Default::default()
    };
    let t = Instant::now();
    let result = NativeSolver::new(settings).solve(
        &model,
        &Options::default(),
        None,
        &CancellationToken::default(),
    );
    let seconds = t.elapsed().as_secs_f64();
    let peak_rss_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(0);

    match result {
        Ok(r) => {
            let status = r.status.as_str();
            if r.status == Status::Optimal {
                let objective = r.evaluation.as_ref().unwrap().objective;
                assert!(
                    (objective - expected).abs() < 1e-6 * (1. + expected.abs()),
                    "{objective} != {expected}"
                );
            }
            let objective = r
                .evaluation
                .as_ref()
                .map(|e| e.objective.to_string())
                .unwrap_or("null".into());
            let d = |key: &str| {
                r.backend
                    .effective_options
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("0")
            };
            println!(
                "{{\"family\":\"{family}\",\"size\":{size},\"sparse\":{sparse},\"warm\":{warm},\"n\":{},\"m\":{},\"nnz\":{},\"peak_rss_kib\":{peak_rss_kib},\"seconds\":{seconds},\"status\":\"{status}\",\"objective\":{objective},\"expected\":{expected},\"pivots\":{},\"nodes\":{},\"peak_entries\":{},\"refactors\":{},\"warm_nodes\":{},\"cold_fallbacks\":{}}}",
                model.n(),
                model.m(),
                model.nnz(),
                r.iterations,
                r.nodes,
                d("diagnostics.sparse_peak_entries"),
                d("diagnostics.sparse_refactors"),
                d("diagnostics.reoptimized_nodes"),
                d("diagnostics.cold_fallbacks")
            );
        }
        Err(Error::Unsupported(_)) if !sparse => println!(
            "{{\"family\":\"{family}\",\"size\":{size},\"sparse\":false,\"warm\":{warm},\"n\":{},\"m\":{},\"nnz\":{},\"peak_rss_kib\":{peak_rss_kib},\"seconds\":{seconds},\"status\":\"dense_guard\"}}",
            model.n(),
            model.m(),
            model.nnz()
        ),
        Err(e) => panic!("{e:?}"),
    }
}
