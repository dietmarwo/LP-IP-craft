use lp_ip_craft::*;
use lp_ip_craft_native::*;
#[test]
fn component_reports_have_unique_keys_and_actual_worker_counts() {
    let m = Model::new(ModelData {
        sense: Sense::Maximize,
        objective: vec![3., 2., 5., 1.],
        objective_offset: 0.,
        variables: Bounds::new(vec![0.; 4], vec![1.; 4]),
        kinds: vec![VariableKind::Continuous; 4],
        matrix: CsrMatrix::from_dense(2, 4, &[1., 1., 0., 0., 0., 0., 1., 1.]).unwrap(),
        rows: Bounds::new(vec![f64::NEG_INFINITY; 2], vec![1.5; 2]),
    })
    .unwrap();
    for workers in [1, 2] {
        let r = NativeSolver::new(NativeOptions {
            component_threads: workers,
            decomposition_min_variables: 1,
            component_parallel_min_entries: 0,
            ..Default::default()
        })
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
        assert_eq!(r.status, Status::Optimal);
        let mut names = std::collections::HashSet::new();
        for (k, _) in &r.backend.effective_options {
            assert!(names.insert(k), "duplicate report key {k}");
        }
        assert_eq!(
            r.backend
                .effective_options
                .iter()
                .find(|(k, _)| k == "diagnostics.component_workers")
                .unwrap()
                .1,
            workers.to_string()
        );
    }
}
