use lp_ip_craft::*;
use lp_ip_craft_native::*;
fn model(free: bool) -> Model {
    Model::new(ModelData {
        sense: Sense::Maximize,
        objective: vec![1.],
        objective_offset: 2.,
        variables: if free {
            Bounds::free(1)
        } else {
            Bounds::new(vec![0.], vec![7.])
        },
        kinds: vec![if free {
            VariableKind::Integer
        } else {
            VariableKind::Continuous
        }],
        matrix: CsrMatrix::new(0, 1, vec![0], vec![], vec![]),
        rows: Bounds::free(0),
    })
    .unwrap()
}
#[test]
fn ordered_batch_matches_serial_at_all_worker_counts_and_keeps_job_errors() {
    let (good, bad) = (model(false), model(true));
    let models = vec![
        &good, &bad, &good, &good, &bad, &good, &good, &good, &good, &good, &good, &good, &good,
        &good, &good, &good, &good,
    ];
    let solver = NativeSolver::default();
    for workers in [1, 2, 4, 8, 16] {
        let results = solver
            .solve_batch(
                &models,
                &Options::default(),
                workers,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(results.len(), models.len());
        for (i, r) in results.into_iter().enumerate() {
            if i == 1 || i == 4 {
                assert!(matches!(r, Err(Error::Unsupported(_))));
            } else {
                let r = r.unwrap();
                assert_eq!(r.status, Status::Optimal);
                assert_eq!(r.evaluation.unwrap().objective, 9.);
            }
        }
    }
}
#[test]
fn batch_empty_invalid_and_cancelled() {
    let solver = NativeSolver::default();
    let token = CancellationToken::default();
    let options = Options::default();
    assert!(
        solver
            .solve_batch(&[], &options, 16, &token)
            .unwrap()
            .is_empty()
    );
    for n in [0, 17, usize::MAX] {
        assert!(matches!(
            solver.solve_batch(&[], &options, n, &token),
            Err(Error::InvalidInput(_))
        ));
    }
    let m = model(false);
    token.cancel();
    let jobs = vec![&m; 20];
    assert!(
        solver
            .solve_batch(&jobs, &options, 4, &token)
            .unwrap()
            .into_iter()
            .all(|r| r.unwrap().status == Status::Cancelled)
    );
}
