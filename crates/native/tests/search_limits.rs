use lp_ip_craft::*;
use lp_ip_craft_native::*;
#[allow(dead_code)]
#[path = "../../../benchmarks/shared/fixtures.rs"]
mod fixtures;
fn run(model: &Model, strong: bool, nodes: Option<u64>, iterations: Option<u64>) -> SolveResult {
    NativeSolver::new(NativeOptions {
        strong_branching: strong,
        node_limit: nodes,
        iteration_limit: iterations,
        component_threads: 1,
        decompose: false,
        ..Default::default()
    })
    .solve(
        model,
        &Options::default(),
        None,
        &CancellationToken::default(),
    )
    .unwrap()
}
fn probes(r: &SolveResult) -> u64 {
    r.backend
        .effective_options
        .iter()
        .find(|(k, _)| k == "diagnostics.strong_branch_probes")
        .unwrap()
        .1
        .parse()
        .unwrap()
}
#[test]
fn root_probes_preserve_optimum_and_obey_shared_limits() {
    let model = fixtures::legacy("nodecopy");
    // Exhaustively enumerate all original binary assignments independently.
    let mut best = f64::NEG_INFINITY;
    for mask in 0..1usize << model.n() {
        let x: Vec<f64> = (0..model.n()).map(|j| ((mask >> j) & 1) as f64).collect();
        let objective: f64 = model.objective().iter().zip(&x).map(|(a, b)| a * b).sum();
        if objective > best
            && check::evaluate(&model, &x, &Tolerances::default())
                .unwrap()
                .feasible_candidate
        {
            best = objective;
        }
    }
    for strong in [false, true] {
        let r = run(&model, strong, None, None);
        assert_eq!(r.status, Status::Optimal);
        assert_eq!(r.evaluation.as_ref().unwrap().objective, best);
        if let Some(b) = r.best_bound {
            assert!(b >= best - 1e-7);
        }
    }
    let r = run(&model, true, Some(1), None);
    assert!(r.nodes <= 1);
    assert_eq!(probes(&r), 0);
    let r = run(&model, true, Some(3), None);
    assert!(r.nodes <= 3);
    if let Some(b) = r.best_bound {
        assert!(b >= best - 1e-7);
    }
    assert!(probes(&r) <= 2);
    let r = run(&model, true, None, Some(45));
    assert!(r.iterations <= 45);
    let r = run(&model, true, None, None);
    assert!(probes(&r) > 0);
}

#[test]
fn conservative_fixings_reduce_search_without_changing_the_optimum() {
    let model = fixtures::legacy("nodecopy");
    let mut results = Vec::new();
    for fixing in [false, true] {
        let r = NativeSolver::new(NativeOptions {
            reduced_cost_fixing: fixing,
            component_threads: 1,
            decompose: false,
            ..Default::default()
        })
        .solve(
            &model,
            &Options::default(),
            None,
            &CancellationToken::default(),
        )
        .unwrap();
        assert_eq!(r.status, Status::Optimal);
        results.push(r);
    }
    assert_eq!(
        results[0].evaluation.as_ref().unwrap().objective,
        results[1].evaluation.as_ref().unwrap().objective
    );
    assert!(results[1].nodes < results[0].nodes);
    let fixed: u64 = results[1]
        .backend
        .effective_options
        .iter()
        .find(|(k, _)| k == "diagnostics.reduced_cost_fixings")
        .unwrap()
        .1
        .parse()
        .unwrap();
    assert!(fixed > 0);
}

#[test]
fn legacy_root_probes_need_two_candidates_reliability_probes_one() {
    // 12-item knapsack with strictly decreasing profit/weight ratios: the root
    // LP takes items 0..3 and 1/3 of item 3, so exactly one variable is
    // fractional. The cardinality row only makes the model eligible.
    let n = 12;
    let weights: Vec<f64> = (0..n).map(|j| 3. + j as f64).collect();
    let mut dense = weights.clone();
    dense.extend(std::iter::repeat_n(1., n));
    let model = Model::new(ModelData {
        sense: Sense::Maximize,
        objective: (0..n).map(|j| 30. - j as f64).collect(),
        objective_offset: 0.,
        variables: Bounds::new(vec![0.; n], vec![1.; n]),
        kinds: vec![VariableKind::Binary; n],
        matrix: CsrMatrix::from_dense(2, n, &dense).unwrap(),
        rows: Bounds::new(vec![f64::NEG_INFINITY; 2], vec![14., 7.]),
    })
    .unwrap();
    let solve = |strong: bool, reliability: bool| {
        NativeSolver::new(NativeOptions {
            strong_branching: strong,
            reliability_branching: reliability,
            component_threads: 1,
            decompose: false,
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
    let plain = solve(false, false);
    let legacy = solve(true, false);
    let reliability = solve(false, true);
    for r in [&plain, &legacy, &reliability] {
        assert_eq!(r.status, Status::Optimal);
        assert_eq!(
            r.evaluation.as_ref().unwrap().objective,
            plain.evaluation.as_ref().unwrap().objective
        );
    }
    assert_eq!(probes(&legacy), 0);
    assert_eq!(
        (legacy.nodes, legacy.iterations),
        (plain.nodes, plain.iterations)
    );
    assert!(probes(&reliability) > 0);
}
