//! Exact GMI/MIR derivation. Numerical multipliers are advice, not an equality.
//!
//! Every derivation is carried out in exact dyadic arithmetic
//! ([`crate::dyadic`]). Aggregation, anchoring and substitution only add and
//! multiply binary64 data, so they stay dyadic. The GMI and MIR rounding
//! functions are applied to the equation scaled to integers at a common
//! exponent, where `frac(a/p)` is the integer remainder `(a mod p)/p`; the
//! resulting inequality is multiplied through by its positive denominators,
//! so it, too, has integer coefficients. No gcd is ever needed.
use crate::{
    Budget,
    dyadic::{Dy, Int, Wide, exponent_of},
    lp::Lp,
    rational,
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

/// Sparse exact accumulator over the model columns, reused across
/// candidates so a derivation never allocates a dense vector.
pub(crate) struct Accumulator {
    values: Vec<Dy>,
    seen: Vec<bool>,
    touched: Vec<usize>,
}
impl Accumulator {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            values: vec![Dy::zero(); n],
            seen: vec![false; n],
            touched: Vec::new(),
        }
    }
    fn add(&mut self, j: usize, v: &Dy) -> Option<()> {
        if v.is_zero() {
            return Some(());
        }
        if !self.seen[j] {
            self.seen[j] = true;
            self.touched.push(j);
        }
        self.values[j] = self.values[j].add(v)?;
        Some(())
    }
    fn clear(&mut self) {
        for &j in &self.touched {
            self.values[j] = Dy::zero();
            self.seen[j] = false;
        }
        self.touched.clear();
    }
    /// Nonzero entries in column order; leaves the accumulator empty.
    fn drain(&mut self) -> Vec<(usize, Dy)> {
        self.touched.sort_unstable();
        let mut out = Vec::with_capacity(self.touched.len());
        for &j in &self.touched {
            let v = std::mem::replace(&mut self.values[j], Dy::zero());
            self.seen[j] = false;
            if !v.is_zero() {
                out.push((j, v));
            }
        }
        self.touched.clear();
        out
    }
}

fn exact(v: f64) -> Option<Dy> {
    Dy::from_f64(v)
}

/// Magnitudes below 2^-37 (about 7e-12) of the largest coefficient may be
/// dropped, when the box allows compensating for them.
const TINY_TOP: i64 = -36;

/// Converts the exact inequality `Σ c_j x_j <= rhs` to binary64.
///
/// The row is first scaled by a power of two (exactly) so its largest
/// coefficient lies in [1, 2). Each coefficient c then becomes a binary64
/// value q, either neighbour of c or zero for a tiny term, and the RHS grows
/// by the exact maximum of `(q - c) x` over the root box `[l, u]`: that is
/// `(q - c) u` for `q > c` and `(q - c) l` for `q < c`. Of the admissible
/// choices (finite endpoint) the one with the smallest compensation is kept.
/// Every point satisfying the exact row then satisfies the converted row.
pub(crate) fn enclose(
    model: &Model,
    coefficients: Vec<(usize, Dy)>,
    rhs: &Dy,
    family: &'static str,
) -> Option<Cut> {
    let top = coefficients.iter().map(|c| c.1.top()).max()?;
    let scale = 1 - top;
    let mut bound = rhs.scale2(scale);
    let mut terms = Vec::new();
    let (lower, upper) = (&model.variables().lower, &model.variables().upper);
    for (j, c) in coefficients {
        let c = c.scale2(scale);
        let compensation = |q: f64| -> Option<Dy> {
            let delta = exact(q)?.sub(&c)?;
            if delta.is_zero() {
                return Some(Dy::zero());
            }
            let endpoint = if delta.is_positive() {
                upper[j]
            } else {
                lower[j]
            };
            delta.mul(&exact(endpoint)?)
        };
        let mut choice = None;
        if c.top() < TINY_TOP {
            choice = compensation(0.).map(|d| (0., d));
        }
        if choice.is_none() && (lower[j] == 0. || upper[j] == 0.) {
            // Rounding toward a zero bound needs no compensation; the other
            // neighbour's is nonnegative (the other bound has that sign).
            let (down, up) = c.bracket()?;
            choice = Some((if lower[j] == 0. { down } else { up }, Dy::zero()));
        }
        if choice.is_none() {
            let (down, up) = c.bracket()?;
            for q in [down, up] {
                if let Some(d) = compensation(q) {
                    if choice
                        .as_ref()
                        .is_none_or(|(_, old): &(f64, Dy)| d.cmp(old) == Some(Ordering::Less))
                    {
                        choice = Some((q, d));
                    }
                }
            }
        }
        let (q, d) = choice?;
        bound = bound.add(&d)?;
        if q != 0. {
            terms.push((j, q));
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
/// Nearest finite valid bound of a variable or row activity, as
/// `(bound, sign, fixed)`: `z = sign * (v - bound) >= 0`. Integer quantities
/// use integral bounds, so z is integral too. `fixed` means `z = 0` on every
/// feasible point.
fn anchor(lo: f64, hi: f64, value: f64, integer: bool) -> Option<(f64, f64, bool)> {
    let (lo, hi) = if integer {
        (lo.ceil(), hi.floor())
    } else {
        (lo, hi)
    };
    if lo > hi {
        return None;
    }
    if lo.is_finite() && (!hi.is_finite() || value - lo <= hi - value) {
        Some((lo, 1., lo == hi))
    } else if hi.is_finite() {
        Some((hi, -1., lo == hi))
    } else {
        None
    }
}
fn signed(v: Dy, sign: f64) -> Dy {
    if sign > 0. { v } else { v.neg() }
}
fn signed_int(v: Int, sign: f64) -> Int {
    if sign > 0. { v } else { v.neg() }
}
/// Converts all values to integers at their smallest common exponent.
fn integers(values: &[&Dy]) -> Option<Vec<Int>> {
    let e = values
        .iter()
        .filter(|v| !v.is_zero())
        .map(|v| v.exponent())
        .min()
        .unwrap_or(0);
    values.iter().map(|v| v.int_at(e)).collect()
}
/// `0.005 <= f / d <= 0.995`, exactly.
fn usable_fraction(f: &Int, d: &Int) -> Option<bool> {
    let f200 = f.mul(&Int::from_u64(200))?;
    Some(
        f200.cmp(d) != Ordering::Less
            && f200.cmp(&d.mul(&Int::from_u64(199))?) != Ordering::Greater,
    )
}
/// Rounds advisory multipliers to integers on a grid relative to the
/// largest one. Any multipliers give a valid aggregation; the grid only
/// keeps the exact integers short.
fn integer_weights(weights: &[f64]) -> Option<Vec<i64>> {
    let largest = weights.iter().fold(0f64, |m, w| m.max(w.abs()));
    if !(largest > 0. && largest.is_finite()) {
        return None;
    }
    // largest lies in [2^(top-1), 2^top); scale it to [2^52, 2^53).
    let shift = 53 - exact(largest)?.top();
    let (a, b) = (shift / 2, shift - shift / 2);
    let factor = |k: i64| 2f64.powi(k as i32);
    Some(
        weights
            .iter()
            .map(|&w| {
                let k = (w * factor(a) * factor(b)).round();
                if k.is_finite() && k.abs() <= 9_007_199_254_740_992. {
                    k as i64
                } else {
                    0
                }
            })
            .collect(),
    )
}
enum Anchored {
    Column(usize, f64, Dy),
    Row(usize, f64, Dy),
}
#[allow(clippy::too_many_arguments)]
/// Exact column sums `Σ k v` for integer k and binary64 v: in place when they
/// fit [`Wide`], otherwise in [`Dy`]. Both are exact, so the result does not
/// depend on which one was used.
fn column_sums(
    terms: &[(usize, Int, f64)],
    wide: &mut Wide,
    acc: &mut Accumulator,
) -> Option<Vec<(usize, Dy)>> {
    let base = terms
        .iter()
        .filter_map(|t| exponent_of(t.2))
        .min()
        .unwrap_or(0);
    wide.reset(base);
    if terms.iter().all(|(j, k, v)| wide.add_int_scaled(*j, k, *v)) {
        return Some(wide.drain());
    }
    wide.reset(base);
    acc.clear();
    for (j, k, v) in terms {
        acc.add(*j, &Dy::from_int(k.clone()).mul(&exact(*v)?)?)?;
    }
    Some(acc.drain())
}
#[allow(clippy::too_many_arguments)]
fn gmi(
    model: &Model,
    pivot: usize,
    weights: &[f64],
    x: &[f64],
    budget: &Budget<'_>,
    cap: u64,
    acc: &mut Accumulator,
    wide: &mut Wide,
) -> Option<Cut> {
    let a = model.matrix();
    acc.clear();
    let stopped = || budget.stop().is_some() || rational::work() >= cap;
    // Exact identity Σ_j (Σ_i k_i a_ij) x_j - Σ_i k_i s_i = 0, s = A x.
    let mut used = Vec::new();
    let mut products = Vec::new();
    for (i, k) in integer_weights(weights)?.into_iter().enumerate() {
        if k == 0 {
            continue;
        }
        if stopped() {
            return None;
        }
        let w = Int::from_i64(k);
        for t in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
            products.push((a.pattern.indices[t], w.clone(), a.values[t]));
        }
        used.push((i, Dy::from_int(w)));
    }
    let aggregate = column_sums(&products, wide, acc)?;
    let p = aggregate
        .iter()
        .find(|t| t.0 == pivot)
        .map(|t| t.1.clone())?;
    // Anchor every other column and row activity: p x_p + Σ c z = beta.
    let mut beta = Dy::zero();
    let mut terms: Vec<(Dy, bool, Anchored)> = Vec::new();
    for (j, c) in aggregate {
        if j == pivot {
            continue;
        }
        let integer = model.kinds()[j].is_integral();
        let (bound, sign, fixed) = anchor(
            model.variables().lower[j],
            model.variables().upper[j],
            x[j],
            integer,
        )?;
        let bound = exact(bound)?;
        beta = beta.sub(&c.mul(&bound)?)?;
        if !fixed {
            terms.push((signed(c, sign), integer, Anchored::Column(j, sign, bound)));
        }
    }
    for (i, w) in used {
        let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        let integer = range.clone().all(|t| {
            a.values[t].fract() == 0. && model.kinds()[a.pattern.indices[t]].is_integral()
        });
        let activity = range.map(|t| a.values[t] * x[a.pattern.indices[t]]).sum();
        let (bound, sign, fixed) = anchor(
            model.rows().lower[i],
            model.rows().upper[i],
            activity,
            integer,
        )?;
        let bound = exact(bound)?;
        // -w s = -w bound - w sign z.
        beta = beta.add(&w.mul(&bound)?)?;
        if !fixed {
            terms.push((
                signed(w.neg(), sign),
                integer,
                Anchored::Row(i, sign, bound),
            ));
        }
    }
    let mut values: Vec<&Dy> = vec![&p, &beta];
    values.extend(terms.iter().map(|t| &t.0));
    let mut ints = integers(&values)?;
    if ints[0].is_negative() {
        ints.iter_mut().for_each(|v| *v = v.neg());
    }
    let (p, beta) = (ints[0].clone(), ints[1].clone());
    // x_p + Σ (c/p) z = beta/p with f0/p = frac(beta/p).
    let (_, f0) = beta.div_rem_euclid(&p)?;
    if !usable_fraction(&f0, &p)? {
        return None;
    }
    let g0 = p.sub(&f0)?;
    // GMI Σ g z >= 1 times f0 g0: integer z gets r g0 (r <= f0) or
    // (p - r) f0, continuous z gets c g0 (c > 0) or -c f0, with r = c mod p.
    let mut rhs = Dy::from_int(f0.mul(&g0)?.neg());
    let mut products = Vec::new();
    for ((_, integer, term), c) in terms.into_iter().zip(&ints[2..]) {
        if stopped() {
            return None;
        }
        let g = if integer {
            let (_, r) = c.div_rem_euclid(&p)?;
            if r.cmp(&f0) != Ordering::Greater {
                r.mul(&g0)?
            } else {
                p.sub(&r)?.mul(&f0)?
            }
        } else if c.is_positive() {
            c.mul(&g0)?
        } else {
            c.neg().mul(&f0)?
        };
        if g.is_zero() {
            continue;
        }
        // -g z <= ..., z = sign (v - bound).
        match term {
            Anchored::Column(j, sign, bound) => {
                let gs = signed_int(g, sign);
                rhs = rhs.sub(&Dy::from_int(gs.clone()).mul(&bound)?)?;
                products.push((j, gs.neg(), 1.));
            }
            Anchored::Row(i, sign, bound) => {
                let gs = signed_int(g, sign);
                rhs = rhs.sub(&Dy::from_int(gs.clone()).mul(&bound)?)?;
                let minus = gs.neg();
                for t in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                    products.push((a.pattern.indices[t], minus.clone(), a.values[t]));
                }
            }
        }
    }
    enclose(model, column_sums(&products, wide, acc)?, &rhs, "gmi")
}
#[derive(Clone)]
struct Vub {
    binary: usize,
    /// The VUB row reads `coefficient x - slope y <= 0` with both positive.
    coefficient: f64,
    slope: f64,
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
                    out[j] = Some(Vub {
                        binary: y,
                        coefficient: c,
                        slope: -d,
                    });
                }
            }
        }
    }
    out
}
/// Odd part of a positive binary64 value's mantissa.
fn odd_part(v: f64) -> u64 {
    let bits = v.to_bits();
    let fraction = bits & ((1u64 << 52) - 1);
    let mantissa = if (bits >> 52) & 2047 == 0 {
        fraction
    } else {
        fraction | (1u64 << 52)
    };
    mantissa >> mantissa.trailing_zeros()
}
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
enum MirTerm {
    Column(usize, f64, Dy),
    /// `t = slope y - coefficient x >= 0` for a VUB row on x.
    Slack(usize, usize, Dy, Dy),
}
/// One row side `Σ a x <= beta`, scaled by `scale` > 0 and with VUB
/// substitutions, anchored to `Σ c z <= beta` over nonnegative z.
struct MirBase {
    terms: Vec<(Dy, bool, MirTerm)>,
    beta: Dy,
    scale: Int,
}
fn mir_base(
    model: &Model,
    row: usize,
    sign: f64,
    x: &[f64],
    vubs: &[Option<Vub>],
    acc: &mut Accumulator,
) -> Option<MirBase> {
    let a = model.matrix();
    acc.clear();
    let range = a.pattern.indptr[row]..a.pattern.indptr[row + 1];
    let side = if sign > 0. {
        model.rows().upper[row]
    } else {
        -model.rows().lower[row]
    };
    // A VUB coefficient with odd part o leaves a/c dyadic once the row is
    // multiplied by a multiple of o; collect the least common multiple.
    let mut scale = Int::from_u64(1);
    let mut substituted = Vec::new();
    for t in range.clone() {
        let j = a.pattern.indices[t];
        if let Some(v) = &vubs[j] {
            let odd = odd_part(v.coefficient);
            let (_, r) = scale.div_rem_euclid(&Int::from_u64(odd))?;
            let factor = odd / gcd(r.to_u64()?, odd);
            let next = scale.mul(&Int::from_u64(factor))?;
            if next.bits() <= 256 {
                scale = next;
                substituted.push(j);
            }
        }
    }
    let scale_dy = Dy::from_int(scale.clone());
    let mut slacks = Vec::new();
    for t in range {
        let j = a.pattern.indices[t];
        let c = exact(sign * a.values[t])?.mul(&scale_dy)?;
        match &vubs[j] {
            Some(v) if substituted.contains(&j) => {
                // c x = (c / coef) (slope y - t).
                let coefficient = exact(v.coefficient)?;
                let slope = exact(v.slope)?;
                let ratio = c.div_exact(&coefficient)?;
                acc.add(v.binary, &ratio.mul(&slope)?)?;
                slacks.push((
                    ratio.neg(),
                    false,
                    MirTerm::Slack(j, v.binary, slope, coefficient),
                ));
            }
            _ => acc.add(j, &c)?,
        }
    }
    let mut beta = exact(side)?.mul(&scale_dy)?;
    let mut terms = Vec::new();
    for (j, c) in acc.drain() {
        let integer = model.kinds()[j].is_integral();
        let (bound, s, fixed) = anchor(
            model.variables().lower[j],
            model.variables().upper[j],
            x[j],
            integer,
        )?;
        let bound = exact(bound)?;
        beta = beta.sub(&c.mul(&bound)?)?;
        if !fixed {
            terms.push((signed(c, s), integer, MirTerm::Column(j, s, bound)));
        }
    }
    if !terms.iter().any(|t| t.1) {
        return None;
    }
    terms.extend(slacks);
    Some(MirBase { terms, beta, scale })
}
fn mir(
    model: &Model,
    base: &MirBase,
    divisor: f64,
    budget: &Budget<'_>,
    cap: u64,
    acc: &mut Accumulator,
) -> Option<Cut> {
    acc.clear();
    let d = exact(divisor)?.mul_int(&base.scale)?;
    let mut values: Vec<&Dy> = vec![&d, &base.beta];
    values.extend(base.terms.iter().map(|t| &t.0));
    let ints = integers(&values)?;
    let (d, beta) = (&ints[0], &ints[1]);
    let (q0, f0) = beta.div_rem_euclid(d)?;
    if !usable_fraction(&f0, d)? {
        return None;
    }
    let g0 = d.sub(&f0)?;
    // MIR times g0 = d (1 - f): integer c -> floor(c/d) g0 + max(0, r - f0),
    // continuous c -> min(0, c), RHS floor(beta/d) g0.
    let mut rhs = Dy::from_int(q0.mul(&g0)?);
    for ((_, integer, term), c) in base.terms.iter().zip(&ints[2..]) {
        if budget.stop().is_some() || rational::work() >= cap {
            return None;
        }
        let h = if *integer {
            let (q, r) = c.div_rem_euclid(d)?;
            let extra = r.sub(&f0)?;
            let rounded = q.mul(&g0)?;
            if extra.is_positive() {
                rounded.add(&extra)?
            } else {
                rounded
            }
        } else if c.is_negative() {
            c.clone()
        } else {
            continue;
        };
        if h.is_zero() {
            continue;
        }
        let h = Dy::from_int(h);
        match term {
            MirTerm::Column(j, sign, bound) => {
                let hs = signed(h, *sign);
                acc.add(*j, &hs)?;
                rhs = rhs.add(&hs.mul(bound)?)?;
            }
            MirTerm::Slack(j, y, slope, coefficient) => {
                acc.add(*y, &h.mul(slope)?)?;
                acc.add(*j, &h.mul(coefficient)?.neg())?;
            }
        }
    }
    enclose(model, acc.drain(), &rhs, "mir")
}
/// Separates GMI cuts from the optimal tableau and MIR cuts from every row,
/// including earlier cuts (rank-2 MIR closes most of gt2's root gap).
/// Separates the active families, `[gmi, mir]`. With both active, GMI may
/// use half the work budget, so it cannot starve MIR.
pub(crate) fn separate(
    model: &Model,
    lp: &mut Lp,
    x: &[f64],
    budget: &Budget<'_>,
    families: [bool; 2],
) -> (Vec<Cut>, Stats) {
    rational::reset_work();
    let mut stats = Stats::default();
    let mut output = Vec::new();
    let settings = budget.settings;
    let cap = settings.cut_exact_work;
    let mut acc = Accumulator::new(model.n());
    let mut wide = Wide::new(model.n());
    if families[0] {
        let gmi_cap = if families[1] { cap / 2 } else { cap };
        let candidates = lp.tableau_candidates(model, settings.mip_cut_limit * 4);
        for (pivot, weights) in candidates {
            if budget.stop().is_some() || rational::work() >= gmi_cap {
                break;
            }
            stats.candidates += 1;
            if let Some(c) = gmi(
                model, pivot, &weights, x, budget, gmi_cap, &mut acc, &mut wide,
            ) {
                output.push(c);
            } else {
                stats.rejected += 1;
            }
        }
    }
    if families[1] && rational::work() < cap {
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
                if budget.stop().is_some() || rational::work() >= cap {
                    break 'rows;
                }
                let base = mir_base(model, i, sign, x, &vubs, &mut acc);
                for divisor in divisors {
                    if budget.stop().is_some() || rational::work() >= cap {
                        break 'rows;
                    }
                    stats.candidates += 1;
                    match base
                        .as_ref()
                        .and_then(|b| mir(model, b, divisor, budget, cap, &mut acc))
                    {
                        Some(c) => output.push(c),
                        None => stats.rejected += 1,
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

    /// The 0.27.0 derivation in gcd-normalized rationals, kept as an
    /// independent oracle for the dyadic implementation.
    #[allow(clippy::too_many_arguments)]
    mod reference {
        use super::super::Cut;
        use crate::{
            Budget,
            rational::{self, Rat},
        };
        use lp_ip_craft::Model;
        use std::cmp::Ordering;
        fn r(v: f64) -> Option<Rat> {
            Rat::from(v)
        }
        fn positive(v: &Rat) -> Option<bool> {
            Some(v.cmp(&Rat::zero())? == Ordering::Greater)
        }
        /// Relax every coefficient conversion over the finite root box, exactly.
        pub(super) fn enclose(
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
        pub(super) fn gmi(
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
        pub(super) struct Vub {
            binary: usize,
            factor: Rat,
        }
        pub(super) fn variable_bounds(model: &Model) -> Vec<Option<Vub>> {
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
        pub(super) fn mir(
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
    }

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
    fn optimum(m: &Model, s: &NativeOptions) -> (Lp, Vec<f64>) {
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
        (lp, x)
    }
    fn separate_at_optimum(m: &Model, s: &NativeOptions) -> Vec<Cut> {
        let cancel = CancellationToken::default();
        let (mut lp, x) = optimum(m, s);
        let budget = Budget {
            start: Instant::now(),
            settings: s,
            cancel: &cancel,
            pivots: 0,
        };
        separate(m, &mut lp, &x, &budget, [s.gmi_cuts, s.mir_cuts]).0
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
        let mut acc = Accumulator::new(m.n());
        let mut found = 0;
        for noise in [0., 1e-12, -1e-12, 0.125, 1e-30] {
            rational::reset_work();
            if let Some(c) = gmi(
                &m,
                0,
                &[0.5, noise],
                &[1.5, 0.],
                &budget,
                4_000_000,
                &mut acc,
                &mut Wide::new(m.n()),
            ) {
                valid(&m, &[c], 3);
                found += 1;
            }
        }
        assert!(found >= 3);
    }
    #[test]
    fn vub_mir_preserves_continuous_slices_and_generates_real_cuts() {
        let mut total = 0;
        // Unit and non-dyadic VUB coefficients (3 x <= 12 y has a/c = 1/3).
        for coefficient in [1., 3., 0.75] {
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
                    vec![
                        1.,
                        1.,
                        0.,
                        0.,
                        coefficient,
                        0.,
                        -4. * coefficient,
                        0.,
                        0.,
                        coefficient,
                        0.,
                        -4. * coefficient,
                    ],
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
    /// Mixed models with fractional coefficients, negative and complemented
    /// bounds, equality and ranged rows, fixed columns and VUB rows.
    fn random_mixed(seed: u64) -> Model {
        let mut rng = seed;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let n = 5;
        let kinds: Vec<_> = (0..n)
            .map(|j| match (next() + j as u64) % 3 {
                0 => VariableKind::Continuous,
                1 => VariableKind::Binary,
                _ => VariableKind::Integer,
            })
            .collect();
        let mut lo = Vec::new();
        let mut up = Vec::new();
        for kind in &kinds {
            let (l, u) = match kind {
                VariableKind::Binary => (0., if next() % 9 == 0 { 0. } else { 1. }),
                VariableKind::Integer => {
                    let l = (next() % 4) as f64 - 2.;
                    (l, l + (next() % 3) as f64)
                }
                VariableKind::Continuous => {
                    let l = (next() % 5) as f64 * 0.5 - 1.;
                    (l, l + 0.75 + (next() % 4) as f64 * 0.5)
                }
            };
            lo.push(l);
            up.push(u);
        }
        let mut a = Vec::new();
        let mut rl = Vec::new();
        let mut ru = Vec::new();
        for _ in 0..3 {
            let mut activity = [0f64; 2];
            for j in 0..n {
                let v = match next() % 6 {
                    0 | 1 => 0.,
                    2 => (next() % 7) as f64 - 3.,
                    3 => ((next() % 13) as f64 - 6.) * 0.375,
                    4 => ((next() % 11) as f64 - 5.) / 3.,
                    _ => ((next() % 9) as f64 - 4.) * 1.25,
                };
                a.push(v);
                activity[0] += (v * lo[j]).min(v * up[j]);
                activity[1] += (v * lo[j]).max(v * up[j]);
            }
            let mid = activity[0] + (activity[1] - activity[0]) * ((next() % 5) as f64 + 1.) / 7.;
            match next() % 4 {
                0 => {
                    rl.push(f64::NEG_INFINITY);
                    ru.push(mid);
                }
                1 => {
                    rl.push(mid);
                    ru.push(f64::INFINITY);
                }
                2 => {
                    rl.push(mid - 0.5);
                    ru.push(mid + 0.25);
                }
                _ => {
                    rl.push(f64::NEG_INFINITY);
                    ru.push(mid + 0.1);
                }
            }
        }
        // A VUB row on the first continuous column, when a binary exists.
        if let (Some(x), Some(y)) = (
            kinds.iter().position(|k| *k == VariableKind::Continuous),
            kinds.iter().position(|k| *k == VariableKind::Binary),
        ) {
            if lo[x] >= 0. && up[y] == 1. {
                let c = [1., 3., 0.5][(next() % 3) as usize];
                let mut row = vec![0.; n];
                row[x] = c;
                row[y] = -c * up[x];
                a.extend(row);
                rl.push(f64::NEG_INFINITY);
                ru.push(0.);
            }
        }
        let c = (0..n).map(|_| (next() % 9) as f64 - 4.).collect();
        model(c, kinds, lo, up, a, rl, ru)
    }
    #[test]
    fn random_mixed_cuts_preserve_every_feasible_grid_point() {
        let mut total = [0usize; 2];
        for seed in 1..400u64 {
            let m = random_mixed(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let lp_model = {
                let mut d = m.data().clone();
                d.kinds.fill(VariableKind::Continuous);
                Model::new(d).unwrap()
            };
            let cancel = CancellationToken::default();
            let s = NativeOptions {
                gmi_cuts: true,
                mir_cuts: true,
                ..Default::default()
            };
            let r = NativeSolver::default()
                .solve(&lp_model, &Options::default(), None, &cancel)
                .unwrap();
            if r.status != Status::Optimal {
                continue;
            }
            let cuts = separate_at_optimum(&m, &s);
            for c in &cuts {
                total[usize::from(c.family == "mir")] += 1;
            }
            valid(&m, &cuts, 5);
        }
        assert!(total[0] > 20 && total[1] > 20, "{total:?}");
    }
    /// Same base equation and rounding function as the rational 0.27.0 code:
    /// both cuts must agree up to a positive factor, apart from rounding.
    fn proportional(a: &Cut, b: &Cut) {
        let scale = |c: &Cut| c.terms.iter().map(|t| t.1.abs()).fold(0., f64::max);
        let (sa, sb) = (scale(a), scale(b));
        let dense = |c: &Cut, s: f64| {
            let mut v = std::collections::BTreeMap::new();
            for &(j, x) in &c.terms {
                if (x / s).abs() > 1e-9 {
                    v.insert(j, x / s);
                }
            }
            v
        };
        let (da, db) = (dense(a, sa), dense(b, sb));
        assert_eq!(
            da.keys().collect::<Vec<_>>(),
            db.keys().collect::<Vec<_>>(),
            "{a:?} {b:?}"
        );
        for (j, x) in &da {
            assert!((x - db[j]).abs() <= 1e-9, "{a:?} {b:?}");
        }
        assert!(
            (a.rhs / sa - b.rhs / sb).abs() <= 1e-8 * (1. + (a.rhs / sa).abs()),
            "{a:?} {b:?}"
        );
    }
    #[test]
    fn dyadic_derivation_matches_the_rational_reference() {
        let s = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: Instant::now(),
            settings: &s,
            cancel: &cancel,
            pivots: 0,
        };
        let mut compared = [0usize; 2];
        let mut rng = 7u64;
        for case in 0..300 {
            let mut next = || {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                rng >> 33
            };
            // Inequality rows and unfixed boxes only: there both derivations
            // keep exactly the same terms.
            let kinds = (0..5)
                .map(|j| {
                    if (j + case) % 4 == 0 {
                        VariableKind::Continuous
                    } else {
                        VariableKind::Integer
                    }
                })
                .collect();
            let a = (0..15)
                .map(|_| ((next() % 17) as f64 - 8.) * 0.75)
                .collect();
            let rhs = (0..3).map(|_| (next() % 9) as f64 + 0.3).collect();
            let m = model(
                (0..5).map(|_| (next() % 7) as f64 - 2.).collect(),
                kinds,
                vec![-1.; 5],
                vec![2.; 5],
                a,
                vec![f64::NEG_INFINITY; 3],
                rhs,
            );
            let (mut lp, x) = optimum(&m, &s);
            let mut acc = Accumulator::new(m.n());
            for (pivot, weights) in lp.tableau_candidates(&m, 8) {
                rational::reset_work();
                let theirs = reference::gmi(&m, pivot, &weights, &x, &budget, 100_000_000);
                rational::reset_work();
                let ours = gmi(
                    &m,
                    pivot,
                    &weights,
                    &x,
                    &budget,
                    100_000_000,
                    &mut acc,
                    &mut Wide::new(m.n()),
                );
                if let Some(theirs) = theirs {
                    proportional(ours.as_ref().expect("dyadic GMI rejected"), &theirs);
                    compared[0] += 1;
                }
            }
            let vubs = variable_bounds(&m);
            let old_vubs = reference::variable_bounds(&m);
            for row in 0..m.m() {
                let base = mir_base(&m, row, 1., &x, &vubs, &mut acc);
                for divisor in [1., 0.75, 1.5, 3.] {
                    rational::reset_work();
                    let theirs =
                        reference::mir(&m, row, 1., divisor, &x, &old_vubs, &budget, 100_000_000);
                    let ours = base
                        .as_ref()
                        .and_then(|b| mir(&m, b, divisor, &budget, 100_000_000, &mut acc));
                    if let Some(theirs) = theirs {
                        proportional(ours.as_ref().expect("dyadic MIR rejected"), &theirs);
                        compared[1] += 1;
                    }
                }
            }
        }
        assert!(compared[0] > 100 && compared[1] > 100, "{compared:?}");
    }
    #[test]
    fn in_place_column_sums_equal_dyadic_sums_and_fall_back_when_too_wide() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (mut wide, mut acc) = (Wide::new(6), Accumulator::new(6));
        for case in 0..400 {
            let mut terms = Vec::new();
            for _ in 0..(next() % 40) {
                let j = (next() % 6) as usize;
                // Big multipliers on the inline/limb boundary and beyond.
                let mut k = Int::from_i64((next() as i64) >> (next() % 60));
                for _ in 0..(next() % 4) {
                    k = k.shl(60).unwrap().add(&Int::from_u64(next())).unwrap();
                }
                let spread = if case % 10 == 0 { 1500 } else { 60 };
                let v = f64::from_bits(next() >> 2) * 2f64.powi((next() % spread) as i32 - 30);
                let v = if next() % 2 == 0 { v } else { -v };
                if v.is_finite() {
                    terms.push((j, k, v));
                }
            }
            let mut expected = Accumulator::new(6);
            for (j, k, v) in &terms {
                expected
                    .add(
                        *j,
                        &Dy::from_int(k.clone())
                            .mul(&Dy::from_f64(*v).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
            }
            let got = column_sums(&terms, &mut wide, &mut acc).unwrap();
            assert_eq!(got, expected.drain(), "case {case}");
        }
        // Cancellation to exactly zero leaves no column.
        let one = Int::from_u64(1);
        let terms = vec![
            (2, one.clone(), 0.75),
            (2, one.neg(), 0.75),
            (3, one, 1e-300),
        ];
        assert_eq!(
            column_sums(&terms, &mut wide, &mut acc).unwrap(),
            vec![(3, Dy::from_f64(1e-300).unwrap())]
        );
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
        let d = |v: f64| Dy::from_f64(v).unwrap();
        // 1 + 2^-60 is not representable; 1e-13 is dropped against |x| <= 1e12.
        let coefficients = vec![(0, d(1.).add(&d(2f64.powi(-60))).unwrap()), (1, d(1e-13))];
        let cut = enclose(&m, coefficients, &d(4.), "test").unwrap();
        assert_eq!(cut.terms.len(), 1);
        assert!(cut.rhs >= 4.1);
        // With a one-sided box only the admissible neighbour is used.
        let m = model(
            vec![0.],
            vec![VariableKind::Continuous],
            vec![0.],
            vec![f64::INFINITY],
            vec![],
            vec![],
            vec![],
        );
        let c = d(1.).add(&d(2f64.powi(-60))).unwrap();
        let cut = enclose(&m, vec![(0, c)], &d(4.), "test").unwrap();
        assert_eq!(cut.terms, vec![(0, 1.)]);
        assert_eq!(cut.rhs, 4.);
        let m = model(
            vec![0.],
            vec![VariableKind::Continuous],
            vec![f64::NEG_INFINITY],
            vec![f64::INFINITY],
            vec![],
            vec![],
            vec![],
        );
        let c = d(1.).add(&d(2f64.powi(-60))).unwrap();
        assert!(enclose(&m, vec![(0, c)], &d(4.), "test").is_none());
    }
}
