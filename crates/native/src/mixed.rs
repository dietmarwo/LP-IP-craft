//! Outward-rounded mixed-row propagation. No rounded activity proves infeasibility.
use crate::{Budget, incumbent, root};
use lp_ip_craft::{Bounds, Model};
use std::collections::VecDeque;
#[derive(Default)]
pub(crate) struct Stats {
    pub rows: u64,
    pub visits: u64,
    pub terms: u64,
    pub changes: u64,
    pub infeasible: u64,
    pub limits: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct Reason {
    pub row: usize,
    pub column: usize,
    pub old: (f64, f64),
    pub new: (f64, f64),
}
pub(crate) struct Domain {
    rows: Vec<usize>,
    incident: Vec<Vec<usize>>,
    last: Vec<usize>,
    first: bool,
    pub stats: Stats,
    pub reasons: Vec<Reason>,
}
fn add(a: f64, b: f64, lower: bool) -> Option<f64> {
    incumbent::dot(std::iter::once((1., b)), a).map(|v| if lower { v.0 } else { v.1 })
}
impl Domain {
    pub fn new(model: &Model, enabled: bool) -> Self {
        let mut rows = Vec::new();
        let mut incident = vec![Vec::new(); model.n()];
        let a = model.matrix();
        if enabled && model.is_mip() {
            for i in 0..model.m() {
                let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
                let exact = range.clone().all(|k| {
                    let (j, c) = (a.pattern.indices[k], a.values[k]);
                    c == 0.
                        || (model.kinds()[j].is_integral()
                            && c.fract() == 0.
                            && c.abs() <= 2147483648.
                            && model.variables().lower[j].abs() <= 2147483648.
                            && model.variables().upper[j].abs() <= 2147483648.)
                }) && [model.rows().lower[i].ceil(), model.rows().upper[i].floor()]
                    .iter()
                    .any(|v| v.is_finite() && v.abs() <= 1125899906842624.);
                if exact
                    || !range.clone().all(|k| {
                        let j = a.pattern.indices[k];
                        model.variables().lower[j].is_finite()
                            && model.variables().upper[j].is_finite()
                    })
                {
                    continue;
                }
                let r = rows.len();
                rows.push(i);
                for k in range {
                    if a.values[k] != 0. {
                        incident[a.pattern.indices[k]].push(r);
                    }
                }
            }
        }
        let stats = Stats {
            rows: rows.len() as u64,
            ..Default::default()
        };
        Self {
            rows,
            incident,
            last: Vec::new(),
            first: true,
            stats,
            reasons: Vec::new(),
        }
    }
    pub fn apply(
        &mut self,
        model: &Model,
        bounds: &mut Bounds,
        changed: &mut Vec<usize>,
        budget: &Budget<'_>,
    ) -> bool {
        self.reasons.clear();
        if self.rows.is_empty() {
            return true;
        }
        let mut queue = VecDeque::new();
        let mut queued = vec![false; self.rows.len()];
        let enqueue = |r: usize, q: &mut VecDeque<usize>, flags: &mut [bool]| {
            if !flags[r] {
                flags[r] = true;
                q.push_back(r);
            }
        };
        if self.first {
            for r in 0..self.rows.len() {
                enqueue(r, &mut queue, &mut queued);
            }
            self.first = false;
        } else {
            for &j in self.last.iter().chain(changed.iter()) {
                for &r in &self.incident[j] {
                    enqueue(r, &mut queue, &mut queued);
                }
            }
        }
        self.last = changed.clone();
        let a = model.matrix();
        let cap = model
            .nnz()
            .saturating_mul(8)
            .saturating_add(model.m() * 32)
            .clamp(4096, 200000);
        let mut work = 0;
        while let Some(r) = queue.pop_front() {
            queued[r] = false;
            if work >= cap || budget.stop().is_some() {
                self.stats.limits += u64::from(work >= cap);
                break;
            }
            let i = self.rows[r];
            self.stats.visits += 1;
            let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
            let entries: Vec<_> = range
                .filter(|&k| a.values[k] != 0.)
                .map(|k| (a.pattern.indices[k], a.values[k]))
                .collect();
            work += entries.len();
            self.stats.terms += entries.len() as u64;
            if work >= cap {
                self.stats.limits += 1;
                break;
            }
            let n = entries.len();
            let mut pmin = vec![0.; n + 1];
            let mut pmax = vec![0.; n + 1];
            let mut smin = vec![0.; n + 1];
            let mut smax = vec![0.; n + 1];
            let mut terms = Vec::new();
            let mut valid = true;
            for &(j, c) in &entries {
                let (l, u) = if c > 0. {
                    (bounds.lower[j], bounds.upper[j])
                } else {
                    (bounds.upper[j], bounds.lower[j])
                };
                let pair = incumbent::dot(std::iter::once((c, l)), 0.)
                    .zip(incumbent::dot(std::iter::once((c, u)), 0.));
                if let Some((mn, mx)) = pair {
                    terms.push((mn.0, mx.1));
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            for k in 0..n {
                if let Some((mn, mx)) =
                    add(pmin[k], terms[k].0, true).zip(add(pmax[k], terms[k].1, false))
                {
                    pmin[k + 1] = mn;
                    pmax[k + 1] = mx;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            for k in (0..n).rev() {
                if let Some((mn, mx)) =
                    add(smin[k + 1], terms[k].0, true).zip(add(smax[k + 1], terms[k].1, false))
                {
                    smin[k] = mn;
                    smax[k] = mx;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            if pmin[n] > model.rows().upper[i] || pmax[n] < model.rows().lower[i] {
                self.stats.infeasible += 1;
                return false;
            }
            for (k, &(j, c)) in entries.iter().enumerate() {
                if work >= cap || budget.stop().is_some() {
                    self.stats.limits += u64::from(work >= cap);
                    return true;
                }
                work += 1;
                self.stats.terms += 1;
                let Some((mn, mx)) =
                    add(pmin[k], smin[k + 1], true).zip(add(pmax[k], smax[k + 1], false))
                else {
                    continue;
                };
                let old = (bounds.lower[j], bounds.upper[j]);
                let (mut lo, mut hi) = old;
                if model.rows().upper[i].is_finite() {
                    if let Some(rhs) = add(model.rows().upper[i], -mn, false) {
                        if let Some(v) = root::quotient(c, rhs, c < 0.) {
                            if c > 0. {
                                hi = hi.min(v);
                            } else {
                                lo = lo.max(v);
                            }
                        }
                    }
                }
                if model.rows().lower[i].is_finite() {
                    if let Some(rhs) = add(model.rows().lower[i], -mx, true) {
                        if let Some(v) = root::quotient(c, rhs, c > 0.) {
                            if c > 0. {
                                lo = lo.max(v);
                            } else {
                                hi = hi.min(v);
                            }
                        }
                    }
                }
                if model.kinds()[j].is_integral() {
                    lo = lo.ceil();
                    hi = hi.floor();
                }
                if lo > hi {
                    self.stats.infeasible += 1;
                    return false;
                }
                // Ignore tiny continuous progress; keeping a wider box is always sound.
                if !model.kinds()[j].is_integral() {
                    if lo - old.0 <= 1e-12 * (1. + old.0.abs()) {
                        lo = old.0;
                    }
                    if old.1 - hi <= 1e-12 * (1. + old.1.abs()) {
                        hi = old.1;
                    }
                }
                if (lo, hi) != old {
                    bounds.lower[j] = lo;
                    bounds.upper[j] = hi;
                    changed.push(j);
                    self.last.push(j);
                    self.reasons.push(Reason {
                        row: i,
                        column: j,
                        old,
                        new: (lo, hi),
                    });
                    debug_assert!(self.reasons.last().is_some_and(|r| r.row < model.m()
                        && r.column < model.n()
                        && r.new.0 >= r.old.0
                        && r.new.1 <= r.old.1));
                    self.stats.changes += 1;
                    for &rr in &self.incident[j] {
                        enqueue(rr, &mut queue, &mut queued);
                    }
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
    #[test]
    fn mixed_dyadic_feasible_grid_and_sibling_restoration() {
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: std::time::Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        for trial in 0..30 {
            let c = 0.5 + (trial % 3) as f64 * 0.25;
            let model = Model::new(ModelData {
                sense: Sense::Minimize,
                objective: vec![0.; 3],
                objective_offset: 0.,
                variables: Bounds::new(vec![0.; 3], vec![2.; 3]),
                kinds: vec![
                    VariableKind::Integer,
                    VariableKind::Continuous,
                    VariableKind::Continuous,
                ],
                matrix: CsrMatrix::from_dense(2, 3, &[1., c, -0.5, -1., 0.5, c]).unwrap(),
                rows: Bounds::new(vec![-0.25, -0.75], vec![1.5, 1.25]),
            })
            .unwrap();
            let mut d = Domain::new(&model, true);
            for value in [0., 2., 1., 0.] {
                let mut b = model.variables().clone();
                b.lower[0] = value;
                b.upper[0] = value;
                let before = b.clone();
                let mut changed = vec![0];
                let ok = d.apply(&model, &mut b, &mut changed, &budget);
                for y in 0..=8 {
                    for z in 0..=8 {
                        let x = [value, y as f64 / 4., z as f64 / 4.];
                        if check::evaluate(&model, &x, &Tolerances::default())
                            .unwrap()
                            .feasible_candidate
                        {
                            assert!(ok);
                            assert!((0..3).all(|j| x[j] >= b.lower[j] && x[j] <= b.upper[j]));
                        }
                    }
                }
                for reason in &d.reasons {
                    assert!(
                        reason.row < 2
                            && reason.column < 3
                            && reason.new.0 >= reason.old.0
                            && reason.new.1 <= reason.old.1
                    );
                }
                assert_eq!(before.lower[0], b.lower[0]);
            }
        }
    }
}
