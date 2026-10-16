//! Shared fixtures.
#![allow(dead_code)]

use lp_ip_craft::{Bounds, CsrMatrix, Duals, Model, ModelData, Sense, VariableKind};

/// minimize 2 x0 + 3 x1 + 5  subject to  x0 + x1 = 1,  x binary.
/// Optimum x = (1, 0) with objective 7.
pub fn assignment_data() -> ModelData {
    ModelData {
        sense: Sense::Minimize,
        objective: vec![2.0, 3.0],
        objective_offset: 5.0,
        variables: Bounds::new(vec![0.0, 0.0], vec![1.0, 1.0]),
        kinds: vec![VariableKind::Binary; 2],
        matrix: CsrMatrix::new(1, 2, vec![0, 2], vec![0, 1], vec![1.0, 1.0]),
        rows: Bounds::fixed(vec![1.0]),
    }
}

pub fn assignment() -> Model {
    Model::new(assignment_data()).unwrap()
}

/// Production planning LP, Example 1 of the project's LP/IP tutorial:
///
/// ```text
/// maximize 45 chair + 80 table + 110 desk
/// s.t.      5 chair + 20 table + 25 desk <= 400   (wood)
///          10 chair + 15 table + 20 desk <= 450   (labor)
///             chair +  3 table +  5 desk <=  60   (finishing)
///          x >= 0
/// ```
///
/// Gurobi and HiGHS report x = (30, 10, 0), profit 2150, row duals
/// (0, 3.667, 8.333) and reduced costs (0, 0, -5).
pub fn production_data() -> ModelData {
    ModelData {
        sense: Sense::Maximize,
        objective: vec![45.0, 80.0, 110.0],
        objective_offset: 0.0,
        variables: Bounds::non_negative(3),
        kinds: vec![VariableKind::Continuous; 3],
        matrix: CsrMatrix::from_dense(3, 3, &[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0])
            .unwrap(),
        rows: Bounds::new(vec![f64::NEG_INFINITY; 3], vec![400.0, 450.0, 60.0]),
    }
}

pub fn production() -> Model {
    Model::new(production_data()).unwrap()
}

pub const PRODUCTION_OPTIMUM: [f64; 3] = [30.0, 10.0, 0.0];

/// Duals of the production LP in the crate's convention, as Gurobi (Pi, RC)
/// and HiGHS (row_dual, col_dual) print them for the maximization.
pub fn production_duals() -> Duals {
    Duals {
        row_duals: vec![0.0, 11.0 / 3.0, 25.0 / 3.0],
        reduced_costs: vec![0.0, 0.0, -5.0],
    }
}

/// Asserts that `result` is an `InvalidInput` error whose message contains `needle`.
pub fn assert_invalid<T: std::fmt::Debug>(result: lp_ip_craft::Result<T>, needle: &str) {
    match result {
        Err(lp_ip_craft::Error::InvalidInput(message)) => assert!(
            message.contains(needle),
            "message {message:?} does not contain {needle:?}"
        ),
        other => panic!("expected InvalidInput containing {needle:?}, got {other:?}"),
    }
}
