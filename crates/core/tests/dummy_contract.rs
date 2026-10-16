mod common;

use common::{assert_invalid, assignment, assignment_data};
use lp_ip_craft::{
    Bounds, CancellationToken, DummySolver, Error, LpIpSolver, Model, Options, Result, Sense,
    SolveResult, Status, Tolerances, VariableKind, WarmStart,
};

fn solve(model: &Model, start: Option<Vec<f64>>) -> Result<SolveResult> {
    DummySolver.solve(
        model,
        &Options::default(),
        start.map(WarmStart::from_primal).as_ref(),
        &CancellationToken::default(),
    )
}

#[test]
fn feasible_optimal_start_is_still_only_a_stub() {
    let model = assignment();
    let r = solve(&model, Some(vec![1.0, 0.0])).unwrap();
    assert_eq!(r.status, Status::Stub);
    assert_eq!(r.candidate, Some(vec![1.0, 0.0]));
    let e = r.evaluation.as_ref().unwrap();
    assert_eq!(e.objective, 7.0); // includes the offset
    assert_eq!(e.row_activity, vec![1.0]);
    assert_eq!((e.primal_violation, e.integrality_violation), (0.0, 0.0));
    assert!(e.feasible_candidate);
    assert!(r.duals.is_none() && r.best_bound.is_none());
    assert!(r.gap(model.sense()).unwrap().is_none());
    assert_eq!((r.nodes, r.iterations), (0, 0));
    assert!(r.message.starts_with("DUMMY"));
}

#[test]
fn fractional_start_is_never_rounded() {
    let r = solve(&assignment(), Some(vec![0.5, 0.5])).unwrap();
    assert_eq!(r.candidate, Some(vec![0.5, 0.5]));
    let e = r.evaluation.unwrap();
    assert_eq!((e.primal_violation, e.integrality_violation), (0.0, 0.5));
    assert!(!e.feasible_candidate);

    let mut data = assignment_data();
    data.kinds = vec![VariableKind::Continuous; 2];
    let r = solve(&Model::new(data).unwrap(), Some(vec![0.5, 0.5])).unwrap();
    assert!(r.evaluation.unwrap().feasible_candidate);
}

#[test]
fn start_outside_bounds_is_evaluated_and_reported_not_repaired() {
    let r = solve(&assignment(), Some(vec![2.0, -1.0])).unwrap();
    assert_eq!(r.status, Status::Stub);
    assert_eq!(r.candidate, Some(vec![2.0, -1.0]));
    let e = r.evaluation.unwrap();
    assert_eq!(e.primal_violation, 1.0);
    assert!(!e.feasible_candidate);
}

#[test]
fn without_start_zero_is_clamped_to_the_bounds() {
    let r = solve(&assignment(), None).unwrap();
    assert_eq!(r.candidate, Some(vec![0.0, 0.0]));
    let e = r.evaluation.unwrap();
    assert_eq!(e.primal_violation, 1.0); // the row is violated; no claim of infeasibility
    assert_eq!(r.status, Status::Stub);

    let mut data = assignment_data();
    data.kinds = vec![VariableKind::Continuous; 2];
    data.variables = Bounds::new(vec![2.0, f64::NEG_INFINITY], vec![5.0, -2.5]);
    let r = solve(&Model::new(data).unwrap(), None).unwrap();
    assert_eq!(r.candidate, Some(vec![2.0, -2.5]));
}

#[test]
fn maximize_keeps_original_objective_sign() {
    let mut data = assignment_data();
    data.sense = Sense::Maximize;
    let r = solve(&Model::new(data).unwrap(), Some(vec![0.0, 1.0])).unwrap();
    assert_eq!(r.evaluation.unwrap().objective, 8.0);
}

#[test]
fn pre_cancelled_solve_has_no_candidate() {
    let cancel = CancellationToken::default();
    cancel.cancel();
    let r = DummySolver
        .solve(&assignment(), &Options::default(), None, &cancel)
        .unwrap();
    assert_eq!(r.status, Status::Cancelled);
    assert!(r.candidate.is_none() && r.evaluation.is_none());
    assert!(cancel.is_cancelled());
}

#[test]
fn input_errors_take_precedence_over_cancellation() {
    let cancel = CancellationToken::default();
    cancel.cancel();
    let bad = WarmStart::from_primal(vec![1.0]);
    assert_invalid(
        DummySolver.solve(&assignment(), &Options::default(), Some(&bad), &cancel),
        "start: expected 2 entries, got 1",
    );
}

#[test]
fn malformed_starts_rejected() {
    assert_invalid(solve(&assignment(), Some(vec![1.0])), "start: expected 2");
    assert_invalid(
        solve(&assignment(), Some(vec![f64::NAN, 0.0])),
        "start: entry 0 is NaN",
    );
}

#[test]
fn threads_other_than_one_are_explicit_errors() {
    let run = |threads| {
        let options = Options {
            threads,
            ..Options::default()
        };
        DummySolver.solve(&assignment(), &options, None, &CancellationToken::default())
    };
    assert!(matches!(run(16), Err(Error::Unsupported(m)) if m.contains("threads = 16")));
    assert_invalid(run(0), "threads must be at least 1");
}

#[test]
fn invalid_tolerances_rejected() {
    for (feasibility, integrality, needle) in [
        (0.0, 1e-7, "feasibility tolerance"),
        (f64::NAN, 1e-7, "feasibility tolerance"),
        (f64::INFINITY, 1e-7, "feasibility tolerance"),
        (1e-7, 0.5, "integrality tolerance"),
        (1e-7, 0.0, "integrality tolerance"),
    ] {
        let options = Options {
            tolerances: Tolerances {
                feasibility,
                integrality,
            },
            threads: 1,
        };
        assert_invalid(
            DummySolver.solve(&assignment(), &options, None, &CancellationToken::default()),
            needle,
        );
    }
}

#[test]
fn overflow_is_an_evaluation_error_not_a_candidate() {
    let mut data = assignment_data();
    data.kinds = vec![VariableKind::Continuous; 2];
    data.variables = Bounds::free(2);
    data.objective = vec![f64::MAX, f64::MAX];
    let model = Model::new(data).unwrap();
    assert!(matches!(
        solve(&model, Some(vec![1.0, 1.0])),
        Err(Error::Evaluation(m)) if m.contains("objective overflowed")
    ));
    assert!(matches!(
        solve(&model, Some(vec![f64::MAX, f64::MAX])),
        Err(Error::Evaluation(m)) if m.contains("activity of row 0 overflowed")
    ));
}

#[test]
fn one_model_serves_independent_solves_on_several_threads() {
    let model = assignment();
    let starts = [[1.0, 0.0], [0.0, 1.0], [0.5, 0.5], [1.0, 1.0]];
    let objectives: Vec<f64> = std::thread::scope(|s| {
        let handles: Vec<_> = starts
            .iter()
            .map(|x| {
                let model = &model;
                s.spawn(move || {
                    solve(model, Some(x.to_vec()))
                        .unwrap()
                        .evaluation
                        .unwrap()
                        .objective
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(objectives, vec![7.0, 8.0, 7.5, 10.0]);
}
