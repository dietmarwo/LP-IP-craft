//! Frozen known-answer performance protocol for the0.28 parallel series.
#[path = "support/parallel_fixtures.rs"]
mod fixtures;
use fixtures::{irregular, network, packing, selection};
use lp_ip_craft::*;
use lp_ip_craft_native::*;
use std::time::Instant;

fn dense(n: usize) -> (Model, f64) {
    let a: Vec<_> = (0..n)
        .flat_map(|i| {
            (0..n).map(move |j| {
                if i == j {
                    16. + n as f64 * 0.5
                } else {
                    (1 + (i * 73 + j * 151) % 11) as f64 * 0.01
                }
            })
        })
        .collect();
    let rhs = a.chunks(n).map(|r| r.iter().sum()).collect();
    let mut c = vec![0.; n];
    for i in 0..n {
        for j in 0..n {
            c[j] += (1 + i % 5) as f64 * a[i * n + j];
        }
    }
    let expected = c.iter().sum();
    (
        Model::new(ModelData {
            sense: Sense::Maximize,
            objective: c,
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![2.; n]),
            kinds: vec![VariableKind::Continuous; n],
            matrix: CsrMatrix::from_dense(n, n, &a).unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY; n], rhs),
        })
        .unwrap(),
        expected,
    )
}
fn fixture(family: &str, n: usize) -> (Model, f64) {
    match family {
        "packing" => packing(n),
        "irregular" => irregular(n),
        "network" => network(n),
        "selection" => selection(n),
        "dense" => dense(n),
        _ => panic!("unknown family"),
    }
}
fn blocks(n: usize, skew: bool) -> (Model, f64) {
    let parts: Vec<_> = (0..16)
        .map(|i| packing(if skew && i >= 14 { n * 4 } else { n }))
        .collect();
    let cols = parts.iter().map(|p| p.0.n()).sum();
    let rows = parts.iter().map(|p| p.0.m()).sum();
    let mut indptr = vec![0];
    let mut indices = Vec::new();
    let mut values = Vec::new();
    let mut c = Vec::new();
    let mut rhs = Vec::new();
    let mut offset = 0;
    for (m, _) in &parts {
        c.extend_from_slice(m.objective());
        rhs.extend_from_slice(&m.rows().upper);
        let a = m.matrix();
        for i in 0..m.m() {
            for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                indices.push(offset + a.pattern.indices[k]);
                values.push(a.values[k]);
            }
            indptr.push(indices.len());
        }
        offset += m.n();
    }
    (
        Model::new(ModelData {
            sense: Sense::Maximize,
            objective: c,
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; cols], vec![2.; cols]),
            kinds: vec![VariableKind::Continuous; cols],
            matrix: CsrMatrix::new(rows, cols, indptr, indices, values),
            rows: Bounds::new(vec![f64::NEG_INFINITY; rows], rhs),
        })
        .unwrap(),
        parts.iter().map(|p| p.1).sum(),
    )
}
fn check_result(m: &Model, expected: f64, r: &SolveResult) -> (f64, f64) {
    assert_eq!(r.status, Status::Optimal, "{}", r.message);
    let x = r.candidate.as_ref().expect("optimal point");
    let e = check::evaluate(m, x, &Tolerances::default()).unwrap();
    assert!(e.feasible_candidate);
    assert!(e.objective.is_finite());
    assert!(
        (e.objective - expected).abs() <= 1e-6 * (1. + expected.abs()),
        "{} != {}",
        e.objective,
        expected
    );
    let mut residual = 0_f64;
    if !m.kinds().iter().any(|k| k.is_integral()) {
        let q = check::lp_residuals(m, x, r.duals.as_ref().expect("LP certificate")).unwrap();
        let scale = 1. + m.objective().iter().fold(0_f64, |a, v| a.max(v.abs()));
        assert!(
            q.primal <= 1e-7
                && q.stationarity <= 1e-7 * scale
                && q.dual_sign <= 1e-7 * scale
                && q.duality_gap <= 1e-7
        );
        residual = q.max();
        assert!(residual.is_finite());
    }
    (e.objective, residual)
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let family = args.get(1).map(String::as_str).unwrap_or("packing");
    let n = args.get(2).map(|v| v.parse().unwrap()).unwrap_or(128);
    let mode = args.get(3).map(String::as_str).unwrap_or("single");
    let inner = args.get(4).map(|v| v.parse().unwrap()).unwrap_or(1);
    let workers = args.get(5).map(|v| v.parse().unwrap()).unwrap_or(1);
    let jobs = args.get(6).map(|v| v.parse().unwrap()).unwrap_or(1);
    assert!(inner * workers <= 8 && inner > 0 && workers > 0 && jobs > 0);
    let mut settings = NativeOptions {
        decompose: false,
        component_threads: 1,
        packing_workers: 1,
        tree_threads: 1,
        time_limit_seconds: Some(20.),
        ..Default::default()
    };
    if mode.contains("components") {
        settings.decompose = true;
        settings.component_threads = workers;
        settings.component_parallel_min_entries = 0;
    }
    if mode == "tree" {
        settings.tree_threads = workers;
        settings.tree_split_depth = 3;
    }
    if mode == "residual" {
        settings.packing_workers = workers;
        settings.packing_root = true;
        settings.packing_neighborhoods = 16;
        settings.packing_neighborhood_size = 24;
        settings.packing_rounds = 2;
    }
    let models: Vec<_> = (0..jobs)
        .map(|i| {
            if mode.contains("components") {
                blocks(n, mode == "skewcomponents")
            } else {
                fixture(
                    family,
                    if mode == "skewbatch" && i >= jobs * 3 / 4 {
                        n * 4
                    } else {
                        n
                    },
                )
            }
        })
        .collect();
    let options = Options {
        threads: inner,
        ..Default::default()
    };
    let token = CancellationToken::default();
    let solver = NativeSolver::new(settings);
    let t = Instant::now();
    let result = if mode.contains("batch") {
        solver
            .solve_batch(
                &models.iter().map(|p| &p.0).collect::<Vec<_>>(),
                &options,
                workers,
                &token,
            )
            .unwrap()
    } else {
        assert_eq!(jobs, 1);
        vec![NativeSolver::new(solver.settings.clone()).solve(&models[0].0, &options, None, &token)]
    };
    let seconds = t.elapsed().as_secs_f64();
    assert_eq!(result.len(), jobs);
    let (mut objective, mut residual, mut pivots, mut nodes) = (0_f64, 0_f64, 0_u64, 0_u64);
    for ((m, e), r) in models.iter().zip(result) {
        let r = r.unwrap();
        let (c, k) = check_result(m, *e, &r);
        objective += c;
        residual = residual.max(k);
        pivots += r.iterations;
        nodes += r.nodes;
    }
    println!(
        "{{\"version\":\"{}\",\"family\":\"{family}\",\"size\":{n},\"mode\":\"{mode}\",\"inner\":{inner},\"workers\":{workers},\"jobs\":{jobs},\"completed\":{jobs},\"seconds\":{seconds},\"objective\":{objective},\"residual\":{residual},\"pivots\":{pivots},\"nodes\":{nodes},\"status\":\"optimal\"}}",
        env!("CARGO_PKG_VERSION")
    );
}
