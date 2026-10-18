//! Bounded owned exact arithmetic for primal witnesses, never a numerical LP kernel.
//! Every binary64 datum is interpreted exactly. Exceeding 8192 bits fails closed.
//!
//! Multiplication and division charge their limb operations to a per-thread
//! work counter, so callers can bound exact computations deterministically
//! (independently of machine speed) with [`reset_work`] and [`work`].
use std::cell::Cell;
use std::cmp::Ordering;
const LIMBS: usize = 256;
thread_local! { static WORK: Cell<u64> = const { Cell::new(0) }; }
fn charge(amount: usize) {
    WORK.with(|w| w.set(w.get().saturating_add(amount as u64)));
}
/// Resets this thread's limb-operation counter.
pub(crate) fn reset_work() {
    WORK.with(|w| w.set(0));
}
/// Limb operations performed on this thread since [`reset_work`].
pub(crate) fn work() -> u64 {
    WORK.with(Cell::get)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Nat(Vec<u32>);
impl Nat {
    fn new(mut v: Vec<u32>) -> Self {
        while v.last() == Some(&0) {
            v.pop();
        }
        Self(v)
    }
    fn small(v: u64) -> Self {
        Self::new(vec![v as u32, (v >> 32) as u32])
    }
    fn zero(&self) -> bool {
        self.0.is_empty()
    }
    fn cmp(&self, b: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&b.0.len())
            .then_with(|| self.0.iter().rev().cmp(b.0.iter().rev()))
    }
    fn bits(&self) -> usize {
        self.0.last().map_or(0, |v| {
            32 * (self.0.len() - 1) + 32 - v.leading_zeros() as usize
        })
    }
    fn add(&self, b: &Self) -> Self {
        let mut v = Vec::new();
        let mut c = 0u64;
        for i in 0..self.0.len().max(b.0.len()) {
            c += u64::from(*self.0.get(i).unwrap_or(&0)) + u64::from(*b.0.get(i).unwrap_or(&0));
            v.push(c as u32);
            c >>= 32;
        }
        if c > 0 {
            v.push(c as u32);
        }
        Self(v)
    }
    fn sub(&self, b: &Self) -> Self {
        debug_assert!(self.cmp(b) != Ordering::Less);
        let mut v = Vec::new();
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let x = i64::from(self.0[i]) - i64::from(*b.0.get(i).unwrap_or(&0)) - borrow;
            v.push(x as u32);
            borrow = i64::from(x < 0);
        }
        Self::new(v)
    }
    fn mul(&self, b: &Self) -> Option<Self> {
        if self.0.len() + b.0.len() > LIMBS {
            return None;
        }
        charge(self.0.len() * b.0.len() + 1);
        let mut v = vec![0; self.0.len() + b.0.len()];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in b.0.iter().enumerate() {
                let x = u64::from(a) * u64::from(b) + u64::from(v[i + j]) + carry;
                v[i + j] = x as u32;
                carry = x >> 32;
            }
            v[i + b.0.len()] = carry as u32;
        }
        Some(Self::new(v))
    }
    fn shift(&self, k: usize) -> Option<Self> {
        if self.zero() {
            return Some(self.clone());
        }
        let mut v = vec![0; k / 32];
        let mut carry = 0u64;
        for &a in &self.0 {
            let x = (u64::from(a) << (k % 32)) | carry;
            v.push(x as u32);
            carry = x >> 32;
        }
        if carry > 0 {
            v.push(carry as u32);
        }
        if v.len() > LIMBS { None } else { Some(Self(v)) }
    }
    /// Quotient and remainder by word-level long division (Knuth, TAOCP vol. 2,
    /// algorithm D, in the formulation of Hacker's Delight `divmnu`).
    fn div_rem(&self, b: &Self) -> (Self, Self) {
        assert!(!b.zero());
        if self.cmp(b) == Ordering::Less {
            return (Self::small(0), self.clone());
        }
        let (u, v) = (&self.0, &b.0);
        let (m, n) = (u.len(), v.len());
        charge((m - n + 1) * n + 1);
        if n == 1 {
            let d = u64::from(v[0]);
            let mut q = vec![0u32; m];
            let mut k = 0u64;
            for j in (0..m).rev() {
                let t = (k << 32) | u64::from(u[j]);
                q[j] = (t / d) as u32;
                k = t % d;
            }
            return (Self::new(q), Self::small(k));
        }
        // Normalize so that the divisor's top limb has its high bit set.
        let s = v[n - 1].leading_zeros();
        let spill = |hi: u32, lo: u32| {
            if s == 0 {
                hi
            } else {
                (hi << s) | (lo >> (32 - s))
            }
        };
        let mut vn = vec![0u32; n];
        for i in (1..n).rev() {
            vn[i] = spill(v[i], v[i - 1]);
        }
        vn[0] = v[0] << s;
        let mut un = vec![0u32; m + 1];
        un[m] = if s == 0 { 0 } else { u[m - 1] >> (32 - s) };
        for i in (1..m).rev() {
            un[i] = spill(u[i], u[i - 1]);
        }
        un[0] = u[0] << s;
        const BASE: u64 = 1 << 32;
        let mut q = vec![0u32; m - n + 1];
        for j in (0..=m - n).rev() {
            let top = (u64::from(un[j + n]) << 32) | u64::from(un[j + n - 1]);
            let mut qhat = top / u64::from(vn[n - 1]);
            let mut rhat = top % u64::from(vn[n - 1]);
            while qhat >= BASE
                || qhat * u64::from(vn[n - 2]) > (rhat << 32) + u64::from(un[j + n - 2])
            {
                qhat -= 1;
                rhat += u64::from(vn[n - 1]);
                if rhat >= BASE {
                    break;
                }
            }
            // Multiply and subtract qhat * vn from un[j..=j + n].
            let mut k = 0i64;
            for i in 0..n {
                let p = qhat * u64::from(vn[i]);
                let t = i64::from(un[i + j]) - k - (p & 0xFFFF_FFFF) as i64;
                un[i + j] = t as u32;
                k = (p >> 32) as i64 - (t >> 32);
            }
            let t = i64::from(un[j + n]) - k;
            un[j + n] = t as u32;
            q[j] = qhat as u32;
            if t < 0 {
                // qhat was one too large: add the divisor back.
                q[j] = q[j].wrapping_sub(1);
                let mut c = 0u64;
                for i in 0..n {
                    let t = u64::from(un[i + j]) + u64::from(vn[i]) + c;
                    un[i + j] = t as u32;
                    c = t >> 32;
                }
                un[j + n] = un[j + n].wrapping_add(c as u32);
            }
        }
        let mut r = vec![0u32; n];
        for i in 0..n - 1 {
            r[i] = if s == 0 {
                un[i]
            } else {
                (un[i] >> s) | (un[i + 1] << (32 - s))
            };
        }
        r[n - 1] = un[n - 1] >> s;
        (Self::new(q), Self::new(r))
    }
    fn gcd(mut a: Self, mut b: Self) -> Self {
        while !b.zero() {
            let r = a.div_rem(&b).1;
            a = b;
            b = r;
        }
        a
    }
    fn leading(&self) -> f64 {
        let mut x = 0.;
        for bit in (self.bits().saturating_sub(53)..self.bits()).rev() {
            x = 2. * x + f64::from((self.0[bit / 32] >> (bit % 32)) & 1);
        }
        x
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Rat {
    negative: bool,
    n: Nat,
    d: Nat,
}
impl Rat {
    fn norm(negative: bool, n: Nat, d: Nat) -> Option<Self> {
        if d.zero() || n.0.len() > LIMBS || d.0.len() > LIMBS {
            return None;
        }
        if n.zero() {
            return Some(Self::zero());
        }
        if d == Nat::small(1) {
            return Some(Self { negative, n, d });
        }
        let g = Nat::gcd(n.clone(), d.clone());
        Some(Self {
            negative,
            n: n.div_rem(&g).0,
            d: d.div_rem(&g).0,
        })
    }
    pub fn zero() -> Self {
        Self {
            negative: false,
            n: Nat::small(0),
            d: Nat::small(1),
        }
    }
    pub fn from(v: f64) -> Option<Self> {
        if !v.is_finite() {
            return None;
        }
        let bits = v.to_bits();
        let e = ((bits >> 52) & 2047) as i32;
        let mant = (bits & ((1u64 << 52) - 1)) | if e == 0 { 0 } else { 1u64 << 52 };
        if mant == 0 {
            return Some(Self::zero());
        }
        let shift = mant.trailing_zeros();
        let exponent = if e == 0 { -1074 } else { e - 1075 } + shift as i32;
        let n = Nat::small(mant >> shift);
        let d = Nat::small(1);
        Some(Self {
            negative: v < 0.,
            n: if exponent > 0 {
                n.shift(exponent as usize)?
            } else {
                n
            },
            d: if exponent < 0 {
                d.shift((-exponent) as usize)?
            } else {
                d
            },
        })
    }
    pub fn is_zero(&self) -> bool {
        self.n.zero()
    }
    pub fn neg(&self) -> Self {
        let mut v = self.clone();
        if !v.is_zero() {
            v.negative = !v.negative;
        }
        v
    }
    pub fn add(&self, b: &Self) -> Option<Self> {
        if self.is_zero() {
            return Some(b.clone());
        }
        if b.is_zero() {
            return Some(self.clone());
        }
        let a = self.n.mul(&b.d)?;
        let c = b.n.mul(&self.d)?;
        let d = self.d.mul(&b.d)?;
        if self.negative == b.negative {
            Self::norm(self.negative, a.add(&c), d)
        } else if a.cmp(&c) != Ordering::Less {
            Self::norm(self.negative, a.sub(&c), d)
        } else {
            Self::norm(b.negative, c.sub(&a), d)
        }
    }
    pub fn sub(&self, b: &Self) -> Option<Self> {
        self.add(&b.neg())
    }
    pub fn mul(&self, b: &Self) -> Option<Self> {
        if self.is_zero() || b.is_zero() {
            return Some(Self::zero());
        }
        Self::norm(
            self.negative != b.negative,
            self.n.mul(&b.n)?,
            self.d.mul(&b.d)?,
        )
    }
    pub fn div(&self, b: &Self) -> Option<Self> {
        Self::norm(
            self.negative != b.negative,
            self.n.mul(&b.d)?,
            self.d.mul(&b.n)?,
        )
    }
    pub fn cmp(&self, b: &Self) -> Option<Ordering> {
        if self.negative != b.negative {
            return Some(if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let ord = self.n.mul(&b.d)?.cmp(&b.n.mul(&self.d)?);
        Some(if self.negative { ord.reverse() } else { ord })
    }
    /// Mathematical floor, including negative nonintegers.
    pub fn floor(&self) -> Self {
        let (q, r) = self.n.div_rem(&self.d);
        Self {
            negative: self.negative,
            n: if self.negative && !r.zero() {
                q.add(&Nat::small(1))
            } else {
                q
            },
            d: Nat::small(1),
        }
    }
    pub fn frac(&self) -> Option<Self> {
        self.sub(&self.floor())
    }
    pub fn upper(&self) -> Option<f64> {
        if self.is_zero() {
            return Some(0.);
        }
        let e = self.n.bits() as i32 - self.d.bits() as i32;
        let mut v = self.n.leading() / self.d.leading()
            * 2f64.powi(e + self.d.bits().min(53) as i32 - self.n.bits().min(53) as i32);
        if self.negative {
            v = -v;
        }
        if !v.is_finite() {
            return None;
        }
        for _ in 0..8 {
            if Self::from(v)?.cmp(self)? != Ordering::Less {
                return Some(v);
            }
            v = if v == 0. {
                f64::from_bits(1)
            } else if v > 0. {
                f64::from_bits(v.to_bits() + 1)
            } else {
                f64::from_bits(v.to_bits() - 1)
            };
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_signed_arithmetic_and_conversion() {
        for a in -30..30 {
            for b in -20..20 {
                let x = Rat::from(a as f64 / 8.).unwrap();
                let y = Rat::from(b as f64 / 4.).unwrap();
                for (z, w) in [
                    (x.add(&y).unwrap(), a as f64 / 8. + b as f64 / 4.),
                    (x.mul(&y).unwrap(), (a * b) as f64 / 32.),
                ] {
                    assert_eq!(z.cmp(&Rat::from(w).unwrap()), Some(Ordering::Equal));
                    assert_eq!(z.upper(), Some(w));
                }
                if b != 0 {
                    let z = x.div(&y).unwrap();
                    assert_ne!(
                        Rat::from(z.upper().unwrap()).unwrap().cmp(&z),
                        Some(Ordering::Less)
                    );
                }
            }
        }
        let tiny = Rat::from(f64::from_bits(1)).unwrap();
        assert_eq!(tiny.upper(), Some(f64::from_bits(1)));
        let one = Rat::from(1.).unwrap();
        let third = one.div(&Rat::from(3.).unwrap()).unwrap();
        assert_eq!(
            third.mul(&Rat::from(3.).unwrap()).unwrap().cmp(&one),
            Some(Ordering::Equal)
        );
        assert!(one.div(&Rat::zero()).is_none());
        assert!(Rat::from(f64::INFINITY).is_none());
    }
}

#[cfg(test)]
mod independent_tests {
    use super::*;
    fn nat(v: u128) -> Nat {
        Nat::new(vec![
            v as u32,
            (v >> 32) as u32,
            (v >> 64) as u32,
            (v >> 96) as u32,
        ])
    }
    #[test]
    fn long_division_and_products_match_machine_integer_oracle() {
        let mut seed = 17u128;
        for _ in 0..1200 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let a = seed;
            let b = (seed.rotate_left(43) >> 48) | 1;
            let (q, r) = nat(a).div_rem(&nat(b));
            assert_eq!(q, nat(a / b));
            assert_eq!(r, nat(a % b));
            let x = a >> 65;
            let y = b >> 35;
            assert_eq!(nat(x).mul(&nat(y)), Some(nat(x * y)));
        }
    }
    #[test]
    fn exact_f64_extremes_and_cancellation() {
        for v in [
            f64::MAX,
            -f64::MAX,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            f64::from_bits(1),
            -f64::from_bits(1),
            1e-300,
            -1e200,
        ] {
            let x = Rat::from(v).unwrap();
            assert_eq!(x.upper(), Some(v));
            assert!(x.add(&x.neg()).unwrap().is_zero());
        }
        let p = Nat::small(1).shift(400).unwrap().add(&Nat::small(1));
        let q = Nat::small(1).shift(500).unwrap().sub(&Nat::small(3));
        let product = p.mul(&q).unwrap();
        assert_eq!(product.div_rem(&p), (q, Nat::small(0)));
    }
}

#[cfg(test)]
mod division_tests {
    use super::*;
    /// `a = q * b + r` with `r < b` built from structured limbs, where the
    /// trial quotient of algorithm D is often too large; the division must
    /// return exactly `(q, r)`, also on the rare add-back path.
    #[test]
    fn long_division_recovers_planted_quotients() {
        let special = [
            0u32,
            1,
            2,
            0x7FFF_FFFF,
            0x8000_0000,
            0xFFFF_FFFE,
            0xFFFF_FFFF,
        ];
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let limbs = |len: usize, next: &mut dyn FnMut() -> u64| -> Vec<u32> {
            (0..len)
                .map(|_| {
                    let r = next();
                    if r % 3 == 0 {
                        r as u32 >> 7
                    } else {
                        special[(r >> 8) as usize % special.len()]
                    }
                })
                .collect()
        };
        let mut checked = 0;
        for _ in 0..20_000 {
            let bl = 1 + (next() % 6) as usize;
            let ql = 1 + (next() % 6) as usize;
            let b = Nat::new(limbs(bl, &mut next));
            if b.zero() {
                continue;
            }
            let q = Nat::new(limbs(ql, &mut next));
            let r = Nat::new(limbs(bl, &mut next));
            let r = if r.cmp(&b) == Ordering::Less {
                r
            } else {
                r.div_rem_bitwise(&b).1
            };
            let a = q.mul(&b).unwrap().add(&r);
            assert_eq!(a.div_rem(&b), (q.clone(), r.clone()), "a={a:?} b={b:?}");
            assert_eq!(a.div_rem(&b), a.div_rem_bitwise(&b));
            checked += 1;
        }
        assert!(checked > 19_000);
    }

    impl Nat {
        /// The original bit-serial division, kept as an independent oracle.
        fn div_rem_bitwise(&self, b: &Self) -> (Self, Self) {
            if self.cmp(b) == Ordering::Less {
                return (Self::small(0), self.clone());
            }
            let mut rem = Self::small(0);
            let mut q = vec![0; self.0.len()];
            for bit in (0..self.bits()).rev() {
                rem = rem.shift(1).unwrap();
                if (self.0[bit / 32] >> (bit % 32)) & 1 != 0 {
                    rem = rem.add(&Self::small(1));
                }
                if rem.cmp(b) != Ordering::Less {
                    rem = rem.sub(b);
                    q[bit / 32] |= 1 << (bit % 32);
                }
            }
            (Self::new(q), rem)
        }
    }

    #[test]
    fn work_counter_charges_products_and_divisions() {
        reset_work();
        let a = Nat::small(u64::MAX).shift(200).unwrap();
        let b = Nat::small(12345).shift(40).unwrap();
        let before = work();
        let _ = a.mul(&b);
        let _ = a.div_rem(&b);
        assert!(work() > before);
        reset_work();
        assert_eq!(work(), 0);
    }
}
