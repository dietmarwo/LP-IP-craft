//! Sparse-column two-phase primal and dual revised simplex, max c'x.
use crate::{
    Budget,
    sparse_basis::{Basis, Column, Lu},
};
use lp_ip_craft::{Error, Result, Status};

#[derive(Clone)]
pub(crate) struct Revised {
    pub n: usize,
    m: usize,
    columns: Vec<Column>,
    c: Vec<f64>,
    rhs: Vec<f64>,
    basic: Vec<usize>,
    is_basic: Vec<bool>,
    artificial: usize,
    factor: Basis,
    x: Vec<f64>,
    pi: Vec<f64>,
    matrix_entries: usize,
    cap: usize,
    pub peak_entries: usize,
    pub refactors: u64,
    pub updates: u64,
}
impl Revised {
    pub fn empty(m: usize, c: &[f64], cap: usize) -> Self {
        Self {
            n: c.len(),
            m,
            columns: vec![vec![]; c.len()],
            c: c.to_vec(),
            rhs: vec![0.; m],
            basic: vec![],
            is_basic: vec![],
            artificial: 0,
            factor: Basis::identity(0),
            x: vec![],
            pi: vec![],
            matrix_entries: 0,
            cap,
            peak_entries: 0,
            refactors: 0,
            updates: 0,
        }
    }
    pub fn set(&mut self, i: usize, j: usize, v: f64) -> Result<()> {
        if j == self.n + 1 {
            self.rhs[i] = v;
        } else if v != 0. {
            if self.matrix_entries >= self.cap {
                return Err(Error::Unsupported(
                    "sparse matrix exceeds max_sparse_entries".into(),
                ));
            }
            self.columns[j].push((i, v));
            self.matrix_entries += 1;
        }
        Ok(())
    }
    pub fn finish(&mut self, m: usize) -> Result<()> {
        self.m = m;
        self.rhs.truncate(m);
        if self.matrix_entries.saturating_add(m.saturating_mul(3)) > self.cap {
            return Err(Error::Unsupported(
                "initial sparse state exceeds max_sparse_entries".into(),
            ));
        }
        self.basic = (self.n..self.n + m).collect();
        self.columns.extend((0..m).map(|i| vec![(i, 1.)]));
        self.artificial = self.columns.len();
        self.columns.push((0..m).map(|i| (i, -1.)).collect());
        self.c.resize(self.columns.len(), 0.);
        self.is_basic = (0..self.columns.len())
            .map(|j| j >= self.n && j < self.n + m)
            .collect();
        self.factor = Basis::identity(m);
        self.matrix_entries += 2 * m;
        self.record();
        Ok(())
    }
    pub fn entries(&self) -> usize {
        self.matrix_entries + self.factor.entries()
    }
    fn record(&mut self) {
        self.peak_entries = self.peak_entries.max(self.entries());
    }
    fn column(&self, j: usize) -> Vec<f64> {
        let mut a = vec![0.; self.m];
        for &(i, v) in &self.columns[j] {
            a[i] = v;
        }
        a
    }
    fn dot(&self, j: usize, y: &[f64]) -> f64 {
        self.columns[j].iter().map(|&(i, a)| a * y[i]).sum()
    }
    fn cost(&self, j: usize, phase: usize) -> f64 {
        if phase == 1 {
            if j == self.artificial { -1. } else { 0. }
        } else {
            self.c[j]
        }
    }
    fn refactor(&mut self, budget: &Budget<'_>) -> std::result::Result<(), Status> {
        // Account for the old representation while constructing the new LU.
        let cap = self.cap.saturating_sub(self.entries());
        let lu = Lu::factor(&self.columns, &self.basic, cap, || budget.stop())?;
        self.peak_entries = self.peak_entries.max(self.entries() + lu.peak_entries);
        self.factor.refactor(lu);
        self.refactors += 1;
        self.record();
        Ok(())
    }
    fn refresh(&mut self, phase: usize, budget: &Budget<'_>) -> std::result::Result<(), Status> {
        if self.factor.updates() >= budget.settings.sparse_refactor_interval {
            self.refactor(budget)?;
        }
        self.x = self.factor.forward(&self.rhs);
        let cb: Vec<_> = self.basic.iter().map(|&j| self.cost(j, phase)).collect();
        self.pi = self.factor.transpose(&cb);
        if self.x.iter().chain(&self.pi).any(|v| !v.is_finite()) {
            return Err(Status::NumericalFailure);
        }
        // Check both basis solves, and refine once before trusting pricing.
        for transpose in [false, true] {
            for attempt in 0..2 {
                let mut residual = if transpose {
                    cb.clone()
                } else {
                    self.rhs.clone()
                };
                let mut scale: Vec<f64> = residual.iter().map(|v| 1. + v.abs()).collect();
                for (k, &j) in self.basic.iter().enumerate() {
                    for &(i, a) in &self.columns[j] {
                        let (r, v) = if transpose {
                            (k, a * self.pi[i])
                        } else {
                            (i, a * self.x[k])
                        };
                        residual[r] -= v;
                        scale[r] += v.abs();
                    }
                }
                if residual
                    .iter()
                    .zip(&scale)
                    .all(|(r, s)| r.abs() <= 1e-10 * s)
                {
                    break;
                }
                if attempt == 1 {
                    return Err(Status::NumericalFailure);
                }
                let correction = if transpose {
                    self.factor.transpose(&residual)
                } else {
                    self.factor.forward(&residual)
                };
                let values = if transpose { &mut self.pi } else { &mut self.x };
                for (v, d) in values.iter_mut().zip(correction) {
                    *v += d;
                }
            }
        }
        Ok(())
    }
    fn pivot(&mut self, r: usize, j: usize, d: &[f64], budget: &mut Budget<'_>) -> Status {
        if !d[r].is_finite() || d[r].abs() <= budget.settings.pivot_tolerance {
            return Status::NumericalFailure;
        }
        self.is_basic[self.basic[r]] = false;
        self.basic[r] = j;
        self.is_basic[j] = true;
        let needed = d.iter().filter(|&&v| v != 0.).count();
        if self.entries().saturating_add(needed) > self.cap {
            if let Err(s) = self.refactor(budget) {
                return s;
            }
        } else {
            self.factor.replace(r, d);
        }
        self.updates += 1;
        budget.pivots += 1;
        self.record();
        Status::Optimal
    }
    fn primal_simplex(&mut self, phase: usize, budget: &mut Budget<'_>, eps: f64) -> Status {
        let mut stalled = 0;
        loop {
            if let Some(s) = budget.stop() {
                return s;
            }
            if let Err(s) = self.refresh(phase, budget) {
                return s;
            }
            if self.x.iter().any(|&x| x < -10. * eps) {
                // A stale update chain can lose primal accuracy on degenerate
                // bases. Rebuild once; do not relax the acceptance tolerance.
                if self.factor.updates() > 0 {
                    if let Err(s) = self
                        .refactor(budget)
                        .and_then(|()| self.refresh(phase, budget))
                    {
                        return s;
                    }
                }
                if self.x.iter().any(|&x| x < -10. * eps) {
                    return Status::NumericalFailure;
                }
            }
            let mut entering = None;
            let mut best = eps;
            for j in 0..self.columns.len() {
                if j % 1024 == 0 {
                    if let Some(s) = budget.stop() {
                        return s;
                    }
                }
                if self.is_basic[j] || j == self.artificial {
                    continue;
                }
                let reduced = self.cost(j, phase) - self.dot(j, &self.pi);
                if !reduced.is_finite() {
                    return Status::NumericalFailure;
                }
                if reduced > eps && (entering.is_none() || reduced > best) {
                    entering = Some(j);
                    best = reduced;
                    if !budget.settings.dantzig_pricing || stalled >= 32 {
                        break;
                    }
                }
            }
            let Some(j) = entering else {
                return Status::Optimal;
            };
            let d = self.factor.forward(&self.column(j));
            if d.iter().any(|v| !v.is_finite()) {
                return Status::NumericalFailure;
            }
            // Harris two-pass ratio test: admit only an eps relaxation of
            // primal feasibility, then prefer the larger pivot. Bland's exact
            // ratio/tie rule takes over after repeated zero-length steps.
            let relaxed = (0..self.m)
                .filter(|&i| d[i] > eps)
                .map(|i| (self.x[i].max(0.) + eps) / d[i])
                .fold(f64::INFINITY, f64::min);
            let r = if stalled >= 32 || !budget.settings.dantzig_pricing {
                (0..self.m).filter(|&i| d[i] > eps).min_by(|&i, &k| {
                    (self.x[i].max(0.) / d[i])
                        .total_cmp(&(self.x[k].max(0.) / d[k]))
                        .then(self.basic[i].cmp(&self.basic[k]))
                })
            } else {
                (0..self.m)
                    .filter(|&i| d[i] > eps && self.x[i].max(0.) / d[i] <= relaxed)
                    .min_by(|&i, &k| {
                        d[k].total_cmp(&d[i])
                            .then(self.basic[i].cmp(&self.basic[k]))
                    })
            };
            let Some(r) = r else {
                return Status::Unbounded;
            };
            if self.x[r].max(0.) / d[r] <= eps {
                stalled += 1;
            } else {
                stalled = 0;
            }
            let s = self.pivot(r, j, &d, budget);
            if s != Status::Optimal {
                return s;
            }
        }
    }
    pub fn solve(&mut self, budget: &mut Budget<'_>, eps: f64) -> Status {
        if let Some(s) = budget.stop() {
            return s;
        }
        if let Some(r) = (0..self.m)
            .min_by(|&i, &k| self.rhs[i].total_cmp(&self.rhs[k]))
            .filter(|&r| self.rhs[r] < -eps)
        {
            let d = self.factor.forward(&self.column(self.artificial));
            let s = self.pivot(r, self.artificial, &d, budget);
            if s != Status::Optimal {
                return s;
            }
            let s = self.primal_simplex(1, budget, eps);
            if s != Status::Optimal {
                return if s == Status::Unbounded {
                    Status::NumericalFailure
                } else {
                    s
                };
            }
            if let Some(r) = self.basic.iter().position(|&j| j == self.artificial) {
                if self.x[r] > eps {
                    return Status::Infeasible;
                }
                // The identity slack columns span every row, so one can
                // replace a zero artificial basic variable without row deletion.
                let mut replacement = None;
                for j in self.n..self.artificial {
                    if let Some(s) = budget.stop() {
                        return s;
                    }
                    if self.is_basic[j] {
                        continue;
                    }
                    let d = self.factor.forward(&self.column(j));
                    if d[r].abs() > eps {
                        replacement = Some((j, d));
                        break;
                    }
                }
                let Some((j, d)) = replacement else {
                    return Status::NumericalFailure;
                };
                let s = self.pivot(r, j, &d, budget);
                if s != Status::Optimal {
                    return s;
                }
            }
        }
        self.primal_simplex(2, budget, eps)
    }
    pub fn primal(&self) -> Vec<f64> {
        let mut x = vec![0.; self.n];
        for (i, &j) in self.basic.iter().enumerate() {
            if j < self.n {
                x[j] = self.x[i];
            }
        }
        x
    }
    pub fn multipliers(&self) -> Vec<f64> {
        self.pi.clone()
    }
    pub fn append(&mut self, row: &[(usize, f64)], rhs: f64) -> Result<()> {
        if !rhs.is_finite() {
            return Err(Error::Evaluation("nonfinite appended inequality".into()));
        }
        let a: std::collections::BTreeMap<_, _> = row.iter().copied().collect();
        let coefficients: Column = self
            .basic
            .iter()
            .enumerate()
            .filter_map(|(i, j)| a.get(j).filter(|&&v| v != 0.).map(|&v| (i, v)))
            .collect();
        if self
            .entries()
            .saturating_add(row.len() + coefficients.len() + 2)
            > self.cap
        {
            return Err(Error::Unsupported(
                "sparse branch exceeds max_sparse_entries".into(),
            ));
        }
        for &(j, v) in row {
            if v != 0. {
                self.columns[j].push((self.m, v));
                self.matrix_entries += 1;
            }
        }
        self.basic.push(self.columns.len());
        self.columns.push(vec![(self.m, 1.)]);
        self.matrix_entries += 1;
        self.c.push(0.);
        self.is_basic.push(true);
        self.rhs.push(rhs);
        self.factor.border(self.m, coefficients);
        self.m += 1;
        self.record();
        Ok(())
    }
    pub fn dual_simplex(&mut self, budget: &mut Budget<'_>, eps: f64) -> Status {
        loop {
            if let Some(s) = budget.stop() {
                return s;
            }
            if let Err(s) = self.refresh(2, budget) {
                return s;
            }
            let r = (0..self.m)
                .filter(|&i| self.x[i] < -eps)
                .min_by_key(|&i| self.basic[i]);
            let Some(r) = r else {
                return Status::Optimal;
            };
            let mut unit = vec![0.; self.m];
            unit[r] = 1.;
            let price_row = self.factor.transpose(&unit);
            let mut entering = None;
            let mut best = f64::INFINITY;
            for j in 0..self.columns.len() {
                if j % 1024 == 0 {
                    if let Some(s) = budget.stop() {
                        return s;
                    }
                }
                if self.is_basic[j] || j == self.artificial {
                    continue;
                }
                let rc = self.c[j] - self.dot(j, &self.pi);
                if !rc.is_finite() || rc > 10. * eps {
                    return Status::NumericalFailure;
                }
                let coefficient = self.dot(j, &price_row);
                if coefficient < -eps {
                    let ratio = (-rc).max(0.) / -coefficient;
                    if ratio < best {
                        best = ratio;
                        entering = Some(j);
                    }
                }
            }
            let Some(j) = entering else {
                return Status::Infeasible;
            };
            let d = self.factor.forward(&self.column(j));
            let s = self.pivot(r, j, &d, budget);
            if s != Status::Optimal {
                return s;
            }
        }
    }
}
