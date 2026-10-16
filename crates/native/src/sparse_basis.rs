//! Sparse LU plus classical product-form and bordered-row basis updates.
//! Work vectors are dense; matrices and factors never are. No entry dropping.
use lp_ip_craft::Status;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type Column = Vec<(usize, f64)>;

#[derive(Clone)]
pub(crate) struct Lu {
    perm: Vec<usize>,
    lower: Vec<Column>,
    upper: Vec<Column>,
    diagonal: Vec<f64>,
    pub peak_entries: usize,
}
impl Lu {
    pub fn identity(n: usize) -> Self {
        Self {
            perm: (0..n).collect(),
            lower: vec![vec![]; n],
            upper: vec![vec![]; n],
            diagonal: vec![1.; n],
            peak_entries: n,
        }
    }
    pub fn entries(&self) -> usize {
        self.diagonal.len()
            + self.lower.iter().map(Vec::len).sum::<usize>()
            + self.upper.iter().map(Vec::len).sum::<usize>()
    }
    pub fn factor(
        columns: &[Column],
        basic: &[usize],
        cap: usize,
        mut stop: impl FnMut() -> Option<Status>,
    ) -> std::result::Result<Self, Status> {
        let n = basic.len();
        let mut rows = vec![BTreeMap::new(); n];
        let mut by_column = vec![BTreeSet::new(); n];
        let mut entries = 0;
        for (j, &id) in basic.iter().enumerate() {
            for &(i, v) in &columns[id] {
                if v != 0. {
                    rows[i].insert(j, v);
                    by_column[j].insert(i);
                    entries += 1;
                    if entries > cap {
                        return Err(Status::NumericalFailure);
                    }
                }
            }
        }
        let mut perm: Vec<_> = (0..n).collect();
        let mut position = perm.clone();
        let mut lower = vec![vec![]; n];
        let mut lower_entries = 0;
        let mut peak_entries = entries;
        for k in 0..n {
            if let Some(s) = stop() {
                return Err(s);
            }
            let maximum = by_column[k]
                .iter()
                .filter(|&&i| position[i] >= k)
                .map(|&i| rows[i][&k].abs())
                .fold(0., f64::max);
            if maximum == 0. || !maximum.is_finite() {
                return Err(Status::NumericalFailure);
            }
            // Threshold row pivoting, with a sparse-row preference. This is
            // not a global Markowitz ordering or a stability guarantee.
            let r = *by_column[k]
                .iter()
                .filter(|&&i| position[i] >= k && rows[i][&k].abs() >= 0.1 * maximum)
                .min_by_key(|&&i| (rows[i].len(), i))
                .ok_or(Status::NumericalFailure)?;
            let old = perm[k];
            let p = position[r];
            perm.swap(k, p);
            position[old] = p;
            position[r] = k;
            let pivot = rows[r][&k];
            let tail: Vec<_> = rows[r].range(k + 1..).map(|(&j, &v)| (j, v)).collect();
            let affected: Vec<_> = by_column[k]
                .iter()
                .copied()
                .filter(|&i| position[i] > k)
                .collect();
            for i in affected {
                if let Some(s) = stop() {
                    return Err(s);
                }
                let multiplier = rows[i].remove(&k).unwrap() / pivot;
                by_column[k].remove(&i);
                entries -= 1;
                if !multiplier.is_finite() {
                    return Err(Status::NumericalFailure);
                }
                lower[i].push((k, multiplier));
                lower_entries += 1;
                for &(j, u) in &tail {
                    let previous = rows[i].get(&j).copied();
                    let v = previous.unwrap_or(0.) - multiplier * u;
                    if !v.is_finite() {
                        return Err(Status::NumericalFailure);
                    }
                    if v == 0. {
                        if previous.is_some() {
                            rows[i].remove(&j);
                            by_column[j].remove(&i);
                            entries -= 1;
                        }
                    } else {
                        rows[i].insert(j, v);
                        if previous.is_none() {
                            by_column[j].insert(i);
                            entries += 1;
                        }
                    }
                    peak_entries = peak_entries.max(entries + lower_entries);
                    if entries + lower_entries > cap {
                        return Err(Status::NumericalFailure);
                    }
                }
            }
        }
        let mut out = Self::identity(n);
        for (k, &r) in perm.iter().enumerate() {
            out.diagonal[k] = rows[r][&k];
            out.lower[k] = std::mem::take(&mut lower[r]);
            out.upper[k] = rows[r].range(k + 1..).map(|(&j, &v)| (j, v)).collect();
        }
        out.perm = perm;
        out.peak_entries = peak_entries;
        Ok(out)
    }
    fn forward(&self, rhs: &[f64]) -> Vec<f64> {
        let n = self.perm.len();
        let mut x = rhs.to_vec();
        for i in 0..n {
            x[i] = rhs[self.perm[i]];
            for &(j, v) in &self.lower[i] {
                x[i] -= v * x[j];
            }
        }
        for i in (0..n).rev() {
            for &(j, v) in &self.upper[i] {
                x[i] -= v * x[j];
            }
            x[i] /= self.diagonal[i];
        }
        x
    }
    fn transpose(&self, rhs: &[f64]) -> Vec<f64> {
        let n = self.perm.len();
        let mut work = rhs.to_vec();
        for i in 0..n {
            work[i] /= self.diagonal[i];
            for &(j, v) in &self.upper[i] {
                work[j] -= v * work[i];
            }
        }
        for i in (0..n).rev() {
            for &(j, v) in &self.lower[i] {
                work[j] -= v * work[i];
            }
        }
        let mut x = work.clone();
        for i in 0..n {
            x[self.perm[i]] = work[i];
        }
        x
    }
}

#[derive(Clone)]
enum Update {
    Column { row: usize, pivot: f64, off: Column },
    Border { row: usize, coefficients: Column },
}
#[derive(Clone)]
pub(crate) struct Basis {
    pub lu: Lu,
    updates: Vec<Update>,
}
impl Basis {
    pub fn identity(n: usize) -> Self {
        Self {
            lu: Lu::identity(n),
            updates: vec![],
        }
    }
    pub fn entries(&self) -> usize {
        self.lu.entries()
            + self
                .updates
                .iter()
                .map(|u| match u {
                    Update::Column { off, .. } => 1 + off.len(),
                    Update::Border { coefficients, .. } => 1 + coefficients.len(),
                })
                .sum::<usize>()
    }
    pub fn updates(&self) -> usize {
        self.updates.len()
    }
    pub fn refactor(&mut self, lu: Lu) {
        self.lu = lu;
        self.updates.clear();
    }
    pub fn replace(&mut self, row: usize, direction: &[f64]) {
        self.updates.push(Update::Column {
            row,
            pivot: direction[row],
            off: direction
                .iter()
                .enumerate()
                .filter_map(|(i, &v)| (i != row && v != 0.).then_some((i, v)))
                .collect(),
        });
    }
    pub fn border(&mut self, row: usize, coefficients: Column) {
        self.updates.push(Update::Border { row, coefficients });
    }
    pub fn forward(&self, rhs: &[f64]) -> Vec<f64> {
        let mut x = self.lu.forward(rhs);
        for u in &self.updates {
            match u {
                Update::Column { row, pivot, off } => {
                    x[*row] /= pivot;
                    for &(i, v) in off {
                        x[i] -= v * x[*row];
                    }
                }
                Update::Border { row, coefficients } => {
                    for &(i, v) in coefficients {
                        x[*row] -= v * x[i];
                    }
                }
            }
        }
        x
    }
    pub fn transpose(&self, rhs: &[f64]) -> Vec<f64> {
        let mut x = rhs.to_vec();
        for u in self.updates.iter().rev() {
            match u {
                Update::Column { row, pivot, off } => {
                    for &(i, v) in off {
                        x[*row] -= v * x[i];
                    }
                    x[*row] /= pivot;
                }
                Update::Border { row, coefficients } => {
                    for &(i, v) in coefficients {
                        x[i] -= v * x[*row];
                    }
                }
            }
        }
        self.lu.transpose(&x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(columns: &[Column], basic: &[usize], b: &Basis) {
        let n = basic.len();
        let rhs: Vec<_> = (0..n).map(|i| (i % 7) as f64 - 3.).collect();
        for transpose in [false, true] {
            let x = if transpose {
                b.transpose(&rhs)
            } else {
                b.forward(&rhs)
            };
            let mut actual = vec![0.; n];
            for (j, &id) in basic.iter().enumerate() {
                for &(i, a) in &columns[id] {
                    if transpose {
                        actual[j] += a * x[i];
                    } else {
                        actual[i] += a * x[j];
                    }
                }
            }
            for (a, r) in actual.iter().zip(&rhs) {
                assert!((a - r).abs() < 1e-8, "{actual:?} != {rhs:?}");
            }
        }
    }
    #[test]
    fn sparse_lu_permutation_and_fill() {
        for n in 2..35 {
            let columns: Vec<Column> = (0..n)
                .map(|j| {
                    (0..n)
                        .filter_map(|i| {
                            let a = if i == (j + 1) % n {
                                6.
                            } else if (i * 17 + j * 3) % 11 < 2 {
                                -0.25
                            } else {
                                0.
                            };
                            (a != 0.).then_some((i, a))
                        })
                        .collect()
                })
                .collect();
            let basic: Vec<_> = (0..n).collect();
            let lu = Lu::factor(&columns, &basic, 100_000, || None).unwrap();
            check(
                &columns,
                &basic,
                &Basis {
                    lu,
                    updates: vec![],
                },
            );
        }
    }
    #[test]
    fn column_and_border_updates_then_refactor() {
        let mut columns: Vec<Column> = (0..4).map(|i| vec![(i, 1.)]).collect();
        let mut basic: Vec<_> = (0..4).collect();
        let mut b = Basis::identity(4);
        columns.push(vec![(0, 2.), (1, -1.), (3, 0.5)]);
        b.replace(0, &b.forward(&[2., -1., 0., 0.5]));
        basic[0] = 4;
        check(&columns, &basic, &b);
        columns[0].push((4, 3.));
        columns[2].push((4, -2.));
        columns.push(vec![(4, 1.)]);
        basic.push(5);
        b.border(4, vec![(2, -2.)]);
        check(&columns, &basic, &b);
        let direction = b.forward(&[1., 0., 0., 0., 3.]);
        b.replace(1, &direction);
        basic[1] = 0;
        check(&columns, &basic, &b);
        b.refactor(Lu::factor(&columns, &basic, 1000, || None).unwrap());
        check(&columns, &basic, &b);
    }
    #[test]
    fn singular_cap_and_cancellation() {
        assert!(Lu::factor(&[vec![]], &[0], 100, || None).is_err());
        assert!(Lu::factor(&[vec![(0, 1.)]], &[0], 0, || None).is_err());
        assert!(matches!(
            Lu::factor(&[vec![(0, 1.)]], &[0], 10, || Some(Status::Cancelled)),
            Err(Status::Cancelled)
        ));
    }
}
