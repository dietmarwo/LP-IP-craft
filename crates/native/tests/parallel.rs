use lp_ip_craft::*;
use lp_ip_craft_native::*;
fn box_model() -> Model {
    Model::new(ModelData {
        sense: Sense::Maximize,
        objective: vec![1.],
        objective_offset: 0.,
        variables: Bounds::new(vec![0.], vec![7.]),
        kinds: vec![VariableKind::Continuous],
        matrix: CsrMatrix::new(0, 1, vec![0], vec![], vec![]),
        rows: Bounds::free(0),
    })
    .unwrap()
}
#[test]
fn native_budget_is_explicit_and_dummy_still_rejects_workers() {
    let m = box_model();
    for threads in [1, 2, 4, 8] {
        let o = Options {
            threads,
            ..Default::default()
        };
        o.validate().unwrap();
        let r = NativeSolver::default()
            .solve(&m, &o, None, &CancellationToken::default())
            .unwrap();
        assert_eq!(r.status, Status::Optimal);
        assert_eq!(r.evaluation.unwrap().objective, 7.);
        assert_eq!(
            r.backend
                .effective_options
                .iter()
                .find(|(k, _)| k == "numeric_threads")
                .unwrap()
                .1,
            threads.to_string()
        );
        if threads > 1 {
            assert!(matches!(
                DummySolver.solve(&m, &o, None, &CancellationToken::default()),
                Err(Error::Unsupported(_))
            ));
        }
    }
    assert!(
        Options {
            threads: 0,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Options {
            threads: 9,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}
#[test]
fn precancel_and_zero_time_keep_existing_precedence() {
    let m = box_model();
    for threads in [1, 8] {
        let o = Options {
            threads,
            ..Default::default()
        };
        let c = CancellationToken::default();
        c.cancel();
        let mut s = NativeSolver::default();
        assert_eq!(s.solve(&m, &o, None, &c).unwrap().status, Status::Cancelled);
        assert!(
            s.solve(&m, &o, Some(&WarmStart::from_primal(vec![f64::NAN])), &c)
                .is_err()
        );
        s.settings.time_limit_seconds = Some(0.);
        assert_eq!(
            s.solve(&m, &o, None, &CancellationToken::default())
                .unwrap()
                .status,
            Status::TimeLimit
        );
    }
}

#[test]
fn mixed_batch_budget_and_old_outer_width_are_explicit() {
    let m = box_model();
    let jobs = vec![&m; 17];
    let s = NativeSolver::default();
    for (outer, inner) in [(1, 8), (2, 4), (4, 2), (8, 1), (16, 1)] {
        let out = s
            .solve_batch(
                &jobs,
                &Options {
                    threads: inner,
                    ..Default::default()
                },
                outer,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(out.len(), 17);
        assert!(
            out.into_iter()
                .all(|v| v.unwrap().status == Status::Optimal)
        );
    }
    for (outer, inner) in [(2, 8), (4, 4), (8, 2), (16, 2)] {
        assert!(matches!(
            s.solve_batch(
                &jobs,
                &Options {
                    threads: inner,
                    ..Default::default()
                },
                outer,
                &CancellationToken::default()
            ),
            Err(Error::InvalidInput(_))
        ));
    }
}

fn dense_model(n: usize, salt: usize) -> (Model, f64) {
    let a: Vec<_> = (0..n)
        .flat_map(|i| {
            (0..n).map(move |j| {
                if i == j {
                    2. * n as f64
                } else {
                    ((i * 37 + j * 19 + salt) % 17 + 1) as f64 / 16.
                }
            })
        })
        .collect();
    let mut c = vec![0.; n];
    let mut rhs = Vec::new();
    for i in 0..n {
        rhs.push(a[i * n..(i + 1) * n].iter().sum());
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
#[test]
fn independent_callers_share_only_execution_and_recover_after_stop_and_error() {
    let shared = std::sync::Arc::new(dense_model(257, 3));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    std::thread::scope(|scope| {
        let handles: Vec<_> = [1, 1, 2, 4]
            .into_iter()
            .enumerate()
            .map(|(worker, threads)| {
                let (shared, barrier) = (shared.clone(), barrier.clone());
                scope.spawn(move || {
                    let mut solver = NativeSolver::new(NativeOptions {
                        decompose: false,
                        component_threads: 1,
                        ..Default::default()
                    });
                    let options = Options {
                        threads,
                        ..Default::default()
                    };
                    barrier.wait();
                    let r = solver
                        .solve(&shared.0, &options, None, &CancellationToken::default())
                        .unwrap();
                    assert_eq!(r.status, Status::Optimal);
                    assert!((r.evaluation.unwrap().objective - shared.1).abs() < 1e-6);
                    solver.settings.time_limit_seconds = Some(0.);
                    assert_eq!(
                        solver
                            .solve(&shared.0, &options, None, &CancellationToken::default())
                            .unwrap()
                            .status,
                        Status::TimeLimit
                    );
                    solver.settings.time_limit_seconds = None;
                    assert!(
                        solver
                            .solve(
                                &shared.0,
                                &options,
                                Some(&WarmStart::from_primal(vec![f64::NAN; 257])),
                                &CancellationToken::default()
                            )
                            .is_err()
                    );
                    let token = CancellationToken::default();
                    token.cancel();
                    assert_eq!(
                        solver
                            .solve(&shared.0, &options, None, &token)
                            .unwrap()
                            .status,
                        Status::Cancelled
                    );
                    for n in [193, 289] {
                        let (m, expected) = dense_model(n, worker + 11);
                        let r = solver
                            .solve(&m, &options, None, &CancellationToken::default())
                            .unwrap();
                        assert_eq!(r.status, Status::Optimal);
                        assert!((r.evaluation.as_ref().unwrap().objective - expected).abs() < 1e-6);
                        let q = check::lp_residuals(
                            &m,
                            r.candidate.as_ref().unwrap(),
                            r.duals.as_ref().unwrap(),
                        )
                        .unwrap();
                        assert!(q.primal < 1e-7 && q.duality_gap < 1e-7);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    });
}
