use lp_ip_craft::*;
use lp_ip_craft_native::*;

#[test]
fn dense_planted_lp_certificates_across_ordering_and_update_methods() {
    for n in [96, 160, 224] {
        for seed in 0..2 {
            let x: Vec<_> = (0..n)
                .map(|j| 1. + ((j * 13 + seed) % 7) as f64 / 10.)
                .collect();
            let y: Vec<_> = (0..n)
                .map(|i| 1. + ((i * 11 + seed) % 5) as f64 / 10.)
                .collect();
            let mut a = vec![0.; n * n];
            for r in 0..n {
                let i = (n - 1 - r + seed) % n;
                for j in 0..n {
                    a[i * n + j] = if j == r {
                        4.
                    } else {
                        ((r * 31 + j * 17 + seed) % 19 + 1) as f64 * 0.0005
                    };
                }
            }
            let b: Vec<f64> = (0..n)
                .map(|i| (0..n).map(|j| a[i * n + j] * x[j]).sum())
                .collect();
            let c: Vec<f64> = (0..n)
                .map(|j| (0..n).map(|i| a[i * n + j] * y[i]).sum())
                .collect();
            let expected: f64 = c.iter().zip(&x).map(|(a, b)| a * b).sum();
            let model = Model::new(ModelData {
                sense: Sense::Maximize,
                objective: c,
                objective_offset: 0.,
                variables: Bounds::new(vec![0.; n], vec![10.; n]),
                kinds: vec![VariableKind::Continuous; n],
                matrix: CsrMatrix::from_dense(n, n, &a).unwrap(),
                rows: Bounds::new(vec![f64::NEG_INFINITY; n], b),
            })
            .unwrap();
            for ft in [false, true] {
                let mut solver = NativeSolver::new(NativeOptions {
                    decompose: false,
                    basis_update: if ft {
                        BasisUpdate::ForrestTomlin
                    } else {
                        BasisUpdate::Eta
                    },
                    factor_ordering: if ft {
                        FactorOrdering::MinDegree
                    } else {
                        FactorOrdering::Natural
                    },
                    ..Default::default()
                });
                let r = solver
                    .solve(
                        &model,
                        &Options::default(),
                        None,
                        &CancellationToken::default(),
                    )
                    .unwrap();
                assert_eq!(
                    r.status,
                    Status::Optimal,
                    "n={n}, seed={seed}, ft={ft}: {r:?}"
                );
                assert!(
                    (r.evaluation.as_ref().unwrap().objective - expected).abs()
                        < 1e-7 * (1. + expected.abs())
                );
                let k = check::lp_residuals(
                    &model,
                    r.candidate.as_ref().unwrap(),
                    r.duals.as_ref().unwrap(),
                )
                .unwrap();
                assert!(
                    k.primal < 1e-7
                        && k.stationarity < 1e-7
                        && k.dual_sign < 1e-7
                        && k.duality_gap < 1e-7,
                    "{k:?}"
                );
            }
        }
    }
}
