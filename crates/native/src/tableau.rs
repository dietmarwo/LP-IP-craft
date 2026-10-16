//! Dense dictionary for max c'y, Ay <= b, y >= 0.
use crate::{Budget, NativeOptions};
use lp_ip_craft::{Error, Result, Status};

pub(crate) const ART: usize = usize::MAX;

#[derive(Clone)]
pub(crate) struct Tableau {
    pub m: usize,
    pub n: usize,
    pub d: Vec<f64>,
    pub basic: Vec<usize>,
    pub nonbasic: Vec<usize>,
}

impl Tableau {
    pub fn empty(m: usize, c: &[f64], opts: &NativeOptions) -> Result<Self> {
        let n = c.len();
        let size = (m + 2)
            .checked_mul(n + 2)
            .filter(|&v| v <= opts.max_tableau_entries)
            .ok_or_else(|| {
                Error::Unsupported("dense tableau exceeds max_tableau_entries".into())
            })?;
        let mut t = Self {
            m,
            n,
            d: vec![0.; size],
            basic: (n..n + m).collect(),
            nonbasic: (0..n).chain([ART]).collect(),
        };
        for i in 0..m {
            t.set(i, n, -1.);
        }
        for (j, &v) in c.iter().enumerate() {
            t.set(m, j, -v);
        }
        t.set(m + 1, n, 1.);
        Ok(t)
    }
    pub fn truncate_rows(&mut self, m: usize) {
        assert!(m <= self.m);
        let w = self.n + 2;
        if m < self.m {
            self.d.copy_within(self.m * w..(self.m + 2) * w, m * w);
            self.d.truncate((m + 2) * w);
            self.basic.truncate(m);
            self.m = m;
        }
    }
    #[inline]
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.d[i * (self.n + 2) + j]
    }
    #[inline]
    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.d[i * (self.n + 2) + j] = v;
    }

    // Reference implementation: every entry reads the old dictionary.
    fn pivot_reference(&mut self, r: usize, s: usize) -> bool {
        let old = self.d.clone();
        let w = self.n + 2;
        let inv = 1.0 / old[r * w + s];
        for i in 0..self.m + 2 {
            if i == r {
                continue;
            }
            for j in 0..w {
                if j != s {
                    self.d[i * w + j] = old[i * w + j] - old[r * w + j] * old[i * w + s] * inv;
                }
            }
        }
        for j in 0..w {
            if j != s {
                self.d[r * w + j] = old[r * w + j] * inv;
            }
        }
        for i in 0..self.m + 2 {
            if i != r {
                self.d[i * w + s] = -old[i * w + s] * inv;
            }
        }
        self.d[r * w + s] = inv;
        std::mem::swap(&mut self.basic[r], &mut self.nonbasic[s]);
        self.d.iter().all(|v| v.is_finite())
    }

    fn pivot(&mut self, r: usize, s: usize, fast: bool) -> bool {
        if !fast {
            return self.pivot_reference(r, s);
        }
        let w = self.n + 2;
        let inv = 1.0 / self.get(r, s);
        for j in 0..w {
            if j != s {
                self.d[r * w + j] *= inv;
            }
        }
        for i in 0..self.m + 2 {
            if i == r {
                continue;
            }
            let factor = self.d[i * w + s];
            if factor != 0.0 {
                for j in 0..w {
                    if j != s {
                        self.d[i * w + j] -= factor * self.d[r * w + j];
                    }
                }
            }
            self.d[i * w + s] = -factor * inv;
        }
        self.d[r * w + s] = inv;
        std::mem::swap(&mut self.basic[r], &mut self.nonbasic[s]);
        self.d.iter().all(|v| v.is_finite())
    }

    fn simplex(&mut self, phase: usize, budget: &mut Budget<'_>, eps: f64) -> Status {
        let objective = if phase == 1 { self.m + 1 } else { self.m };
        let mut stalled = 0;
        let mut bland = !budget.settings.dantzig_pricing;
        loop {
            if let Some(s) = budget.stop() {
                return s;
            }
            let s = (0..=self.n)
                .filter(|&j| self.nonbasic[j] != ART && self.get(objective, j) < -eps)
                .min_by(|&a, &b| {
                    if bland {
                        self.nonbasic[a].cmp(&self.nonbasic[b])
                    } else {
                        self.get(objective, a)
                            .total_cmp(&self.get(objective, b))
                            .then(self.nonbasic[a].cmp(&self.nonbasic[b]))
                    }
                });
            let Some(s) = s else {
                return Status::Optimal;
            };
            let r = (0..self.m)
                .filter(|&i| self.get(i, s) > eps)
                .min_by(|&i, &k| {
                    let a = self.get(i, self.n + 1) / self.get(i, s);
                    let b = self.get(k, self.n + 1) / self.get(k, s);
                    a.total_cmp(&b).then(self.basic[i].cmp(&self.basic[k]))
                });
            let Some(r) = r else {
                return Status::Unbounded;
            };
            let before = self.get(objective, self.n + 1);
            if !self.pivot(r, s, budget.settings.fast_pivots) {
                return Status::NumericalFailure;
            }
            if self.get(objective, self.n + 1) - before <= eps * (1.0 + before.abs()) {
                stalled += 1;
            } else {
                stalled = 0;
            }
            if stalled >= 32 {
                bland = true;
            }
            budget.pivots += 1;
        }
    }

    pub fn solve(&mut self, budget: &mut Budget<'_>, eps: f64) -> Status {
        if let Some(s) = budget.stop() {
            return s;
        }
        let r = (0..self.m)
            .min_by(|&i, &k| self.get(i, self.n + 1).total_cmp(&self.get(k, self.n + 1)));
        if let Some(r) = r.filter(|&i| self.get(i, self.n + 1) < -eps) {
            if !self.pivot(r, self.n, budget.settings.fast_pivots) {
                return Status::NumericalFailure;
            }
            budget.pivots += 1;
            let s = self.simplex(1, budget, eps);
            if s != Status::Optimal {
                return if s == Status::Unbounded {
                    Status::NumericalFailure
                } else {
                    s
                };
            }
            if self.get(self.m + 1, self.n + 1) < -eps {
                return Status::Infeasible;
            }
            if self.get(self.m + 1, self.n + 1).abs() > eps * 10.0 {
                return Status::NumericalFailure;
            }
            if let Some(r) = self.basic.iter().position(|&id| id == ART) {
                if let Some(s) = (0..=self.n)
                    .filter(|&j| self.nonbasic[j] != ART && self.get(r, j).abs() > eps)
                    .min_by_key(|&j| self.nonbasic[j])
                {
                    if let Some(s) = budget.stop() {
                        return s;
                    }
                    if !self.pivot(r, s, budget.settings.fast_pivots) {
                        return Status::NumericalFailure;
                    }
                    budget.pivots += 1;
                }
            }
        }
        self.simplex(2, budget, eps)
    }

    pub fn primal(&self) -> Vec<f64> {
        let mut x = vec![0.0; self.n];
        for i in 0..self.m {
            if self.basic[i] < self.n {
                x[self.basic[i]] = self.get(i, self.n + 1);
            }
        }
        x
    }
    pub fn multipliers(&self) -> Vec<f64> {
        let mut y = vec![0.0; self.m];
        for j in 0..=self.n {
            let id = self.nonbasic[j];
            if id >= self.n && id != ART && id - self.n < self.m {
                y[id - self.n] = self.get(self.m, j);
            }
        }
        y
    }

    /// Substitute a new inequality through the current dictionary. The new
    /// slack is initially basic; the old objective remains dual feasible.
    pub fn append(&mut self, a: &[f64], rhs: f64, opts: &NativeOptions) -> Result<()> {
        let w = self.n + 2;
        let size = (self.m + 3)
            .checked_mul(w)
            .filter(|&s| s <= opts.max_tableau_entries)
            .ok_or_else(|| {
                Error::Unsupported("appended tableau exceeds max_tableau_entries".into())
            })?;
        let mut row = vec![0.0; w];
        row[self.n + 1] = rhs;
        for (j, &id) in self.nonbasic.iter().enumerate() {
            if id < self.n {
                row[j] = a[id];
            }
        }
        for i in 0..self.m {
            let id = self.basic[i];
            let coefficient = if id < self.n { a[id] } else { 0.0 };
            if coefficient != 0.0 {
                for (j, v) in row.iter_mut().enumerate() {
                    *v -= coefficient * self.get(i, j);
                }
            }
        }
        if row.iter().any(|v| !v.is_finite()) {
            return Err(Error::Evaluation("nonfinite appended inequality".into()));
        }
        let old_m = self.m;
        self.d.resize(size, 0.0);
        self.d
            .copy_within(old_m * w..(old_m + 2) * w, (old_m + 1) * w);
        self.d[old_m * w..(old_m + 1) * w].copy_from_slice(&row);
        self.basic.push(self.n + old_m);
        self.m += 1;
        Ok(())
    }

    pub fn dual_simplex(&mut self, budget: &mut Budget<'_>, eps: f64) -> Status {
        loop {
            if let Some(s) = budget.stop() {
                return s;
            }
            let r = (0..self.m)
                .filter(|&i| self.get(i, self.n + 1) < -eps)
                .min_by_key(|&i| self.basic[i]);
            let Some(r) = r else {
                return Status::Optimal;
            };
            let s = (0..=self.n)
                .filter(|&j| self.nonbasic[j] != ART && self.get(r, j) < -eps)
                .min_by(|&a, &b| {
                    let x = self.get(self.m, a) / -self.get(r, a);
                    let y = self.get(self.m, b) / -self.get(r, b);
                    x.total_cmp(&y)
                        .then(self.nonbasic[a].cmp(&self.nonbasic[b]))
                });
            let Some(s) = s else {
                return Status::Infeasible;
            };
            if !self.pivot(r, s, budget.settings.fast_pivots) {
                return Status::NumericalFailure;
            }
            budget.pivots += 1;
        }
    }
}
