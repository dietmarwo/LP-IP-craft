use lp_ip_craft::*;
use lp_ip_craft_native::*;
#[test]
fn box_fast_path_matches_general_solver_and_limits() {
    for sense in [Sense::Minimize, Sense::Maximize] {
        let model = Model::new(ModelData {
            sense,
            objective: vec![2., -3., 0., 4.],
            objective_offset: 7.,
            variables: Bounds::new(
                vec![-2., -3., f64::NEG_INFINITY, 2.],
                vec![3., 4., f64::INFINITY, 2.],
            ),
            kinds: vec![VariableKind::Continuous; 4],
            matrix: CsrMatrix::new(0, 4, vec![0], vec![], vec![]),
            rows: Bounds::free(0),
        })
        .unwrap();
        let run = |presolve, limit| {
            NativeSolver::new(NativeOptions {
                presolve,
                iteration_limit: limit,
                ..Default::default()
            })
            .solve(
                &model,
                &Options::default(),
                None,
                &CancellationToken::default(),
            )
            .unwrap()
        };
        let (fast, full) = (run(true, None), run(false, None));
        assert_eq!(fast.status, Status::Optimal);
        assert_eq!(
            fast.evaluation.unwrap().objective,
            full.evaluation.unwrap().objective
        );
        assert_eq!(run(true, Some(0)).status, Status::IterationLimit);
        let mut d = model.into_data();
        d.variables.upper[0] = f64::INFINITY;
        d.sense = Sense::Maximize;
        assert_eq!(
            NativeSolver::default()
                .solve(
                    &Model::new(d).unwrap(),
                    &Options::default(),
                    None,
                    &CancellationToken::default()
                )
                .unwrap()
                .status,
            Status::Unbounded
        );
    }
}
