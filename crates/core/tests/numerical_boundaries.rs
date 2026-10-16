use lp_ip_craft::*;
#[test]
fn checked_gap_distinguishes_overflow_from_zero_objective_infinity() {
    assert!(matches!(
        Gap::try_new(Sense::Minimize, 1e308, -1e308),
        Err(Error::Evaluation(_))
    ));
    assert!(Gap::try_new(Sense::Minimize, 1e-308, -1e308).is_err());
    assert!(
        Gap::try_new(Sense::Minimize, 0.0, -1.0)
            .unwrap()
            .relative
            .is_infinite()
    );
    assert_eq!(
        Gap::try_new(Sense::Minimize, 0.0, 0.0).unwrap().relative,
        0.0
    );
    assert_eq!(
        Gap::try_new(Sense::Minimize, 1.0, 2.0).unwrap().absolute,
        -1.0
    );
}
#[test]
fn overflowing_dual_product_is_an_error() {
    let model = Model::new(ModelData {
        objective: vec![1.0],
        objective_offset: 0.0,
        sense: Sense::Minimize,
        variables: Bounds::free(1),
        kinds: vec![VariableKind::Continuous],
        matrix: CsrMatrix::from_dense(2, 1, &[1e308, -1e308]).unwrap(),
        rows: Bounds::fixed(vec![0.0; 2]),
    })
    .unwrap();
    assert!(matches!(
        check::lp_residuals(
            &model,
            &[0.0],
            &Duals {
                row_duals: vec![1e308; 2],
                reduced_costs: vec![0.0]
            }
        ),
        Err(Error::Evaluation(_))
    ));
}
#[test]
fn checked_bounds_reject_mismatch_nonfinite_and_overflow() {
    assert!(
        Bounds::new(vec![0.0], vec![])
            .checked_violation(&[-1.0])
            .is_err()
    );
    assert!(Bounds::free(1).checked_violation(&[f64::NAN]).is_err());
    assert!(matches!(
        Bounds::fixed(vec![1e308]).checked_violation(&[-1e308]),
        Err(Error::Evaluation(_))
    ));
}
