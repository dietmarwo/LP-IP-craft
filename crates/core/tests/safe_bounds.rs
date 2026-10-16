mod common;

use lp_ip_craft::{Bounds, CsrMatrix, Model, SafeBoundStatus, Sense, safe_dual_bound};

#[test]
fn corrected_bound_survives_perturbed_duals_in_both_senses() {
    for (sense, y, optimum) in [
        (Sense::Minimize, 2.0 + 1e-7, 7.0),
        (Sense::Maximize, 3.0 - 1e-7, 8.0),
    ] {
        let mut data = common::assignment_data();
        data.sense = sense;
        let model = Model::new(data).unwrap();
        let report = safe_dual_bound(&model, &[y]).unwrap();
        assert_eq!(report.status, SafeBoundStatus::Finite);
        assert_eq!(report.index, None);
        let bound = report.bound.unwrap();
        match sense {
            Sense::Minimize => assert!(bound <= optimum),
            Sense::Maximize => assert!(bound >= optimum),
        }
        assert!((bound - optimum).abs() < 1e-12);
    }
}

#[test]
fn box_only_and_mip_use_the_same_continuous_bound() {
    let mut data = common::assignment_data();
    data.matrix = CsrMatrix::from_dense(0, 2, &[]).unwrap();
    data.rows = Bounds::new(vec![], vec![]);
    data.variables = Bounds::new(vec![-2.0, -3.0], vec![4.0, 5.0]);
    data.kinds = vec![lp_ip_craft::VariableKind::Integer; 2];
    let model = Model::new(data.clone()).unwrap();
    let lower = safe_dual_bound(&model, &[]).unwrap().bound.unwrap();
    assert!(lower <= -8.0 && lower > -8.0 - 1e-12);
    data.sense = Sense::Maximize;
    let upper = safe_dual_bound(&Model::new(data).unwrap(), &[])
        .unwrap()
        .bound
        .unwrap();
    assert!((28.0..28.0 + 1e-12).contains(&upper));
}

#[test]
fn invalid_row_sign_reports_row_without_a_model_infeasibility_claim() {
    let mut data = common::assignment_data();
    data.rows = Bounds::new(vec![f64::NEG_INFINITY], vec![1.0]);
    let model = Model::new(data).unwrap();
    let report = safe_dual_bound(&model, &[1.0]).unwrap();
    assert_eq!(report.status, SafeBoundStatus::UnboundedRow);
    assert_eq!(report.index, Some(0));
    assert_eq!(report.bound, None);
    assert!(safe_dual_bound(&model, &[0.0]).unwrap().bound.is_some());
    assert!(safe_dual_bound(&model, &[-1.0]).unwrap().bound.is_some());
}

#[test]
fn free_variable_needs_an_enclosed_zero_not_a_tolerance() {
    let mut data = common::assignment_data();
    data.kinds = vec![lp_ip_craft::VariableKind::Continuous; 2];
    data.variables = Bounds::new(vec![f64::NEG_INFINITY, 0.0], vec![f64::INFINITY, 1.0]);
    let model = Model::new(data).unwrap();
    // A coefficient is 1, so the exact residual 2 - 1*2 stays exactly zero.
    assert!(safe_dual_bound(&model, &[2.0]).unwrap().bound.is_some());
    let report = safe_dual_bound(&model, &[2.0 + 1e-12]).unwrap();
    assert_eq!(report.status, SafeBoundStatus::UnboundedVariable);
    assert_eq!(report.index, Some(0));
    assert!(report.bound.is_none());
}

#[test]
fn free_rows_zero_duals_fixed_variables_and_empty_rows() {
    let mut data = common::assignment_data();
    data.variables = Bounds::fixed(vec![0.0, 1.0]);
    data.rows = Bounds::new(vec![f64::NEG_INFINITY, 0.0], vec![f64::INFINITY, 0.0]);
    data.matrix = CsrMatrix::from_dense(2, 2, &[1.0, 1.0, 0.0, 0.0]).unwrap();
    let model = Model::new(data).unwrap();
    let report = safe_dual_bound(&model, &[-0.0, 1e300]).unwrap();
    assert_eq!(report.status, SafeBoundStatus::Finite);
    assert!(report.bound.unwrap() <= 8.0);
    assert!(report.bound.unwrap() > 8.0 - 1e-12);
}

#[test]
fn overflow_is_unavailable_and_bad_input_is_an_error() {
    let model = common::assignment();
    common::assert_invalid(safe_dual_bound(&model, &[]), "expected 1 entries");
    common::assert_invalid(safe_dual_bound(&model, &[f64::NAN]), "row duals");
    common::assert_invalid(safe_dual_bound(&model, &[f64::INFINITY]), "row duals");
    let mut data = common::assignment_data();
    data.rows = Bounds::fixed(vec![2.0]);
    let result = safe_dual_bound(&Model::new(data).unwrap(), &[f64::MAX]).unwrap();
    assert_eq!(result.status, SafeBoundStatus::ArithmeticOverflow);
    assert!(result.bound.is_none() && result.index.is_none());
}

#[test]
fn tutorial_with_explicit_finite_box_has_a_useful_safe_bound() {
    let mut data = common::production_data();
    // Valid bounds implied by the original wood/finishing resource rows.
    // Bound derivation is explicit here; the checker does no propagation.
    data.variables.upper = vec![80.0, 20.0, 12.0];
    let model = Model::new(data).unwrap();
    let bound = safe_dual_bound(&model, &common::production_duals().row_duals)
        .unwrap()
        .bound
        .unwrap();
    assert!((2150.0..2150.0 + 1e-9).contains(&bound));
}
