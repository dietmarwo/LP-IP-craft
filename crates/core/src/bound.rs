//! Conservative Lagrangian bounds for the exact model represented by binary64
//! inputs. Unlike residual checks, these computations enclose rounding error.
//!
//! The derivation, assumptions and limitations are in `docs/safe-dual-bounds.md`.
//! This module neither validates an incumbent exactly nor certifies a solver's
//! search tree. It does not change the floating-point environment.

use crate::error::{Result, ensure, ensure_finite};
use crate::{Model, Sense};

/// Outcome of an independent dual-bound calculation, not a solver status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SafeBoundStatus {
    /// A finite conservative bound was computed.
    Finite,
    /// A nonzero multiplier selects an infinite row bound.
    UnboundedRow,
    /// The residual enclosure cannot be bounded over an infinite variable side.
    UnboundedVariable,
    /// An intermediate enclosure or outward sum was not finite.
    ArithmeticOverflow,
    /// The current arithmetic environment fails the gradual-underflow probes.
    UnsupportedArithmetic,
}

impl SafeBoundStatus {
    /// Stable lowercase name used by the Python API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Finite => "finite",
            Self::UnboundedRow => "unbounded_row",
            Self::UnboundedVariable => "unbounded_variable",
            Self::ArithmeticOverflow => "arithmetic_overflow",
            Self::UnsupportedArithmetic => "unsupported_arithmetic",
        }
    }
}

/// Result of [`safe_dual_bound`]. An unavailable bound is never an assertion
/// that the optimization problem is unbounded or infeasible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafeDualBound {
    /// Lower bound for minimization, upper bound for maximization, including
    /// the objective offset. Present exactly when status is `Finite`.
    pub bound: Option<f64>,
    /// Why the calculation succeeded or could not supply a finite bound.
    pub status: SafeBoundStatus,
    /// Zero-based row/variable index for `UnboundedRow`/`UnboundedVariable`;
    /// `None` for all other outcomes.
    pub index: Option<usize>,
}

/// Computes a conservative bound from arbitrary finite row multipliers in
/// the [`crate::Duals`] original-sense convention. No primal point, reduced
/// costs, solver status or feasibility tolerance is needed.
///
/// For minimization, with `r = c - A^T y`, the bound is an outward-rounded
/// lower enclosure of `c0 + sum inf(y[i] * row[i]) + sum inf(r[j] * x[j])`,
/// where each infimum uses the model's row or variable interval. Maximization
/// is handled by exact sign reversal. Integrality is relaxed, so the bound
/// is also valid for every feasible integer point of a MIP.
///
/// Validity refers to the exact real model whose coefficients are the stored
/// binary64 values. Standard IEEE binary64 arithmetic with gradual underflow
/// is required; do not build with fast-math or change that environment during
/// the call. Lightweight probes reject flush-to-zero/denormals-are-zero modes.
/// This is not an exact primal certificate or a safe pruning decision.
///
/// Bounds may be weak or unavailable. With duals from an LP solver, a column
/// with an infinite bound usually makes the result `UnboundedVariable`: its
/// exact residual must have the safe sign (be exactly zero for a free column),
/// and solver duals meet that only to the solver's tolerance, so the exact
/// Lagrangian bound of those duals is often infinite itself. Request the duals
/// from an LP whose one-sided costs are shifted by a small margin (see
/// `docs/safe-dual-bounds.md`), or give free columns finite bounds.
///
/// Runs in O(nnz + n + m) time and O(n) extra memory. Invalid multiplier
/// length/nonfinite entries return [`crate::Error::InvalidInput`]. Arithmetic
/// failure instead returns a typed result without a bound.
pub fn safe_dual_bound(model: &Model, row_duals: &[f64]) -> Result<SafeDualBound> {
    safe_dual_bound_with_bounds(model, model.variables(), row_duals)
}

/// Conservative dual bound using replacement variable bounds. The matrix,
/// objective and rows are borrowed unchanged; bounds are validated first.
pub fn safe_dual_bound_with_bounds(
    model: &Model,
    bounds: &crate::Bounds,
    row_duals: &[f64],
) -> Result<SafeDualBound> {
    bounds.validate(model.n(), "replacement variable bounds")?;

    ensure(row_duals.len() == model.m(), || {
        format!(
            "row duals: expected {} entries, got {}",
            model.m(),
            row_duals.len()
        )
    })?;
    ensure_finite(row_duals, "row duals")?;
    if !gradual_underflow() {
        return Ok(unavailable(SafeBoundStatus::UnsupportedArithmetic, None));
    }
    Ok(match compute(model, bounds, row_duals) {
        Ok(bound) => SafeDualBound {
            bound: Some(bound),
            status: SafeBoundStatus::Finite,
            index: None,
        },
        Err((status, index)) => unavailable(status, index),
    })
}

fn unavailable(status: SafeBoundStatus, index: Option<usize>) -> SafeDualBound {
    SafeDualBound {
        bound: None,
        status,
        index,
    }
}

type Failure = (SafeBoundStatus, Option<usize>);
const OVERFLOW: Failure = (SafeBoundStatus::ArithmeticOverflow, None);

fn compute(model: &Model, bounds: &crate::Bounds, y: &[f64]) -> std::result::Result<f64, Failure> {
    let orient = |v: f64| match model.sense() {
        Sense::Minimize => v,
        Sense::Maximize => -v,
    };
    let mut residual: Vec<_> = model
        .objective()
        .iter()
        .map(|&c| Interval::point(orient(c)))
        .collect();
    let mut total = orient(model.objective_offset());
    let matrix = model.matrix();
    for (i, &original) in y.iter().enumerate() {
        let v = orient(original);
        if v == 0.0 {
            continue; // Includes free rows with zero multipliers, no 0 * inf.
        }
        let selected = if v > 0.0 {
            model.rows().lower[i]
        } else {
            model.rows().upper[i]
        };
        if !selected.is_finite() {
            return Err((SafeBoundStatus::UnboundedRow, Some(i)));
        }
        let row_term = product(v, selected).ok_or(OVERFLOW)?.lo;
        total = addition(total, row_term).ok_or(OVERFLOW)?.lo;
        for k in matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1] {
            let j = matrix.pattern.indices[k];
            let a = matrix.values[k];
            let p = product(a, v).ok_or(OVERFLOW)?;
            residual[j] = residual[j].subtract(p).ok_or(OVERFLOW)?;
        }
    }
    for (j, r) in residual.into_iter().enumerate() {
        let (l, u) = (bounds.lower[j], bounds.upper[j]);
        if (l == f64::NEG_INFINITY && r.hi > 0.0) || (u == f64::INFINITY && r.lo < 0.0) {
            return Err((SafeBoundStatus::UnboundedVariable, Some(j)));
        }
        // A bilinear function attains its minimum at a corner of a finite
        // rectangle. Above, all unbounded directions with negative infimum
        // have been excluded. A wholly free box is possible only for r=[0,0].
        let mut term = f64::INFINITY;
        for x in [l, u].into_iter().filter(|x| x.is_finite()) {
            for coefficient in [r.lo, r.hi] {
                term = term.min(product(coefficient, x).ok_or(OVERFLOW)?.lo);
            }
        }
        if term == f64::INFINITY {
            debug_assert!(r.lo == 0.0 && r.hi == 0.0);
            term = 0.0;
        }
        total = addition(total, term).ok_or(OVERFLOW)?.lo;
    }
    Ok(orient(total))
}

#[derive(Clone, Copy, Debug)]
struct Interval {
    lo: f64,
    hi: f64,
}

impl Interval {
    fn point(v: f64) -> Self {
        Self { lo: v, hi: v }
    }

    fn subtract(self, other: Self) -> Option<Self> {
        Some(Self {
            lo: addition(self.lo, -other.hi)?.lo,
            hi: addition(self.hi, -other.lo)?.hi,
        })
    }
}

// Preserve only elementary exact identities. In every other case the two
// adjacent floats enclose the exact result, including a product rounded to 0.
// Reject nonfinite endpoints rather than claim a finite bound after overflow.
fn addition(a: f64, b: f64) -> Option<Interval> {
    if a == 0.0 {
        Some(Interval::point(b))
    } else if b == 0.0 {
        Some(Interval::point(a))
    } else if a == -b {
        Some(Interval::point(0.0))
    } else {
        enclose(a + b)
    }
}

fn product(a: f64, b: f64) -> Option<Interval> {
    if a == 0.0 || b == 0.0 {
        Some(Interval::point(0.0))
    } else if a == 1.0 {
        Some(Interval::point(b))
    } else if b == 1.0 {
        Some(Interval::point(a))
    } else if a == -1.0 {
        Some(Interval::point(-b))
    } else if b == -1.0 {
        Some(Interval::point(-a))
    } else {
        enclose(a * b)
    }
}

fn enclose(v: f64) -> Option<Interval> {
    if !v.is_finite() {
        return None;
    }
    let (lo, hi) = (next_down(v), -next_down(-v));
    (lo.is_finite() && hi.is_finite()).then_some(Interval { lo, hi })
}

// f64::next_down requires Rust 1.86; the crate supports 1.85. Input is finite.
fn next_down(v: f64) -> f64 {
    if v == 0.0 {
        -f64::from_bits(1)
    } else if v > 0.0 {
        f64::from_bits(v.to_bits() - 1)
    } else {
        f64::from_bits(v.to_bits() + 1)
    }
}

fn gradual_underflow() -> bool {
    // black_box prevents constant-folding the probes. No unsafe access to
    // thread-local hardware control words, and no environment mutation.
    use std::hint::black_box as bb;
    let normal_to_subnormal = bb(f64::MIN_POSITIVE) * bb(0.5);
    let tiny = bb(f64::from_bits(1));
    normal_to_subnormal.to_bits() == (1_u64 << 51)
        && (tiny + tiny).to_bits() == 2
        && (tiny * bb(2.0)).to_bits() == 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_cover_zero_subnormal_and_extremes() {
        assert_eq!(next_down(0.0).to_bits(), (-f64::from_bits(1)).to_bits());
        assert_eq!(next_down(-0.0), -f64::from_bits(1));
        assert_eq!(next_down(f64::from_bits(1)), 0.0);
        assert_eq!(next_down(-f64::from_bits(1)), -f64::from_bits(2));
        assert_eq!(next_down(1.0).to_bits(), 1.0_f64.to_bits() - 1);
        assert_eq!(next_down(-1.0).to_bits(), (-1.0_f64).to_bits() + 1);
        assert_eq!(next_down(-f64::MAX), f64::NEG_INFINITY);
        assert!(gradual_underflow());
    }

    #[test]
    fn exact_identities_and_rounded_underflow() {
        let tiny = f64::from_bits(1);
        assert_eq!(addition(1.0, -1.0).unwrap().lo, 0.0);
        assert_eq!(product(tiny, -1.0).unwrap().lo, -tiny);
        let p = product(tiny, 0.5).unwrap();
        assert_eq!((p.lo, p.hi), (-tiny, tiny));
        assert!(product(f64::MAX, 2.0).is_none());
        assert!(addition(f64::MAX, f64::MAX).is_none());
        assert!(enclose(f64::MAX).is_none());
    }
}
