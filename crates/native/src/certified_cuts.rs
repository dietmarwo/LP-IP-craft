//! Exact GMI/MIR derivation. Numerical multipliers are advice, not an equality.
use crate::{
    Budget,
    lp::Lp,
    rational::{self, Rat},
};
use lp_ip_craft::{Model, Result};
use std::cmp::Ordering;
#[derive(Clone, Debug)]
pub(crate) struct Cut {
    pub terms: Vec<(usize, f64)>,
    pub rhs: f64,
    pub family: &'static str,
}
#[derive(Default)]
pub(crate) struct Stats {
    pub candidates: u64,
    pub rejected: u64,
    pub work: u64,
    pub vubs: u64,
}
fn r(v: f64) -> Option<Rat> {
    Rat::from(v)
}
fn positive(v: &Rat) -> Option<bool> {
    Some(v.cmp(&Rat::zero())? == Ordering::Greater)
}
/// Relax every coefficient conversion over the finite root box, exactly.
pub(crate) fn enclose(
    model: &Model,
    coefficients: &[Rat],
    rhs: &Rat,
    family: &'static str,
) -> Option<Cut> {
    let mut bound = rhs.clone();
    let mut terms = Vec::new();
    for (j, c) in coefficients.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        let v = c.upper()?;
        let v = if v.abs() < 1e-11 { 0. } else { v };
        let delta = r(v)?.sub(c)?;
        if !delta.is_zero() {
            let endpoint = if positive(&delta)? {
                model.variables().upper[j]
            } else {
                model.variables().lower[j]
            };
            bound = bound.add(&delta.mul(&r(endpoint)?)?)?;
        }
        if v != 0. {
            terms.push((j, v));
        }
    }
    if terms.is_empty() || terms.len() > 1024 {
        return None;
    }
    let max = terms.iter().map(|t| t.1.abs()).fold(0., f64::max);
    let min = terms
        .iter()
        .map(|t| t.1.abs())
        .fold(f64::INFINITY, f64::min);
    if max / min > 1e6 {
        return None;
    }
    Some(Cut {
        terms,
        rhs: bound.upper()?,
        family,
    })
}
pub(crate) fn append(model: &mut Model, cuts: &[Cut]) -> Result<()> {
    let mut data = model.data().clone();
    for cut in cuts {
        for &(j, c) in &cut.terms {
            data.matrix.pattern.indices.push(j);
            data.matrix.values.push(c);
        }
        data.matrix.pattern.indptr.push(data.matrix.values.len());
        data.matrix.pattern.rows += 1;
        data.rows.lower.push(f64::NEG_INFINITY);
        data.rows.upper.push(cut.rhs);
    }
    *model = Model::new(data)?;
    Ok(())
}
fn efficacy(c: &Cut, x: &[f64]) -> f64 {
    let violation = c.terms.iter().map(|&(j, v)| v * x[j]).sum::<f64>() - c.rhs;
    violation
        / c.terms
            .iter()
            .map(|t| t.1 * t.1)
            .sum::<f64>()
            .sqrt()
            .max(1e-30)
}
fn cosine(a: &Cut, b: &Cut) -> f64 {
    let (mut i, mut j, mut dot) = (0, 0, 0.);
    while i < a.terms.len() && j < b.terms.len() {
        match a.terms[i].0.cmp(&b.terms[j].0) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                dot += a.terms[i].1 * b.terms[j].1;
                i += 1;
                j += 1;
            }
        }
    }
    dot.abs()
        / (a.terms.iter().map(|t| t.1 * t.1).sum::<f64>()
            * b.terms.iter().map(|t| t.1 * t.1).sum::<f64>())
        .sqrt()
        .max(1e-30)
}
pub(crate) fn select(model: &Model, mut cuts: Vec<Cut>, x: &[f64], limit: usize) -> Vec<Cut> {
    cuts.retain(|c| {
        efficacy(c, x) > 1e-6
            && !(0..model.m()).any(|i| {
                let a = model.matrix();
                let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
                range.len() == c.terms.len()
                    && model.rows().upper[i] <= c.rhs
                    && range
                        .zip(&c.terms)
                        .all(|(k, &(j, v))| a.pattern.indices[k] == j && a.values[k] == v)
            })
    });
    cuts.sort_by(|a, b| efficacy(b, x).total_cmp(&efficacy(a, x)));
    let mut out = Vec::new();
    for c in cuts {
        if out.len() >= limit {
            break;
        }
        if out.iter().all(|old| cosine(&c, old) < 0.95) {
            out.push(c);
        }
    }
    out
}
/// An affine nonnegative variable z = sum terms*x + constant.
#[derive(Clone)]
struct Term {
    coefficients: Vec<(usize, Rat)>,
    constant: Rat,
    integer: bool,
}
fn anchor(lo: f64, hi: f64, value: f64, integer: bool) -> Option<(f64, f64)> {
    let (lo, hi) = if integer {
        (lo.ceil(), hi.floor())
    } else {
        (lo, hi)
    };
    if lo > hi {
        return None;
    }
    if lo.is_finite() && (!hi.is_finite() || value - lo <= hi - value) {
        Some((lo, 1.))
    } else if hi.is_finite() {
        Some((hi, -1.))
    } else {
        None
    }
}
fn gmi(
    model: &Model,
    pivot: usize,
    weights: &[f64],
    x: &[f64],
    budget: &Budget<'_>,
    cap: u64,
) -> Option<Cut> {
    let a = model.matrix();
    let mut aggregate = vec![Rat::zero(); model.n()];
    for (i, &weight) in weights.iter().enumerate() {
        if weight == 0. {
            continue;
        }
        if budget.stop().is_some() || rational::work() >= cap {
            return None;
        }
        let w = r(weight)?;
        for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
            let j = a.pattern.indices[k];
            aggregate[j] = aggregate[j].add(&w.mul(&r(a.values[k])?)?)?;
        }
    }
    let pivot_a = aggregate[pivot].clone();
    if pivot_a.is_zero() {
        return None;
    }
    let mut b = Rat::zero();
    let mut transformed = Vec::new();
    for (j, c) in aggregate.iter().enumerate() {
        if j == pivot || c.is_zero() {
            continue;
        }
        let (bound, sign) = anchor(
            model.variables().lower[j],
            model.variables().upper[j],
            x[j],
            model.kinds()[j].is_integral(),
        )?;
        let c = c.div(&pivot_a)?;
        b = b.sub(&c.mul(&r(bound)?)?)?;
        transformed.push((
            c.mul(&r(sign)?)?,
            Term {
                coefficients: vec![(j, r(sign)?)],
                constant: r(-sign * bound)?,
                integer: model.kinds()[j].is_integral(),
            },
        ));
    }
    for (i, &weight) in weights.iter().enumerate() {
        if weight == 0. {
            continue;
        }
        let integer = (a.pattern.indptr[i]..a.pattern.indptr[i + 1]).all(|k| {
            a.values[k].fract() == 0. && model.kinds()[a.pattern.indices[k]].is_integral()
        });
        let activity = (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
            .map(|k| a.values[k] * x[a.pattern.indices[k]])
            .sum();
        let (bound, sign) = anchor(
            model.rows().lower[i],
            model.rows().upper[i],
            activity,
            integer,
        )?;
        let c = r(-weight)?.div(&pivot_a)?;
        b = b.sub(&c.mul(&r(bound)?)?)?;
        let coefficients = (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
            .map(|k| Some((a.pattern.indices[k], r(sign)?.mul(&r(a.values[k])?)?)))
            .collect::<Option<Vec<_>>>()?;
        transformed.push((
            c.mul(&r(sign)?)?,
            Term {
                coefficients,
                constant: r(-sign * bound)?,
                integer,
            },
        ));
    }
    let f = b.frac()?;
    let fv = f.upper()?;
    if !(0.005..=0.995).contains(&fv) {
        return None;
    }
    let one = r(1.)?;
    let om = one.sub(&f)?;
    let mut coefficients = vec![Rat::zero(); model.n()];
    let mut rhs = r(-1.)?;
    for (c, term) in transformed {
        if budget.stop().is_some() || rational::work() >= cap {
            return None;
        }
        let g = if term.integer {
            let fj = c.frac()?;
            if fj.cmp(&f)? != Ordering::Greater {
                fj.div(&f)?
            } else {
                one.sub(&fj)?.div(&om)?
            }
        } else if positive(&c)? {
            c.div(&f)?
        } else {
            c.neg().div(&om)?
        };
        // sum g*z >=1 -> -sum g*affine <= -1 + sum g*constant.
        for (j, v) in term.coefficients {
            coefficients[j] = coefficients[j].sub(&g.mul(&v)?)?;
        }
        rhs = rhs.add(&g.mul(&term.constant)?)?;
    }
    enclose(model, &coefficients, &rhs, "gmi")
}
#[derive(Clone)]
struct Vub {
    binary: usize,
    factor: Rat,
}
fn variable_bounds(model: &Model) -> Vec<Option<Vub>> {
    let mut out: Vec<Option<Vub>> = vec![None; model.n()];
    let a = model.matrix();
    for i in 0..model.m() {
        let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        if range.len() != 2 {
            continue;
        }
        for (sign, bound) in [(1., model.rows().upper[i]), (-1., -model.rows().lower[i])] {
            if bound != 0. {
                continue;
            }
            for (k, l) in [
                (range.start, range.start + 1),
                (range.start + 1, range.start),
            ] {
                let j = a.pattern.indices[k];
                let y = a.pattern.indices[l];
                let (c, d) = (sign * a.values[k], sign * a.values[l]);
                if !model.kinds()[j].is_integral()
                    && model.kinds()[y].is_integral()
                    && model.variables().lower[y] == 0.
                    && model.variables().upper[y] == 1.
                    && c > 0.
                    && d < 0.
                {
                    if let Some(factor) = r(-d).and_then(|v| v.div(&r(c)?)) {
                        out[j] = Some(Vub { binary: y, factor });
                    }
                }
            }
        }
    }
    out
}
#[allow(clippy::too_many_arguments)]
fn mir(
    model: &Model,
    row: usize,
    sign: f64,
    divisor: f64,
    x: &[f64],
    vubs: &[Option<Vub>],
    budget: &Budget<'_>,
    cap: u64,
) -> Option<Cut> {
    let a = model.matrix();
    let bound = if sign > 0. {
        model.rows().upper[row]
    } else {
        -model.rows().lower[row]
    };
    let d = r(divisor)?;
    let mut b = r(bound)?;
    let mut coeff = vec![Rat::zero(); model.n()];
    let mut substitutions = Vec::new();
    for k in a.pattern.indptr[row]..a.pattern.indptr[row + 1] {
        coeff[a.pattern.indices[k]] = r(sign * a.values[k])?;
    }
    // Replace continuous x by d*y-z for a certified nonnegative VUB slack.
    for (j, vub) in vubs.iter().enumerate() {
        if let Some(v) = vub {
            if !coeff[j].is_zero() {
                let c = coeff[j].clone();
                coeff[v.binary] = coeff[v.binary].add(&c.mul(&v.factor)?)?;
                coeff[j] = Rat::zero();
                substitutions.push((
                    c.neg(),
                    Term {
                        coefficients: vec![(v.binary, v.factor.clone()), (j, r(-1.)?)],
                        constant: Rat::zero(),
                        integer: false,
                    },
                ));
            }
        }
    }
    for (j, c) in coeff.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        let (bound, sgn) = anchor(
            model.variables().lower[j],
            model.variables().upper[j],
            x[j],
            model.kinds()[j].is_integral(),
        )?;
        b = b.sub(&c.mul(&r(bound)?)?)?;
        substitutions.push((
            c.mul(&r(sgn)?)?,
            Term {
                coefficients: vec![(j, r(sgn)?)],
                constant: r(-sgn * bound)?,
                integer: model.kinds()[j].is_integral(),
            },
        ));
    }
    b = b.div(&d)?;
    let f = b.frac()?;
    let fv = f.upper()?;
    if !(0.005..=0.995).contains(&fv) {
        return None;
    }
    let om = r(1.)?.sub(&f)?;
    let mut rhs = b.floor();
    let mut result = vec![Rat::zero(); model.n()];
    for (c, t) in substitutions {
        if budget.stop().is_some() || rational::work() >= cap {
            return None;
        }
        let c = c.div(&d)?;
        let rounded = if t.integer {
            let base = c.floor();
            let fraction = c.frac()?;
            if fraction.cmp(&f)? == Ordering::Greater {
                base.add(&fraction.sub(&f)?.div(&om)?)?
            } else {
                base
            }
        } else if positive(&c)? {
            Rat::zero()
        } else {
            c.div(&om)?
        };
        for (j, v) in t.coefficients {
            result[j] = result[j].add(&rounded.mul(&v)?)?;
        }
        rhs = rhs.sub(&rounded.mul(&t.constant)?)?;
    }
    enclose(model, &result, &rhs, "mir")
}
pub(crate) fn separate(
    model: &Model,
    lp: &mut Lp,
    x: &[f64],
    budget: &Budget<'_>,
) -> (Vec<Cut>, Stats) {
    rational::reset_work();
    let mut stats = Stats::default();
    let mut output = Vec::new();
    let settings = budget.settings;
    let cap = settings.cut_exact_work;
    if settings.gmi_cuts {
        let gmi_cap = if settings.mir_cuts { cap / 2 } else { cap };
        let candidates = lp.tableau_candidates(model, settings.mip_cut_limit * 4);
        for (pivot, weights) in candidates {
            if budget.stop().is_some() || rational::work() >= gmi_cap {
                break;
            }
            stats.candidates += 1;
            if let Some(c) = gmi(model, pivot, &weights, x, budget, gmi_cap) {
                output.push(c);
            } else {
                stats.rejected += 1;
            }
        }
    }
    if settings.mir_cuts && rational::work() < cap {
        let vubs = variable_bounds(model);
        stats.vubs = vubs.iter().filter(|v| v.is_some()).count() as u64;
        'rows: for i in 0..model.m() {
            for sign in [1., -1.] {
                let side = if sign > 0. {
                    model.rows().upper[i]
                } else {
                    -model.rows().lower[i]
                };
                if !side.is_finite() {
                    continue;
                }
                let a = model.matrix();
                let mut divisors = vec![1.];
                for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                    let c = a.values[k].abs();
                    if c > 1e-6 && !divisors.contains(&c) {
                        divisors.push(c);
                    }
                }
                divisors.truncate(6);
                for divisor in divisors {
                    if budget.stop().is_some() || rational::work() >= cap {
                        break 'rows;
                    }
                    stats.candidates += 1;
                    if let Some(c) = mir(model, i, sign, divisor, x, &vubs, budget, cap) {
                        output.push(c);
                    } else {
                        stats.rejected += 1;
                    }
                }
            }
        }
    }
    stats.work = rational::work();
    (select(model, output, x, settings.mip_cut_limit), stats)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::*;
    use lp_ip_craft::{Bounds, CsrMatrix, ModelData, VariableKind};
    pub fn model(
        c: Vec<f64>,
        kinds: Vec<VariableKind>,
        lo: Vec<f64>,
        up: Vec<f64>,
        a: Vec<f64>,
        rl: Vec<f64>,
        ru: Vec<f64>,
    ) -> Model {
        let n = c.len();
        Model::new(ModelData {
            sense: Sense::Maximize,
            objective: c,
            objective_offset: 0.,
            variables: Bounds::new(lo, up),
            kinds,
            matrix: CsrMatrix::from_dense(rl.len(), n, &a).unwrap(),
            rows: Bounds::new(rl, ru),
        })
        .unwrap()
    }
    fn separate_at_optimum(m: &Model, s: &NativeOptions) -> Vec<Cut> {
        let cancel = CancellationToken::default();
        let options = Options::default();
        let mut budget = Budget {
            start: Instant::now(),
            settings: s,
            cancel: &cancel,
            pivots: 0,
        };
        let mut lp = Lp::new(
            m,
            &vec![true; m.m()],
            &vec![true; m.n()],
            controls(m, &options, s),
        )
        .unwrap();
        assert_eq!(lp.solve(&mut budget, true).unwrap(), Status::Optimal);
        let x = lp.point();
        eprintln!("x={x:?} tableau={:?}", lp.tableau_candidates(m, 4));
        separate(m, &mut lp, &x, &budget).0
    }
    fn feasible(m: &Model, x: &[f64]) -> bool {
        check::evaluate(m, x, &Options::default().tolerances)
            .unwrap()
            .feasible_candidate
    }
    fn valid(m: &Model, cuts: &[Cut], grid: usize) {
        let n = m.n();
        for mask in 0..grid.pow(n as u32) {
            let mut index = mask;
            let mut x = Vec::new();
            for j in 0..n {
                let k = index % grid;
                index /= grid;
                x.push(if m.kinds()[j].is_integral() {
                    m.variables().lower[j] + k as f64
                } else {
                    m.variables().lower[j]
                        + (m.variables().upper[j] - m.variables().lower[j]) * k as f64
                            / (grid - 1) as f64
                });
            }
            if !feasible(m, &x) {
                continue;
            }
            for c in cuts {
                let lhs = c.terms.iter().map(|&(j, a)| a * x[j]).sum::<f64>();
                assert!(
                    lhs <= c.rhs + 1e-9 * (1. + c.rhs.abs()),
                    "invalid {c:?} at {x:?}"
                );
            }
        }
    }
    #[test]
    fn gmi_cuts_a_fractional_integer_tableau_and_survives_negative_bounds() {
        for lower in [0., -2.] {
            let m = model(
                vec![1., 1.],
                vec![VariableKind::Integer; 2],
                vec![lower; 2],
                vec![lower + 2.; 2],
                vec![2., 2.],
                vec![f64::NEG_INFINITY],
                vec![4. * lower + 3.],
            );
            let s = NativeOptions {
                gmi_cuts: true,
                ..Default::default()
            };
            let cuts = separate_at_optimum(&m, &s);
            assert!(!cuts.is_empty());
            valid(&m, &cuts, 3);
            let mut cut = m.clone();
            append(&mut cut, &cuts).unwrap();
            let mut d = cut.data().clone();
            d.kinds.fill(VariableKind::Continuous);
            let r = NativeSolver::default()
                .solve(
                    &Model::new(d).unwrap(),
                    &Options::default(),
                    None,
                    &CancellationToken::default(),
                )
                .unwrap();
            assert!((r.evaluation.unwrap().objective - (2. * lower + 1.)).abs() < 1e-7);
        }
    }
    #[test]
    fn arbitrary_dyadic_btran_residuals_remain_in_the_exact_equality() {
        let m = model(
            vec![1., 1.],
            vec![VariableKind::Integer; 2],
            vec![0.; 2],
            vec![2.; 2],
            vec![2., 2., 1., 0.],
            vec![f64::NEG_INFINITY; 2],
            vec![3., 2.],
        );
        let s = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: Instant::now(),
            settings: &s,
            cancel: &cancel,
            pivots: 0,
        };
        for noise in [0., 1e-12, -1e-12, 0.125] {
            rational::reset_work();
            if let Some(c) = gmi(&m, 0, &[0.5, noise], &[1.5, 0.], &budget, 4_000_000) {
                valid(&m, &[c], 3);
            }
        }
    }
    #[test]
    fn vub_mir_preserves_continuous_slices_and_generates_real_cuts() {
        let mut total = 0;
        for capacity in [1.5, 2.5, 3.5, 5.5, 6.5] {
            let m = model(
                vec![1., 1., -0.5, -0.5],
                vec![
                    VariableKind::Continuous,
                    VariableKind::Continuous,
                    VariableKind::Binary,
                    VariableKind::Binary,
                ],
                vec![0.; 4],
                vec![4., 4., 1., 1.],
                vec![1., 1., 0., 0., 1., 0., -4., 0., 0., 1., 0., -4.],
                vec![f64::NEG_INFINITY; 3],
                vec![capacity, 0., 0.],
            );
            let s = NativeOptions {
                mir_cuts: true,
                ..Default::default()
            };
            let cuts = separate_at_optimum(&m, &s);
            total += cuts.len();
            valid(&m, &cuts, 5);
        }
        assert!(total > 0);
    }
    #[test]
    fn random_small_gmi_and_mir_cuts_preserve_every_integer_point() {
        let mut rng = 19u64;
        let mut total = 0;
        for _ in 0..80 {
            let mut next = || {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                rng >> 32
            };
            let a = (0..18).map(|_| (next() % 9) as f64 - 4.).collect();
            let c = (0..6).map(|_| (next() % 9) as f64 - 4.).collect();
            let upper = (0..3).map(|_| (next() % 8) as f64 + 0.5).collect();
            let m = model(
                c,
                vec![VariableKind::Integer; 6],
                vec![0.; 6],
                vec![2.; 6],
                a,
                vec![f64::NEG_INFINITY; 3],
                upper,
            );
            let s = NativeOptions {
                gmi_cuts: true,
                mir_cuts: true,
                ..Default::default()
            };
            let cuts = separate_at_optimum(&m, &s);
            total += cuts.len();
            valid(&m, &cuts, 3);
        }
        assert!(total > 20);
    }
    #[test]
    fn exact_box_compensation_protects_tiny_dropped_terms_and_negative_variables() {
        let m = model(
            vec![0., 0.],
            vec![VariableKind::Continuous; 2],
            vec![-2., -1e12],
            vec![3., 1e12],
            vec![],
            vec![],
            vec![],
        );
        let coefficients = vec![
            r(1.).unwrap().div(&r(3.).unwrap()).unwrap(),
            r(1e-13).unwrap(),
        ];
        let rhs = r(4.).unwrap();
        let cut = enclose(&m, &coefficients, &rhs, "test").unwrap();
        assert_eq!(cut.terms.len(), 1);
        assert!(cut.rhs >= 4.1);
    }
}
