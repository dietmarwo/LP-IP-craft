use lp_ip_craft::*;
use lp_ip_craft_native::*;
#[allow(dead_code)]
#[path = "../examples/sparse_benchmark.rs"]
mod fixtures;
fn run(m: &Model, warm: bool, interval: usize) -> SolveResult {
    NativeSolver::new(NativeOptions {
        sparse: true,
        decompose: false,
        node_reoptimization: warm,
        sparse_refactor_interval: interval,
        ..Default::default()
    })
    .solve(m, &Options::default(), None, &CancellationToken::default())
    .unwrap()
}
#[test]
fn planted_primal_dual_and_network_certificates() {
    for size in [8, 17, 64] {
        for fixture in [fixtures::packing, fixtures::network, fixtures::irregular] {
            let (m, expected) = fixture(size);
            for interval in [1, 8, 64, 10000] {
                let r = run(&m, true, interval);
                assert_eq!(
                    r.status,
                    Status::Optimal,
                    "size={size}, interval={interval}: {r:?}"
                );
                assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-6);
                let k = check::lp_residuals(
                    &m,
                    r.candidate.as_ref().unwrap(),
                    r.duals.as_ref().unwrap(),
                )
                .unwrap();
                assert!(k.duality_gap < 1e-7 && k.primal < 1e-7);
            }
        }
    }
}
#[test]
fn sparse_ignores_dense_cap_and_enforces_own_cap() {
    let (m, expected) = fixtures::packing(96);
    let mut solver = NativeSolver::new(NativeOptions {
        sparse: true,
        max_tableau_entries: 4,
        decompose: false,
        ..Default::default()
    });
    let r = solver
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
    assert_eq!(r.status, Status::Optimal);
    assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-6);
    solver.settings.max_sparse_entries = 10;
    assert!(matches!(
        solver.solve(&m, &Options::default(), None, &CancellationToken::default()),
        Err(Error::Unsupported(_))
    ));
    solver.settings.max_sparse_entries = 2_000_000;
    solver.settings.sparse_refactor_interval = 0;
    assert!(matches!(
        solver.solve(&m, &Options::default(), None, &CancellationToken::default()),
        Err(Error::InvalidInput(_))
    ));
}
#[test]
fn warm_and_cold_mip_match_independent_dynamic_program() {
    for n in [8, 12, 16] {
        let (m, expected) = fixtures::selection(n);
        for interval in [1, 64] {
            for warm in [false, true] {
                let r = run(&m, warm, interval);
                assert_eq!(r.status, Status::Optimal, "{r:?}");
                assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-7);
                let d: std::collections::BTreeMap<_, _> =
                    r.backend.effective_options.into_iter().collect();
                if warm {
                    assert!(d["diagnostics.reoptimized_nodes"].parse::<u64>().unwrap() > 0);
                }
            }
        }
    }
}
#[test]
fn sparse_limits_cancel_and_primal_start_semantics() {
    let (m, _) = fixtures::packing(32);
    let token = CancellationToken::default();
    token.cancel();
    let mut solver = NativeSolver::new(NativeOptions {
        sparse: true,
        ..Default::default()
    });
    assert_eq!(
        solver
            .solve(&m, &Options::default(), None, &token)
            .unwrap()
            .status,
        Status::Cancelled
    );
    solver.settings.iteration_limit = Some(1);
    let r = solver
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
    assert_eq!(r.status, Status::IterationLimit);
    assert_eq!(r.iterations, 1);
    solver.settings.iteration_limit = None;
    let r = solver
        .solve(
            &m,
            &Options::default(),
            Some(&WarmStart::from_primal(vec![1.; m.n()])),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(r.backend.start_status, StartStatus::Rejected);
}

#[test]
fn degenerate_cycling_example_and_nearly_dependent_rows() {
    // Beale's cycling example. x1=x3=1 gives the known optimum 1.
    let cases = [
        (
            vec![10., -57., -9., -24.],
            vec![0.5, -5.5, -2.5, 9., 0.5, -1.5, -0.5, 1., 1., 0., 0., 0.],
            vec![0., 0., 1.],
            1.,
        ),
        // Planted x=(1,1), y=(1,1); almost parallel active rows.
        (
            vec![2., 2.0000001],
            vec![1., 1., 1., 1.0000001],
            vec![2., 2.0000001],
            4.0000001,
        ),
    ];
    for (c, a, rhs, expected) in cases {
        let n = c.len();
        let rows = rhs.len();
        let m = Model::new(ModelData {
            sense: Sense::Maximize,
            objective: c,
            objective_offset: 0.,
            variables: Bounds::non_negative(n),
            kinds: vec![VariableKind::Continuous; n],
            matrix: CsrMatrix::from_dense(rows, n, &a).unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY; rows], rhs),
        })
        .unwrap();
        for interval in [1, 64] {
            let r = run(&m, true, interval);
            assert_eq!(r.status, Status::Optimal, "{r:?}");
            assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-7);
        }
    }
}

#[test]
fn larger_degenerate_basis_drift_regression() {
    let (m, expected) = fixtures::packing(256);
    let r = run(&m, true, 64);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-7);
}

#[test]
fn sparse_original_unit_checks_with_scaled_rows() {
    let (m, expected) = fixtures::packing(32);
    let mut data = m.into_data();
    for i in 0..data.rows.len() {
        let scale = [1e-6, 1., 1e6][i % 3];
        for k in data.matrix.pattern.indptr[i]..data.matrix.pattern.indptr[i + 1] {
            data.matrix.values[k] *= scale;
        }
        data.rows.upper[i] *= scale;
    }
    let m = Model::new(data).unwrap();
    let r = run(&m, true, 8);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-7);
}
