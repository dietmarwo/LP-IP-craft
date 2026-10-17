use lp_ip_craft::*;
use lp_ip_craft_native::*;
#[test]
fn box_fast_path_matches_general_solver_and_limits() {
    for sense in [Sense::Minimize, Sense::Maximize] {
        let model = Model::new(ModelData {
            sense,
            objective: vec![2., -3., 0., 4.],
            objective_offset: 7.,
            variables: Bounds::new(
                vec![-2., -3., f64::NEG_INFINITY, 2.],
                vec![3., 4., f64::INFINITY, 2.],
            ),
            kinds: vec![VariableKind::Continuous; 4],
            matrix: CsrMatrix::new(0, 4, vec![0], vec![], vec![]),
            rows: Bounds::free(0),
        })
        .unwrap();
        let run = |presolve, limit| {
            NativeSolver::new(NativeOptions {
                presolve,
                iteration_limit: limit,
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
        let (fast, full) = (run(true, None), run(false, None));
        assert_eq!(fast.status, Status::Optimal);
        assert_eq!(
            fast.evaluation.unwrap().objective,
            full.evaluation.unwrap().objective
        );
        assert_eq!(run(true, Some(0)).status, Status::IterationLimit);
        let mut d = model.into_data();
        d.variables.upper[0] = f64::INFINITY;
        d.sense = Sense::Maximize;
        assert_eq!(
            NativeSolver::default()
                .solve(
                    &Model::new(d).unwrap(),
                    &Options::default(),
                    None,
                    &CancellationToken::default()
                )
                .unwrap()
                .status,
            Status::Unbounded
        );
    }
}

#[test]
fn integer_propagation_matches_enumeration_with_signed_rows() {
    let mut seed = 91u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as usize
    };
    for case in 0..80 {
        let a: Vec<f64> = (0..12).map(|_| (next() % 7) as f64 - 3.).collect();
        let c: Vec<f64> = (0..4).map(|_| (next() % 9) as f64 - 4.).collect();
        let lower: Vec<f64> = (0..3).map(|_| (next() % 9) as f64 - 5.3).collect();
        let upper: Vec<f64> = lower
            .iter()
            .map(|&l| l + (next() % 7) as f64 + 0.1)
            .collect();
        let model = Model::new(ModelData {
            sense: if case % 2 == 0 {
                Sense::Minimize
            } else {
                Sense::Maximize
            },
            objective: c,
            objective_offset: 2.,
            variables: Bounds::new(vec![-1.2; 4], vec![2.7; 4]),
            kinds: vec![VariableKind::Integer; 4],
            matrix: CsrMatrix::from_dense(3, 4, &a).unwrap(),
            rows: Bounds::new(lower, upper),
        })
        .unwrap();
        let mut best: Option<f64> = None;
        for code in 0..256 {
            let x: Vec<_> = (0..4)
                .map(|j| ((code >> (2 * j)) & 3) as f64 - 1.)
                .collect();
            let e = check::evaluate(&model, &x, &Tolerances::default()).unwrap();
            if e.feasible_candidate {
                best = Some(best.map_or(e.objective, |b| {
                    if case % 2 == 0 {
                        b.min(e.objective)
                    } else {
                        b.max(e.objective)
                    }
                }));
            }
        }
        for warm in [false, true] {
            let r = NativeSolver::new(NativeOptions {
                decompose: false,
                node_reoptimization: warm,
                ..Default::default()
            })
            .solve(
                &model,
                &Options::default(),
                None,
                &CancellationToken::default(),
            )
            .unwrap();
            if let Some(expected) = best {
                assert_eq!(r.status, Status::Optimal, "case {case}: {r:?}");
                assert!((r.evaluation.unwrap().objective - expected).abs() < 1e-7);
            } else {
                assert_eq!(r.status, Status::Infeasible, "case {case}: {r:?}");
            }
        }
    }
}
