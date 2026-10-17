use lp_ip_craft::*;
use lp_ip_craft_native::*;
fn model(c: &[f64], a: &[f64], rows: Bounds, bounds: Bounds, sense: Sense, integer: bool) -> Model {
    Model::new(ModelData {
        sense,
        objective: c.to_vec(),
        objective_offset: 0.0,
        variables: bounds,
        kinds: vec![
            if integer {
                VariableKind::Integer
            } else {
                VariableKind::Continuous
            };
            c.len()
        ],
        matrix: CsrMatrix::from_dense(rows.len(), c.len(), a).unwrap(),
        rows,
    })
    .unwrap()
}
/// Solves with the defaults and, as a differential check, with
/// largest-infeasibility pricing and a refactorization after every update.
fn run(m: &Model) -> SolveResult {
    let reference = NativeSolver::default()
        .solve(m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
    let variant = NativeSolver::new(NativeOptions {
        steepest_edge: false,
        refactor_interval: 1,
        ..Default::default()
    })
    .solve(m, &Options::default(), None, &CancellationToken::default())
    .unwrap();
    assert_eq!(
        variant.status, reference.status,
        "variant: {variant:?}; reference: {reference:?}"
    );
    if let (Some(a), Some(b)) = (&variant.evaluation, &reference.evaluation) {
        assert!(
            (a.objective - b.objective).abs() < 1e-6 * (1. + a.objective.abs()),
            "{variant:?}"
        );
    }
    reference
}
#[test]
fn analytic_lp_both_senses_and_duals() {
    for sense in [Sense::Minimize, Sense::Maximize] {
        let c = if sense == Sense::Maximize {
            vec![3., 2.]
        } else {
            vec![-3., -2.]
        };
        let m = model(
            &c,
            &[1., 1., 2., 1.],
            Bounds::new(vec![f64::NEG_INFINITY; 2], vec![4., 5.]),
            Bounds::non_negative(2),
            sense,
            false,
        );
        let r = run(&m);
        assert_eq!(r.status, Status::Optimal, "{r:?}");
        assert!((r.evaluation.unwrap().objective.abs() - 9.).abs() < 1e-7);
        let k = check::lp_residuals(&m, r.candidate.as_ref().unwrap(), r.duals.as_ref().unwrap())
            .unwrap();
        assert!(k.duality_gap < 1e-8 && k.primal < 1e-8);
    }
}
#[test]
fn phase_one_lower_equal_ranged() {
    for (l, u, expect) in [(2., 2., 2.), (2., 3., 2.)] {
        let m = model(
            &[1.],
            &[1.],
            Bounds::new(vec![l], vec![u]),
            Bounds::free(1),
            Sense::Minimize,
            false,
        );
        let r = run(&m);
        assert_eq!(r.status, Status::Optimal, "{r:?}");
        assert!((r.candidate.unwrap()[0] - expect).abs() < 1e-8);
    }
}
#[test]
fn upper_only_and_offset() {
    let mut d = model(
        &[-2.],
        &[],
        Bounds::free(0),
        Bounds::new(vec![f64::NEG_INFINITY], vec![3.]),
        Sense::Minimize,
        false,
    )
    .into_data();
    d.objective_offset = 7.;
    let r = run(&Model::new(d).unwrap());
    assert_eq!(r.status, Status::Optimal);
    assert_eq!(r.evaluation.unwrap().objective, 1.);
}
#[test]
fn infeasible_and_unbounded() {
    let m = model(
        &[1.],
        &[1., 1.],
        Bounds::new(vec![2., f64::NEG_INFINITY], vec![f64::INFINITY, 1.]),
        Bounds::free(1),
        Sense::Minimize,
        false,
    );
    assert_eq!(run(&m).status, Status::Infeasible);
    let m = model(
        &[-1.],
        &[],
        Bounds::free(0),
        Bounds::non_negative(1),
        Sense::Minimize,
        false,
    );
    assert_eq!(run(&m).status, Status::Unbounded);
}
#[test]
fn empty_rows_and_fixed_variables() {
    let m = model(
        &[2., -1.],
        &[0., 0.],
        Bounds::fixed(vec![0.]),
        Bounds::fixed(vec![3., 4.]),
        Sense::Minimize,
        false,
    );
    let r = run(&m);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert_eq!(r.evaluation.unwrap().objective, 2.);
}
#[test]
fn tiny_supported_coefficient() {
    let m = model(
        &[1.],
        &[1e-11],
        Bounds::fixed(vec![2e-11]),
        Bounds::new(vec![0.], vec![10.]),
        Sense::Minimize,
        false,
    );
    let r = run(&m);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert!((r.candidate.unwrap()[0] - 2.).abs() < 1e-7);
}
#[test]
fn limits_and_validation_before_cancel() {
    let m = model(
        &[1.],
        &[1.],
        Bounds::fixed(vec![2.]),
        Bounds::non_negative(1),
        Sense::Minimize,
        false,
    );
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert_eq!(
        NativeSolver::default()
            .solve(&m, &Options::default(), None, &cancel)
            .unwrap()
            .status,
        Status::Cancelled
    );
    assert!(
        NativeSolver::default()
            .solve(
                &m,
                &Options::default(),
                Some(&WarmStart::from_primal(vec![])),
                &cancel
            )
            .is_err()
    );
    for s in [
        NativeOptions {
            time_limit_seconds: Some(0.),
            ..Default::default()
        },
        NativeOptions {
            iteration_limit: Some(0),
            ..Default::default()
        },
    ] {
        let r = NativeSolver::new(s)
            .solve(&m, &Options::default(), None, &CancellationToken::default())
            .unwrap();
        assert!(matches!(
            r.status,
            Status::TimeLimit | Status::IterationLimit
        ));
    }
}
#[test]
fn factor_entry_cap() {
    let m = model(
        &[1.; 10],
        &[1.; 100],
        Bounds::fixed(vec![1.; 10]),
        Bounds::non_negative(10),
        Sense::Minimize,
        false,
    );
    let r = NativeSolver::new(NativeOptions {
        max_factor_entries: 5,
        presolve: false,
        ..Default::default()
    })
    .solve(&m, &Options::default(), None, &CancellationToken::default());
    assert!(matches!(r, Err(Error::Unsupported(_))), "{r:?}");
}
#[test]
fn binary_enumeration_oracle() {
    let mut seed = 9127u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as usize
    };
    for case in 0..80 {
        let n = 7;
        let c: Vec<_> = (0..n).map(|_| 1. + (next() % 13) as f64).collect();
        let a: Vec<_> = (0..n).map(|_| 1. + (next() % 9) as f64).collect();
        let rhs = 8. + (next() % 15) as f64;
        let sense = if case % 2 == 0 {
            Sense::Maximize
        } else {
            Sense::Minimize
        };
        let c: Vec<_> = c
            .into_iter()
            .map(|v| if sense == Sense::Minimize { -v } else { v })
            .collect();
        let m = model(
            &c,
            &a,
            Bounds::new(vec![f64::NEG_INFINITY], vec![rhs]),
            Bounds::new(vec![0.; n], vec![1.; n]),
            sense,
            true,
        );
        let exact = (0..1 << n)
            .filter_map(|mask| {
                let x: Vec<_> = (0..n).map(|j| ((mask >> j) & 1) as f64).collect();
                let e = evaluate(&m, &x, &Tolerances::default()).unwrap();
                e.feasible_candidate.then_some(e.objective)
            })
            .reduce(|a, b| {
                if sense == Sense::Minimize {
                    a.min(b)
                } else {
                    a.max(b)
                }
            })
            .unwrap();
        let r = run(&m);
        assert_eq!(r.status, Status::Optimal, "case {case}: {r:?}");
        assert!(
            (r.evaluation.unwrap().objective - exact).abs() < 1e-6,
            "case {case}"
        );
        if let Some(b) = r.best_bound {
            assert!(if sense == Sense::Minimize {
                b <= exact + 1e-9
            } else {
                b >= exact - 1e-9
            });
        }
    }
}
#[test]
fn mip_limit_retains_start() {
    let m = model(
        &[-1., -2.],
        &[1., 1.],
        Bounds::new(vec![f64::NEG_INFINITY], vec![1.5]),
        Bounds::new(vec![0.; 2], vec![1.; 2]),
        Sense::Minimize,
        true,
    );
    let r = NativeSolver::new(NativeOptions {
        node_limit: Some(0),
        ..Default::default()
    })
    .solve(
        &m,
        &Options::default(),
        Some(&WarmStart::from_primal(vec![1., 0.])),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(r.status, Status::NodeLimit);
    assert_eq!(r.evaluation.unwrap().objective, -1.);
    assert!(r.best_bound.is_none());
}

#[test]
fn presolve_differential_fixed_and_empty() {
    for fixed in [0., 2.] {
        let m = model(
            &[7., -1.],
            &[3., 1., 0., 0.],
            Bounds::new(vec![6., 0.], vec![8., 0.]),
            Bounds::new(vec![fixed, 0.], vec![fixed, 10.]),
            Sense::Minimize,
            false,
        );
        let a = run(&m);
        let b = NativeSolver::new(NativeOptions {
            presolve: false,
            ..Default::default()
        })
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
        assert_eq!(a.status, b.status);
        if let (Some(a), Some(b)) = (a.evaluation, b.evaluation) {
            assert!((a.objective - b.objective).abs() < 1e-8);
        }
    }
    let m = model(
        &[1.],
        &[1.],
        Bounds::fixed(vec![2.]),
        Bounds::fixed(vec![1.]),
        Sense::Minimize,
        false,
    );
    assert_eq!(run(&m).status, Status::Infeasible);
}

#[test]
fn pricing_refactor_and_presolve_modes_agree() {
    for sense in [Sense::Minimize, Sense::Maximize] {
        let m = model(
            &[3., 2., -1.],
            &[1., 1., 2., 2., 1., -1., 0., 1., 1.],
            Bounds::new(vec![-2., -3., 0.], vec![4., 5., 3.]),
            Bounds::new(vec![-1.; 3], vec![4.; 3]),
            sense,
            false,
        );
        let reference = run(&m);
        for steepest_edge in [false, true] {
            for refactor_interval in [1, 3, 64] {
                for presolve in [false, true] {
                    let r = NativeSolver::new(NativeOptions {
                        steepest_edge,
                        refactor_interval,
                        presolve,
                        ..Default::default()
                    })
                    .solve(&m, &Options::default(), None, &CancellationToken::default())
                    .unwrap();
                    assert_eq!(r.status, reference.status);
                    assert!(
                        (r.evaluation.unwrap().objective
                            - reference.evaluation.as_ref().unwrap().objective)
                            .abs()
                            < 1e-7
                    );
                }
            }
        }
    }
}

#[test]
fn warm_and_cold_integer_search_agree() {
    let m = model(
        &[-7., -11., -5., -6.],
        &[3., 5., 2., 4.],
        Bounds::new(vec![f64::NEG_INFINITY], vec![8.5]),
        Bounds::new(vec![0.; 4], vec![1.; 4]),
        Sense::Minimize,
        true,
    );
    let a = NativeSolver::new(NativeOptions {
        incumbent_heuristic: false,
        objective_lattice: false,
        ..Default::default()
    })
    .solve(&m, &Options::default(), None, &CancellationToken::default())
    .unwrap();
    let b = NativeSolver::new(NativeOptions {
        incumbent_heuristic: false,
        objective_lattice: false,
        node_reoptimization: false,
        ..Default::default()
    })
    .solve(&m, &Options::default(), None, &CancellationToken::default())
    .unwrap();
    assert_eq!(a.status, Status::Optimal, "{a:?}");
    assert_eq!(b.status, Status::Optimal);
    assert_eq!(
        a.evaluation.unwrap().objective,
        b.evaluation.unwrap().objective
    );
    assert!(
        a.backend
            .effective_options
            .iter()
            .any(|(k, v)| k == "diagnostics.reoptimized_nodes" && v != "0")
    );
}

#[test]
fn empty_rows_are_dropped_or_kept_consistently() {
    let m = model(
        &[-1., -2.],
        &[0., 0., 1., 1., 0., 0.],
        Bounds::new(vec![f64::NEG_INFINITY; 3], vec![0., 1.5, 0.]),
        Bounds::new(vec![0.; 2], vec![1.; 2]),
        Sense::Minimize,
        false,
    );
    for presolve in [false, true] {
        let r = NativeSolver::new(NativeOptions {
            presolve,
            ..Default::default()
        })
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
        assert_eq!(r.status, Status::Optimal);
        assert!((r.evaluation.unwrap().objective + 2.5).abs() < 1e-8);
    }
}

fn diagnostic(r: &SolveResult, key: &str) -> u64 {
    r.backend
        .effective_options
        .iter()
        .find(|(k, _)| k == &format!("diagnostics.{key}"))
        .map(|(_, v)| v.parse().unwrap())
        .unwrap()
}

#[test]
fn free_and_one_sided_columns_need_dual_phase_one() {
    // minimize x0 + 2 x1 - x2, x0 free, x1 <= 4, x2 >= -1:
    //   x0 - x1 = 1, x0 + x1 + x2 >= 3, x2 - x0 <= 2.
    // The slack basis is dual infeasible for all three columns.
    let m = model(
        &[1., 2., -1.],
        &[1., -1., 0., 1., 1., 1., -1., 0., 1.],
        Bounds::new(vec![1., 3., f64::NEG_INFINITY], vec![1., f64::INFINITY, 2.]),
        Bounds::new(
            vec![f64::NEG_INFINITY, f64::NEG_INFINITY, -1.],
            vec![f64::INFINITY, 4., f64::INFINITY],
        ),
        Sense::Minimize,
        false,
    );
    let r = run(&m);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    let k =
        check::lp_residuals(&m, r.candidate.as_ref().unwrap(), r.duals.as_ref().unwrap()).unwrap();
    assert!(k.max() < 1e-9, "{k:?}");
    // By hand: x1 = x0 - 1 and the objective is 3 x0 - 2 - x2, so x2 = 2 + x0
    // is best; the covering row then needs 3 x0 + 1 >= 3, i.e. x0 >= 2/3,
    // and the optimum 2 x0 - 4 is -8/3 at x = (2/3, -1/3, 8/3).
    let x = r.candidate.unwrap();
    assert!(
        (r.evaluation.unwrap().objective + 8. / 3.).abs() < 1e-9,
        "{x:?}"
    );
}

#[test]
fn unbounded_and_infeasible_with_rows_and_free_columns() {
    // minimize -x0 subject to x0 - x1 <= 1 with x free: unbounded along x0 = x1.
    let m = model(
        &[-1., 0.],
        &[1., -1.],
        Bounds::new(vec![f64::NEG_INFINITY], vec![1.]),
        Bounds::free(2),
        Sense::Minimize,
        false,
    );
    assert_eq!(run(&m).status, Status::Unbounded);
    // Adding x0 - x1 >= 2 makes the same directions infeasible.
    let m = model(
        &[-1., 0.],
        &[1., -1., 1., -1.],
        Bounds::new(vec![f64::NEG_INFINITY, 2.], vec![1., f64::INFINITY]),
        Bounds::free(2),
        Sense::Minimize,
        false,
    );
    assert_eq!(run(&m).status, Status::Infeasible);
}

#[test]
fn certification_lp_enables_pruning_on_mixed_models() {
    // Continuous columns on tight rows with decimal data: the LP incumbents
    // do not certify by interval arithmetic as they are.
    let (n, rows) = (12, 5);
    let mut seed = 77u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 33) % 1000) as f64 / 100.
    };
    let a: Vec<f64> = (0..rows * n).map(|_| next() + 0.13).collect();
    let c: Vec<f64> = (0..n).map(|_| next() + 0.7).collect();
    let rhs: Vec<f64> = (0..rows)
        .map(|i| a[i * n..(i + 1) * n].iter().sum::<f64>() * 0.37)
        .collect();
    let mut data = model(
        &c,
        &a,
        Bounds::new(vec![f64::NEG_INFINITY; rows], rhs),
        Bounds::new(vec![0.; n], vec![1.; n]),
        Sense::Maximize,
        true,
    )
    .into_data();
    for j in (0..n).step_by(3) {
        data.kinds[j] = VariableKind::Continuous;
    }
    let m = Model::new(data).unwrap();
    let r = NativeSolver::default()
        .solve(&m, &Options::default(), None, &CancellationToken::default())
        .unwrap();
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert!(
        diagnostic(&r, "certified_incumbents") + diagnostic(&r, "rational_witness_successes") > 0,
        "{r:?}"
    );
    assert!(diagnostic(&r, "pruned_nodes") > 0, "{r:?}");
    let b = r.best_bound.unwrap();
    assert!(b >= r.evaluation.unwrap().objective - 1e-9);
}

#[test]
fn best_bound_search_restores_sibling_bases() {
    let m = model(
        &[-7., -11., -5., -6., -9., -4., -8., -3.],
        &[
            3., 5., 2., 4., 4., 2., 5., 1., 2., 1., 4., 3., 1., 5., 2., 4.,
        ],
        Bounds::new(vec![f64::NEG_INFINITY; 2], vec![9.5, 8.5]),
        Bounds::new(vec![0.; 8], vec![1.; 8]),
        Sense::Minimize,
        true,
    );
    let r = run(&m);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert!(diagnostic(&r, "basis_restores") > 0, "{r:?}");
    assert_eq!(diagnostic(&r, "cold_fallbacks"), 0);
    assert_eq!(diagnostic(&r, "unverified_infeasible_nodes"), 0);
}

#[test]
fn root_fixed_columns_are_substituted() {
    let m = model(
        &[1., 2., 3.],
        &[1., 1., 1., 2., 0., 1.],
        Bounds::new(vec![4., f64::NEG_INFINITY], vec![f64::INFINITY, 5.]),
        Bounds::new(vec![0., 1.5, 0.], vec![10., 1.5, 10.]),
        Sense::Minimize,
        false,
    );
    let r = run(&m);
    assert_eq!(r.status, Status::Optimal, "{r:?}");
    assert_eq!(diagnostic(&r, "presolve_removed_columns"), 1);
    assert_eq!(r.candidate.as_ref().unwrap()[1], 1.5);
    assert!((r.evaluation.unwrap().objective - 5.5).abs() < 1e-9);
}
