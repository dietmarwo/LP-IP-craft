//! Conservative incumbent checks used for pruning, separate from tolerance
//! acceptance. Equality rows may prevent certification; that disables pruning.
use lp_ip_craft::{Model, Sense};
#[inline]
fn down(v: f64) -> f64 {
    if v == 0.0 {
        -f64::from_bits(1)
    } else if v > 0.0 {
        f64::from_bits(v.to_bits() - 1)
    } else {
        f64::from_bits(v.to_bits() + 1)
    }
}
#[inline]
fn enclose(v: f64, exact: bool) -> Option<(f64, f64)> {
    let pair = if exact { (v, v) } else { (down(v), -down(-v)) };
    (pair.0.is_finite() && pair.1.is_finite()).then_some(pair)
}
/// An integer of magnitude at most 2^50. For such |v| the i64 conversion is
/// exact truncation, so this equals `v.fract() == 0.0` without a libm call
/// (baseline x86-64 has no rounding instruction); NaN and infinities fail.
#[inline]
fn small_integer(v: f64) -> bool {
    v.abs() <= 1_125_899_906_842_624.0 && (v as i64) as f64 == v
}
/// Outward enclosure of `a * b`; None if not finite.
#[inline]
pub(crate) fn product(a: f64, b: f64) -> Option<(f64, f64)> {
    let v = a * b;
    enclose(
        v,
        a == 0.0
            || b == 0.0
            || a.abs() == 1.0
            || b.abs() == 1.0
            || (small_integer(a) && small_integer(b) && small_integer(v)),
    )
}
/// Outward enclosure of `a + b`; None if not finite.
#[inline]
pub(crate) fn add(a: f64, b: f64) -> Option<(f64, f64)> {
    let v = a + b;
    enclose(
        v,
        a == 0.0
            || b == 0.0
            || a == -b
            || (small_integer(a) && small_integer(b) && small_integer(v)),
    )
}
#[inline]
pub(crate) fn dot(terms: impl Iterator<Item = (f64, f64)>, offset: f64) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (offset, offset);
    for (a, b) in terms {
        let (l, u) = product(a, b)?;
        lo = add(lo, l)?.0;
        hi = add(hi, u)?.1;
    }
    Some((lo, hi))
}
/// A conservative objective upper endpoint in minimization orientation, only
/// if all original constraints are satisfied by the entire activity enclosure.
pub(crate) fn upper(model: &Model, x: &[f64]) -> Option<f64> {
    upper_with_bounds(model, model.variables(), x)
}
pub(crate) fn upper_with_bounds(
    model: &Model,
    bounds: &lp_ip_craft::Bounds,
    x: &[f64],
) -> Option<f64> {
    for (j, &v) in x.iter().enumerate() {
        if !v.is_finite()
            || v < bounds.lower[j]
            || v > bounds.upper[j]
            || (model.kinds()[j].is_integral() && v.fract() != 0.0)
        {
            return None;
        }
    }
    let a = model.matrix();
    for i in 0..model.m() {
        let (lo, hi) = dot(
            (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
                .map(|k| (a.values[k], x[a.pattern.indices[k]])),
            0.0,
        )?;
        if lo < model.rows().lower[i] || hi > model.rows().upper[i] {
            return None;
        }
    }
    let s = if model.sense() == Sense::Minimize {
        1.0
    } else {
        -1.0
    };
    Some(
        dot(
            model.objective().iter().zip(x).map(|(&c, &v)| (s * c, v)),
            s * model.objective_offset(),
        )?
        .1,
    )
}
/// Sufficient conditions for an exactly representable integral objective.
pub(crate) fn integer_lattice(model: &Model) -> bool {
    model.objective_offset() == 0.0
        && model.kinds().iter().all(|k| k.is_integral())
        && model.objective().iter().all(|&v| small_integer(v))
        && model
            .objective()
            .iter()
            .enumerate()
            .map(|(j, &c)| {
                c.abs()
                    * model.variables().lower[j]
                        .abs()
                        .max(model.variables().upper[j].abs())
            })
            .sum::<f64>()
            < 562_949_953_421_312.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elementary_enclosures() {
        assert_eq!(product(3., 7.), Some((21., 21.)));
        let x = product(0.1, 0.2).unwrap();
        assert!(x.0 <= 0.02 && x.1 >= 0.02 && x.0 < x.1);
        assert!(product(f64::MAX, 2.).is_none());
        assert_eq!(add(2., -2.), Some((0., 0.)));
    }
}

#[cfg(test)]
mod small_integer_tests {
    use super::small_integer;
    #[test]
    fn matches_fract_definition() {
        let limit = 1_125_899_906_842_624.0;
        let mut values = vec![
            0.0,
            -0.0,
            0.5,
            -0.5,
            1.0,
            -1.0,
            limit,
            -limit,
            limit + 1.0,
            limit - 0.5,
            limit * 2.0,
            9.007_199_254_740_993e15,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ];
        let mut seed = 5u64;
        for _ in 0..20000 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let v = f64::from_bits(seed);
            values.push(v);
            values.push(((seed >> 11) % 4096) as f64 / 8.0 - 256.0);
            values.push((seed >> 13) as f64);
        }
        for v in values {
            assert_eq!(
                small_integer(v),
                v.fract() == 0.0 && v.abs() <= limit,
                "{v:e}"
            );
        }
    }
}
