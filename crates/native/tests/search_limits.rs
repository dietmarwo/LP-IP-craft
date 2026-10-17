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
        assert_eq!(r.evaluation.unwrap().objective, best);
    }
    let r = run(&model, true, Some(1), None);
    assert!(r.nodes <= 1);
    assert_eq!(probes(&r), 0);
    let r = run(&model, true, Some(3), None);
    assert!(r.nodes <= 3);
    assert!(probes(&r) <= 2);
    let r = run(&model, true, None, Some(45));
    assert!(r.iterations <= 45);
    let r = run(&model, true, None, None);
    assert!(probes(&r) > 0);
}
