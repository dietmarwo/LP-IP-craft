use lp_ip_craft::*;

fn model(c: Vec<f64>, matrix: CsrMatrix, bounds: Bounds, rows: Bounds) -> Model {
    let n = c.len();
    Model::new(ModelData {
        sense: Sense::Minimize,
        objective: c,
        objective_offset: 0.,
        variables: bounds,
        kinds: vec![VariableKind::Continuous; n],
        matrix,
        rows,
    })
    .unwrap()
}

#[test]
fn prepared_updates_enclose_exact_dyadic_lagrangians_in_both_senses() {
    let mut seed = 71u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) % 31) as i128 - 15
    };
    for case in 0..256 {
        let (n, m) = (case % 33 + 1, case % 5 + 1);
        let c: Vec<_> = (0..n).map(|_| random()).collect();
        let a: Vec<_> = (0..m * n).map(|_| random()).collect();
        let y: Vec<_> = (0..m).map(|_| random()).collect();
        let rl: Vec<_> = (0..m).map(|_| random() - 16).collect();
        let ru: Vec<_> = rl.iter().map(|v| v + 40).collect();
        let mut lo: Vec<_> = (0..n).map(|_| random() - 16).collect();
        let mut hi: Vec<_> = lo.iter().map(|v| v + 40).collect();
        let offset = random();
        let sign = if case % 2 == 0 { 1i128 } else { -1 };
        let mut data = model(
            c.iter().map(|&v| v as f64 / 8.).collect(),
            CsrMatrix::from_dense(m, n, &a.iter().map(|&v| v as f64 / 8.).collect::<Vec<_>>())
                .unwrap(),
            Bounds::new(
                lo.iter().map(|&v| v as f64 / 4.).collect(),
                hi.iter().map(|&v| v as f64 / 4.).collect(),
            ),
            Bounds::new(
                rl.iter().map(|&v| v as f64 / 4.).collect(),
                ru.iter().map(|&v| v as f64 / 4.).collect(),
            ),
        )
        .into_data();
        data.sense = if sign == 1 {
            Sense::Minimize
        } else {
            Sense::Maximize
        };
        data.objective_offset = offset as f64 / 8.;
        let model = Model::new(data).unwrap();
        let y_float: Vec<_> = y.iter().map(|&v| v as f64 / 8.).collect();
        let mut bounds = model.variables().clone();
        let mut prepared = PreparedDualBound::new(&model, &bounds, &y_float).unwrap();
        for update in 0..20 {
            if update > 0 {
                let j = (update * 7 + case) % n;
                lo[j] = random() - 16;
                hi[j] = lo[j] + random().abs() + 1;
                bounds.lower[j] = lo[j] as f64 / 4.;
                bounds.upper[j] = hi[j] as f64 / 4.;
                prepared
                    .set_bound(j, bounds.lower[j], bounds.upper[j])
                    .unwrap();
            }
            // Exact i128 computation in units of 1/256, independent of interval arithmetic.
            let mut exact = sign * offset * 32;
            for i in 0..m {
                exact += 8 * ((sign * y[i] * rl[i]).min(sign * y[i] * ru[i]));
            }
            for j in 0..n {
                let r = sign * (c[j] * 8 - (0..m).map(|i| a[i * n + j] * y[i]).sum::<i128>());
                exact += (r * lo[j]).min(r * hi[j]);
            }
            let exact = exact as f64 / 256.;
            let cached = prepared.bound().bound.unwrap() * sign as f64;
            let fresh = safe_dual_bound_with_bounds(&model, &bounds, &y_float)
                .unwrap()
                .bound
                .unwrap()
                * sign as f64;
            assert!(
                cached <= exact && fresh <= exact,
                "case {case}, update {update}: {cached} / {fresh} > {exact}"
            );
            assert!((cached - exact).abs() < 1e-9 * (1. + exact.abs()));
            assert!((fresh - exact).abs() < 1e-9 * (1. + exact.abs()));
        }
    }
}

#[test]
fn mutable_unbounded_and_overflow_terms_recover_and_invalid_updates_are_atomic() {
    let model = model(
        vec![1., -2.],
        CsrMatrix::new(0, 2, vec![0], vec![], vec![]),
        Bounds::new(vec![0., 0.], vec![1., 1.]),
        Bounds::free(0),
    );
    let mut p = PreparedDualBound::new(&model, model.variables(), &[]).unwrap();
    let before = p.bound();
    for (j, l, u) in [
        (2, 0., 1.),
        (0, f64::NAN, 1.),
        (0, 2., 1.),
        (0, f64::INFINITY, f64::INFINITY),
        (0, f64::NEG_INFINITY, f64::NEG_INFINITY),
    ] {
        assert!(p.set_bound(j, l, u).is_err());
        assert_eq!(p.bound(), before);
    }
    p.set_bound(1, 0., f64::INFINITY).unwrap();
    assert_eq!(p.bound().status, SafeBoundStatus::UnboundedVariable);
    assert_eq!(p.bound().index, Some(1));
    p.set_bound(1, 0., 3.).unwrap();
    assert!(p.bound().bound.unwrap() <= -6.);
    p.set_bound(0, f64::NEG_INFINITY, 1.).unwrap();
    assert_eq!(p.bound().index, Some(0));
    p.set_bound(0, 0., 1.).unwrap();
    assert!(p.bound().bound.is_some());

    let model = model_for_overflow();
    let mut p = PreparedDualBound::new(&model, model.variables(), &[]).unwrap();
    assert_eq!(p.bound().status, SafeBoundStatus::ArithmeticOverflow);
    for j in 0..3 {
        p.set_bound(j, 0., 0.).unwrap();
    }
    assert!(p.bound().bound.unwrap().is_finite());
    p.set_bound(3, 8., 8.).unwrap();
    assert_eq!(p.bound().status, SafeBoundStatus::ArithmeticOverflow);
    p.set_bound(3, 1., 1.).unwrap();
    assert!(p.bound().bound.is_some());
}
fn model_for_overflow() -> Model {
    model(
        vec![f64::MAX / 8.; 4],
        CsrMatrix::new(0, 4, vec![0], vec![], vec![]),
        Bounds::fixed(vec![4.; 4]),
        Bounds::free(0),
    )
}

#[test]
fn immutable_row_failures_zero_residuals_and_empty_rows() {
    let free = model(
        vec![2.],
        CsrMatrix::from_dense(1, 1, &[1.]).unwrap(),
        Bounds::free(1),
        Bounds::fixed(vec![1.]),
    );
    let mut p = PreparedDualBound::new(&free, free.variables(), &[2.]).unwrap();
    assert_eq!(p.bound().bound, Some(2.));
    p.set_bound(0, -5., 8.).unwrap();
    assert_eq!(p.bound().bound, Some(2.));
    assert!(PreparedDualBound::new(&free, free.variables(), &[]).is_err());
    assert!(PreparedDualBound::new(&free, free.variables(), &[f64::NAN]).is_err());
    let row = model(
        vec![0.],
        CsrMatrix::from_dense(1, 1, &[1.]).unwrap(),
        Bounds::fixed(vec![0.]),
        Bounds::free(1),
    );
    let mut p = PreparedDualBound::new(&row, row.variables(), &[1.]).unwrap();
    p.set_bound(0, 0., 0.).unwrap();
    assert_eq!(p.bound().status, SafeBoundStatus::UnboundedRow);
    let empty = model(
        vec![0.],
        CsrMatrix::new(0, 1, vec![0], vec![], vec![]),
        Bounds::free(1),
        Bounds::free(0),
    );
    assert_eq!(
        PreparedDualBound::new(&empty, empty.variables(), &[])
            .unwrap()
            .bound()
            .bound,
        Some(0.)
    );
}
