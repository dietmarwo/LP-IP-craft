//! Conservative integer-domain propagation with incremental exact activities.
use crate::Budget;
use lp_ip_craft::{Bounds, Model};
use std::collections::VecDeque;

struct Row {
    terms: Vec<(usize, i128)>,
    lower: Option<i128>,
    upper: Option<i128>,
    min: i128,
    max: i128,
    max_width: i128,
}
pub(crate) struct Domain {
    integers: Vec<usize>,
    rows: Vec<Row>,
    incident: Vec<Vec<(usize, i128)>>,
    lower: Vec<i128>,
    upper: Vec<i128>,
    pending: VecDeque<usize>,
    queued: Vec<bool>,
    last_changed: Vec<usize>,
    touched: Vec<usize>,
    marked: Vec<bool>,
    first: bool,
    work_limit: usize,
}
fn floor_div(a: i128, b: i128) -> i128 {
    debug_assert!(b > 0);
    a.div_euclid(b)
}
fn ceil_div(a: i128, b: i128) -> i128 {
    -floor_div(-a, b)
}
impl Domain {
    pub fn new(model: &Model, enabled: bool) -> Self {
        let n = if enabled && model.is_mip() {
            model.n()
        } else {
            0
        };
        let mut domain = Self {
            integers: Vec::new(),
            rows: Vec::new(),
            incident: vec![Vec::new(); n],
            lower: model
                .variables()
                .lower
                .iter()
                .take(n)
                .map(|v| v.ceil() as i128)
                .collect(),
            upper: model
                .variables()
                .upper
                .iter()
                .take(n)
                .map(|v| v.floor() as i128)
                .collect(),
            pending: VecDeque::new(),
            queued: Vec::new(),
            last_changed: Vec::new(),
            touched: Vec::new(),
            marked: vec![false; n],
            first: true,
            work_limit: 4096,
        };
        if !enabled || !model.is_mip() {
            return domain;
        }
        domain.integers = (0..n)
            .filter(|&j| {
                model.kinds()[j].is_integral()
                    && (model.variables().lower[j].fract() != 0.
                        || model.variables().upper[j].fract() != 0.)
            })
            .collect();
        let a = model.matrix();
        for i in 0..model.m() {
            let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
            // Each product is at most 2^62; at most usize::MAX terms means
            // every activity and activity difference fits signed i128.
            if !range.clone().all(|k| {
                let (j, v) = (a.pattern.indices[k], a.values[k]);
                v == 0.
                    || (model.kinds()[j].is_integral()
                        && v.fract() == 0.
                        && v.abs() <= 2_147_483_648.
                        && model.variables().lower[j].abs() <= 2_147_483_648.
                        && model.variables().upper[j].abs() <= 2_147_483_648.)
            }) {
                continue;
            }
            let endpoint =
                |v: f64| (v.is_finite() && v.abs() <= 1_125_899_906_842_624.).then_some(v as i128);
            let lower = endpoint(model.rows().lower[i].ceil());
            let upper = endpoint(model.rows().upper[i].floor());
            if lower.is_none() && upper.is_none() {
                continue;
            }
            let terms: Vec<_> = range
                .filter(|&k| a.values[k] != 0.)
                .map(|k| (a.pattern.indices[k], a.values[k] as i128))
                .collect();
            let (mut min, mut max) = (0, 0);
            let mut max_width = 0;
            for &(j, v) in &terms {
                let (l, u) = (domain.lower[j], domain.upper[j]);
                max_width = max_width.max(v.abs() * (u - l));
                min += v * if v > 0 { l } else { u };
                max += v * if v > 0 { u } else { l };
                domain.incident[j].push((domain.rows.len(), v));
            }
            domain.rows.push(Row {
                terms,
                lower,
                upper,
                min,
                max,
                max_width,
            });
        }
        domain.queued = vec![false; domain.rows.len()];
        let nnz = domain.rows.iter().map(|r| r.terms.len()).sum::<usize>();
        domain.work_limit = nnz
            .saturating_mul(16)
            .saturating_add(domain.rows.len().saturating_mul(64))
            .max(4096);
        domain
    }
    fn enqueue(&mut self, row: usize) {
        if !self.queued[row] {
            self.queued[row] = true;
            self.pending.push_back(row);
        }
    }
    fn synchronize(&mut self, j: usize, l: i128, u: i128) {
        let (old_l, old_u) = (self.lower[j], self.upper[j]);
        if l == old_l && u == old_u {
            return;
        }
        self.lower[j] = l;
        self.upper[j] = u;
        for k in 0..self.incident[j].len() {
            let (i, a) = self.incident[j][k];
            let row = &mut self.rows[i];
            row.min += a * if a > 0 { l - old_l } else { u - old_u };
            row.max += a * if a > 0 { u - old_u } else { l - old_l };
            self.enqueue(i);
        }
    }
    /// Appended columns, including every implication, must be restored on exit.
    /// Across nodes synchronize both the preceding and current changed lists:
    /// this accounts for bounds restored by NodeBox before entering a sibling.
    pub fn apply(
        &mut self,
        bounds: &mut Bounds,
        changed: &mut Vec<usize>,
        budget: &Budget<'_>,
    ) -> bool {
        for &j in &self.integers {
            let (l, u) = (bounds.lower[j].ceil(), bounds.upper[j].floor());
            if l > u {
                return false;
            }
            if l != bounds.lower[j] || u != bounds.upper[j] {
                bounds.lower[j] = l;
                bounds.upper[j] = u;
                changed.push(j);
            }
        }
        if self.rows.is_empty() {
            return true;
        }
        while let Some(i) = self.pending.pop_front() {
            self.queued[i] = false;
        }
        self.touched.clear();
        for &j in self.last_changed.iter().chain(changed.iter()) {
            if !self.marked[j] {
                self.marked[j] = true;
                self.touched.push(j);
            }
        }
        for k in 0..self.touched.len() {
            let j = self.touched[k];
            self.marked[j] = false;
            self.synchronize(j, bounds.lower[j] as i128, bounds.upper[j] as i128);
        }
        self.last_changed.clear();
        self.last_changed.extend_from_slice(changed);
        if self.first {
            for i in 0..self.rows.len() {
                self.enqueue(i);
            }
            self.first = false;
        }
        let mut work = 0usize;
        while let Some(i) = self.pending.pop_front() {
            self.queued[i] = false;
            if work >= self.work_limit || budget.stop().is_some() {
                return true;
            }
            let row = &self.rows[i];
            if row.lower.is_some_and(|l| row.max < l) || row.upper.is_some_and(|u| row.min > u) {
                return false;
            }
            // Root domain widths are conservative upper bounds at every child.
            // If neither side can tighten even the widest term, skip the row.
            if row.upper.is_none_or(|b| b - row.min >= row.max_width)
                && row.lower.is_none_or(|b| row.max - b >= row.max_width)
            {
                continue;
            }
            for k in 0..self.rows[i].terms.len() {
                work = work.saturating_add(1);
                if work % 1024 == 0 && (work >= self.work_limit || budget.stop().is_some()) {
                    return true;
                }
                let row = &self.rows[i];
                let (j, a) = row.terms[k];
                let (l, u) = (self.lower[j], self.upper[j]);
                let width = a.abs() * (u - l);
                let amin = a * if a > 0 { l } else { u };
                let amax = a * if a > 0 { u } else { l };
                let (mut lo, mut hi) = (l, u);
                if a > 0 {
                    if let Some(b) = row.upper.filter(|&b| b - row.min < width) {
                        hi = hi.min(floor_div(b - (row.min - amin), a));
                    }
                    if let Some(b) = row.lower.filter(|&b| row.max - b < width) {
                        lo = lo.max(ceil_div(b - (row.max - amax), a));
                    }
                } else {
                    if let Some(b) = row.upper.filter(|&b| b - row.min < width) {
                        lo = lo.max(ceil_div((row.min - amin) - b, -a));
                    }
                    if let Some(b) = row.lower.filter(|&b| row.max - b < width) {
                        hi = hi.min(floor_div((row.max - amax) - b, -a));
                    }
                }
                if lo > hi {
                    return false;
                }
                if lo != l || hi != u {
                    bounds.lower[j] = lo as f64;
                    bounds.upper[j] = hi as f64;
                    changed.push(j);
                    self.last_changed.push(j);
                    self.synchronize(j, lo, hi);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeOptions;
    use lp_ip_craft::*;
    fn apply(domain: &mut Domain, bounds: &mut Bounds, changed: &mut Vec<usize>) -> bool {
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: std::time::Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        domain.apply(bounds, changed, &budget)
    }
    #[test]
    fn long_rows_root_implications_and_sibling_restoration() {
        let n = 40;
        let mut values = vec![0.; n * n];
        let mut lo = vec![f64::NEG_INFINITY; n];
        let mut hi = vec![0.; n];
        // Reverse chain order requires more than the old three full scans.
        for i in 0..n - 1 {
            let j = n - 2 - i;
            values[i * n + j] = 1.;
            values[i * n + j + 1] = -1.;
        }
        for j in 0..n {
            values[(n - 1) * n + j] = 1.;
        }
        lo[n - 1] = 20.;
        hi[n - 1] = 40.;
        let model = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![0.; n],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![1.; n]),
            kinds: vec![VariableKind::Binary; n],
            matrix: CsrMatrix::from_dense(n, n, &values).unwrap(),
            rows: Bounds::new(lo, hi),
        })
        .unwrap();
        let mut d = Domain::new(&model, true);
        let mut b = model.variables().clone();
        let mut c = vec![0];
        b.lower[0] = 1.;
        assert!(apply(&mut d, &mut b, &mut c));
        assert!(b.lower.iter().all(|&v| v == 1.));
        // NodeBox restores every implication before moving to the sibling.
        b = model.variables().clone();
        b.upper[n - 1] = 0.;
        c = vec![n - 1];
        assert!(!apply(&mut d, &mut b, &mut c));
        b = model.variables().clone();
        c.clear();
        assert!(apply(&mut d, &mut b, &mut c));
        assert_eq!(b.lower[0], 0.);
        assert_eq!(b.upper[n - 1], 1.);
    }
    #[test]
    fn propagation_never_removes_an_enumerated_integer_point() {
        let n = 10;
        let mut seed = 38471u64;
        for trial in 0..80 {
            let mut next = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((seed >> 32) % 7) as i32 - 3
            };
            let data: Vec<f64> = (0..4 * n).map(|_| next() as f64).collect();
            let witness: Vec<f64> = (0..n).map(|_| f64::from(next() > 0)).collect();
            let center: Vec<f64> = data
                .chunks(n)
                .map(|r| r.iter().zip(&witness).map(|(a, x)| a * x).sum())
                .collect();
            let m = Model::new(ModelData {
                sense: Sense::Minimize,
                objective: vec![0.; n],
                objective_offset: 0.,
                variables: Bounds::new(vec![0.; n], vec![1.; n]),
                kinds: vec![VariableKind::Binary; n],
                matrix: CsrMatrix::from_dense(4, n, &data).unwrap(),
                rows: Bounds::new(
                    center.iter().map(|v| v - 1.3).collect(),
                    center.iter().map(|v| v + 1.8).collect(),
                ),
            })
            .unwrap();
            let mut domain = Domain::new(&m, true);
            for branch in 0..5 {
                let mut b = m.variables().clone();
                let j = (trial + branch) % n;
                let v = f64::from(branch % 2 == 0);
                b.lower[j] = v;
                b.upper[j] = v;
                let before = b.clone();
                let mut changed = vec![j];
                let ok = apply(&mut domain, &mut b, &mut changed);
                for mask in 0..1usize << n {
                    let x: Vec<f64> = (0..n).map(|j| ((mask >> j) & 1) as f64).collect();
                    if x.iter()
                        .enumerate()
                        .any(|(j, &v)| v < before.lower[j] || v > before.upper[j])
                    {
                        continue;
                    }
                    if !check::evaluate(&m, &x, &Tolerances::default())
                        .unwrap()
                        .feasible_candidate
                    {
                        continue;
                    }
                    assert!(ok, "trial {trial} branch {branch}");
                    assert!(
                        x.iter()
                            .enumerate()
                            .all(|(j, &v)| v >= b.lower[j] && v <= b.upper[j])
                    );
                }
            }
        }
    }
}
