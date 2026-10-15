mod common;

use common::{assert_invalid, assignment_data, production_data};
use lp_ip_craft::{Bounds, CsrMatrix, Error, Model, Sense, VariableKind};

#[test]
fn valid_models_expose_their_parts() {
    let model = Model::new(assignment_data()).unwrap();
    assert_eq!((model.n(), model.m(), model.nnz()), (2, 1, 2));
    assert_eq!(model.sense(), Sense::Minimize);
    assert_eq!(model.objective_offset(), 5.0);
    assert!(model.is_mip());
    assert!(!Model::new(production_data()).unwrap().is_mip());
    assert_eq!(model.clone().into_data(), assignment_data());
}

#[test]
fn rowless_model_with_free_variable_is_valid() {
    let model = Model::new(lp_ip_craft::ModelData {
        sense: Sense::Minimize,
        objective: vec![1.0],
        objective_offset: 0.0,
        variables: Bounds::free(1),
        kinds: vec![VariableKind::Continuous],
        matrix: CsrMatrix::new(0, 1, vec![0], vec![], vec![]),
        rows: Bounds::new(vec![], vec![]),
    })
    .unwrap();
    assert_eq!((model.n(), model.m(), model.nnz()), (1, 0, 0));
}

#[test]
fn fixed_binaries_and_fractional_integer_bounds_are_accepted() {
    let mut data = assignment_data();
    data.variables = Bounds::new(vec![1.0, 0.0], vec![1.0, 0.0]);
    Model::new(data).unwrap();
    // Integer bounds are used exactly as given; a backend may tighten them.
    let mut data = assignment_data();
    data.kinds = vec![VariableKind::Integer; 2];
    data.variables = Bounds::new(vec![0.5, -2.5], vec![3.7, 1.0]);
    Model::new(data).unwrap();
}

#[test]
fn zero_variables_rejected() {
    let mut data = assignment_data();
    data.objective.clear();
    assert_invalid(Model::new(data), "at least one variable");
}

#[test]
fn non_finite_objective_and_offset_rejected() {
    let mut data = assignment_data();
    data.objective[1] = f64::INFINITY;
    assert_invalid(Model::new(data), "objective: entry 1 is inf");
    let mut data = assignment_data();
    data.objective_offset = f64::NAN;
    assert_invalid(Model::new(data), "objective offset is NaN");
}

#[test]
fn invalid_variable_bounds_rejected_with_entry() {
    for (lower, upper) in [
        (f64::NAN, 1.0),
        (1.0, 0.0),
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    ] {
        let mut data = assignment_data();
        data.kinds = vec![VariableKind::Continuous; 2];
        data.variables = Bounds::new(vec![0.0, lower], vec![1.0, upper]);
        assert_invalid(Model::new(data), "variable bounds: entry 1");
    }
    let mut data = assignment_data();
    data.variables.upper.pop();
    assert_invalid(Model::new(data), "variable bounds: expected 2 entries");
}

#[test]
fn kinds_must_match_and_binaries_stay_in_unit_box() {
    let mut data = assignment_data();
    data.kinds.pop();
    assert_invalid(
        Model::new(data),
        "variable kinds: expected 2 entries, got 1",
    );
    let mut data = assignment_data();
    data.variables.upper[1] = 2.0;
    assert_invalid(Model::new(data), "variable 1 is binary");
    let mut data = assignment_data();
    data.variables.lower[0] = -1.0;
    assert_invalid(Model::new(data), "variable 0 is binary");
}

#[test]
fn invalid_row_bounds_rejected() {
    let mut data = assignment_data();
    data.rows.upper[0] = f64::NEG_INFINITY;
    assert_invalid(Model::new(data), "row bounds: entry 0");
    let mut data = assignment_data();
    data.rows = Bounds::new(vec![1.0, 1.0], vec![1.0, 1.0]);
    assert_invalid(Model::new(data), "matrix: shape is 1 x 2, expected");
}

type Patch = fn(&mut CsrMatrix);

#[test]
fn malformed_csr_rejected() {
    let cases: [(Patch, &str); 8] = [
        (|a| a.pattern.indptr = vec![0], "indptr has 1 entries"),
        (|a| a.pattern.indptr = vec![1, 2], "must start at 0"),
        (|a| a.pattern.indptr = vec![0, 3], "must start at 0"),
        (|a| a.pattern.indices = vec![0, 0], "strictly increasing"),
        (|a| a.pattern.indices = vec![1, 0], "strictly increasing"),
        (|a| a.pattern.indices = vec![0, 2], "column index 2"),
        (|a| a.values = vec![1.0], "1 values for 2 stored entries"),
        (|a| a.values[1] = f64::NAN, "matrix values: entry 1 is NaN"),
    ];
    for (patch, needle) in cases {
        let mut data = assignment_data();
        patch(&mut data.matrix);
        assert_invalid(Model::new(data), needle);
    }
}

#[test]
fn decreasing_row_pointers_rejected() {
    let mut data = assignment_data();
    data.matrix = CsrMatrix::new(2, 2, vec![0, 2, 1], vec![0, 1], vec![1.0, 1.0]);
    data.rows = Bounds::fixed(vec![1.0, 1.0]);
    assert_invalid(Model::new(data), "must start at 0 and end at");
    let mut data = assignment_data();
    data.matrix = CsrMatrix::new(3, 2, vec![0, 2, 1, 2], vec![0, 1], vec![1.0, 1.0]);
    data.rows = Bounds::fixed(vec![1.0, 1.0, 1.0]);
    assert_invalid(Model::new(data), "indptr decreases at row 1");
}

#[test]
fn from_dense_skips_zeros_and_checks_size() {
    let a = CsrMatrix::from_dense(2, 3, &[1.0, 0.0, 2.0, 0.0, 0.0, 0.0]).unwrap();
    assert_eq!(a.pattern.indptr, vec![0, 2, 2]);
    assert_eq!(a.pattern.indices, vec![0, 2]);
    assert_eq!(a.values, vec![1.0, 2.0]);
    assert_invalid(CsrMatrix::from_dense(2, 2, &[1.0]), "1 values for a 2 x 2");
    assert_invalid(
        CsrMatrix::from_dense(1, 2, &[1.0, f64::INFINITY]),
        "entry 1 is inf",
    );
}

#[test]
fn error_display_has_category_and_message_has_not() {
    let e = Error::InvalidInput("x".into());
    assert_eq!(e.to_string(), "invalid input: x");
    assert_eq!(e.message(), "x");
    assert_eq!(Error::Unsupported("y".into()).to_string(), "unsupported: y");
}
