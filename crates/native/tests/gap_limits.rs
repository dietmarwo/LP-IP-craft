use lp_ip_craft::*;
use lp_ip_craft_native::*;
#[allow(dead_code)]
#[path = "../examples/family_benchmark.rs"]
mod fixtures;

fn solve(m: &Model, settings: NativeOptions) -> SolveResult {
    NativeSolver::new(settings)
        .solve(m, &Options::default(), None, &CancellationToken::default())
        .unwrap()
}
#[test]
fn gap_stops_early_with_a_valid_feasible_incumbent() {
    let mut found = false;
    for n in [16, 32, 48] {
        let (m, _) = fixtures::selection(n);
        let full = solve(&m, NativeOptions::default());
        let early = solve(
            &m,
            NativeOptions {
                mip_relative_gap: Some(0.05),
                ..Default::default()
            },
        );
        assert!(early.evaluation.as_ref().unwrap().feasible_candidate);
        let g = early.gap(m.sense()).unwrap().unwrap();
        assert!(g.absolute >= 0.0);
        if early.status == Status::GapLimit {
            assert!(g.relative <= 0.05);
            assert!(early.nodes < full.nodes);
            found = true;
        } else {
            assert_eq!(early.status, Status::Optimal);
        }
    }
    assert!(found, "fixtures must exercise an open nonzero gap");
}
#[test]
fn zero_limits_preserve_the_search_and_lps_ignore_them() {
    let (m, _) = fixtures::selection(16);
    let a = solve(&m, NativeOptions::default());
    let b = solve(
        &m,
        NativeOptions {
            mip_relative_gap: Some(0.0),
            mip_absolute_gap: Some(0.0),
            ..Default::default()
        },
    );
    assert_eq!(a.status, b.status);
    assert_eq!(a.nodes, b.nodes);
    assert_eq!(a.iterations, b.iterations);
    assert_eq!(a.candidate, b.candidate);
    assert_eq!(a.best_bound, b.best_bound);
    let (lp, _) = fixtures::packing(16);
    let a = solve(&lp, NativeOptions::default());
    let b = solve(
        &lp,
        NativeOptions {
            mip_relative_gap: Some(1.0),
            ..Default::default()
        },
    );
    assert_eq!(a.status, b.status);
    assert_eq!(a.candidate, b.candidate);
    assert_eq!(a.iterations, b.iterations);
}
#[test]
fn invalid_controls_and_unimplemented_paths_are_rejected() {
    for v in [f64::NAN, f64::INFINITY, -1.0] {
        for relative in [true, false] {
            let mut s = NativeOptions::default();
            if relative {
                s.mip_relative_gap = Some(v)
            } else {
                s.mip_absolute_gap = Some(v)
            };
            assert!(matches!(s.validate(), Err(Error::InvalidInput(_))));
        }
    }
    let (m, _) = fixtures::selection(16);
    for path in ["tree", "packing", "neighborhoods", "cuts"] {
        let mut s = NativeOptions {
            mip_relative_gap: Some(0.01),
            ..Default::default()
        };
        match path {
            "tree" => s.tree_threads = 2,
            "packing" => s.packing_root = true,
            "neighborhoods" => s.packing_neighborhoods = 1,
            _ => s.packing_cuts = true,
        };
        assert!(matches!(
            NativeSolver::new(s).solve(
                &m,
                &Options::default(),
                None,
                &CancellationToken::default()
            ),
            Err(Error::Unsupported(_))
        ));
    }
}
