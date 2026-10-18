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
