//! Exact rational reconstruction of a nearby feasible point with fixed integers.
//! A rational witness may exist when no exactly feasible binary64 display point does.
//!
//! Effort is bounded by limb operations of the exact arithmetic (see
//! [`crate::rational::work`]), not by wall time, so whether a certificate is
//! found does not depend on machine speed or load.
use crate::{
    Budget,
    rational::{self, Rat},
};
use lp_ip_craft::{Model, Sense};
use std::cmp::Ordering;
pub(crate) struct Witness {
    pub upper: f64,
    pub point: Vec<f64>,
}
/// Limb operations allowed for one witness attempt (the largest public-model
/// attempt measured needs about 7e4).
pub(crate) const ATTEMPT_WORK: u64 = 8_000_000;

/// Tries to certify `x` (integers rounded and fixed) by an exactly feasible
/// rational point. Returns the witness, if any, and the limb operations used.
pub(crate) fn upper(
    model: &Model,
    x: &[f64],
    budget: &Budget<'_>,
    limit: u64,
) -> (Option<Witness>, u64) {
    rational::reset_work();
    let witness = reconstruct(model, x, budget, limit);
    (witness, rational::work())
}

fn reconstruct(model: &Model, x: &[f64], budget: &Budget<'_>, limit: u64) -> Option<Witness> {
    let stopped = || budget.stop().is_some() || rational::work() >= limit;
    if model.m() > 1024 || model.nnz() > 20000 || stopped() {
        return None;
    }
    let mut point: Vec<Rat> = x.iter().map(|&v| Rat::from(v)).collect::<Option<_>>()?;
    let mut free = Vec::new();
    for j in 0..model.n() {
        let (l, u) = (model.variables().lower[j], model.variables().upper[j]);
        let v = if model.kinds()[j].is_integral() {
            x[j].round()
        } else if l.is_finite() && (x[j] - l).abs() <= 1e-8 * (1. + l.abs()) {
            l
        } else if u.is_finite() && (x[j] - u).abs() <= 1e-8 * (1. + u.abs()) {
            u
        } else {
            free.push(j);
            x[j]
        };
        point[j] = Rat::from(v)?;
    }
    if free.len() > 64 {
        return None;
    }
    let n = free.len();
    let mut positions = vec![None; model.n()];
    for (k, &j) in free.iter().enumerate() {
        positions[j] = Some(k);
    }
    let a = model.matrix();
    let mut equations = Vec::new();
    for i in 0..model.m() {
        let activity: f64 = (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
            .map(|k| a.values[k] * x[a.pattern.indices[k]])
            .sum();
        let (l, u) = (model.rows().lower[i], model.rows().upper[i]);
        let side =
            if l == u || (l.is_finite() && (activity - l).abs() <= 1e-6 * (1. + activity.abs())) {
                Some(l)
            } else if u.is_finite() && (activity - u).abs() <= 1e-6 * (1. + activity.abs()) {
                Some(u)
            } else {
                None
            };
        if let Some(rhs) = side {
            equations.push((l != u, i, rhs));
        }
    }
    equations.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut basis: Vec<Option<Vec<Rat>>> = vec![None; n];
    let mut rank = 0;
    for (_, i, rhs) in equations {
        if rank == n {
            break;
        }
        if stopped() {
            return None;
        }
        let mut row = vec![Rat::zero(); n + 1];
        row[n] = Rat::from(rhs)?;
        for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
            let j = a.pattern.indices[k];
            let coefficient = Rat::from(a.values[k])?;
            if let Some(p) = positions[j] {
                row[p] = coefficient;
            } else {
                row[n] = row[n].sub(&coefficient.mul(&point[j])?)?;
            }
        }
        for p in 0..n {
            if row[p].is_zero() {
                continue;
            }
            if let Some(old) = &basis[p] {
                let v = row[p].clone();
                for k in p..=n {
                    row[k] = row[k].sub(&v.mul(&old[k])?)?;
                }
            } else {
                let v = row[p].clone();
                for value in row.iter_mut().skip(p) {
                    *value = value.div(&v)?;
                }
                basis[p] = Some(row);
                rank += 1;
                break;
            }
        }
    }
    for p in (0..n).rev() {
        if let Some(row) = &basis[p] {
            let mut v = row[n].clone();
            for k in p + 1..n {
                v = v.sub(&row[k].mul(&point[free[k]])?)?;
            }
            point[free[p]] = v;
        }
    }
    for (j, v) in point.iter().enumerate() {
        for (b, lower) in [
            (model.variables().lower[j], true),
            (model.variables().upper[j], false),
        ] {
            if b.is_finite() {
                let c = v.cmp(&Rat::from(b)?)?;
                if (lower && c == Ordering::Less) || (!lower && c == Ordering::Greater) {
                    return None;
                }
            }
        }
    }
    for i in 0..model.m() {
        if i % 16 == 0 && stopped() {
            return None;
        }
        let mut sum = Rat::zero();
        for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
            sum = sum.add(&Rat::from(a.values[k])?.mul(&point[a.pattern.indices[k]])?)?;
        }
        for (b, lower) in [
            (model.rows().lower[i], true),
            (model.rows().upper[i], false),
        ] {
            if b.is_finite() {
                let c = sum.cmp(&Rat::from(b)?)?;
                if (lower && c == Ordering::Less) || (!lower && c == Ordering::Greater) {
                    return None;
                }
            }
        }
    }
    let sign = if model.sense() == Sense::Minimize {
        1.
    } else {
        -1.
    };
    let mut value = Rat::from(sign * model.objective_offset())?;
    for (j, v) in point.iter().enumerate() {
        value = value.add(&Rat::from(sign * model.objective()[j])?.mul(v)?)?;
    }
    Some(Witness {
        upper: value.upper()?,
        point: point.iter().map(Rat::upper).collect::<Option<_>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeOptions;
    use lp_ip_craft::*;
    #[test]
    fn rational_boundary_witness_and_false_candidate() {
        let make = |upper| {
            Model::new(ModelData {
                sense: Sense::Minimize,
                objective: vec![0., 1.],
                objective_offset: 2.,
                variables: Bounds::new(vec![0., 0.], vec![1., upper]),
                kinds: vec![VariableKind::Binary, VariableKind::Continuous],
                matrix: CsrMatrix::from_dense(1, 2, &[0., 3.]).unwrap(),
                rows: Bounds::fixed(vec![1.]),
            })
            .unwrap()
        };
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: std::time::Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let m = make(1.);
        let u = upper(&m, &[0., 1. / 3.], &budget, ATTEMPT_WORK)
            .0
            .unwrap()
            .upper;
        assert!((2. + 1. / 3. ..2. + 1. / 3. + 1e-14).contains(&u));
        assert!(crate::incumbent::upper(&m, &[0., 1. / 3.]).is_none());
        assert!(
            upper(&make(1. / 3.), &[0., 1. / 3.], &budget, ATTEMPT_WORK)
                .0
                .is_none()
        );
        assert!(
            upper(&m, &[0., 1. / 3.], &budget, 1).0.is_none(),
            "work cap"
        );
        cancel.cancel();
        assert!(upper(&m, &[0., 1. / 3.], &budget, ATTEMPT_WORK).0.is_none());
    }
}

#[cfg(test)]
mod incompatible_decimal_test {
    use super::*;
    use crate::NativeOptions;
    use lp_ip_craft::*;
    #[test]
    fn numerical_decimal_equality_is_not_an_exact_integer_certificate() {
        let model = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![0.; 3],
            objective_offset: 0.,
            variables: Bounds::new(vec![60., 0., 0.], vec![60., 100., 100.]),
            kinds: vec![
                VariableKind::Continuous,
                VariableKind::Integer,
                VariableKind::Integer,
            ],
            matrix: CsrMatrix::from_dense(1, 3, &[0.9, 1., -1.]).unwrap(),
            rows: Bounds::fixed(vec![0.]),
        })
        .unwrap();
        let x = [60., 0., 54.];
        assert!(
            check::evaluate(&model, &x, &Tolerances::default())
                .unwrap()
                .feasible_candidate
        );
        let s = NativeOptions::default();
        let c = CancellationToken::default();
        let b = Budget {
            start: std::time::Instant::now(),
            settings: &s,
            cancel: &c,
            pivots: 0,
        };
        assert!(upper(&model, &x, &b, ATTEMPT_WORK).0.is_none());
    }
}
