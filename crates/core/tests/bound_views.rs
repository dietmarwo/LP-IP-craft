use lp_ip_craft::*;
#[test]
fn borrowed_bounds_match_materialized_models() {
    for sense in [Sense::Minimize, Sense::Maximize] {
        let model = Model::new(ModelData {
            sense,
            objective: vec![2., -3., 1.],
            objective_offset: 0.5,
            variables: Bounds::new(vec![-3.; 3], vec![4.; 3]),
            kinds: vec![VariableKind::Continuous; 3],
            matrix: CsrMatrix::from_dense(2, 3, &[1., 2., -1., -2., 0., 3.]).unwrap(),
            rows: Bounds::new(vec![-2., -3.], vec![5., 7.]),
        })
        .unwrap();
        for upper in [1., 2., 3.] {
            let bounds = Bounds::new(vec![0., -1., 0.], vec![upper, 2., 3.]);
            let mut data = model.data().clone();
            data.variables = bounds.clone();
            let copied = Model::new(data).unwrap();
            let x = [0.5, 0., 1.];
            let duals = Duals {
                row_duals: vec![1., -0.5],
                reduced_costs: vec![0.2, 0.3, -0.1],
            };
            assert_eq!(
                check::lp_residuals_with_bounds(&model, &bounds, &x, &duals),
                check::lp_residuals(&copied, &x, &duals)
            );
            assert_eq!(
                safe_dual_bound_with_bounds(&model, &bounds, &duals.row_duals),
                safe_dual_bound(&copied, &duals.row_duals)
            );
        }
        let invalid = Bounds::new(vec![0.], vec![1.]);
        assert!(safe_dual_bound_with_bounds(&model, &invalid, &[0., 0.]).is_err());
        assert!(
            check::lp_residuals_with_bounds(
                &model,
                &invalid,
                &[0.; 3],
                &Duals {
                    row_duals: vec![0.; 2],
                    reduced_costs: vec![0.; 3]
                }
            )
            .is_err()
        );
    }
}
