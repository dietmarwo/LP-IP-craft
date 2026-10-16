//! Acceptance fixtures for the HiGHS adapter: analytic optima, exhaustive
//! integer oracles, status mapping, limits, starts, cancellation and backend
//! limits. Thresholds are fixture requirements, not application targets.

use lp_ip_craft::check::lp_residuals;
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, Error, LpIpSolver, Model, ModelData, Options, Result,
    Sense, SolveResult, StartStatus, Status, Tolerances, VariableKind, WarmStart,
};
use lp_ip_craft_highs::{HighsOptions, HighsSolver};

const INF: f64 = f64::INFINITY;

fn close(value: f64, reference: f64) -> bool {
    (value - reference).abs() <= 1e-8 * reference.abs().max(1.0)
}

#[allow(clippy::too_many_arguments)]
fn model(
    sense: Sense,
    objective: Vec<f64>,
    offset: f64,
    variables: Bounds,
    kinds: Vec<VariableKind>,
    dense_rows: &[f64],
    rows: Bounds,
) -> Model {
    Model::new(ModelData {
        matrix: CsrMatrix::from_dense(rows.len(), objective.len(), dense_rows).unwrap(),
        sense,
        objective,
        objective_offset: offset,
        variables,
        kinds,
        rows,
    })
    .unwrap()
}

fn run(
    settings: HighsOptions,
    model: &Model,
    start: Option<Vec<f64>>,
    cancel: &CancellationToken,
) -> Result<SolveResult> {
    HighsSolver::new(settings).solve(
        model,
        &Options::default(),
        start.map(WarmStart::from_primal).as_ref(),
        cancel,
    )
}

fn solve(model: &Model) -> SolveResult {
    run(
        HighsOptions::default(),
        model,
        None,
        &CancellationToken::default(),
    )
    .unwrap()
}

fn objective(result: &SolveResult) -> f64 {
    result.evaluation.as_ref().expect("a candidate").objective
}

/// Deterministic xorshift numbers in [0, 1).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % 1_000_000) as f64 / 1_000_000.0
    }
}

/// maximize p^T x subject to m knapsack rows at 40 % of their row sums.
fn knapsack(n: usize, m: usize, seed: u64, kind: VariableKind, upper: f64) -> Model {
    let mut rng = Rng(seed);
    let a: Vec<f64> = (0..n * m)
        .map(|_| (rng.next() * 50.0).floor() + 1.0)
        .collect();
    let capacity = (0..m)
        .map(|i| a[i * n..(i + 1) * n].iter().sum::<f64>() * 0.4)
        .collect();
    model(
        Sense::Maximize,
        (0..n).map(|_| (rng.next() * 100.0).floor() + 1.0).collect(),
        0.0,
        Bounds::new(vec![0.0; n], vec![upper; n]),
        vec![kind; n],
        &a,
        Bounds::new(vec![-INF; m], capacity),
    )
}

/// 80 x 40 sparse LP with coefficients up to 140, bounds up to 2e4 and row
/// ranges +-1e5: magnitudes at which absolute complementarity products exceed
/// 1e-7 at correct optima.
fn random_lp(seed: u64) -> Model {
    let mut rng = Rng(seed * 7919);
    let (n, m) = (80, 40);
    let a: Vec<f64> = (0..n * m)
        .map(|_| {
            if rng.next() < 0.3 {
                (rng.next() - 0.3) * 200.0
            } else {
                0.0
            }
        })
        .collect();
    let sense = if seed % 2 == 0 {
        Sense::Minimize
    } else {
        Sense::Maximize
    };
    let objective = (0..n).map(|_| (rng.next() - 0.5) * 2000.0).collect();
    let upper = (0..n).map(|_| 1e4 * (1.0 + rng.next())).collect();
    model(
        sense,
        objective,
        0.0,
        Bounds::new(vec![0.0; n], upper),
        vec![VariableKind::Continuous; n],
        &a,
        Bounds::new(vec![-1e5; m], vec![1e5; m]),
    )
}

// --- analytic fixtures --------------------------------------------------------

#[test]
fn lp_optimum_with_offsets_and_checked_duals() {
    // maximize 3x + 2y + offset, x + y <= 4, 0 <= x <= 2, 0 <= y <= 3: (2, 2), 10
    for offset in [-7.0, 0.0, 7.0] {
        let m = model(
            Sense::Maximize,
            vec![3.0, 2.0],
            offset,
            Bounds::new(vec![0.0, 0.0], vec![2.0, 3.0]),
            vec![VariableKind::Continuous; 2],
            &[1.0, 1.0],
            Bounds::new(vec![-INF], vec![4.0]),
        );
        let r = solve(&m);
        assert_eq!(r.status, Status::Optimal, "{}", r.message);
        let x = r.candidate.as_ref().unwrap();
        assert!(close(x[0], 2.0) && close(x[1], 2.0), "{x:?}");
        assert!(close(objective(&r), 10.0 + offset));
        let residuals = lp_residuals(&m, x, r.duals.as_ref().unwrap()).unwrap();
        assert!(residuals.max() < 1e-9, "{residuals:?}");
        assert!(r.best_bound.is_none() && r.gap(m.sense()).unwrap().is_none());
        assert_eq!(r.nodes, 0);
        let report = &r.backend;
        assert_eq!(report.name, "highs");
        assert_eq!(report.version.as_deref(), Some("1.15.0"));
        assert_eq!(report.raw_status.as_deref(), Some("Optimal"));
        assert_eq!(report.start_status, StartStatus::NotProvided);
        assert!(!report.time_budget_exceeded);
        let options: Vec<&str> = report
            .effective_options
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert!(options.contains(&"threads") && !options.contains(&"mip_rel_gap"));
    }
}

#[test]
fn lp_duals_match_the_published_tutorial_values() {
    // Example 1 of the LP/IP tutorial; Gurobi and HiGHS report these duals.
    let m = model(
        Sense::Maximize,
        vec![45.0, 80.0, 110.0],
        0.0,
        Bounds::non_negative(3),
        vec![VariableKind::Continuous; 3],
        &[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0],
        Bounds::new(vec![-INF; 3], vec![400.0, 450.0, 60.0]),
    );
    let r = solve(&m);
    assert_eq!(r.status, Status::Optimal);
    assert!(close(objective(&r), 2150.0));
    let duals = r.duals.unwrap();
    for (got, expected) in duals.row_duals.iter().zip([0.0, 11.0 / 3.0, 25.0 / 3.0]) {
        assert!(close(*got, expected), "{duals:?}");
    }
    for (got, expected) in duals.reduced_costs.iter().zip([0.0, 0.0, -5.0]) {
        assert!(close(*got, expected), "{duals:?}");
    }
}

#[test]
fn integer_optimum_is_not_the_lp_relaxation() {
    // 2x + y <= 4, x + 2y <= 4, x, y >= 0 integer: IP optimum 6 at (2, 0);
    // the LP relaxation reaches 20/3 at (4/3, 4/3).
    for sense in [Sense::Maximize, Sense::Minimize] {
        let sign = if sense == Sense::Maximize { 1.0 } else { -1.0 };
        for offset in [-7.0, 0.0, 7.0] {
            let build = |kind| {
                model(
                    sense,
                    vec![3.0 * sign, 2.0 * sign],
                    offset,
                    Bounds::non_negative(2),
                    vec![kind; 2],
                    &[2.0, 1.0, 1.0, 2.0],
                    Bounds::new(vec![-INF; 2], vec![4.0; 2]),
                )
            };
            let relaxation = solve(&build(VariableKind::Continuous));
            assert!(close(objective(&relaxation), 20.0 / 3.0 * sign + offset));
            let ip = build(VariableKind::Integer);
            let r = solve(&ip);
            assert_eq!(r.status, Status::Optimal, "{}", r.message);
            assert!(close(objective(&r), 6.0 * sign + offset));
            assert_eq!(r.evaluation.as_ref().unwrap().integrality_violation, 0.0);
            assert!(close(r.best_bound.unwrap(), 6.0 * sign + offset));
            assert!(r.duals.is_none(), "no duals are fabricated for a MIP");
            let gap = r.gap(sense).unwrap().unwrap();
            assert!(gap.absolute.abs() < 1e-8, "{gap:?}");
        }
    }
}

#[test]
fn mixed_binary_continuous_optimum() {
    // minimize 2x + 3y, x + 2y >= 2.5, 0 <= x <= 3, y binary: (0.5, 1), 4
    let m = model(
        Sense::Minimize,
        vec![2.0, 3.0],
        0.0,
        Bounds::new(vec![0.0, 0.0], vec![3.0, 1.0]),
        vec![VariableKind::Continuous, VariableKind::Binary],
        &[1.0, 2.0],
        Bounds::new(vec![2.5], vec![INF]),
    );
    let r = solve(&m);
    assert_eq!(r.status, Status::Optimal);
    assert!(close(objective(&r), 4.0));
    let x = r.candidate.unwrap();
    assert!(close(x[0], 0.5) && close(x[1], 1.0), "{x:?}");
}

#[test]
fn infeasible_models_have_no_candidate_or_bound() {
    // x >= 1 and x <= 0 as separate rows.
    let lp = model(
        Sense::Minimize,
        vec![1.0],
        0.0,
        Bounds::free(1),
        vec![VariableKind::Continuous],
        &[1.0, 1.0],
        Bounds::new(vec![1.0, -INF], vec![INF, 0.0]),
    );
    let r = solve(&lp);
    assert_eq!(r.status, Status::Infeasible);
    assert!(r.candidate.is_none() && r.best_bound.is_none());
    // A binary forced into [0.25, 0.75]: the LP relaxation is feasible.
    let mip = model(
        Sense::Minimize,
        vec![1.0],
        0.0,
        Bounds::new(vec![0.0], vec![1.0]),
        vec![VariableKind::Binary],
        &[1.0],
        Bounds::new(vec![0.25], vec![0.75]),
    );
    let r = solve(&mip);
    assert_eq!(r.status, Status::Infeasible);
    assert!(r.candidate.is_none() && r.best_bound.is_none());
}

#[test]
fn unbounded_lp_is_never_an_optimal_zero() {
    let m = model(
        Sense::Minimize,
        vec![-1.0],
        0.0,
        Bounds::non_negative(1),
        vec![VariableKind::Continuous],
        &[],
        Bounds::free(0),
    );
    let r = solve(&m);
    assert!(
        matches!(r.status, Status::Unbounded | Status::InfeasibleOrUnbounded),
        "{:?}",
        r.status
    );
    if let Some(evaluation) = &r.evaluation {
        assert!(
            evaluation.feasible_candidate,
            "a returned point is feasible"
        );
    }
}

#[test]
fn fixed_free_empty_duplicate_rows_and_explicit_zeros() {
    // minimize x0 + x1 + 3, x0 fixed at 0.25, x1 free, x2 in [0, 1];
    // rows: x0 + x1 >= 1 twice, an empty row in [-1, 1], 0 * x2 in [-1, 1].
    let m = Model::new(ModelData {
        sense: Sense::Minimize,
        objective: vec![1.0, 1.0, 0.0],
        objective_offset: 3.0,
        variables: Bounds::new(vec![0.25, -INF, 0.0], vec![0.25, INF, 1.0]),
        kinds: vec![VariableKind::Continuous; 3],
        matrix: CsrMatrix::new(
            4,
            3,
            vec![0, 2, 4, 4, 5],
            vec![0, 1, 0, 1, 2],
            vec![1.0, 1.0, 1.0, 1.0, 0.0],
        ),
        rows: Bounds::new(vec![1.0, 1.0, -1.0, -1.0], vec![INF, INF, 1.0, 1.0]),
    })
    .unwrap();
    let r = solve(&m);
    assert_eq!(r.status, Status::Optimal, "{}", r.message);
    assert!(close(objective(&r), 4.0));
    let x = r.candidate.as_ref().unwrap();
    assert!(close(x[0], 0.25) && close(x[1], 0.75), "{x:?}");
    let residuals = lp_residuals(&m, x, r.duals.as_ref().unwrap()).unwrap();
    assert!(residuals.max() < 1e-9, "{residuals:?}");
}

// --- exhaustive oracles -------------------------------------------------------

#[test]
fn binary_knapsacks_agree_with_enumeration() {
    for seed in 1..12 {
        let costs: Vec<f64> = (0..7)
            .map(|i| ((i * 13 + seed * 7) % 19 + 1) as f64)
            .collect();
        let weights: Vec<f64> = (0..7)
            .map(|i| ((i * 11 + seed * 3) % 13 + 1) as f64)
            .collect();
        let best = (0..128_u32)
            .filter(|mask| {
                (0..7)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| weights[i])
                    .sum::<f64>()
                    <= 20.0
            })
            .map(|mask| {
                (0..7)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| costs[i])
                    .sum::<f64>()
            })
            .fold(f64::NEG_INFINITY, f64::max);
        let m = model(
            Sense::Maximize,
            costs,
            0.0,
            Bounds::new(vec![0.0; 7], vec![1.0; 7]),
            vec![VariableKind::Binary; 7],
            &weights,
            Bounds::new(vec![-INF], vec![20.0]),
        );
        let r = solve(&m);
        assert_eq!(r.status, Status::Optimal);
        assert!(close(objective(&r), best));
        // Maximizing, a valid bound is an upper bound.
        assert!(r.best_bound.unwrap() >= best - 1e-8);
    }
}

#[test]
fn bounded_integer_minimization_agrees_with_enumeration() {
    // minimize c^T x, x in {-2, ..., 3}^3, two coupling rows.
    for seed in 1..8_u64 {
        let mut rng = Rng(seed * 104_729);
        let c: Vec<f64> = (0..3).map(|_| (rng.next() * 10.0).floor() - 5.0).collect();
        let a: Vec<f64> = (0..6).map(|_| (rng.next() * 7.0).floor() - 3.0).collect();
        let (lo, hi) = (-4.0, 5.0);
        let mut best = INF;
        for x0 in -2..=3 {
            for x1 in -2..=3 {
                for x2 in -2..=3 {
                    let x = [x0 as f64, x1 as f64, x2 as f64];
                    let rows = [0, 1].map(|i| (0..3).map(|j| a[i * 3 + j] * x[j]).sum::<f64>());
                    if rows.iter().all(|&r| (lo..=hi).contains(&r)) {
                        best = best.min((0..3).map(|j| c[j] * x[j]).sum());
                    }
                }
            }
        }
        let m = model(
            Sense::Minimize,
            c,
            0.0,
            Bounds::new(vec![-2.0; 3], vec![3.0; 3]),
            vec![VariableKind::Integer; 3],
            &a,
            Bounds::new(vec![lo; 2], vec![hi; 2]),
        );
        let r = solve(&m);
        if best.is_finite() {
            assert_eq!(r.status, Status::Optimal, "seed {seed}");
            assert!(close(objective(&r), best), "seed {seed}");
            // Minimizing, a valid bound is a lower bound.
            assert!(r.best_bound.unwrap() <= best + 1e-8);
        } else {
            assert_eq!(r.status, Status::Infeasible, "seed {seed}");
        }
    }
}

// --- acceptance checks ----------------------------------------------------------

#[test]
fn dual_checks_accept_correct_optima_at_larger_magnitudes() {
    // Before the scale-free duality gap, seed 13 was reported as a numerical
    // failure: complementarity 1.5e-7 against an absolute limit of 1e-7.
    let mut optimal = 0;
    for seed in 1..=40 {
        let m = random_lp(seed);
        let r = solve(&m);
        let evaluation = r.evaluation.as_ref().unwrap();
        let residuals =
            lp_residuals(&m, r.candidate.as_ref().unwrap(), r.duals.as_ref().unwrap()).unwrap();
        assert!(residuals.duality_gap < 1e-9, "seed {seed}: {residuals:?}");
        match r.status {
            Status::Optimal => optimal += 1,
            // Rows near 1e5 can miss an absolute 1e-7 by roundoff; that is a
            // primal finding, never a dual one.
            Status::NumericalFailure => {
                assert!(!evaluation.feasible_candidate, "seed {seed}: {}", r.message)
            }
            other => panic!("seed {seed}: {other:?}"),
        }
        if seed == 13 {
            assert_eq!(r.status, Status::Optimal, "{}", r.message);
        }
    }
    assert!(optimal >= 38, "{optimal}");
}

// --- starts, limits and cancellation ----------------------------------------------

#[test]
fn starts_are_submitted_never_rounded_and_validated() {
    let m = knapsack(7, 1, 5, VariableKind::Binary, 1.0);
    let token = CancellationToken::default();
    let fractional = run(HighsOptions::default(), &m, Some(vec![0.5; 7]), &token).unwrap();
    assert_eq!(fractional.status, Status::Optimal);
    assert!(matches!(
        fractional.backend.start_status,
        StartStatus::Submitted | StartStatus::Rejected
    ));
    assert_eq!(
        fractional.evaluation.unwrap().integrality_violation,
        0.0,
        "the incumbent is integral; the fractional start was not adopted"
    );
    let bad = |start: Vec<f64>| run(HighsOptions::default(), &m, Some(start), &token);
    assert!(matches!(bad(vec![0.0; 6]), Err(Error::InvalidInput(e)) if e.contains("expected 7")));
    assert!(matches!(bad(vec![f64::NAN; 7]), Err(Error::InvalidInput(e)) if e.contains("entry 0")));
    assert!(matches!(bad(vec![1e20; 7]), Err(Error::Unsupported(e)) if e.contains("start")));
}

#[test]
fn time_budget_of_zero_stops_before_conversion() {
    let m = knapsack(20, 3, 1, VariableKind::Binary, 1.0);
    let settings = HighsOptions {
        time_limit_seconds: Some(0.0),
        ..HighsOptions::default()
    };
    let r = run(settings, &m, None, &CancellationToken::default()).unwrap();
    assert_eq!(r.status, Status::TimeLimit);
    assert!(r.candidate.is_none() && r.backend.time_budget_exceeded);
    assert!(r.backend.raw_status.is_none(), "HiGHS never ran");
}

#[test]
fn node_limit_keeps_a_submitted_incumbent_and_reports_the_reason() {
    let m = knapsack(100, 12, 3, VariableKind::Binary, 1.0);
    let settings = HighsOptions {
        node_limit: Some(0),
        ..HighsOptions::default()
    };
    for start in [None, Some(vec![0.0; 100])] {
        let had_start = start.is_some();
        let r = run(settings.clone(), &m, start, &CancellationToken::default()).unwrap();
        assert_eq!(r.status, Status::NodeLimit, "{}", r.message);
        assert_eq!(
            r.backend.raw_status.as_deref(),
            Some("ReachedSolutionLimit")
        );
        assert_eq!(r.nodes, 0);
        assert_eq!(r.candidate.is_some(), had_start);
        if let Some(evaluation) = &r.evaluation {
            assert!(evaluation.feasible_candidate && evaluation.objective == 0.0);
        }
        assert!(
            r.backend
                .effective_options
                .contains(&("mip_max_nodes".into(), "0".into()))
        );
    }
}

#[test]
fn iteration_limit_stops_an_lp() {
    let settings = HighsOptions {
        iteration_limit: Some(0),
        ..HighsOptions::default()
    };
    let r = run(settings, &random_lp(3), None, &CancellationToken::default()).unwrap();
    assert_eq!(r.status, Status::IterationLimit, "{}", r.message);
}

#[test]
fn limits_highs_would_ignore_are_rejected() {
    let token = CancellationToken::default();
    let lp = random_lp(1);
    let mip = knapsack(10, 2, 1, VariableKind::Binary, 1.0);
    let nodes = HighsOptions {
        node_limit: Some(10),
        ..HighsOptions::default()
    };
    assert!(
        matches!(run(nodes, &lp, None, &token), Err(Error::Unsupported(e)) if e.contains("node_limit"))
    );
    let iterations = HighsOptions {
        iteration_limit: Some(10),
        ..HighsOptions::default()
    };
    assert!(
        matches!(run(iterations, &mip, None, &token), Err(Error::Unsupported(e)) if e.contains("iteration_limit"))
    );
}

#[test]
fn cancellation_before_and_during_the_run() {
    let small = knapsack(10, 2, 1, VariableKind::Binary, 1.0);
    let token = CancellationToken::default();
    token.cancel();
    let r = run(HighsOptions::default(), &small, None, &token).unwrap();
    assert_eq!(r.status, Status::Cancelled);
    assert!(r.candidate.is_none() && r.backend.raw_status.is_none());
    // Input errors take precedence over a pending cancellation.
    assert!(matches!(
        run(
            HighsOptions::default(),
            &small,
            Some(vec![f64::NAN; 10]),
            &token
        ),
        Err(Error::InvalidInput(_))
    ));

    // A request from another thread reaches HiGHS through the interrupt callback.
    let hard = knapsack(400, 30, 11, VariableKind::Binary, 1.0);
    let token = CancellationToken::default();
    let settings = HighsOptions {
        time_limit_seconds: Some(20.0),
        ..HighsOptions::default()
    };
    let r = std::thread::scope(|s| {
        let canceller = token.clone();
        s.spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            canceller.cancel();
        });
        run(settings, &hard, None, &token).unwrap()
    });
    // Finishing before the request is legitimate; a stop must say why.
    match r.status {
        Status::Cancelled => assert_eq!(r.backend.raw_status.as_deref(), Some("ReachedInterrupt")),
        Status::Optimal => {}
        other => panic!("{other:?}: {}", r.message),
    }
    if let Some(evaluation) = &r.evaluation {
        assert!(
            evaluation.feasible_candidate,
            "a kept incumbent is feasible"
        );
    }
}

// --- backend limits and reporting ---------------------------------------------------

#[test]
fn values_highs_cannot_represent_are_unsupported() {
    let token = CancellationToken::default();
    let base = || ModelData {
        sense: Sense::Minimize,
        objective: vec![1.0, 1.0],
        objective_offset: 0.0,
        variables: Bounds::new(vec![0.0; 2], vec![10.0; 2]),
        kinds: vec![VariableKind::Integer; 2],
        matrix: CsrMatrix::from_dense(1, 2, &[1.0, 1.0]).unwrap(),
        rows: Bounds::new(vec![1.0], vec![INF]),
    };
    type Patch = fn(&mut ModelData);
    let cases: [(Patch, &str); 6] = [
        (|d| d.matrix.values[0] = 1e-13, "matrix value 0"),
        (|d| d.matrix.values[1] = 1e15, "matrix value 1"),
        (|d| d.objective[0] = 1e20, "objective coefficient 0"),
        (|d| d.objective_offset = -1e20, "objective offset"),
        (|d| d.rows.upper[0] = 1e20, "row 0 has finite bound"),
        (|d| d.variables.upper[1] = 1e16, "integer variable 1"),
    ];
    for (patch, needle) in cases {
        let mut data = base();
        patch(&mut data);
        let m = Model::new(data).unwrap();
        match run(HighsOptions::default(), &m, None, &token) {
            Err(Error::Unsupported(e)) => assert!(e.contains(needle), "{e}"),
            other => panic!("{needle}: {other:?}"),
        }
    }
    let m = Model::new(base()).unwrap();
    let options = Options {
        tolerances: Tolerances {
            feasibility: 1e-11,
            integrality: 1e-7,
        },
        threads: 1,
    };
    assert!(matches!(
        HighsSolver::default().solve(&m, &options, None, &token),
        Err(Error::Unsupported(e)) if e.contains("feasibility tolerance")
    ));
}

#[test]
fn malformed_settings_name_the_field() {
    let m = knapsack(5, 1, 1, VariableKind::Binary, 1.0);
    let cases = [
        (
            HighsOptions {
                time_limit_seconds: Some(-1.0),
                ..HighsOptions::default()
            },
            "time_limit_seconds",
        ),
        (
            HighsOptions {
                seed: u32::MAX,
                ..HighsOptions::default()
            },
            "seed",
        ),
        (
            HighsOptions {
                mip_relative_gap: f64::NAN,
                ..HighsOptions::default()
            },
            "mip_relative_gap",
        ),
        (
            HighsOptions {
                dual_tolerance: 1e-11,
                ..HighsOptions::default()
            },
            "dual_tolerance",
        ),
    ];
    for (settings, field) in cases {
        match run(settings, &m, None, &CancellationToken::default()) {
            Err(Error::InvalidInput(e)) => assert!(e.contains(field), "{e}"),
            other => panic!("{field}: {other:?}"),
        }
    }
}

#[test]
fn mip_reports_nodes_and_lp_iterations() {
    let r = solve(&knapsack(60, 8, 7, VariableKind::Binary, 1.0));
    assert_eq!(r.status, Status::Optimal);
    assert!(r.iterations > 0);
    assert!(r.best_bound.is_some() && r.duals.is_none());
}

#[test]
fn independent_solves_run_on_several_threads() {
    let m = knapsack(40, 6, 3, VariableKind::Integer, 3.0);
    let reference = objective(&solve(&m));
    let objectives: Vec<f64> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..6).map(|_| s.spawn(|| objective(&solve(&m)))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(objectives.iter().all(|&z| z == reference), "{objectives:?}");
}
