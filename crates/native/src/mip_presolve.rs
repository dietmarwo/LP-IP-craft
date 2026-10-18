//! Opt-in primal reductions and optimum-preserving objective fixes.
use crate::{certified_cuts, dyadic::Dy, rational, *};
#[derive(Default)]
pub(crate) struct Stats {
    pub rounds: u64,
    pub bounds: u64,
    pub redundant: u64,
    pub forcing: u64,
    pub coefficients: u64,
    pub dual_fixed: u64,
    pub work: u64,
    pub conflicts: u64,
}
fn dy(v: f64) -> Option<Dy> {
    Dy::from_f64(v)
}
/// Exact i128 fast path; every emitted integer is exactly representable in f64.
fn tighten_integral(model: &Model, i: usize, sign: f64) -> Option<certified_cuts::Cut> {
    let a = model.matrix();
    let rhs = if sign > 0. {
        model.rows().upper[i]
    } else {
        -model.rows().lower[i]
    };
    let exact = |v: f64| v.is_finite() && v.fract() == 0. && v.abs() <= 2_147_483_648.;
    if !exact(rhs) {
        return None;
    }
    let mut terms = Vec::new();
    let mut maximum = 0i128;
    for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
        let j = a.pattern.indices[k];
        let c = sign * a.values[k];
        let bound = if c > 0. {
            model.variables().upper[j]
        } else {
            model.variables().lower[j]
        };
        if !exact(c) || !exact(bound) {
            return None;
        }
        let c = c as i128;
        maximum = maximum.checked_add(c.checked_mul(bound as i128)?)?;
        terms.push((j, c));
    }
    let mut bound = rhs as i128;
    let delta = maximum.checked_sub(bound)?;
    if delta <= 0 {
        return None;
    }
    let mut changed = false;
    for (j, c) in &mut terms {
        if model.kinds()[*j].is_integral()
            && model.variables().lower[*j] == 0.
            && model.variables().upper[*j] == 1.
            && c.abs() > delta
        {
            if *c > 0 {
                bound = maximum.checked_sub(*c)?;
                maximum = bound.checked_add(delta)?;
                *c = delta;
            } else {
                *c = -delta;
            }
            changed = true;
        }
    }
    if !changed
        || bound.abs() > 9_007_199_254_740_991
        || terms.iter().any(|t| t.1.abs() > 9_007_199_254_740_991)
    {
        return None;
    }
    Some(certified_cuts::Cut {
        terms: terms
            .into_iter()
            .filter(|t| t.1 != 0)
            .map(|(j, c)| (j, c as f64))
            .collect(),
        rhs: bound as f64,
        family: "tightening",
    })
}
fn tighten_row(model: &Model, i: usize, sign: f64) -> Option<certified_cuts::Cut> {
    if let Some(cut) = tighten_integral(model, i, sign) {
        return Some(cut);
    }
    // Exact dyadic fallback: the maximum activity, its excess over the side
    // and the replacement coefficient/RHS are sums and products of binary64
    // data, so no division is needed.
    let a = model.matrix();
    let bound = if sign > 0. {
        model.rows().upper[i]
    } else {
        -model.rows().lower[i]
    };
    let mut maximum = Dy::zero();
    let mut row = Vec::new();
    for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
        let j = a.pattern.indices[k];
        let c = sign * a.values[k];
        let b = if c > 0. {
            model.variables().upper[j]
        } else {
            model.variables().lower[j]
        };
        maximum = maximum.add(&dy(c)?.mul(&dy(b)?)?)?;
        row.push((j, c));
    }
    row.sort_by_key(|t| t.0);
    let delta = maximum.sub(&dy(bound)?)?;
    if !delta.is_positive() {
        return None;
    }
    for &(j, c) in &row {
        if !model.kinds()[j].is_integral()
            || model.variables().lower[j] != 0.
            || model.variables().upper[j] != 1.
            || dy(c.abs())?.cmp(&delta)? != std::cmp::Ordering::Greater
        {
            continue;
        }
        let (replacement, rhs) = if c > 0. {
            (delta.clone(), maximum.sub(&dy(c)?)?)
        } else {
            (delta.neg(), dy(bound)?)
        };
        let coefficients = row
            .iter()
            .map(|&(k, v)| Some((k, if k == j { replacement.clone() } else { dy(v)? })))
            .collect::<Option<Vec<_>>>()?;
        return certified_cuts::enclose(model, coefficients, &rhs, "tightening");
    }
    None
}
pub(crate) fn run(
    model: &mut Model,
    settings: &NativeOptions,
    options: &Options,
    cancel: &CancellationToken,
    began: Instant,
) -> Result<Stats> {
    let mut stats = Stats::default();
    rational::reset_work();
    let budget = Budget {
        start: began,
        settings,
        cancel,
        pivots: 0,
    };
    for _ in 0..settings.mip_presolve_rounds {
        if budget.stop().is_some() {
            break;
        }
        stats.rounds += 1;
        let before = model.clone();
        if settings.mip_presolve {
            let mut b = model.variables().clone();
            let mut changed = Vec::new();
            let mut exact = propagate::Domain::new(model, true);
            let mut mixed = mixed::Domain::new(model, true);
            for _ in 0..4 {
                let prev = changed.len();
                if !exact.apply(&mut b, &mut changed, &budget)
                    || !mixed.apply(model, &mut b, &mut changed, &budget)
                {
                    stats.conflicts += 1;
                    break;
                }
                if prev == changed.len() {
                    break;
                }
            }
            if (0..model.n()).all(|j| b.lower[j] <= b.upper[j]) {
                stats.bounds += (0..model.n())
                    .filter(|&j| {
                        b.lower[j] != model.variables().lower[j]
                            || b.upper[j] != model.variables().upper[j]
                    })
                    .count() as u64;
                let mut d = model.data().clone();
                d.variables = b;
                *model = Model::new(d)?;
            }
            let a = model.matrix();
            let mut d = model.data().clone();
            let active = presolve::active_rows(model, model.variables(), true);
            for (i, &keep) in active.iter().enumerate() {
                if !keep && (d.rows.lower[i].is_finite() || d.rows.upper[i].is_finite()) {
                    d.rows.lower[i] = f64::NEG_INFINITY;
                    d.rows.upper[i] = f64::INFINITY;
                    stats.redundant += 1;
                }
                let terms = (a.pattern.indptr[i]..a.pattern.indptr[i + 1]).map(|k| {
                    let j = a.pattern.indices[k];
                    let c = a.values[k];
                    (
                        c,
                        if c > 0. {
                            model.variables().upper[j]
                        } else {
                            model.variables().lower[j]
                        },
                    )
                });
                if let Some((l, u)) = incumbent::dot(terms, 0.) {
                    if l == u && l == model.rows().lower[i] {
                        stats.forcing += 1;
                    }
                }
            }
            *model = Model::new(d)?;
        }
        if settings.coefficient_tightening {
            for i in 0..model.m() {
                if budget.stop().is_some() || rational::work() >= settings.cut_exact_work {
                    break;
                }
                let sign = if model.rows().lower[i] == f64::NEG_INFINITY
                    && model.rows().upper[i].is_finite()
                {
                    1.
                } else if model.rows().upper[i] == f64::INFINITY
                    && model.rows().lower[i].is_finite()
                {
                    -1.
                } else {
                    continue;
                };
                if let Some(cut) = tighten_row(model, i, sign) {
                    let mut data = model.data().clone();
                    let a = model.matrix();
                    let mut indices = Vec::new();
                    let mut values = Vec::new();
                    let mut ptr = vec![0];
                    for row in 0..model.m() {
                        if row == i {
                            for &(j, v) in &cut.terms {
                                indices.push(j);
                                values.push(sign * v);
                            }
                        } else {
                            let range = a.pattern.indptr[row]..a.pattern.indptr[row + 1];
                            indices.extend_from_slice(&a.pattern.indices[range.clone()]);
                            values.extend_from_slice(&a.values[range]);
                        }
                        ptr.push(values.len());
                    }
                    data.matrix.pattern.indptr = ptr;
                    data.matrix.pattern.indices = indices;
                    data.matrix.values = values;
                    if sign > 0. {
                        data.rows.upper[i] = cut.rhs;
                    } else {
                        data.rows.lower[i] = -cut.rhs;
                    }
                    *model = Model::new(data)?;
                    stats.coefficients += 1;
                }
            }
        }
        if settings.dual_presolve {
            let mut count = vec![0; model.n()];
            let mut only = vec![(0, 0.); model.n()];
            let a = model.matrix();
            for i in 0..model.m() {
                if !model.rows().lower[i].is_finite() && !model.rows().upper[i].is_finite() {
                    continue;
                }
                for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                    if a.values[k] != 0. {
                        let j = a.pattern.indices[k];
                        count[j] += 1;
                        only[j] = (i, a.values[k]);
                    }
                }
            }
            let mut d = model.data().clone();
            for j in 0..model.n() {
                if count[j] > 1 || d.variables.lower[j] == d.variables.upper[j] {
                    continue;
                }
                let cost = oriented(model, model.objective()[j]);
                let down = count[j] == 0 || {
                    let (i, c) = only[j];
                    if c > 0. {
                        model.rows().lower[i] == f64::NEG_INFINITY
                    } else {
                        model.rows().upper[i] == f64::INFINITY
                    }
                };
                let up = count[j] == 0 || {
                    let (i, c) = only[j];
                    if c > 0. {
                        model.rows().upper[i] == f64::INFINITY
                    } else {
                        model.rows().lower[i] == f64::NEG_INFINITY
                    }
                };
                let target = if cost >= 0. && down {
                    d.variables.lower[j]
                } else if cost <= 0. && up {
                    d.variables.upper[j]
                } else {
                    continue;
                };
                let target = if model.kinds()[j].is_integral() {
                    if cost >= 0. && down {
                        target.ceil()
                    } else {
                        target.floor()
                    }
                } else {
                    target
                };
                if target.is_finite()
                    && target >= d.variables.lower[j]
                    && target <= d.variables.upper[j]
                {
                    d.variables.lower[j] = target;
                    d.variables.upper[j] = target;
                    stats.dual_fixed += 1;
                }
            }
            *model = Model::new(d)?;
        }
        if *model == before {
            break;
        }
    }
    stats.work = rational::work();
    let _ = options;
    Ok(stats)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::certified_cuts::tests::model;
    use lp_ip_craft::VariableKind;
    #[test]
    fn binary_big_m_tightening_is_exactly_integer_equivalent_in_mixed_rows() {
        for sign in [-1., 1.] {
            let m = model(
                vec![10., 1.],
                vec![VariableKind::Binary, VariableKind::Continuous],
                vec![0., 0.],
                vec![1., 10.],
                vec![100. * sign, sign],
                vec![if sign > 0. { 5. } else { f64::NEG_INFINITY }],
                vec![if sign < 0. { -5. } else { f64::INFINITY }],
            );
            let mut reduced = m.clone();
            let s = NativeOptions {
                coefficient_tightening: true,
                ..Default::default()
            };
            let stats = run(
                &mut reduced,
                &s,
                &Options::default(),
                &CancellationToken::default(),
                Instant::now(),
            )
            .unwrap();
            assert!(stats.coefficients > 0);
            assert!(reduced.matrix().values[0].abs() < 100.);
            for binary in [0., 1.] {
                for k in 0..=40 {
                    let x = [binary, k as f64 / 4.];
                    assert_eq!(
                        check::evaluate(&m, &x, &Options::default().tolerances)
                            .unwrap()
                            .feasible_candidate,
                        check::evaluate(&reduced, &x, &Options::default().tolerances)
                            .unwrap()
                            .feasible_candidate
                    );
                }
            }
        }
    }
    #[test]
    fn singleton_dual_fixing_is_sense_aware_and_general_integer_safe() {
        let mut m = model(
            vec![-1., 1.],
            vec![VariableKind::Integer; 2],
            vec![0.; 2],
            vec![3.; 2],
            vec![1., 0.],
            vec![f64::NEG_INFINITY],
            vec![5.],
        );
        let s = NativeOptions {
            dual_presolve: true,
            ..Default::default()
        };
        run(
            &mut m,
            &s,
            &Options::default(),
            &CancellationToken::default(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(m.variables().lower[0], 0.);
        assert_eq!(m.variables().upper[0], 0.);
        assert_eq!(m.variables().lower[1], 3.);
    }
}
