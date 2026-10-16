use lp_ip_craft::*;
use lp_ip_craft_highs::*;
fn solve(m: &Model) -> SolveResult {
    HighsSolver::default()
        .solve(m, &Options::default(), None, &CancellationToken::default())
        .unwrap()
}
fn model(
    c: Vec<f64>,
    a: Vec<f64>,
    rows: Bounds,
    vars: Bounds,
    kinds: Vec<VariableKind>,
    sense: Sense,
    offset: f64,
) -> Model {
    Model::new(ModelData {
        matrix: CsrMatrix::from_dense(rows.len(), c.len(), &a).unwrap(),
        objective: c,
        objective_offset: offset,
        variables: vars,
        kinds,
        rows,
        sense,
    })
    .unwrap()
}
#[test]
fn analytic_lp_offsets_and_duals() {
    for offset in [-7.0, 0.0, 7.0] {
        let m = model(
            vec![3.0, 2.0],
            vec![1.0, 1.0],
            Bounds::new(vec![f64::NEG_INFINITY], vec![4.0]),
            Bounds::new(vec![0.0; 2], vec![2.0, 3.0]),
            vec![VariableKind::Continuous; 2],
            Sense::Maximize,
            offset,
        );
        let r = solve(&m);
        assert_eq!(r.status, Status::Optimal);
        assert!((r.evaluation.as_ref().unwrap().objective - (10.0 + offset)).abs() < 1e-8);
        assert!(
            check::lp_residuals(&m, r.candidate.as_ref().unwrap(), r.duals.as_ref().unwrap())
                .unwrap()
                .max()
                < 1e-7
        );
        assert!(r.best_bound.is_none());
        assert_eq!(r.backend.name, "highs");
    }
}
#[test]
fn integer_relaxation_is_not_incumbent_and_offsets_apply_to_bound() {
    for sense in [Sense::Maximize, Sense::Minimize] {
        for offset in [-7.0, 7.0] {
            let sign = if sense == Sense::Maximize { 1.0 } else { -1.0 };
            let m = model(
                vec![3.0 * sign, 2.0 * sign],
                vec![2.0, 1.0, 1.0, 2.0],
                Bounds::new(vec![f64::NEG_INFINITY; 2], vec![4.0; 2]),
                Bounds::non_negative(2),
                vec![VariableKind::Integer; 2],
                sense,
                offset,
            );
            let r = solve(&m);
            assert_eq!(r.status, Status::Optimal);
            let z = r.evaluation.as_ref().unwrap().objective;
            assert!((z - (6.0 * sign + offset)).abs() < 1e-8);
            assert!((r.best_bound.unwrap() - z).abs() < 1e-8);
            assert!(r.duals.is_none());
            assert!(r.gap(sense).unwrap().absolute.abs() < 1e-8);
        }
    }
}
#[test]
fn mixed_binary_continuous() {
    let m = model(
        vec![2.0, 3.0],
        vec![1.0, 2.0],
        Bounds::new(vec![2.5], vec![f64::INFINITY]),
        Bounds::new(vec![0.0; 2], vec![3.0, 1.0]),
        vec![VariableKind::Continuous, VariableKind::Binary],
        Sense::Minimize,
        0.0,
    );
    let r = solve(&m);
    assert_eq!(r.status, Status::Optimal);
    assert!((r.evaluation.unwrap().objective - 4.0).abs() < 1e-8);
}
#[test]
fn infeasible_unbounded_and_fixed_models() {
    let infeasible = model(
        vec![1.0],
        vec![1.0, 1.0],
        Bounds::new(vec![1.0, f64::NEG_INFINITY], vec![f64::INFINITY, 0.0]),
        Bounds::free(1),
        vec![VariableKind::Continuous],
        Sense::Minimize,
        0.0,
    );
    assert_eq!(solve(&infeasible).status, Status::Infeasible);
    let unbounded = model(
        vec![-1.0],
        vec![],
        Bounds::free(0),
        Bounds::non_negative(1),
        vec![VariableKind::Continuous],
        Sense::Minimize,
        0.0,
    );
    assert!(matches!(
        solve(&unbounded).status,
        Status::Unbounded | Status::InfeasibleOrUnbounded
    ));
    let fixed = model(
        vec![2.0],
        vec![],
        Bounds::free(0),
        Bounds::fixed(vec![3.0]),
        vec![VariableKind::Continuous],
        Sense::Minimize,
        1.0,
    );
    assert_eq!(solve(&fixed).evaluation.unwrap().objective, 7.0);
}
#[test]
fn exhaustive_binary_oracle() {
    for seed in 1..12 {
        let costs: Vec<f64> = (0..7)
            .map(|i| ((i * 13 + seed * 7) % 19 + 1) as f64)
            .collect();
        let weights: Vec<f64> = (0..7)
            .map(|i| ((i * 11 + seed * 3) % 13 + 1) as f64)
            .collect();
        let capacity = 20.0;
        let mut best = 0.0_f64;
        for mask in 0..128 {
            let (w, z) = (0..7)
                .filter(|i| mask & (1 << i) != 0)
                .fold((0.0, 0.0), |(w, z), i| (w + weights[i], z + costs[i]));
            if w <= capacity {
                best = best.max(z);
            }
        }
        let m = model(
            costs,
            weights,
            Bounds::new(vec![f64::NEG_INFINITY], vec![capacity]),
            Bounds::new(vec![0.0; 7], vec![1.0; 7]),
            vec![VariableKind::Binary; 7],
            Sense::Maximize,
            0.0,
        );
        let r = solve(&m);
        assert_eq!(r.status, Status::Optimal);
        assert!((r.evaluation.unwrap().objective - best).abs() < 1e-8);
        assert!(r.best_bound.unwrap() >= best - 1e-8);
    }
}
#[test]
fn starts_limits_and_invalid_input_precedence() {
    let m = model(
        vec![1.0],
        vec![],
        Bounds::free(0),
        Bounds::new(vec![0.0], vec![1.0]),
        vec![VariableKind::Binary],
        Sense::Minimize,
        0.0,
    );
    let mut backend = HighsSolver::default();
    let r = backend
        .solve(
            &m,
            &Options::default(),
            Some(&WarmStart::from_primal(vec![0.5])),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(
        r.backend.start_status.as_str(),
        "submitted" | "rejected"
    ));
    assert_eq!(r.status, Status::Optimal);
    backend.settings.time_limit_seconds = Some(0.0);
    let r = backend
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
    assert_eq!(r.status, Status::TimeLimit);
    assert!(r.candidate.is_none());
    let token = CancellationToken::default();
    token.cancel();
    assert!(
        backend
            .solve(
                &m,
                &Options::default(),
                Some(&WarmStart::from_primal(vec![f64::NAN])),
                &token
            )
            .is_err()
    );
    backend.settings = HighsOptions::default();
    assert_eq!(
        backend
            .solve(&m, &Options::default(), None, &token)
            .unwrap()
            .status,
        Status::Cancelled
    );
}
