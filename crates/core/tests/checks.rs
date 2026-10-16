mod common;

use common::{
    PRODUCTION_OPTIMUM, assert_invalid, assignment, production, production_data, production_duals,
};
use lp_ip_craft::check::lp_residuals;
use lp_ip_craft::{
    Bounds, CsrMatrix, Duals, Evaluation, Gap, Model, ModelData, Sense, SolveResult, Status,
    Tolerances, VariableKind, evaluate,
};

const TOL: f64 = 1e-12;

#[test]
fn evaluate_reproduces_the_tutorial_optimum() {
    let e = evaluate(&production(), &PRODUCTION_OPTIMUM, &Tolerances::default()).unwrap();
    assert_eq!(e.objective, 2150.0);
    assert_eq!(e.row_activity, vec![350.0, 450.0, 60.0]);
    assert_eq!(e.primal_violation, 0.0);
    assert!(e.feasible_candidate);
}

#[test]
fn evaluate_uses_tolerances_at_their_boundary() {
    let model = assignment();
    let strict = Tolerances {
        feasibility: 0.25,
        integrality: 0.25,
    };
    let e = evaluate(&model, &[0.75, 0.0], &strict).unwrap();
    assert_eq!((e.primal_violation, e.integrality_violation), (0.25, 0.25));
    assert!(e.feasible_candidate); // `<=` on both
    let e = evaluate(&model, &[0.7, 0.0], &strict).unwrap();
    assert!(!e.feasible_candidate);
}

#[test]
fn evaluate_rejects_bad_points_and_tolerances() {
    let model = assignment();
    assert_invalid(
        evaluate(&model, &[1.0], &Tolerances::default()),
        "point: expected 2",
    );
    assert_invalid(
        evaluate(&model, &[1.0, f64::INFINITY], &Tolerances::default()),
        "point: entry 1 is inf",
    );
    let bad = Tolerances {
        feasibility: -1.0,
        integrality: 0.1,
    };
    assert_invalid(evaluate(&model, &[1.0, 0.0], &bad), "feasibility tolerance");
}

#[test]
fn published_duals_satisfy_the_sign_convention_when_maximizing() {
    let r = lp_residuals(&production(), &PRODUCTION_OPTIMUM, &production_duals()).unwrap();
    assert!(r.max() < TOL, "{r:?}");
}

#[test]
fn minimizing_the_negated_objective_negates_the_duals() {
    let mut data = production_data();
    data.sense = Sense::Minimize;
    data.objective.iter_mut().for_each(|c| *c = -*c);
    let model = Model::new(data).unwrap();
    let published = production_duals();
    let negated = Duals {
        row_duals: published.row_duals.iter().map(|y| -y).collect(),
        reduced_costs: published.reduced_costs.iter().map(|z| -z).collect(),
    };
    let r = lp_residuals(&model, &PRODUCTION_OPTIMUM, &negated).unwrap();
    assert!(r.max() < TOL, "{r:?}");
    // The maximization signs do not fit the minimization.
    let r = lp_residuals(&model, &PRODUCTION_OPTIMUM, &published).unwrap();
    assert!(r.stationarity > 1.0, "{r:?}");
}

#[test]
fn complementarity_detects_a_feasible_but_not_optimal_point() {
    // x = 0 is feasible and the duals are stationary, but labor (dual 11/3)
    // and finishing (dual 25/3) are not at their capacities.
    let r = lp_residuals(&production(), &[0.0, 0.0, 0.0], &production_duals()).unwrap();
    assert!(r.primal == 0.0 && r.stationarity < TOL && r.dual_sign == 0.0);
    assert!(
        (r.complementarity - 450.0 * 11.0 / 3.0).abs() < 1e-9,
        "{r:?}"
    );
    // P = 0, D = 450 * 11/3 + 60 * 25/3 = 2150: the scale-free gap is large too.
    assert!((r.duality_gap - 2150.0 / 2151.0).abs() < 1e-12, "{r:?}");
}

/// minimize or maximize x subject to x >= 1 (as a row), x free.
fn one_sided(sense: Sense) -> Model {
    Model::new(ModelData {
        sense,
        objective: vec![1.0],
        objective_offset: 0.0,
        variables: Bounds::free(1),
        kinds: vec![VariableKind::Continuous],
        matrix: CsrMatrix::new(1, 1, vec![0, 1], vec![0], vec![1.0]),
        rows: Bounds::new(vec![1.0], vec![f64::INFINITY]),
    })
    .unwrap()
}

#[test]
fn dual_sign_flags_duals_that_select_an_infinite_bound() {
    let duals = Duals {
        row_duals: vec![1.0],
        reduced_costs: vec![0.0],
    };
    // Minimizing, y = 1 > 0 selects the finite lower bound: the optimum.
    let r = lp_residuals(&one_sided(Sense::Minimize), &[1.0], &duals).unwrap();
    assert_eq!(r.max(), 0.0);
    // Maximizing the same unbounded LP, y = 1 would select the missing upper bound.
    let r = lp_residuals(&one_sided(Sense::Maximize), &[1.0], &duals).unwrap();
    assert_eq!(
        (r.stationarity, r.complementarity, r.dual_sign),
        (0.0, 0.0, 1.0)
    );
}

#[test]
fn lp_residuals_reject_malformed_duals() {
    let model = production();
    let mut duals = production_duals();
    duals.row_duals.pop();
    assert_invalid(
        lp_residuals(&model, &PRODUCTION_OPTIMUM, &duals),
        "row duals: expected 3",
    );
    let mut duals = production_duals();
    duals.reduced_costs[2] = f64::NAN;
    assert_invalid(
        lp_residuals(&model, &PRODUCTION_OPTIMUM, &duals),
        "reduced costs: entry 2 is NaN",
    );
}

#[test]
fn gap_definition_follows_the_objective_sense() {
    assert_eq!(
        Gap::new(Sense::Minimize, 10.0, 8.0),
        Gap {
            absolute: 2.0,
            relative: 0.2
        }
    );
    assert_eq!(
        Gap::new(Sense::Maximize, 10.0, 12.0),
        Gap {
            absolute: 2.0,
            relative: 0.2
        }
    );
    assert_eq!(
        Gap::new(Sense::Minimize, -10.0, -12.0),
        Gap {
            absolute: 2.0,
            relative: 0.2
        }
    );
}

#[test]
fn gap_at_zero_objective_and_for_invalid_bounds() {
    assert_eq!(Gap::new(Sense::Minimize, 0.0, 0.0).relative, 0.0);
    assert_eq!(Gap::new(Sense::Minimize, 0.0, -1.0).relative, f64::INFINITY);
    // A "lower bound" above the incumbent is invalid; it is reported, not clamped.
    let gap = Gap::new(Sense::Minimize, 10.0, 11.0);
    assert_eq!((gap.absolute, gap.relative), (-1.0, -0.1));
    assert_eq!(
        Gap::new(Sense::Maximize, 0.0, -1.0).relative,
        f64::NEG_INFINITY
    );
}

#[test]
fn result_gap_needs_a_bound_and_a_feasible_candidate() {
    let evaluation = |feasible_candidate| Evaluation {
        objective: 10.0,
        row_activity: vec![],
        primal_violation: 0.0,
        integrality_violation: 0.0,
        feasible_candidate,
    };
    let mut result = SolveResult {
        backend: Default::default(),
        status: Status::Stub,
        candidate: Some(vec![0.0]),
        evaluation: Some(evaluation(true)),
        duals: None,
        best_bound: None,
        nodes: 0,
        iterations: 0,
        message: String::new(),
    };
    assert!(result.gap(Sense::Minimize).unwrap().is_none());
    result.best_bound = Some(8.0);
    assert_eq!(result.gap(Sense::Minimize).unwrap().unwrap().absolute, 2.0);
    result.evaluation = Some(evaluation(false));
    assert!(result.gap(Sense::Minimize).unwrap().is_none());
}

#[test]
fn duality_gap_is_scale_free_where_complementarity_is_not() {
    // Scaling objective and duals by 1e6 multiplies complementarity by 1e6
    // but leaves the relative primal-dual gap unchanged.
    let mut data = production_data();
    data.objective.iter_mut().for_each(|c| *c *= 1e6);
    let scaled = Model::new(data).unwrap();
    let published = production_duals();
    let duals = Duals {
        row_duals: published.row_duals.iter().map(|y| y * 1e6).collect(),
        reduced_costs: published.reduced_costs.iter().map(|z| z * 1e6).collect(),
    };
    let near_optimum = [30.0, 10.0 - 1e-9, 0.0];
    let base = lp_residuals(&production(), &near_optimum, &production_duals()).unwrap();
    let big = lp_residuals(&scaled, &near_optimum, &duals).unwrap();
    assert!(
        big.complementarity > 1e5 * base.complementarity,
        "{base:?} {big:?}"
    );
    assert!(
        (big.duality_gap - base.duality_gap).abs() < 1e-14,
        "{base:?} {big:?}"
    );
    assert!(base.duality_gap < 1e-10, "{base:?}");
}
