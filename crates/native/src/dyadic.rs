//! Exact signed integers and dyadic rationals `m * 2^e` for cut derivation.
//!
//! Every binary64 value is a dyadic rational, and sums and products of
//! dyadics stay dyadic, so the aggregation and substitution steps of the
//! certified cuts need no gcd normalization. The only divisions are the
//! integer quotient/remainder steps of the GMI and MIR rounding functions.
//! All results are exact or the operation fails closed (`None`) when a
//! magnitude would exceed the shared limb limit of [`crate::rational`].
//! Products, quotients, sums and shifts charge the shared per-thread work
//! counter, so callers keep their deterministic limb-operation budgets.
use crate::rational::{Nat, charge};
use std::cmp::Ordering;

/// Largest magnitude in bits, shared with `rational::Rat`.
const BITS: usize = 8192;

/// Signed arbitrary-precision integer. Values below 2^127 in magnitude are
/// held inline, larger ones as limbs; the form is canonical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Int(Repr);
#[derive(Clone, Debug, PartialEq, Eq)]
enum Repr {
    /// Never `i128::MIN`, so negation cannot overflow.
    Small(i128),
    /// Magnitude of at least 2^127.
    Big { negative: bool, magnitude: Nat },
}

impl Int {
    pub(crate) const fn zero() -> Self {
        Self(Repr::Small(0))
    }
    fn small(v: i128) -> Option<Self> {
        (v != i128::MIN).then_some(Self(Repr::Small(v)))
    }
    fn from_parts(negative: bool, magnitude: Nat) -> Self {
        if magnitude.bits() <= 127 {
            let v = magnitude.low_u128() as i128;
            return Self(Repr::Small(if negative { -v } else { v }));
        }
        Self(Repr::Big {
            negative,
            magnitude,
        })
    }
    /// Sign and magnitude, materialized for the limb path.
    fn parts(&self) -> (bool, Nat) {
        match &self.0 {
            Repr::Small(v) => (*v < 0, Nat::from_u128(v.unsigned_abs())),
            Repr::Big {
                negative,
                magnitude,
            } => (*negative, magnitude.clone()),
        }
    }
    fn limbs(&self) -> usize {
        match &self.0 {
            Repr::Small(_) => 4,
            Repr::Big { magnitude, .. } => magnitude.limbs(),
        }
    }
    pub(crate) fn from_u64(v: u64) -> Self {
        Self(Repr::Small(i128::from(v)))
    }
    pub(crate) fn from_i64(v: i64) -> Self {
        Self(Repr::Small(i128::from(v)))
    }
    pub(crate) fn is_zero(&self) -> bool {
        self.0 == Repr::Small(0)
    }
    pub(crate) fn is_negative(&self) -> bool {
        match &self.0 {
            Repr::Small(v) => *v < 0,
            Repr::Big { negative, .. } => *negative,
        }
    }
    pub(crate) fn is_positive(&self) -> bool {
        !self.is_negative() && !self.is_zero()
    }
    pub(crate) fn bits(&self) -> usize {
        match &self.0 {
            Repr::Small(v) => 128 - v.unsigned_abs().leading_zeros() as usize,
            Repr::Big { magnitude, .. } => magnitude.bits(),
        }
    }
    /// The value, when it is a nonnegative integer below 2^64.
    pub(crate) fn to_u64(&self) -> Option<u64> {
        match &self.0 {
            Repr::Small(v) => u64::try_from(*v).ok(),
            Repr::Big { .. } => None,
        }
    }
    /// The low 64 bits of `|self| >> k`.
    fn magnitude_bits_from(&self, k: usize) -> u64 {
        match &self.0 {
            Repr::Small(v) => (v.unsigned_abs().checked_shr(k as u32).unwrap_or(0)) as u64,
            Repr::Big { magnitude, .. } => magnitude.shr(k).low_u64(),
        }
    }
    /// Little-endian 64-bit limbs of the magnitude.
    fn magnitude_u64(&self) -> Vec<u64> {
        match &self.0 {
            Repr::Small(v) => {
                let m = v.unsigned_abs();
                vec![m as u64, (m >> 64) as u64]
            }
            Repr::Big { magnitude, .. } => magnitude
                .digits()
                .chunks(2)
                .map(|c| u64::from(c[0]) | (u64::from(c.get(1).copied().unwrap_or(0)) << 32))
                .collect(),
        }
    }
    pub(crate) fn abs(&self) -> Self {
        if self.is_negative() {
            self.neg()
        } else {
            self.clone()
        }
    }
    pub(crate) fn neg(&self) -> Self {
        match &self.0 {
            Repr::Small(v) => Self(Repr::Small(-v)),
            Repr::Big {
                negative,
                magnitude,
            } => Self(Repr::Big {
                negative: !negative,
                magnitude: magnitude.clone(),
            }),
        }
    }
    pub(crate) fn add(&self, b: &Self) -> Option<Self> {
        if let (Repr::Small(x), Repr::Small(y)) = (&self.0, &b.0) {
            charge(1);
            if let Some(v) = x.checked_add(*y).and_then(Self::small) {
                return Some(v);
            }
        }
        if self.is_zero() {
            return Some(b.clone());
        }
        if b.is_zero() {
            return Some(self.clone());
        }
        charge(self.limbs().max(b.limbs()) + 1);
        let ((sa, ma), (sb, mb)) = (self.parts(), b.parts());
        let out = if sa == sb {
            Self::from_parts(sa, ma.add(&mb))
        } else {
            match ma.cmp(&mb) {
                Ordering::Less => Self::from_parts(sb, mb.sub(&ma)),
                _ => Self::from_parts(sa, ma.sub(&mb)),
            }
        };
        (out.bits() <= BITS).then_some(out)
    }
    pub(crate) fn sub(&self, b: &Self) -> Option<Self> {
        self.add(&b.neg())
    }
    pub(crate) fn mul(&self, b: &Self) -> Option<Self> {
        if let (Repr::Small(x), Repr::Small(y)) = (&self.0, &b.0) {
            charge(1);
            if let Some(v) = x.checked_mul(*y).and_then(Self::small) {
                return Some(v);
            }
        }
        if self.is_zero() || b.is_zero() {
            return Some(Self::zero());
        }
        let ((sa, ma), (sb, mb)) = (self.parts(), b.parts());
        Some(Self::from_parts(sa != sb, ma.mul(&mb)?))
    }
    /// `self * 2^k`.
    pub(crate) fn shl(&self, k: usize) -> Option<Self> {
        if self.is_zero() || k == 0 {
            return Some(self.clone());
        }
        if self.bits() + k > BITS {
            return None;
        }
        if let Repr::Small(v) = &self.0 {
            if self.bits() + k <= 126 {
                charge(1);
                return Some(Self(Repr::Small(v << k)));
            }
        }
        charge(self.limbs() + k / 32 + 1);
        let (negative, magnitude) = self.parts();
        Some(Self::from_parts(negative, magnitude.shift(k)?))
    }
    fn trailing_zeros(&self) -> usize {
        match &self.0 {
            Repr::Small(0) => 0,
            Repr::Small(v) => v.trailing_zeros() as usize,
            Repr::Big { magnitude, .. } => magnitude.trailing_zeros(),
        }
    }
    /// `self / 2^k` for a value with at least k low zero bits.
    fn shr_exact(&self, k: usize) -> Self {
        debug_assert!(self.trailing_zeros() >= k || self.is_zero());
        match &self.0 {
            Repr::Small(v) => Self(Repr::Small(v >> k)),
            Repr::Big {
                negative,
                magnitude,
            } => Self::from_parts(*negative, magnitude.shr(k)),
        }
    }
    /// Euclidean division by a positive divisor: `self = q d + r`, `0 <= r < d`.
    pub(crate) fn div_rem_euclid(&self, d: &Self) -> Option<(Self, Self)> {
        if !d.is_positive() {
            return None;
        }
        if let (Repr::Small(a), Repr::Small(b)) = (&self.0, &d.0) {
            charge(1);
            return Some((
                Self(Repr::Small(a.div_euclid(*b))),
                Self(Repr::Small(a.rem_euclid(*b))),
            ));
        }
        let ((negative, a), (_, b)) = (self.parts(), d.parts());
        let (q, r) = a.div_rem(&b);
        if !negative {
            return Some((Self::from_parts(false, q), Self::from_parts(false, r)));
        }
        if r.zero() {
            return Some((Self::from_parts(true, q), Self::zero()));
        }
        Some((
            Self::from_parts(true, q.add(&Nat::small(1))),
            Self::from_parts(false, b.sub(&r)),
        ))
    }
    pub(crate) fn cmp(&self, b: &Self) -> Ordering {
        match (&self.0, &b.0) {
            (Repr::Small(x), Repr::Small(y)) => x.cmp(y),
            // A big value exceeds every small one in magnitude.
            (Repr::Big { negative, .. }, Repr::Small(_)) => {
                if *negative {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (Repr::Small(_), Repr::Big { negative, .. }) => {
                if *negative {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (
                Repr::Big {
                    negative: na,
                    magnitude: ma,
                },
                Repr::Big {
                    negative: nb,
                    magnitude: mb,
                },
            ) => match (na, nb) {
                (false, true) => Ordering::Greater,
                (true, false) => Ordering::Less,
                (false, false) => ma.cmp(mb),
                (true, true) => mb.cmp(ma),
            },
        }
    }
}

/// Exact dyadic rational `m * 2^e`, normalized so that m is odd (or zero
/// with e = 0). Equal values therefore have equal representations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dy {
    m: Int,
    e: i64,
}

impl Dy {
    pub(crate) const fn zero() -> Self {
        Self {
            m: Int::zero(),
            e: 0,
        }
    }
    pub(crate) fn new(m: Int, e: i64) -> Self {
        if m.is_zero() {
            return Self::zero();
        }
        let tz = m.trailing_zeros();
        if tz == 0 {
            Self { m, e }
        } else {
            Self {
                m: m.shr_exact(tz),
                e: e + tz as i64,
            }
        }
    }
    pub(crate) fn from_int(m: Int) -> Self {
        Self::new(m, 0)
    }
    /// The exact value of a finite binary64 number.
    pub(crate) fn from_f64(v: f64) -> Option<Self> {
        if !v.is_finite() {
            return None;
        }
        let bits = v.to_bits();
        let biased = ((bits >> 52) & 2047) as i64;
        let fraction = bits & ((1u64 << 52) - 1);
        let mantissa = if biased == 0 {
            fraction
        } else {
            fraction | (1u64 << 52)
        };
        if mantissa == 0 {
            return Some(Self::zero());
        }
        let exponent = if biased == 0 { -1074 } else { biased - 1075 };
        let m = Int::from_u64(mantissa);
        Some(Self::new(if v < 0. { m.neg() } else { m }, exponent))
    }
    pub(crate) fn is_zero(&self) -> bool {
        self.m.is_zero()
    }
    pub(crate) fn is_negative(&self) -> bool {
        self.m.is_negative()
    }
    pub(crate) fn is_positive(&self) -> bool {
        self.m.is_positive()
    }
    pub(crate) fn exponent(&self) -> i64 {
        self.e
    }
    /// For nonzero values, `2^(top-1) <= |self| < 2^top`.
    pub(crate) fn top(&self) -> i64 {
        self.m.bits() as i64 + self.e
    }
    pub(crate) fn neg(&self) -> Self {
        Self {
            m: self.m.neg(),
            e: self.e,
        }
    }
    /// `self * 2^k`.
    pub(crate) fn scale2(&self, k: i64) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        Self {
            m: self.m.clone(),
            e: self.e + k,
        }
    }
    pub(crate) fn add(&self, b: &Self) -> Option<Self> {
        if self.is_zero() {
            return Some(b.clone());
        }
        if b.is_zero() {
            return Some(self.clone());
        }
        let e = self.e.min(b.e);
        let shift = |v: &Self| -> Option<Int> {
            let k = usize::try_from(v.e - e).ok()?;
            v.m.shl(k)
        };
        Some(Self::new(shift(self)?.add(&shift(b)?)?, e))
    }
    pub(crate) fn sub(&self, b: &Self) -> Option<Self> {
        self.add(&b.neg())
    }
    pub(crate) fn mul(&self, b: &Self) -> Option<Self> {
        if self.is_zero() || b.is_zero() {
            return Some(Self::zero());
        }
        Some(Self {
            m: self.m.mul(&b.m)?,
            e: self.e.checked_add(b.e)?,
        })
    }
    pub(crate) fn mul_int(&self, b: &Int) -> Option<Self> {
        self.mul(&Self::from_int(b.clone()))
    }
    /// `self / d`, only when the quotient is itself dyadic (the odd part of
    /// d divides the odd part of self); otherwise `None`.
    pub(crate) fn div_exact(&self, d: &Self) -> Option<Self> {
        if d.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(Self::zero());
        }
        let (q, r) = self.m.abs().div_rem_euclid(&d.m.abs())?;
        if !r.is_zero() {
            return None;
        }
        let q = if self.is_negative() != d.is_negative() {
            q.neg()
        } else {
            q
        };
        Some(Self::new(q, self.e.checked_sub(d.e)?))
    }
    pub(crate) fn cmp(&self, b: &Self) -> Option<Ordering> {
        let d = self.sub(b)?;
        Some(if d.is_zero() {
            Ordering::Equal
        } else if d.is_negative() {
            Ordering::Less
        } else {
            Ordering::Greater
        })
    }
    /// The integer `self / 2^e0`; fails unless that is an integer.
    pub(crate) fn int_at(&self, e0: i64) -> Option<Int> {
        if self.is_zero() {
            return Some(Int::zero());
        }
        self.m.shl(usize::try_from(self.e - e0).ok()?)
    }
    /// The nearest binary64 values below and above: `(down, up)` with
    /// `down <= self <= up`, equal when `self` is representable. Fails closed
    /// outside the normal binary64 range (zero is always representable).
    pub(crate) fn bracket(&self) -> Option<(f64, f64)> {
        if self.is_zero() {
            return Some((0., 0.));
        }
        let bits = self.m.bits();
        let (mantissa, exponent, exact) = if bits <= 53 {
            (self.m.magnitude_bits_from(0), self.e, true)
        } else {
            // m is odd, so dropping low bits always loses information.
            (
                self.m.magnitude_bits_from(bits - 53),
                self.e + (bits - 53) as i64,
                false,
            )
        };
        let width = 64 - mantissa.leading_zeros() as i64;
        let normalized = mantissa << (53 - width);
        let biased = exponent - (53 - width) + 52 + 1023;
        if !(1..=2046).contains(&biased) {
            return None;
        }
        let toward_zero = f64::from_bits(
            ((self.is_negative() as u64) << 63)
                | ((biased as u64) << 52)
                | (normalized & ((1u64 << 52) - 1)),
        );
        if exact {
            return Some((toward_zero, toward_zero));
        }
        // One ulp further from zero; may overflow to infinity, which fails.
        let away = f64::from_bits(toward_zero.to_bits() + 1);
        if !away.is_finite() {
            return None;
        }
        Some(if self.is_negative() {
            (away, toward_zero)
        } else {
            (toward_zero, away)
        })
    }
    /// The smallest bracketing binary64 value at or above `self`; tiny
    /// positive values round up to the smallest normal number.
    pub(crate) fn upper(&self) -> Option<f64> {
        if self.top() < -1000 {
            return Some(if self.is_positive() {
                f64::MIN_POSITIVE
            } else {
                0.
            });
        }
        Some(self.bracket()?.1)
    }
}

/// An integer prepared for repeated use in [`Wide::add_int_scaled`]: the
/// limbs of a large magnitude are extracted once.
pub(crate) struct Multiplier {
    pub(crate) value: Int,
    limbs: Vec<u64>,
}
impl Multiplier {
    pub(crate) fn new(value: Int) -> Self {
        let mut limbs = match &value.0 {
            Repr::Small(_) => Vec::new(),
            Repr::Big { .. } => value.magnitude_u64(),
        };
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Self { value, limbs }
    }
}

/// 64-bit limbs per column of [`Wide`]: 1536 bits.
const WIDE: usize = 24;

/// Exact sparse fixed-point accumulator. Each column holds a two's
/// complement integer of [`WIDE`] limbs, the value being that integer times
/// `2^base`. Additions are in place and never normalize, so a long sum of
/// products costs one pass over the addend's limbs each. An addend that does
/// not fit (exponent below `base`, or magnitude at or above
/// 2^(64 (WIDE - 1)) after shifting) is refused; the caller then repeats the
/// derivation with [`Dy`] arithmetic, so results never depend on the width.
/// Fewer than 2^63 addends below that bound cannot overflow the sign.
pub(crate) struct Wide {
    base: i64,
    limbs: Vec<[u64; WIDE]>,
    seen: Vec<bool>,
    touched: Vec<usize>,
}

impl Wide {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            base: 0,
            limbs: vec![[0; WIDE]; n],
            seen: vec![false; n],
            touched: Vec::new(),
        }
    }
    /// Empties every column and sets the common exponent.
    pub(crate) fn reset(&mut self, base: i64) {
        for &j in &self.touched {
            self.limbs[j] = [0; WIDE];
            self.seen[j] = false;
        }
        self.touched.clear();
        self.base = base;
    }
    /// Adds `(-1)^negative * magnitude * 2^e`; false if it does not fit.
    fn add_magnitude(&mut self, j: usize, negative: bool, magnitude: &[u64], e: i64) -> bool {
        let Some(top) = magnitude.iter().rposition(|&v| v != 0) else {
            return true;
        };
        let bits = 64 * top + 64 - magnitude[top].leading_zeros() as usize;
        let Ok(shift) = usize::try_from(e - self.base) else {
            return false;
        };
        if shift + bits > 64 * (WIDE - 1) {
            return false;
        }
        charge(top + 2);
        if !self.seen[j] {
            self.seen[j] = true;
            self.touched.push(j);
        }
        let (word, bit) = (shift / 64, shift % 64);
        let column = &mut self.limbs[j];
        let mut carry = 0u64;
        let mut borrow = false;
        for i in 0..WIDE - word {
            let low = magnitude.get(i).copied().unwrap_or(0);
            let piece = if bit == 0 {
                low
            } else {
                let below = if i == 0 {
                    0
                } else {
                    magnitude.get(i - 1).copied().unwrap_or(0)
                };
                (low << bit) | (below >> (64 - bit))
            };
            let past = i > top + 1;
            if past && carry == 0 && !borrow {
                break;
            }
            let slot = &mut column[word + i];
            if negative {
                let (v, b1) = slot.overflowing_sub(piece);
                let (v, b2) = v.overflowing_sub(u64::from(borrow));
                *slot = v;
                borrow = b1 || b2;
            } else {
                let (v, c1) = slot.overflowing_add(piece);
                let (v, c2) = v.overflowing_add(carry);
                *slot = v;
                carry = u64::from(c1 || c2);
            }
        }
        true
    }
    /// Adds `g * v` exactly (`v = 1` for a plain integer).
    pub(crate) fn add_int_scaled(&mut self, j: usize, g: &Multiplier, v: f64) -> bool {
        let Some((negative, mantissa, e)) = parts(v) else {
            return v == 0.;
        };
        let (g, magnitude) = (&g.value, &g.limbs);
        if let Repr::Small(k) = &g.0 {
            // Inline multiplier: the product has at most 3 limbs.
            let k = k.unsigned_abs();
            let low = u128::from(k as u64) * u128::from(mantissa);
            let high = u128::from((k >> 64) as u64) * u128::from(mantissa) + (low >> 64);
            let product = [low as u64, high as u64, (high >> 64) as u64];
            return self.add_magnitude(j, negative != g.is_negative(), &product, e);
        }
        // A magnitude of WIDE - 1 nonzero limbs or more never fits, and a
        // refused addition charges nothing.
        if magnitude.len() >= WIDE {
            return false;
        }
        let mut product = [0u64; WIDE];
        let mut carry = 0u128;
        for (slot, &limb) in product.iter_mut().zip(magnitude) {
            let t = u128::from(limb) * u128::from(mantissa) + carry;
            *slot = t as u64;
            carry = t >> 64;
        }
        product[magnitude.len()] = carry as u64;
        self.add_magnitude(
            j,
            negative != g.is_negative(),
            &product[..=magnitude.len()],
            e,
        )
    }
    fn value(&self, j: usize) -> Dy {
        let column = &self.limbs[j];
        let negative = column[WIDE - 1] >> 63 == 1;
        let mut magnitude = *column;
        if negative {
            // Two's complement negation.
            let mut carry = true;
            for limb in &mut magnitude {
                let (v, c) = (!*limb).overflowing_add(u64::from(carry));
                *limb = v;
                carry = c;
            }
        }
        if magnitude[2..].iter().all(|&v| v == 0) && magnitude[1] >> 63 == 0 {
            let v = (i128::from(magnitude[1]) << 64) | i128::from(magnitude[0]);
            return Dy::new(Int(Repr::Small(if negative { -v } else { v })), self.base);
        }
        let digits = magnitude
            .iter()
            .flat_map(|&v| [v as u32, (v >> 32) as u32])
            .collect();
        Dy::new(Int::from_parts(negative, Nat::new(digits)), self.base)
    }
    /// Nonzero columns in order, as dyadics; leaves the accumulator empty.
    pub(crate) fn drain(&mut self) -> Vec<(usize, Dy)> {
        self.touched.sort_unstable();
        let out = self
            .touched
            .iter()
            .map(|&j| (j, self.value(j)))
            .filter(|(_, v)| !v.is_zero())
            .collect();
        let base = self.base;
        self.reset(base);
        out
    }
}

/// Sign, integer mantissa and exponent of a nonzero finite binary64 value.
fn parts(v: f64) -> Option<(bool, u64, i64)> {
    if !v.is_finite() || v == 0. {
        return None;
    }
    let bits = v.to_bits();
    let biased = ((bits >> 52) & 2047) as i64;
    let fraction = bits & ((1u64 << 52) - 1);
    let mantissa = if biased == 0 {
        fraction
    } else {
        fraction | (1u64 << 52)
    };
    let e = if biased == 0 { -1074 } else { biased - 1075 };
    Some((v < 0., mantissa, e))
}

/// The exponent of v's integer mantissa (the scale [`Wide`] needs as base).
pub(crate) fn exponent_of(v: f64) -> Option<i64> {
    parts(v).map(|p| p.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dy(v: f64) -> Dy {
        Dy::from_f64(v).unwrap()
    }
    #[test]
    fn signed_integer_arithmetic_matches_i128() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let a = (next() as i64) >> (next() % 40);
            let b = (next() as i64) >> (next() % 40);
            let (x, y) = (Int::from_i64(a), Int::from_i64(b));
            let wide = |v: i128| {
                let m = Int::from_u64((v.unsigned_abs() >> 64) as u64)
                    .shl(64)
                    .unwrap()
                    .add(&Int::from_u64(v.unsigned_abs() as u64))
                    .unwrap();
                if v < 0 { m.neg() } else { m }
            };
            assert_eq!(x.add(&y).unwrap(), wide(a as i128 + b as i128));
            assert_eq!(x.sub(&y).unwrap(), wide(a as i128 - b as i128));
            assert_eq!(x.mul(&y).unwrap(), wide(a as i128 * b as i128));
            assert_eq!(x.cmp(&y), a.cmp(&b));
            if b > 0 {
                let (q, r) = x.div_rem_euclid(&y).unwrap();
                assert_eq!(q, Int::from_i64(a.div_euclid(b)));
                assert_eq!(r, Int::from_i64(a.rem_euclid(b)));
            } else {
                assert!(x.div_rem_euclid(&y).is_none());
            }
        }
        assert!(!Int::from_i64(0).neg().is_negative());
    }
    #[test]
    fn limb_path_and_the_inline_boundary_are_consistent() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let big = |next: &mut dyn FnMut() -> u64| {
            // Values from a few bits to about 300 bits, both signs, often
            // straddling 2^127.
            let mut v = Int::from_u64(next() | 1);
            for _ in 0..(next() % 5) {
                v = v
                    .shl((next() % 70) as usize)
                    .unwrap()
                    .add(&Int::from_u64(next()))
                    .unwrap();
            }
            if next() % 2 == 0 { v.neg() } else { v }
        };
        let two127 = Int::from_u64(1).shl(127).unwrap();
        assert_eq!(two127.bits(), 128);
        assert_eq!(two127.sub(&Int::from_u64(1)).unwrap().bits(), 127);
        assert!(two127.neg().is_negative());
        assert_eq!(two127.neg().add(&two127).unwrap(), Int::zero());
        for _ in 0..5_000 {
            let (x, y) = (big(&mut next), big(&mut next));
            let s = x.add(&y).unwrap();
            assert_eq!(s.sub(&y).unwrap(), x);
            assert_eq!(s.sub(&x).unwrap(), y);
            assert_eq!(x.cmp(&y), x.sub(&y).unwrap().cmp(&Int::zero()));
            assert_eq!(x.cmp(&y), y.cmp(&x).reverse());
            let p = x.mul(&y).unwrap();
            assert_eq!(p.is_negative(), x.is_negative() != y.is_negative());
            let d = y.abs();
            let (q, r) = p.div_rem_euclid(&d).unwrap();
            assert!(r.is_zero());
            assert_eq!(q.mul(&d).unwrap(), p);
            let off = Int::from_u64(next() % 1000);
            let (q, r) = p.add(&off).unwrap().div_rem_euclid(&d).unwrap();
            assert!(!r.is_negative() && r.cmp(&d) == Ordering::Less);
            assert_eq!(q.mul(&d).unwrap().add(&r).unwrap(), p.add(&off).unwrap());
            let k = (next() % 90) as usize;
            let shifted = x.shl(k).unwrap();
            assert_eq!(shifted.trailing_zeros(), x.trailing_zeros() + k);
            assert_eq!(shifted.shr_exact(k), x);
        }
    }
    #[test]
    fn dyadic_arithmetic_and_rounding_are_exact() {
        let values = [
            0.,
            1.,
            -1.,
            0.1,
            -0.3,
            1e-300,
            -1e300,
            3.5,
            1e-17,
            f64::MIN_POSITIVE,
            123456789.123,
        ];
        for &a in &values {
            for &b in &values {
                let s = dy(a).add(&dy(b)).unwrap();
                let (down, up) = s.bracket().unwrap_or((f64::NAN, f64::NAN));
                if down.is_finite() {
                    assert!(dy(down).cmp(&s).unwrap() != Ordering::Greater);
                    assert!(dy(up).cmp(&s).unwrap() != Ordering::Less);
                    if down != up {
                        // Adjacent binary64 values.
                        let next = if down >= 0. {
                            f64::from_bits(down.to_bits() + 1)
                        } else {
                            f64::from_bits(down.to_bits() - 1)
                        };
                        assert_eq!(next, up);
                    }
                    // Round-to-nearest lies in the bracket.
                    let nearest = a + b;
                    assert!(down <= nearest && nearest <= up, "{a} {b}");
                }
                let p = dy(a).mul(&dy(b)).unwrap();
                if let Some((down, up)) = p.bracket() {
                    let nearest = a * b;
                    assert!(down <= nearest && nearest <= up, "{a} {b}");
                }
                assert!(dy(a).sub(&dy(a)).unwrap().is_zero());
            }
            if a != 0. && a.abs() >= f64::MIN_POSITIVE {
                assert_eq!(dy(a).bracket(), Some((a, a)));
                assert_eq!(dy(a).upper(), Some(a));
            }
        }
        let third_ish = dy(1.).add(&dy(2f64.powi(-60))).unwrap();
        let (down, up) = third_ish.bracket().unwrap();
        assert_eq!(down, 1.);
        assert_eq!(up, 1. + f64::EPSILON);
        let negative = third_ish.neg().bracket().unwrap();
        assert_eq!(negative, (-1. - f64::EPSILON, -1.));
        assert!(Dy::from_f64(f64::INFINITY).is_none());
        assert_eq!(dy(6.).int_at(1), Some(Int::from_u64(3)));
        assert_eq!(dy(0.75).int_at(-2), Some(Int::from_u64(3)));
        assert!(dy(0.75).int_at(0).is_none());
    }
}
