//! Bounded-variable dual revised simplex for LPs and branch-and-bound nodes.
//!
//! Internal form: `Ã x̃ − r̃ = 0` with `l ≤ (x̃, r̃) ≤ u`. Here `Ã = R A S`
//! is the kept submatrix equilibrated by powers of two, `x̃ = S⁻¹ x` are the
//! scaled structural variables and `r̃ = R (A x)` the scaled row activities
//! ("logicals", basis column `−e_i`); root-fixed columns that presolve drops
//! shift the row bounds instead. The objective `σ c`
//! (`σ = −1` for maximization) becomes `σ c S / C`. Powers of two keep bounds
//! and nonbasic values exact, so every nonbasic variable sits exactly on an
//! original bound. Variable bounds are never rows: branching changes bounds
//! in place and keeps the factorized basis.
//!
//! Each dual simplex iteration chooses a primal infeasible basic variable by
//! dual steepest edge, computes its tableau row from `ρ = B⁻ᵀ e_r` with the
//! row-wise matrix, and chooses the entering variable with a bound-flipping
//! ratio test and Harris tolerances. Dual infeasibility on columns without a
//! finite box is removed by an auxiliary phase 1 (bounds replaced by
//! `{0}`, `[0, 1]`, `[−1, 0]` or `[−1, 1]`). Cost perturbation is switched on
//! after a run of degenerate steps and removed before optimality is declared.
//! Optima are declared on primal and dual values recomputed from scratch
//! (after a refactorization for cold solves); callers check them again.

use crate::Budget;
use crate::lu::{FactorError, Lu};
use lp_ip_craft::{Duals, Error, Model, Result, Sense, Status};

/// Entries of `ρ` at or below this magnitude are skipped in the pivot row.
const DROP: f64 = 1e-14;
const NIL: usize = usize::MAX;
/// Degenerate (zero dual step) iterations before costs are perturbed.
const DEGENERATE_STREAK: usize = 25;
/// Smallest steepest-edge weight kept after an update.
const MIN_WEIGHT: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Basic,
    Lower,
    Upper,
    /// Nonbasic free variable at zero.
    Zero,
}

/// Kernel controls derived from the solver options.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Controls {
    pub pivot_tolerance: f64,
    pub refactor_interval: usize,
    pub adaptive_refactor: bool,
    pub basis_update: crate::BasisUpdate,
    pub factor_pivot_search: usize,
    pub factor_ordering: crate::FactorOrdering,
    pub max_factor_entries: usize,
    pub steepest_edge: bool,
    /// Primal tolerance target in original units.
    pub feasibility: f64,
    /// Dual tolerance target in original units (already objective-scaled).
    pub dual: f64,
}

/// Counters accumulated over the lifetime of one LP.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    pub peak_entries: usize,
    pub refactors: u64,
    pub updates: u64,
    pub ft_updates: u64,
    pub bound_flips: u64,
    pub repairs: u64,
    pub restores: u64,
    pub perturbations: u64,
}

/// A basis with nonbasic states and steepest-edge weights.
pub(crate) struct Basis {
    id: u64,
    basic: Vec<usize>,
    state: Vec<State>,
    weight: Vec<f64>,
}

#[derive(Clone, Copy)]
struct Candidate {
    j: usize,
    /// Pivot row entry with the sign of the leaving direction applied.
    a: f64,
    ratio: f64,
    harris: f64,
}

enum Outcome {
    Optimal,
    Infeasible,
    Restart,
    Stopped(Status),
    Failure,
}

pub(crate) struct Lp {
    n: usize,
    m: usize,
    rows: Vec<usize>,
    /// Model column of each LP column, and LP column (or `NIL`) of each
    /// model column; dropped columns are fixed at `fixed`.
    cols: Vec<usize>,
    col_of: Vec<usize>,
    fixed: Vec<f64>,
    col_start: Vec<usize>,
    col_index: Vec<usize>,
    col_value: Vec<f64>,
    row_start: Vec<usize>,
    row_index: Vec<usize>,
    row_value: Vec<f64>,
    col_scale: Vec<f64>,
    row_scale: Vec<f64>,
    cost_scale: f64,
    sign: f64,
    cost0: Vec<f64>,
    cost: Vec<f64>,
    lower: Vec<f64>,
    upper: Vec<f64>,
    ptol: Vec<f64>,
    dtol: Vec<f64>,
    basic: Vec<usize>,
    state: Vec<State>,
    x: Vec<f64>,
    d: Vec<f64>,
    pi: Vec<f64>,
    weight: Vec<f64>,
    lu: Option<Lu>,
    factor_workspace: crate::active::FactorWorkspace,
    rho: Vec<f64>,
    rho_index: Vec<usize>,
    column: Vec<f64>,
    column_index: Vec<usize>,
    tau: Vec<f64>,
    work: Vec<f64>,
    alpha: Vec<f64>,
    marked: Vec<bool>,
    touched: Vec<usize>,
    candidates: Vec<Candidate>,
    flips: Vec<usize>,
    controls: Controls,
    perturbed: bool,
    primal_stale: bool,
    dual_stale: bool,
    fresh_optimum: bool,
    seed: u64,
    next_id: u64,
    basis_id: Option<u64>,
    ray: Option<Vec<f64>>,
    iteration_guard: u64,
    pub stats: Stats,
}

fn pow2(v: f64) -> f64 {
    let e = v.log2().round().clamp(-60.0, 60.0) as i32;
    2f64.powi(e)
}

impl Lp {
    /// Builds the scaled LP over the rows with `keep_rows[i]` and columns
    /// with `keep_cols[j]` and the model's variable bounds, with a logical
    /// (slack) starting basis. Dropped columns must be fixed (`l = u`); their
    /// contribution shifts the row bounds.
    pub fn new(
        model: &Model,
        keep_rows: &[bool],
        keep_cols: &[bool],
        controls: Controls,
    ) -> Result<Self> {
        let rows: Vec<usize> = (0..model.m()).filter(|&i| keep_rows[i]).collect();
        let cols: Vec<usize> = (0..model.n()).filter(|&j| keep_cols[j]).collect();
        let (n, m) = (cols.len(), rows.len());
        let mut col_of = vec![NIL; model.n()];
        for (j, &c) in cols.iter().enumerate() {
            col_of[c] = j;
        }
        let fixed = model.variables().lower.clone();
        debug_assert!(
            (0..model.n()).all(|j| keep_cols[j] || fixed[j] == model.variables().upper[j])
        );
        let a = model.matrix();
        let mut col_start = vec![0usize; n + 1];
        let mut shift = vec![0.0; m];
        for (k, &i) in rows.iter().enumerate() {
            for e in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                let (c, v) = (a.pattern.indices[e], a.values[e]);
                if v == 0.0 {
                    continue;
                }
                match col_of[c] {
                    NIL => shift[k] += v * fixed[c],
                    j => col_start[j + 1] += 1,
                }
            }
        }
        for j in 0..n {
            col_start[j + 1] += col_start[j];
        }
        let nnz = col_start[n];
        let mut fill = col_start.clone();
        let mut col_index = vec![0usize; nnz];
        let mut col_value = vec![0.0; nnz];
        for (k, &i) in rows.iter().enumerate() {
            for e in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                let v = a.values[e];
                let j = col_of[a.pattern.indices[e]];
                if v != 0.0 && j != NIL {
                    col_index[fill[j]] = k;
                    col_value[fill[j]] = v;
                    fill[j] += 1;
                }
            }
        }
        let (row_scale, col_scale) = scale_factors(m, &col_start, &col_index, &col_value);
        for j in 0..n {
            for e in col_start[j]..col_start[j + 1] {
                col_value[e] *= row_scale[col_index[e]] * col_scale[j];
            }
        }
        // Row-wise copy of the scaled matrix.
        let mut row_start = vec![0usize; m + 1];
        for &k in &col_index {
            row_start[k + 1] += 1;
        }
        for k in 0..m {
            row_start[k + 1] += row_start[k];
        }
        let mut fill = row_start.clone();
        let mut row_index = vec![0usize; nnz];
        let mut row_value = vec![0.0; nnz];
        for j in 0..n {
            for e in col_start[j]..col_start[j + 1] {
                let k = col_index[e];
                row_index[fill[k]] = j;
                row_value[fill[k]] = col_value[e];
                fill[k] += 1;
            }
        }
        let sign = if model.sense() == Sense::Minimize {
            1.0
        } else {
            -1.0
        };
        let raw: Vec<f64> = (0..n)
            .map(|j| sign * model.objective()[cols[j]] * col_scale[j])
            .collect();
        let largest = raw.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        let cost_scale = if largest > 0.0 && largest.is_finite() {
            pow2(largest)
        } else {
            1.0
        };
        let mut cost0: Vec<f64> = raw.iter().map(|v| v / cost_scale).collect();
        cost0.resize(n + m, 0.0);
        if cost0.iter().any(|v| !v.is_finite()) {
            return Err(Error::Unsupported("objective scaling overflow".into()));
        }
        let mut lower = vec![0.0; n + m];
        let mut upper = vec![0.0; n + m];
        for (k, &i) in rows.iter().enumerate() {
            lower[n + k] = scaled(model.rows().lower[i] - shift[k], row_scale[k])?;
            upper[n + k] = scaled(model.rows().upper[i] - shift[k], row_scale[k])?;
        }
        let feasibility = 0.1 * controls.feasibility;
        let dual = 0.1 * controls.dual;
        let mut ptol = Vec::with_capacity(n + m);
        let mut dtol = Vec::with_capacity(n + m);
        for &s in &col_scale {
            ptol.push((feasibility / s).clamp(1e-12, 1e-7));
            dtol.push((dual * s / cost_scale).clamp(1e-12, 1e-7));
        }
        for &r in &row_scale {
            ptol.push((feasibility * r).clamp(1e-12, 1e-7));
            dtol.push((dual / (cost_scale * r)).clamp(1e-12, 1e-7));
        }
        let mut lp = Self {
            n,
            m,
            rows,
            cols,
            col_of,
            fixed,
            col_start,
            col_index,
            col_value,
            row_start,
            row_index,
            row_value,
            col_scale,
            row_scale,
            cost_scale,
            sign,
            cost: cost0.clone(),
            cost0,
            lower,
            upper,
            ptol,
            dtol,
            basic: Vec::new(),
            state: vec![State::Lower; n + m],
            x: vec![0.0; n + m],
            d: vec![0.0; n + m],
            pi: vec![0.0; m],
            weight: vec![1.0; m],
            lu: None,
            factor_workspace: Default::default(),
            rho: vec![0.0; m],
            rho_index: Vec::new(),
            column: vec![0.0; m],
            column_index: Vec::new(),
            tau: vec![0.0; m],
            work: vec![0.0; m],
            alpha: vec![0.0; n + m],
            marked: vec![false; n + m],
            touched: Vec::new(),
            candidates: Vec::new(),
            flips: Vec::new(),
            controls,
            perturbed: false,
            primal_stale: true,
            dual_stale: true,
            fresh_optimum: true,
            seed: 0x9E37_79B9_7F4A_7C15 ^ ((n as u64) << 20) ^ m as u64,
            next_id: 0,
            basis_id: None,
            ray: None,
            iteration_guard: 50 * (n + m) as u64 + 10_000,
            stats: Stats::default(),
        };
        lp.set_bounds(&model.variables().lower, &model.variables().upper)?;
        lp.reset_to_slack();
        Ok(lp)
    }

    /// Shrinks bounds on rows containing continuous columns inward by a margin
    /// above primal tolerance (for incumbent certification). A row whose
    /// interval is too narrow keeps its bounds.
    pub fn shrink_rows(&mut self, model: &Model) {
        let a = model.matrix();
        for k in self.n..self.n + self.m {
            let i = self.rows[k - self.n];
            // Certification fixes integer columns. Shrinking a tight
            // all-integer row would make that fixed point infeasible.
            if (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
                .any(|e| a.values[e] != 0. && !model.kinds()[a.pattern.indices[e]].is_integral())
            {
                self.shrink(k);
            }
        }
        self.primal_stale = true;
    }

    /// Fixes the integral variables at `x` and gives the others their
    /// `bounds` shrunk like [`Lp::shrink_rows`] (original units).
    pub fn fix_for_certification(
        &mut self,
        x: &[f64],
        integral: &[bool],
        bounds: &lp_ip_craft::Bounds,
    ) -> Result<()> {
        for j in 0..self.n {
            let c = self.cols[j];
            if integral[c] {
                self.set_lp_bound(j, x[c], x[c])?;
            } else {
                self.set_lp_bound(j, bounds.lower[c], bounds.upper[c])?;
                self.shrink(j);
            }
        }
        Ok(())
    }

    fn shrink(&mut self, j: usize) {
        let margin = 16.0 * self.ptol[j];
        let (l, u) = (self.lower[j], self.upper[j]);
        let room = if l.is_finite() && u.is_finite() {
            u - l
        } else {
            f64::INFINITY
        };
        if room <= 4.0 * margin {
            return;
        }
        let pad = |v: f64| margin * v.abs().max(1.0);
        if l.is_finite() {
            self.lower[j] = l + pad(l);
        }
        if u.is_finite() {
            self.upper[j] = u - pad(u);
        }
        if self.lower[j] > self.upper[j] {
            (self.lower[j], self.upper[j]) = (l, u);
        }
        if self.state[j] != State::Basic {
            self.state[j] = self.valid_state(j, self.state[j]);
            self.primal_stale = true;
        }
    }

    /// Number of model rows not represented in the LP.
    pub fn removed_rows(&self, model: &Model) -> usize {
        model.m() - self.m
    }

    /// Number of model columns fixed outside the LP.
    pub fn removed_columns(&self) -> usize {
        self.col_of.len() - self.n
    }

    /// Replaces the structural bounds (original units, model columns). The
    /// basis is kept.
    pub fn set_bounds(&mut self, lower: &[f64], upper: &[f64]) -> Result<()> {
        for c in 0..self.col_of.len() {
            self.set_bound(c, lower[c], upper[c])?;
        }
        Ok(())
    }

    /// Replaces the bounds of model column `c` (original units). Columns
    /// dropped as fixed keep their value.
    pub fn set_bound(&mut self, c: usize, lower: f64, upper: f64) -> Result<()> {
        let j = self.col_of[c];
        if j == NIL {
            debug_assert!(lower == self.fixed[c] && upper == self.fixed[c]);
            return Ok(());
        }
        self.set_lp_bound(j, lower, upper)
    }

    fn set_lp_bound(&mut self, j: usize, lower: f64, upper: f64) -> Result<()> {
        let l = scaled(lower, 1.0 / self.col_scale[j])?;
        let u = scaled(upper, 1.0 / self.col_scale[j])?;
        if l != self.lower[j] || u != self.upper[j] {
            self.lower[j] = l;
            self.upper[j] = u;
            if self.state[j] != State::Basic {
                self.state[j] = self.valid_state(j, self.state[j]);
                self.primal_stale = true;
            }
        }
        Ok(())
    }

    /// Returns to the logical basis `−I` with unit steepest-edge weights.
    pub fn reset_to_slack(&mut self) {
        let (n, m) = (self.n, self.m);
        self.basic = (n..n + m).collect();
        for k in 0..m {
            self.state[n + k] = State::Basic;
        }
        for j in 0..n {
            let preferred = if self.cost[j] >= 0.0 {
                State::Lower
            } else {
                State::Upper
            };
            self.state[j] = self.valid_state(j, preferred);
        }
        self.weight.fill(1.0);
        self.lu = None;
        self.primal_stale = true;
        self.basis_id = None;
    }

    /// Captures the current basis; [`Lp::matches`] holds until the next solve.
    pub fn snapshot(&mut self) -> Basis {
        self.next_id += 1;
        self.basis_id = Some(self.next_id);
        Basis {
            id: self.next_id,
            basic: self.basic.clone(),
            state: self.state.clone(),
            weight: self.weight.clone(),
        }
    }

    /// Approximate heap bytes of one snapshot.
    pub fn snapshot_bytes(&self) -> usize {
        (self.n + self.m) * std::mem::size_of::<State>()
            + self.m * (std::mem::size_of::<usize>() + std::mem::size_of::<f64>())
    }

    pub fn matches(&self, basis: &Basis) -> bool {
        self.basis_id == Some(basis.id)
    }

    /// Reinstates a snapshot; the next solve refactorizes it.
    pub fn restore(&mut self, basis: &Basis) {
        if self.matches(basis) {
            return;
        }
        self.stats.restores += 1;
        self.basis_id = Some(basis.id);
        if self.lu.is_some() && self.restore_by_updates(basis) {
            return;
        }
        self.basic.clone_from(&basis.basic);
        self.state.clone_from(&basis.state);
        self.weight.clone_from(&basis.weight);
        for j in 0..self.n + self.m {
            if self.state[j] != State::Basic {
                self.state[j] = self.valid_state(j, self.state[j]);
            }
        }
        self.lu = None;
        self.primal_stale = true;
    }

    /// Reaches a snapshot that differs in a few basis positions by eta
    /// column replacements on the current factorization. Returns false
    /// (leaving the caller to refactorize) when the difference is large or a
    /// replacement pivot is not clearly nonzero.
    fn restore_by_updates(&mut self, basis: &Basis) -> bool {
        let lu = self.lu.as_ref().expect("factorized basis");
        let room = self.controls.refactor_interval.saturating_sub(lu.updates());
        let mut pending: Vec<usize> = (0..self.m)
            .filter(|&p| self.basic[p] != basis.basic[p])
            .collect();
        if pending.len() > room.min(16) {
            return false;
        }
        while !pending.is_empty() {
            // Replace only with currently nonbasic columns, so no column is
            // ever basic twice.
            let Some(k) = pending
                .iter()
                .position(|&p| self.state[basis.basic[p]] != State::Basic)
            else {
                return false;
            };
            let p = pending.swap_remove(k);
            let entering = basis.basic[p];
            self.compute_column(entering);
            let largest = self.column.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            if self.column[p].abs() < 1e-7 * largest.max(1.0) {
                return false;
            }
            let lu = self.lu.as_mut().expect("factorized basis");
            if !lu.update_sparse(p, &self.column, self.column_index.iter().copied()) {
                return false;
            }
            self.stats.ft_updates += u64::from(lu.uses_ft());
            self.stats.updates += 1;
            self.stats.peak_entries = self.stats.peak_entries.max(lu.entries());
            let leaving = self.basic[p];
            self.basic[p] = entering;
            self.state[entering] = State::Basic;
            self.state[leaving] = State::Lower;
        }
        self.state.clone_from(&basis.state);
        self.weight.clone_from(&basis.weight);
        for j in 0..self.n + self.m {
            if self.state[j] != State::Basic {
                self.state[j] = self.valid_state(j, self.state[j]);
            }
        }
        self.primal_stale = true;
        self.dual_stale = true;
        true
    }

    /// Structural values in original units, for every model column.
    pub fn point(&self) -> Vec<f64> {
        let mut x = self.fixed.clone();
        for (j, &c) in self.cols.iter().enumerate() {
            x[c] = self.x[j] * self.col_scale[j];
        }
        x
    }

    /// Row duals and reduced costs in the original sense and units. A row
    /// price whose sign selects an infinite row bound is within the dual
    /// tolerance at an optimum and is reported as zero, so that it cannot
    /// make a conservative dual bound unavailable.
    pub fn duals(&self, model: &Model) -> Duals {
        let mut y = vec![0.0; model.m()];
        for (k, &i) in self.rows.iter().enumerate() {
            let price = self.pi[k];
            let infinite = if price > 0.0 {
                self.lower[self.n + k] == f64::NEG_INFINITY
            } else {
                self.upper[self.n + k] == f64::INFINITY
            };
            if !infinite {
                y[i] = self.sign * self.cost_scale * self.row_scale[k] * price;
            }
        }
        let mut at = vec![0.0; model.n()];
        model.matrix().multiply_transpose(&y, &mut at);
        Duals {
            reduced_costs: model
                .objective()
                .iter()
                .zip(at)
                .map(|(&c, a)| c - a)
                .collect(),
            row_duals: y,
        }
    }

    /// Re-checks the last infeasibility claim with its Farkas row
    /// `ρᵀ(Ã x̃ − r̃) = 0`: the combination must be bounded away from zero
    /// over the current box, independently of the factorization. A numerical
    /// check (tiny coefficients are ignored), not an exact certificate.
    pub fn infeasibility_verified(&self) -> bool {
        let Some(rho) = &self.ray else {
            return false;
        };
        let mut coefficient = Vec::with_capacity(self.n + self.m);
        for j in 0..self.n {
            let mut v = 0.0;
            for e in self.col_start[j]..self.col_start[j + 1] {
                v += self.col_value[e] * rho[self.col_index[e]];
            }
            coefficient.push(v);
        }
        coefficient.extend(rho.iter().map(|v| -v));
        let largest = coefficient.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        if largest == 0.0 || !largest.is_finite() {
            return false;
        }
        // Range [lo, hi] of the combination over the box, with the sums of
        // magnitudes of each side for a rounding margin.
        let (mut lo, mut hi) = (0.0_f64, 0.0_f64);
        let (mut lo_size, mut hi_size) = (largest, largest);
        for (j, &v) in coefficient.iter().enumerate() {
            if v.abs() <= 1e-12 * largest {
                continue;
            }
            let (l, u) = (self.lower[j], self.upper[j]);
            let (a, b) = if v > 0.0 {
                (v * l, v * u)
            } else {
                (v * u, v * l)
            };
            lo += a;
            hi += b;
            lo_size += a.abs();
            hi_size += b.abs();
        }
        hi < -1e-9 * hi_size || lo > 1e-9 * lo_size
    }

    fn lu(&mut self) -> &mut Lu {
        self.lu.as_mut().expect("factorized basis")
    }

    fn valid_state(&self, j: usize, preferred: State) -> State {
        let (l, u) = (self.lower[j], self.upper[j]);
        match (l.is_finite(), u.is_finite()) {
            (true, true) if l == u => State::Lower,
            (true, true) => match preferred {
                State::Upper => State::Upper,
                State::Lower => State::Lower,
                _ if self.d[j] < 0.0 => State::Upper,
                _ => State::Lower,
            },
            (true, false) => State::Lower,
            (false, true) => State::Upper,
            (false, false) => State::Zero,
        }
    }

    fn state_from_dual(&self, j: usize) -> State {
        let preferred = if self.d[j] >= 0.0 {
            State::Lower
        } else {
            State::Upper
        };
        self.valid_state(j, preferred)
    }

    fn value(&self, j: usize) -> f64 {
        match self.state[j] {
            State::Lower => self.lower[j],
            State::Upper => self.upper[j],
            State::Zero => 0.0,
            State::Basic => self.x[j],
        }
    }

    fn boxed(&self, j: usize) -> bool {
        self.lower[j].is_finite() && self.upper[j].is_finite()
    }

    /// Adds `scale * a_j` (structural or logical column) to a row vector.
    fn add_column(
        col_start: &[usize],
        col_index: &[usize],
        col_value: &[f64],
        n: usize,
        j: usize,
        scale: f64,
        out: &mut [f64],
    ) {
        if j < n {
            for e in col_start[j]..col_start[j + 1] {
                out[col_index[e]] += scale * col_value[e];
            }
        } else {
            out[j - n] -= scale;
        }
    }

    /// Factorizes the basis, replacing dependent columns by logicals.
    /// Returns whether the basis had to be repaired.
    fn refactor(&mut self) -> Result<bool> {
        let _profile = crate::profile::enter("refactor_other");
        let mut repaired = false;
        for attempt in 0..4 {
            if attempt == 3 {
                // Repairs did not converge numerically; the slack basis always does.
                self.reset_to_slack();
            }
            let column = |p: usize, out: &mut Vec<(usize, f64)>| {
                let j = self.basic[p];
                if j < self.n {
                    let range = self.col_start[j]..self.col_start[j + 1];
                    out.extend(
                        self.col_index[range.clone()]
                            .iter()
                            .copied()
                            .zip(self.col_value[range].iter().copied()),
                    );
                } else {
                    out.push((j - self.n, -1.0));
                }
            };
            #[cfg(feature = "basis-replay")]
            crate::replay::capture(self.m, column, &[&self.rho, &self.pi]);
            let ordered = match self.controls.factor_ordering {
                crate::FactorOrdering::Natural => false,
                crate::FactorOrdering::MinDegree => true,
                crate::FactorOrdering::Auto => {
                    self.m >= 2048
                        && self
                            .lu
                            .as_ref()
                            .is_some_and(|old| old.dense_dimension() > self.m / 8)
                }
            };
            match Lu::factor_with_workspace(
                self.m,
                column,
                self.controls.max_factor_entries,
                &mut self.factor_workspace,
                crate::lu::FactorOptions {
                    search: self.controls.factor_pivot_search,
                    ordered,
                },
            ) {
                Ok(mut lu) => {
                    #[cfg(feature = "kernel-profile")]
                    if std::env::var_os("LPIP_TRACE_FACTORS").is_some() {
                        eprintln!(
                            "factor {:?}",
                            (
                                self.m,
                                lu.factor_sizes(),
                                self.lu.as_ref().map_or(0, |old| old.average_update_nnz())
                            )
                        );
                    }
                    // FT avoids dense eta files on sparse factors. Keep the
                    // compact product form when previous updates stayed sparse
                    // or the new factors themselves have substantial fill.
                    if self.controls.basis_update == crate::BasisUpdate::ForrestTomlin
                        || (self.controls.basis_update == crate::BasisUpdate::Auto
                            && lu.ft_suitable()
                            && self.lu.as_ref().is_some_and(|old| {
                                old.average_update_nnz()
                                    .saturating_mul(self.controls.refactor_interval)
                                    > 4 * lu.entries()
                            }))
                    {
                        lu.enable_ft();
                    }
                    self.stats.refactors += 1;
                    self.stats.peak_entries = self.stats.peak_entries.max(lu.entries());
                    self.lu = Some(lu);
                    self.primal_stale = true;
                    return Ok(repaired || attempt == 3);
                }
                Err(FactorError::TooLarge) => {
                    return Err(Error::Unsupported(format!(
                        "basis factorization exceeds max_factor_entries = {}",
                        self.controls.max_factor_entries
                    )));
                }
                Err(FactorError::Singular { positions, rows }) => {
                    repaired = true;
                    self.stats.repairs += positions.len() as u64;
                    for (p, i) in positions.into_iter().zip(rows) {
                        let j = self.basic[p];
                        let logical = self.n + i;
                        let (l, u) = (self.lower[j], self.upper[j]);
                        let nearer = if (self.x[j] - l).abs() <= (u - self.x[j]).abs() {
                            State::Lower
                        } else {
                            State::Upper
                        };
                        self.state[j] = self.valid_state(j, nearer);
                        self.basic[p] = logical;
                        self.state[logical] = State::Basic;
                        self.weight[p] = 1.0;
                    }
                }
            }
        }
        unreachable!("the slack basis factorizes")
    }

    /// Basic values from the nonbasic values: `x_B = −B⁻¹ N x_N`.
    fn compute_primal(&mut self) {
        let _profile = crate::profile::enter("recompute_primal");
        let (n, m) = (self.n, self.m);
        self.work.fill(0.0);
        for j in 0..n + m {
            if self.state[j] != State::Basic {
                let v = self.value(j);
                self.x[j] = v;
                if v != 0.0 {
                    Self::add_column(
                        &self.col_start,
                        &self.col_index,
                        &self.col_value,
                        n,
                        j,
                        v,
                        &mut self.work,
                    );
                }
            }
        }
        let lu = self.lu.as_mut().expect("factorized basis");
        lu.ftran(&mut self.work);
        for p in 0..m {
            self.x[self.basic[p]] = -self.work[p];
        }
        self.primal_stale = false;
    }

    /// Row prices `π = B⁻ᵀ c_B` and reduced costs `d = c − [Ã −I]ᵀ π`.
    fn compute_dual(&mut self) {
        let _profile = crate::profile::enter("recompute_dual");
        self.dual_stale = false;
        let n = self.n;
        for p in 0..self.m {
            self.pi[p] = self.cost[self.basic[p]];
        }
        let lu = self.lu.as_mut().expect("factorized basis");
        lu.btran(&mut self.pi);
        // A basic logical column −e_i forces π_i = 0 exactly.
        for k in 0..self.m {
            if self.state[n + k] == State::Basic {
                self.pi[k] = 0.0;
            }
        }
        for j in 0..n {
            self.d[j] = if self.state[j] == State::Basic {
                0.0
            } else {
                let mut v = self.cost[j];
                for e in self.col_start[j]..self.col_start[j + 1] {
                    v -= self.col_value[e] * self.pi[self.col_index[e]];
                }
                v
            };
        }
        for k in 0..self.m {
            self.d[n + k] = if self.state[n + k] == State::Basic {
                0.0
            } else {
                self.cost[n + k] + self.pi[k]
            };
        }
    }

    /// Moves boxed nonbasic variables with wrong-signed reduced costs to the
    /// other bound. Returns the number of flips.
    fn flip_to_dual_feasible(&mut self) -> usize {
        let mut flips = 0;
        for j in 0..self.n + self.m {
            if self.state[j] == State::Basic || !self.boxed(j) || self.lower[j] == self.upper[j] {
                continue;
            }
            let next = match self.state[j] {
                State::Lower if self.d[j] < -self.dtol[j] => State::Upper,
                State::Upper if self.d[j] > self.dtol[j] => State::Lower,
                _ => continue,
            };
            self.state[j] = next;
            flips += 1;
        }
        if flips > 0 {
            self.primal_stale = true;
            self.stats.bound_flips += flips as u64;
        }
        flips
    }

    fn nonboxed_dual_infeasible(&self) -> bool {
        (0..self.n + self.m).any(|j| {
            if self.state[j] == State::Basic || self.boxed(j) {
                return false;
            }
            let (d, tol) = (self.d[j], self.dtol[j]);
            match self.state[j] {
                State::Lower => d < -tol,
                State::Upper => d > tol,
                _ => d.abs() > tol,
            }
        })
    }

    /// Dual steepest-edge pricing over the primal infeasible basic variables.
    fn choose_row(&self) -> Option<usize> {
        let _profile = crate::profile::enter("pricing_choose");
        let mut best = None;
        let mut score = 0.0;
        for (p, &j) in self.basic.iter().enumerate() {
            let v = self.x[j];
            let infeasibility = if v < self.lower[j] - self.ptol[j] {
                self.lower[j] - v
            } else if v > self.upper[j] + self.ptol[j] {
                v - self.upper[j]
            } else {
                continue;
            };
            let s = infeasibility * infeasibility / self.weight[p];
            if s > score {
                score = s;
                best = Some(p);
            }
        }
        best
    }

    /// `ρ = B⁻ᵀ e_r` and its pivot row over the nonbasic variables.
    fn compute_row(&mut self, r: usize) {
        let _profile = crate::profile::enter("pricing_row");
        for &j in &self.touched {
            self.alpha[j] = 0.0;
            self.marked[j] = false;
        }
        self.touched.clear();
        for &i in &self.rho_index {
            self.rho[i] = 0.;
        }
        self.rho[r] = 1.0;
        let lu = self.lu.as_mut().expect("factorized basis");
        lu.btran_unit_indexed(r, &mut self.rho, &mut self.rho_index);
        if self.controls.steepest_edge {
            self.weight[r] = self
                .rho_index
                .iter()
                .map(|&i| self.rho[i] * self.rho[i])
                .sum::<f64>()
                .max(MIN_WEIGHT);
        }
        let n = self.n;
        for &k in &self.rho_index {
            let v = self.rho[k];
            if v.abs() <= DROP {
                continue;
            }
            let logical = n + k;
            if self.state[logical] != State::Basic {
                if !self.marked[logical] {
                    self.marked[logical] = true;
                    self.touched.push(logical);
                }
                self.alpha[logical] -= v;
            }
            for e in self.row_start[k]..self.row_start[k + 1] {
                let j = self.row_index[e];
                if self.state[j] != State::Basic {
                    if !self.marked[j] {
                        self.marked[j] = true;
                        self.touched.push(j);
                    }
                    self.alpha[j] += v * self.row_value[e];
                }
            }
        }
    }

    /// Bound-flipping ratio test with Harris tolerances. `s` is +1 if the
    /// leaving variable moves to its upper bound and −1 for the lower bound.
    /// A breakpoint is passed (its boxed variable flipped) only while the
    /// leaving variable stays infeasible by more than `slack`. Returns the
    /// entering variable and the dual step length `θ ≥ 0`; `self.flips`
    /// holds the variables passed on the way.
    fn ratio_test(&mut self, s: f64, infeasibility: f64, slack: f64) -> Option<(usize, f64)> {
        let _profile = crate::profile::enter("ratio_test");
        self.candidates.clear();
        self.flips.clear();
        let tol = self.controls.pivot_tolerance;
        for &j in &self.touched {
            if self.lower[j] == self.upper[j] {
                continue;
            }
            let a = s * self.alpha[j];
            let blocks = match self.state[j] {
                State::Lower => a > tol,
                State::Upper => a < -tol,
                State::Zero => a.abs() > tol,
                State::Basic => false,
            };
            if blocks {
                let d = self.d[j];
                self.candidates.push(Candidate {
                    j,
                    a,
                    ratio: (d / a).max(0.0),
                    harris: ((d + a.signum() * self.dtol[j]) / a).max(0.0),
                });
            }
        }
        if self.candidates.is_empty() {
            return None;
        }
        // Without a flip at the first breakpoint the Harris pass covers all
        // candidates and no ordering is needed.
        let first = self
            .candidates
            .iter()
            .copied()
            .min_by(|x, y| x.ratio.total_cmp(&y.ratio))
            .expect("nonempty");
        let first_drop = first.a.abs() * (self.upper[first.j] - self.lower[first.j]);
        if first_drop >= infeasibility - slack {
            let bound = self
                .candidates
                .iter()
                .fold(f64::INFINITY, |b, c| b.min(c.harris));
            let c = self
                .candidates
                .iter()
                .filter(|c| c.ratio <= bound)
                .max_by(|x, y| x.a.abs().total_cmp(&y.a.abs()))
                .copied()
                .unwrap_or(first);
            return Some((c.j, c.ratio));
        }
        self.candidates
            .sort_unstable_by(|x, y| x.ratio.total_cmp(&y.ratio));
        let mut slope = infeasibility;
        let mut k = 0;
        while k < self.candidates.len() {
            let c = self.candidates[k];
            let drop = c.a.abs() * (self.upper[c.j] - self.lower[c.j]);
            if drop < slope - slack {
                slope -= drop;
                self.flips.push(c.j);
                k += 1;
            } else {
                break;
            }
        }
        if k == self.candidates.len() {
            return None; // Every breakpoint passed: the row stays infeasible.
        }
        let mut bound = f64::INFINITY;
        let mut end = k;
        while end < self.candidates.len() && self.candidates[end].ratio <= bound {
            bound = bound.min(self.candidates[end].harris);
            end += 1;
        }
        let mut q = k;
        for t in k..end {
            let c = self.candidates[t];
            if c.ratio <= bound && c.a.abs() > self.candidates[q].a.abs() {
                q = t;
            }
        }
        let c = self.candidates[q];
        Some((c.j, c.ratio))
    }

    /// `B⁻¹ a_q` into `self.column`.
    fn compute_column(&mut self, q: usize) {
        let _profile = crate::profile::enter("column_setup");
        if self.column_index.len() < self.m / 8 {
            for &i in &self.column_index {
                self.column[i] = 0.;
            }
        } else {
            self.column.fill(0.);
        }
        Self::add_column(
            &self.col_start,
            &self.col_index,
            &self.col_value,
            self.n,
            q,
            1.0,
            &mut self.column,
        );
        let lu = self.lu.as_mut().expect("factorized basis");
        lu.ftran_entering_indexed(&mut self.column, &mut self.column_index);
    }

    fn pivot(&mut self, r: usize, q: usize, theta: f64, s: f64, to_upper: bool) {
        let _profile = crate::profile::enter("pivot_other");
        let (n, m) = (self.n, self.m);
        let p = self.basic[r];
        if !self.flips.is_empty() {
            self.work.fill(0.0);
            for t in 0..self.flips.len() {
                let j = self.flips[t];
                let old = self.value(j);
                self.state[j] = if self.state[j] == State::Lower {
                    State::Upper
                } else {
                    State::Lower
                };
                let new = self.value(j);
                self.x[j] = new;
                Self::add_column(
                    &self.col_start,
                    &self.col_index,
                    &self.col_value,
                    n,
                    j,
                    new - old,
                    &mut self.work,
                );
            }
            let lu = self.lu.as_mut().expect("factorized basis");
            lu.ftran(&mut self.work);
            for i in 0..m {
                self.x[self.basic[i]] -= self.work[i];
            }
            self.stats.bound_flips += self.flips.len() as u64;
        }
        let target = if to_upper {
            self.upper[p]
        } else {
            self.lower[p]
        };
        let step = (self.x[p] - target) / self.column[r];
        if step != 0.0 {
            for &i in &self.column_index {
                let v = self.column[i];
                if v != 0.0 {
                    self.x[self.basic[i]] -= step * v;
                }
            }
        }
        self.x[q] += step;
        self.x[p] = target;
        let t = s * theta;
        if t != 0.0 {
            for &j in &self.touched {
                self.d[j] -= t * self.alpha[j];
            }
        }
        self.d[p] = -t;
        self.d[q] = 0.0;
        if self.controls.steepest_edge {
            self.tau.copy_from_slice(&self.rho);
            let lu = self.lu.as_mut().expect("factorized basis");
            lu.ftran(&mut self.tau);
            let (wr, ar) = (self.weight[r], self.column[r]);
            for &i in &self.column_index {
                let v = self.column[i];
                if i != r && v != 0.0 {
                    let ratio = v / ar;
                    self.weight[i] =
                        (self.weight[i] + ratio * (ratio * wr - 2.0 * self.tau[i])).max(MIN_WEIGHT);
                }
            }
            self.weight[r] = (wr / (ar * ar)).max(MIN_WEIGHT);
        }
        let lu = self.lu.as_mut().expect("factorized basis");
        lu.update_sparse(r, &self.column, self.column_index.iter().copied());
        self.stats.ft_updates += u64::from(lu.uses_ft());
        self.stats.updates += 1;
        self.stats.peak_entries = self.stats.peak_entries.max(lu.entries());
        self.basic[r] = q;
        self.state[q] = State::Basic;
        self.state[p] = if to_upper { State::Upper } else { State::Lower };
        if self.lower[p] == self.upper[p] {
            self.state[p] = State::Lower;
        }
    }

    fn random(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Shifts nonbasic costs away from zero reduced cost, keeping dual
    /// feasibility, to break dual degeneracy.
    fn perturb(&mut self) {
        self.perturbed = true;
        self.stats.perturbations += 1;
        for j in 0..self.n + self.m {
            if self.state[j] == State::Basic || self.lower[j] == self.upper[j] {
                continue;
            }
            let magnitude = 5e-7 * (1.0 + self.cost0[j].abs()) * (1.0 + self.random());
            let delta = match self.state[j] {
                State::Lower => magnitude,
                State::Upper => -magnitude,
                _ => continue,
            };
            self.cost[j] += delta;
            self.d[j] += delta;
        }
    }

    /// Restores the unperturbed costs; the caller recomputes duals.
    fn unperturb(&mut self) -> bool {
        if !self.perturbed {
            return false;
        }
        self.cost.clone_from(&self.cost0);
        self.perturbed = false;
        true
    }

    /// Fresh factorization, primal and dual values. Returns whether the
    /// dual simplex must restart (basis repaired or dual feasibility lost).
    fn refresh(&mut self) -> Result<bool> {
        let repaired = self.refactor()?;
        self.compute_dual();
        self.flip_to_dual_feasible();
        self.compute_primal();
        Ok(repaired || self.nonboxed_dual_infeasible())
    }

    fn phase_two(&mut self, budget: &mut Budget<'_>, iterations: &mut u64) -> Result<Outcome> {
        let mut streak = 0;
        loop {
            if let Some(s) = budget.stop() {
                return Ok(Outcome::Stopped(s));
            }
            if *iterations > self.iteration_guard {
                return Ok(Outcome::Failure);
            }
            let adaptive = self.controls.adaptive_refactor;
            let (interval, cap) = (
                self.controls.refactor_interval,
                self.controls.max_factor_entries,
            );
            let lu = self.lu();
            if lu.needs_refactor(interval, cap, adaptive) && self.refresh()? {
                return Ok(Outcome::Restart);
            }
            let Some(r) = self.choose_row() else {
                return Ok(Outcome::Optimal);
            };
            let p = self.basic[r];
            let to_upper = self.x[p] > self.upper[p];
            let target = if to_upper {
                self.upper[p]
            } else {
                self.lower[p]
            };
            let s = if to_upper { 1.0 } else { -1.0 };
            let infeasibility = (self.x[p] - target).abs();
            self.compute_row(r);
            let Some((q, theta)) = self.ratio_test(s, infeasibility, self.ptol[p]) else {
                if self.lu().updates() > 0 {
                    if self.refresh()? {
                        return Ok(Outcome::Restart);
                    }
                    continue;
                }
                self.ray = Some(self.rho.clone());
                return Ok(Outcome::Infeasible);
            };
            self.compute_column(q);
            let (from_column, from_row) = (self.column[r], self.alpha[q]);
            if (from_column - from_row).abs() > 1e-7 * from_column.abs() + 1e-11 {
                if self.lu().updates() > 0 {
                    if self.refresh()? {
                        return Ok(Outcome::Restart);
                    }
                    continue;
                }
                if from_column.abs() < 1e-12 {
                    return Ok(Outcome::Failure);
                }
            }
            streak = if theta == 0.0 { streak + 1 } else { 0 };
            self.pivot(r, q, theta, s, to_upper);
            budget.pivots += 1;
            *iterations += 1;
            if streak >= DEGENERATE_STREAK && !self.perturbed {
                self.perturb();
            }
        }
    }

    /// Auxiliary problem with every bound replaced by a small box around 0;
    /// its optimal basis is dual feasible for the true bounds unless the LP
    /// is dual infeasible. Returns `None` to continue with the true bounds.
    fn phase_one(
        &mut self,
        budget: &mut Budget<'_>,
        iterations: &mut u64,
    ) -> Result<Option<Status>> {
        let true_lower = self.lower.clone();
        let true_upper = self.upper.clone();
        for j in 0..self.n + self.m {
            let (l, u) = (true_lower[j], true_upper[j]);
            (self.lower[j], self.upper[j]) = match (l.is_finite(), u.is_finite()) {
                (true, true) => (0.0, 0.0),
                (true, false) => (0.0, 1.0),
                (false, true) => (-1.0, 0.0),
                (false, false) => (-1.0, 1.0),
            };
        }
        for j in 0..self.n + self.m {
            if self.state[j] != State::Basic {
                self.state[j] = self.state_from_dual(j);
            }
        }
        self.compute_primal();
        let outcome = self.phase_two(budget, iterations)?;
        self.lower = true_lower;
        self.upper = true_upper;
        if self.unperturb() {
            self.compute_dual();
        }
        for j in 0..self.n + self.m {
            if self.state[j] != State::Basic {
                self.state[j] = self.state_from_dual(j);
            }
        }
        self.compute_primal();
        Ok(match outcome {
            Outcome::Optimal if self.nonboxed_dual_infeasible() => Some(Status::Unbounded),
            Outcome::Optimal | Outcome::Restart => None,
            Outcome::Stopped(s) => Some(s),
            Outcome::Infeasible | Outcome::Failure => Some(Status::NumericalFailure),
        })
    }

    /// The LP is dual infeasible: unbounded if primal feasible, else
    /// infeasible. Primal feasibility is decided with zero costs, where every
    /// basis is dual feasible.
    fn classify(&mut self, budget: &mut Budget<'_>, iterations: &mut u64) -> Result<Status> {
        let costs = std::mem::replace(&mut self.cost0, vec![0.0; self.n + self.m]);
        self.cost.fill(0.0);
        self.perturbed = false;
        self.compute_dual();
        let status = self.optimize(budget, iterations, false)?;
        self.cost0 = costs;
        self.cost.clone_from(&self.cost0);
        self.perturbed = false;
        self.compute_dual();
        Ok(match status {
            Status::Optimal => Status::Unbounded,
            s => s,
        })
    }

    /// Solves the LP from the current basis. `Optimal` is declared only with
    /// primal and dual values recomputed from scratch and feasible; with
    /// `fresh_optimum` (cold solves), also on a fresh factorization.
    pub fn solve(&mut self, budget: &mut Budget<'_>, fresh_optimum: bool) -> Result<Status> {
        self.ray = None;
        self.basis_id = None;
        if let Some(s) = budget.stop() {
            return Ok(s);
        }
        if self.lu.is_none() {
            self.refactor()?;
            self.dual_stale = true;
        }
        if self.dual_stale {
            self.compute_dual();
        }
        if self.primal_stale {
            self.compute_primal();
        }
        let mut iterations = 0u64;
        self.fresh_optimum = fresh_optimum;
        self.optimize(budget, &mut iterations, true)
    }

    /// Restartable driver: dual phase 1 when needed, phase 2, and the final
    /// unperturbed recomputation. Without `classify`, dual infeasibility can
    /// only come from perturbation drift (zero costs) and is recomputed away.
    fn optimize(
        &mut self,
        budget: &mut Budget<'_>,
        iterations: &mut u64,
        classify: bool,
    ) -> Result<Status> {
        let mut fresh = self.fresh_optimum;
        for _ in 0..12 {
            if self.flip_to_dual_feasible() > 0 {
                self.compute_primal();
            }
            if self.nonboxed_dual_infeasible() {
                if !classify {
                    self.unperturb();
                    self.compute_dual();
                    continue;
                }
                match self.phase_one(budget, iterations)? {
                    None => continue,
                    Some(Status::Unbounded) => return self.classify(budget, iterations),
                    Some(s) => return Ok(s),
                }
            }
            match self.phase_two(budget, iterations)? {
                Outcome::Optimal => {
                    self.unperturb();
                    if fresh && self.lu().updates() > 0 && self.refactor()? {
                        self.compute_dual();
                        continue;
                    }
                    self.compute_dual();
                    self.compute_primal();
                    if self.flip_to_dual_feasible() > 0 {
                        self.compute_primal();
                    }
                    if self.nonboxed_dual_infeasible() || self.choose_row().is_some() {
                        fresh = true; // recheck on a fresh factorization
                        continue;
                    }
                    return Ok(Status::Optimal);
                }
                Outcome::Restart => continue,
                Outcome::Infeasible => {
                    if self.unperturb() {
                        self.compute_dual();
                    }
                    return Ok(Status::Infeasible);
                }
                Outcome::Stopped(s) => {
                    if self.unperturb() {
                        self.compute_dual();
                    }
                    return Ok(s);
                }
                Outcome::Failure => return Ok(Status::NumericalFailure),
            }
        }
        Ok(Status::NumericalFailure)
    }
}

/// `v * scale` for a finite or infinite bound; finite bounds must stay finite.
fn scaled(v: f64, scale: f64) -> Result<f64> {
    if !v.is_finite() {
        return Ok(v);
    }
    let s = v * scale;
    if s.is_finite() && (s != 0.0 || v == 0.0) {
        Ok(s)
    } else {
        Err(Error::Unsupported(
            "bound out of range after power-of-two scaling".into(),
        ))
    }
}

/// Geometric-mean row and column scaling followed by column equilibration,
/// rounded to powers of two.
fn scale_factors(
    m: usize,
    col_start: &[usize],
    col_index: &[usize],
    col_value: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let n = col_start.len() - 1;
    let mut r = vec![1.0; m];
    let mut s = vec![1.0; n];
    let (small, large) = col_value
        .iter()
        .fold((f64::INFINITY, 0.0_f64), |(a, b), v| {
            (a.min(v.abs()), b.max(v.abs()))
        });
    if small >= 1.0 / 32.0 && large <= 32.0 {
        return (r, s); // already well scaled
    }
    let mut lo = vec![0.0_f64; m];
    let mut hi = vec![0.0_f64; m];
    for _ in 0..4 {
        lo.fill(f64::INFINITY);
        hi.fill(0.0);
        for j in 0..n {
            for e in col_start[j]..col_start[j + 1] {
                let (k, v) = (col_index[e], col_value[e].abs() * s[j]);
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
        for k in 0..m {
            if hi[k] > 0.0 {
                r[k] = 1.0 / (lo[k].sqrt() * hi[k].sqrt());
            }
        }
        for j in 0..n {
            let (mut a, mut b) = (f64::INFINITY, 0.0_f64);
            for e in col_start[j]..col_start[j + 1] {
                let v = col_value[e].abs() * r[col_index[e]];
                a = a.min(v);
                b = b.max(v);
            }
            if b > 0.0 {
                s[j] = 1.0 / (a.sqrt() * b.sqrt());
            }
        }
    }
    for v in &mut r {
        *v = pow2(*v);
    }
    for j in 0..n {
        let mut b = 0.0_f64;
        for e in col_start[j]..col_start[j + 1] {
            b = b.max(col_value[e].abs() * r[col_index[e]]);
        }
        s[j] = if b > 0.0 { pow2(1.0 / b) } else { 1.0 };
    }
    (r, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeOptions;
    use lp_ip_craft::*;

    fn controls() -> Controls {
        Controls {
            pivot_tolerance: 1e-9,
            refactor_interval: 16,
            adaptive_refactor: true,
            basis_update: crate::BasisUpdate::ForrestTomlin,
            factor_pivot_search: 4,
            factor_ordering: crate::FactorOrdering::Auto,
            max_factor_entries: usize::MAX,
            steepest_edge: true,
            feasibility: 1e-7,
            dual: 1e-7,
        }
    }

    fn solve(model: &Model) -> (Status, Lp) {
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let mut budget = Budget {
            start: std::time::Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let mut lp = Lp::new(
            model,
            &vec![true; model.m()],
            &vec![true; model.n()],
            controls(),
        )
        .unwrap();
        let status = lp.solve(&mut budget, true).unwrap();
        (status, lp)
    }

    #[test]
    fn scaling_is_by_powers_of_two() {
        for v in [1e-11, 0.3, 1.0, 7.0, 1e9] {
            let p = pow2(v);
            assert_eq!(p.to_bits() & ((1u64 << 52) - 1), 0, "{p}");
            assert!(p / v < 1.5 && v / p < 1.5);
        }
    }

    #[test]
    fn production_lp_duals_match_the_tutorial() {
        let model = Model::new(ModelData {
            sense: Sense::Maximize,
            objective: vec![45.0, 80.0, 110.0],
            objective_offset: 0.0,
            variables: Bounds::non_negative(3),
            kinds: vec![VariableKind::Continuous; 3],
            matrix: CsrMatrix::from_dense(
                3,
                3,
                &[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0],
            )
            .unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY; 3], vec![400.0, 450.0, 60.0]),
        })
        .unwrap();
        let (status, lp) = solve(&model);
        assert_eq!(status, Status::Optimal);
        let x = lp.point();
        for (a, b) in x.iter().zip([30.0, 10.0, 0.0]) {
            assert!((a - b).abs() < 1e-9, "{x:?}");
        }
        let duals = lp.duals(&model);
        for (a, b) in duals.row_duals.iter().zip([0.0, 11.0 / 3.0, 25.0 / 3.0]) {
            assert!((a - b).abs() < 1e-9, "{duals:?}");
        }
        assert!(check::lp_residuals(&model, &x, &duals).unwrap().max() < 1e-9);
    }

    #[test]
    fn infeasible_rows_have_a_verified_farkas_combination() {
        let model = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![1.0, 1.0],
            objective_offset: 0.0,
            variables: Bounds::new(vec![0.0; 2], vec![1.0; 2]),
            kinds: vec![VariableKind::Continuous; 2],
            matrix: CsrMatrix::from_dense(2, 2, &[1.0, 1.0, 1.0, -1.0]).unwrap(),
            rows: Bounds::new(vec![2.5, f64::NEG_INFINITY], vec![f64::INFINITY, 5.0]),
        })
        .unwrap();
        let (status, lp) = solve(&model);
        assert_eq!(status, Status::Infeasible);
        assert!(lp.infeasibility_verified());
    }
}
