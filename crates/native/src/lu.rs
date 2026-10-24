//! Sparse LU factorization of simplex bases with Markowitz pivot selection
//! and a relative threshold, plus adaptive product-form / Forrest–Tomlin updates.
//!
//! The basis `B` has one column per basis position. Factorization chooses a
//! pivot sequence `(row r_k, position p_k)` and stores, per pivot `k`, the
//! multipliers `L_k` of the rows it eliminates and the remaining pivot row
//! `U_k` over positions pivoted later. Exact cancellations are removed from
//! both active orientations and their Markowitz counts. Once the
//! active submatrix is dense enough, elimination finishes with a dense
//! partial-pivoting kernel.
//!
//! L and U are kept both by pivot and transposed, so both triangular solves
//! of `ftran` and `btran` push updates from nonzero entries only. Solves use
//! dense work vectors of the basis dimension.

use crate::active::{Active, Entry, FactorWorkspace};

const NIL: usize = usize::MAX;
/// Relative pivot threshold: `|a_rp| >= THRESHOLD * max_i |a_ip|`.
const THRESHOLD: f64 = 0.1;
/// Absolute pivot tolerance on the (scaled) basis entries.
const ABS_PIVOT: f64 = 1e-11;
/// Candidate columns/rows examined after the first acceptable pivot.
#[derive(Clone, Copy)]
pub(crate) struct FactorOptions {
    pub search: usize,
    pub ordered: bool,
    pub replay: bool,
}
impl Default for FactorOptions {
    fn default() -> Self {
        Self {
            search: 4,
            ordered: false,
            replay: false,
        }
    }
}
/// Active-submatrix density that switches to dense elimination.
const DENSE_DENSITY: f64 = 0.3;

/// Why a factorization was not produced.
#[derive(Debug)]
pub(crate) enum FactorError {
    /// No acceptable pivot for these positions; `rows` are the rows left
    /// uncovered (same length). Replacing each position's column by the
    /// logical column of one of these rows gives a factorizable basis.
    Singular {
        positions: Vec<usize>,
        rows: Vec<usize>,
    },
    /// The factors would exceed the entry cap.
    TooLarge,
}

/// Doubly linked count buckets (rows or columns by active entry count).
struct Buckets {
    head: Vec<usize>,
    next: Vec<usize>,
    prev: Vec<usize>,
    count: Vec<usize>,
}

impl Buckets {
    fn new(items: usize, max_count: usize) -> Self {
        Self {
            head: vec![NIL; max_count + 1],
            next: vec![NIL; items],
            prev: vec![NIL; items],
            count: vec![0; items],
        }
    }
    fn insert(&mut self, x: usize, c: usize) {
        self.count[x] = c;
        let h = self.head[c];
        self.next[x] = h;
        self.prev[x] = NIL;
        if h != NIL {
            self.prev[h] = x;
        }
        self.head[c] = x;
    }
    fn remove(&mut self, x: usize) {
        let (p, n) = (self.prev[x], self.next[x]);
        if p != NIL {
            self.next[p] = n;
        } else {
            self.head[self.count[x]] = n;
        }
        if n != NIL {
            self.prev[n] = p;
        }
    }
    fn change(&mut self, x: usize, c: usize) {
        if self.count[x] != c {
            self.remove(x);
            self.insert(x, c);
        }
    }
}

#[derive(Clone, Copy)]
struct Eta {
    support: u64,
    position: usize,
    pivot: f64,
    start: usize,
    end: usize,
}

/// Compressed lists: `index[start[k]..start[k + 1]]` with matching values.
#[derive(Default)]
pub(super) struct Lists {
    start: Vec<usize>,
    index: Vec<usize>,
    value: Vec<f64>,
}

impl Lists {
    pub(super) fn get(&self, k: usize) -> (&[usize], &[f64]) {
        let range = self.start[k]..self.start[k + 1];
        (&self.index[range.clone()], &self.value[range])
    }
    /// Regroups the entries into `groups` lists keyed by their index; each
    /// regrouped entry stores `owner[k]` of the list `k` it came from.
    fn transpose(&self, groups: usize, owner: &[usize]) -> Self {
        let mut start = vec![0usize; groups + 1];
        for &g in &self.index {
            start[g + 1] += 1;
        }
        for g in 0..groups {
            start[g + 1] += start[g];
        }
        let mut fill = start.clone();
        let mut index = vec![0; self.index.len()];
        let mut value = vec![0.0; self.index.len()];
        for (k, &who) in owner.iter().enumerate().take(self.start.len() - 1) {
            for e in self.start[k]..self.start[k + 1] {
                let g = self.index[e];
                index[fill[g]] = who;
                value[fill[g]] = self.value[e];
                fill[g] += 1;
            }
        }
        Self {
            start,
            index,
            value,
        }
    }
}

/// Reusable symbolic reach, bounded to avoid overhead on dense solves.
struct Reach {
    seen: Vec<bool>,
    nodes: Vec<usize>,
    cooldown: [usize; 4],
}
impl Reach {
    fn find(
        &mut self,
        v: &[f64],
        graph: &Lists,
        keys: Option<&[usize]>,
        rank: &[usize],
        channel: usize,
    ) -> bool {
        self.find_seeded(v, graph, keys, rank, channel, None)
    }
    #[allow(clippy::too_many_arguments)]
    fn find_seeded(
        &mut self,
        v: &[f64],
        graph: &Lists,
        keys: Option<&[usize]>,
        rank: &[usize],
        channel: usize,
        seeds: Option<&[usize]>,
    ) -> bool {
        let _timer = crate::profile::enter(if channel < 2 {
            "reach_ftran"
        } else {
            "reach_btran"
        });
        if self.cooldown[channel] > 0 {
            self.cooldown[channel] -= 1;
            return false;
        }
        if v.len() < 128 {
            return false;
        }
        for &i in &self.nodes {
            self.seen[i] = false;
        }
        self.nodes.clear();
        let limit = v.len() / if channel < 2 { 32 } else { 8 };
        for at in 0..seeds.map_or(v.len(), |s| s.len()) {
            let i = seeds.map_or(at, |s| s[at]);
            if v[i] != 0. {
                self.seen[i] = true;
                self.nodes.push(i);
                if self.nodes.len() > limit {
                    self.cooldown[channel] = 8;
                    return false;
                }
            }
        }
        let mut at = 0;
        while at < self.nodes.len() {
            let i = self.nodes[at];
            let key = keys.map_or(i, |k| k[i]);
            for &j in graph.get(key).0 {
                if !self.seen[j] {
                    self.seen[j] = true;
                    self.nodes.push(j);
                    if self.nodes.len() > limit {
                        self.cooldown[channel] = 8;
                        return false;
                    }
                }
            }
            at += 1;
        }
        self.nodes.sort_unstable_by_key(|&i| rank[i]);
        true
    }
}

/// A basis factorization with product-form or Forrest–Tomlin updates.
pub(crate) struct Lu {
    m: usize,
    prow: Vec<usize>,
    pcol: Vec<usize>,
    diag: Vec<f64>,
    diag_inv: Vec<f64>,
    /// Multipliers per pivot step, by row.
    l: Lists,
    /// The same multipliers per eliminated row, by pivot row.
    l_rows: Lists,
    /// Pivot rows per step, by position.
    u: Lists,
    /// The same entries per position, by pivot row.
    u_cols: Lists,
    etas: Vec<Eta>,
    eta_index: Vec<usize>,
    eta_value: Vec<f64>,
    work: Vec<f64>,
    work_index: Vec<usize>,
    work_sparse: bool,
    result_index: Vec<usize>,
    result_marked: Vec<bool>,
    result_sparse: bool,
    row_step: Vec<usize>,
    col_step: Vec<usize>,
    row_position: Vec<usize>,
    reach: Reach,
    eta_support: Vec<usize>,
    eta_marked: Vec<bool>,
    ft: Option<super::ft::Ft>,
    force_refactor: bool,
    sparse_btran: bool,
    update_nnz: usize,
    factor_work: u64,
    eta_work: u64,
    dense: Option<crate::dense::DenseTail>,
    dense_dimension: usize,
    replayed_pivots: usize,
}

impl Lu {
    /// Factorizes the basis whose position `p` has the sparse column written
    /// by `column(p, out)` (entries `(row, value)`, rows `< m`, distinct).
    /// `cap` bounds the stored entries (diagonal, L and U).
    #[cfg(test)]
    pub fn factor(
        m: usize,
        column: impl FnMut(usize, &mut Vec<(usize, f64)>),
        cap: usize,
    ) -> Result<Self, FactorError> {
        Self::factor_with_workspace(
            m,
            column,
            cap,
            &mut FactorWorkspace::default(),
            FactorOptions::default(),
        )
    }
    #[cfg(any(test, feature = "basis-replay"))]
    pub fn factor_with_workspace(
        m: usize,
        column: impl FnMut(usize, &mut Vec<(usize, f64)>),
        cap: usize,
        workspace: &mut FactorWorkspace,
        options: FactorOptions,
    ) -> Result<Self, FactorError> {
        Self::factor_with_execution(
            m,
            column,
            cap,
            workspace,
            options,
            &crate::parallel::Execution::default(),
        )
    }
    pub fn factor_with_execution(
        m: usize,
        mut column: impl FnMut(usize, &mut Vec<(usize, f64)>),
        cap: usize,
        workspace: &mut FactorWorkspace,
        options: FactorOptions,
        execution: &crate::parallel::Execution,
    ) -> Result<Self, FactorError> {
        let _profile = crate::profile::enter("factorization");
        let _assembly = crate::profile::enter("factor_assembly");
        let FactorWorkspace {
            matrix: a,
            slot,
            pivot_row,
            pivot_col,
            row_slots,
            input,
            order_cache,
            basis_signature,
            pivot_cache,
        } = workspace;
        if m > u32::MAX as usize {
            return Err(FactorError::TooLarge);
        }
        a.reset(m);
        let mut signature = 0xcbf29ce484222325u64;
        let mut hash = |word: u64| {
            signature ^= word;
            signature = signature.wrapping_mul(0x100000001b3);
        };
        if options.replay {
            hash(m as u64);
            hash(options.search as u64);
            hash(u64::from(options.ordered));
        }
        for p in 0..m {
            input.clear();
            column(p, input);
            if options.replay {
                hash(p as u64);
                hash(input.len() as u64);
            }
            for &(i, v) in input.iter() {
                if options.replay {
                    hash(i as u64);
                    hash(v.to_bits());
                }
                if v != 0. {
                    a.insert(p, i, v);
                }
            }
        }
        let replay = options.replay && *basis_signature == signature;
        let mut colb = Buckets::new(m, m);
        let mut rowb = Buckets::new(m, m);
        let mut active = 0usize;
        if options.ordered {
            let _order = crate::profile::enter("factor_ordering");
            order_cache.prepare(a);
            for &p in order_cache.order.iter().rev() {
                colb.insert(p, a.cols[p].len());
                active += a.cols[p].len();
            }
        } else {
            for (p, col) in a.cols.iter().enumerate() {
                colb.insert(p, col.len());
                active += col.len();
            }
        }
        for (i, row) in a.rows.iter().enumerate() {
            rowb.insert(i, row.len());
        }
        let mut col_done = vec![false; m];
        let mut row_done = vec![false; m];
        // Scatter map from row to slot in the column being updated.
        slot.resize(m, NIL);
        slot.fill(NIL);
        let empty = || Lists {
            start: vec![0],
            ..Lists::default()
        };
        let mut lu = Self {
            m,
            prow: Vec::with_capacity(m),
            pcol: Vec::with_capacity(m),
            diag: Vec::with_capacity(m),
            diag_inv: Vec::new(),
            l: empty(),
            l_rows: Lists::default(),
            u: empty(),
            u_cols: Lists::default(),
            etas: Vec::new(),
            eta_index: Vec::new(),
            eta_value: Vec::new(),
            work: vec![0.0; m],
            work_index: Vec::new(),
            work_sparse: false,
            result_index: Vec::new(),
            result_marked: vec![false; m],
            result_sparse: false,
            row_step: vec![0; m],
            col_step: vec![0; m],
            row_position: vec![0; m],
            eta_support: Vec::new(),
            eta_marked: vec![false; m],
            ft: None,
            force_refactor: false,
            sparse_btran: false,
            update_nnz: 0,
            factor_work: 0,
            eta_work: 0,
            dense: None,
            dense_dimension: 0,
            replayed_pivots: 0,
            reach: Reach {
                seen: vec![false; m],
                nodes: Vec::new(),
                cooldown: [0; 4],
            },
        };
        drop(_assembly);

        let mut singular = false;
        while lu.prow.len() < m {
            let remaining = (m - lu.prow.len()) as f64;
            if active as f64 > DENSE_DENSITY * remaining * remaining {
                break;
            }
            let _search = crate::profile::enter("factor_pivot_search");
            let cached = if replay {
                pivot_cache.get(lu.prow.len()).copied().filter(|&(r, p)| {
                    r < m && p < m && !row_done[r] && !col_done[p] && {
                        let max = a.column_max(p);
                        a.cols[p]
                            .iter()
                            .find(|e| e.row as usize == r)
                            .is_some_and(|e| {
                                e.value.is_finite()
                                    && e.value.abs() > ABS_PIVOT
                                    && max.is_finite()
                                    && e.value.abs() >= THRESHOLD * max
                            })
                    }
                })
            } else {
                None
            };
            if cached.is_some() {
                lu.replayed_pivots += 1;
            }
            let candidate = cached.or_else(|| {
                find_pivot(
                    a,
                    &colb,
                    &rowb,
                    m,
                    options.search,
                    options.ordered.then_some(order_cache.rank.as_slice()),
                )
            });
            drop(_search);
            let Some((r, p)) = candidate else {
                singular = true;
                break;
            };
            let _extract = crate::profile::enter("factor_extract");
            // Save numeric order before swap removals update the cross indices.
            pivot_row.clear();
            row_slots.clear();
            row_slots.extend_from_slice(&a.rows[r]);
            let mut diagonal = 0.;
            active -= row_slots.len();
            for &(q, k) in row_slots.iter() {
                let (q, k) = (q as usize, k as usize);
                let value = a.cols[q][k].value;
                if q == p {
                    diagonal = value;
                } else {
                    pivot_row.push((q, value));
                }
                a.remove(q, k);
            }
            pivot_col.clear();
            active -= a.cols[p].len();
            for e in &a.cols[p] {
                pivot_col.push((e.row as usize, e.value / diagonal));
            }
            while !a.cols[p].is_empty() {
                a.remove(p, a.cols[p].len() - 1);
            }
            colb.remove(p);
            rowb.remove(r);
            col_done[p] = true;
            row_done[r] = true;
            drop(_extract);
            let _eliminate = crate::profile::enter("factor_sparse_eliminate");
            lu.factor_work = lu
                .factor_work
                .saturating_add((pivot_row.len() as u64).saturating_mul(pivot_col.len() as u64));
            // Schur complement update; scatter map is per column, O(m) memory.
            for &(q, u) in pivot_row.iter() {
                if u == 0. {
                    continue;
                }
                for (k, e) in a.cols[q].iter().enumerate() {
                    slot[e.row as usize] = k;
                }
                a.dirty(q);
                for &(i, l) in pivot_col.iter() {
                    let delta = -l * u;
                    if slot[i] != NIL {
                        a.cols[q][slot[i]].value += delta;
                    } else {
                        slot[i] = a.cols[q].len();
                        a.insert(q, i, delta);
                        active += 1;
                    }
                }
                let mut at = 0;
                while at < a.cols[q].len() {
                    let e = a.cols[q][at];
                    slot[e.row as usize] = NIL;
                    if e.value == 0. {
                        a.remove(q, at);
                        rowb.change(e.row as usize, a.rows[e.row as usize].len());
                        active -= 1;
                    } else {
                        at += 1;
                    }
                }
                colb.change(q, a.cols[q].len());
            }
            for &(i, _) in pivot_col.iter() {
                rowb.change(i, a.rows[i].len());
            }
            lu.push_pivot(r, p, diagonal, pivot_col, pivot_row);
            if lu.entries() > cap {
                return Err(FactorError::TooLarge);
            }
        }
        if !singular && lu.prow.len() < m {
            lu.dense_phase(&a.cols, &mut row_done, &mut col_done, cap, execution)?;
        }
        if lu.prow.len() < m {
            return Err(FactorError::Singular {
                positions: (0..m).filter(|&p| !col_done[p]).collect(),
                rows: (0..m).filter(|&i| !row_done[i]).collect(),
            });
        }
        let _pack = crate::profile::enter("factor_pack");
        lu.diag_inv = lu.diag.iter().map(|d| 1. / d).collect();
        // L by eliminated row: entries name the pivot row of their step.
        lu.l_rows = lu.l.transpose(m, &lu.prow);
        // U by position: entries name the pivot row of their step.
        lu.u_cols = lu.u.transpose(m, &lu.prow);
        for k in 0..m {
            lu.row_step[lu.prow[k]] = k;
            lu.col_step[lu.pcol[k]] = k;
            lu.row_position[lu.prow[k]] = lu.pcol[k];
        }
        if options.replay {
            *basis_signature = signature;
            pivot_cache.clear();
            pivot_cache.extend(lu.prow.iter().copied().zip(lu.pcol.iter().copied()));
        }
        Ok(lu)
    }

    pub fn replayed_pivots(&self) -> usize {
        self.replayed_pivots
    }

    fn push_pivot(
        &mut self,
        r: usize,
        p: usize,
        a: f64,
        col: &[(usize, f64)],
        row: &[(usize, f64)],
    ) {
        self.prow.push(r);
        self.pcol.push(p);
        self.diag.push(a);
        for &(i, l) in col {
            if l != 0.0 {
                self.l.index.push(i);
                self.l.value.push(l);
            }
        }
        self.l.start.push(self.l.index.len());
        for &(q, v) in row {
            if v != 0.0 {
                self.u.index.push(q);
                self.u.value.push(v);
            }
        }
        self.u.start.push(self.u.index.len());
    }

    /// Dense right-looking elimination with partial pivoting of the rows and
    /// positions not yet pivoted. Columns without an acceptable pivot stay
    /// unpivoted (the basis is singular).
    fn dense_phase(
        &mut self,
        cols: &[Vec<Entry>],
        row_done: &mut [bool],
        col_done: &mut [bool],
        cap: usize,
        execution: &crate::parallel::Execution,
    ) -> Result<(), FactorError> {
        let rows: Vec<_> = (0..self.m).filter(|&i| !row_done[i]).collect();
        let positions: Vec<_> = (0..self.m).filter(|&p| !col_done[p]).collect();
        let n = rows.len();
        if n >= 64
            && self
                .entries()
                .checked_add(n.saturating_mul(n))
                .is_some_and(|v| v <= cap)
        {
            let _profile = crate::profile::enter("factor_dense");
            if let Some(tail) =
                crate::dense::DenseTail::factor(cols, rows, positions, self.m, execution)
            {
                self.dense_dimension = n;
                self.factor_work = self
                    .factor_work
                    .saturating_add((n as u64).saturating_pow(3) / 3);
                for k in 0..n {
                    let (r, p) = (tail.rows[k], tail.cols[k]);
                    row_done[r] = true;
                    col_done[p] = true;
                    self.prow.push(r);
                    self.pcol.push(p);
                    self.diag.push(tail.data[k * n + k]);
                }
                self.l.start.resize(self.m + 1, self.l.index.len());
                self.u.start.resize(self.m + 1, self.u.index.len());
                self.dense = Some(tail);
                return Ok(());
            }
        }
        self.dense_phase_legacy(cols, row_done, col_done, cap)
    }

    fn dense_phase_legacy(
        &mut self,
        cols: &[Vec<Entry>],
        row_done: &mut [bool],
        col_done: &mut [bool],
        cap: usize,
    ) -> Result<(), FactorError> {
        let _dense = crate::profile::enter("factor_dense");
        let m = self.m;
        let rows_left: Vec<usize> = (0..m).filter(|&i| !row_done[i]).collect();
        let cols_left: Vec<usize> = (0..m).filter(|&p| !col_done[p]).collect();
        let k = rows_left.len();
        self.dense_dimension = k;
        if self
            .entries()
            .checked_add(k.saturating_mul(k) / 2)
            .is_none_or(|v| v > cap)
        {
            return Err(FactorError::TooLarge);
        }
        let mut dense_row = vec![NIL; m];
        for (a, &i) in rows_left.iter().enumerate() {
            dense_row[i] = a;
        }
        let mut d = vec![0.0; k * k];
        for (c, &p) in cols_left.iter().enumerate() {
            for e in &cols[p] {
                d[c * k + dense_row[e.row as usize]] = e.value;
            }
        }
        let mut live: Vec<usize> = (0..k).collect();
        let mut col = Vec::with_capacity(k);
        let mut row = Vec::with_capacity(k);
        for c in 0..k {
            let (head, tail) = d.split_at_mut((c + 1) * k);
            let pivot_column = &mut head[c * k..];
            let mut best = NIL;
            let mut largest = ABS_PIVOT;
            for (t, &a) in live.iter().enumerate() {
                let v = pivot_column[a].abs();
                if v > largest {
                    largest = v;
                    best = t;
                }
            }
            if best == NIL {
                continue;
            }
            let star = live.swap_remove(best);
            let pivot = pivot_column[star];
            col.clear();
            for &a in &live {
                let l = pivot_column[a] / pivot;
                pivot_column[a] = l;
                if l != 0.0 {
                    col.push((rows_left[a], l));
                }
            }
            row.clear();
            for (offset, later) in tail.chunks_exact_mut(k).enumerate() {
                let u = later[star];
                if u == 0.0 {
                    continue;
                }
                row.push((cols_left[c + 1 + offset], u));
                for &a in &live {
                    later[a] -= pivot_column[a] * u;
                }
            }
            row_done[rows_left[star]] = true;
            col_done[cols_left[c]] = true;
            self.push_pivot(rows_left[star], cols_left[c], pivot, &col, &row);
            if self.entries() > cap {
                return Err(FactorError::TooLarge);
            }
        }
        Ok(())
    }

    pub fn dense_dimension(&self) -> usize {
        self.dense_dimension
    }
    #[cfg(feature = "basis-replay")]
    pub fn replay_details(&self) -> (usize, u64) {
        let mut h = 0xcbf29ce484222325u64;
        for &v in self.prow.iter().chain(&self.pcol) {
            h = (h ^ v as u64).wrapping_mul(0x100000001b3);
        }
        (self.dense_dimension, h)
    }
    #[cfg(feature = "kernel-profile")]
    pub fn factor_sizes(&self) -> (usize, usize) {
        {
            let extra = self
                .dense
                .as_ref()
                .map_or(0, |d| d.len() * (d.len() - 1) / 2);
            (self.l.value.len() + extra, self.u.value.len() + extra)
        }
    }
    pub fn ft_suitable(&self) -> bool {
        self.dense.is_none()
            && self.m >= 128
            && self.l.value.len() <= 4 * self.m
            && self.entries() <= 32 * self.m
    }
    pub fn average_update_nnz(&self) -> usize {
        self.update_nnz / self.updates().max(1)
    }
    pub fn uses_ft(&self) -> bool {
        self.ft.is_some() && !self.force_refactor
    }
    pub fn enable_ft(&mut self) {
        self.materialize_dense();
        self.ft = Some(super::ft::Ft::new(
            &self.prow, &self.pcol, &self.diag, &self.u,
        ));
        self.u = Lists::default();
        self.u_cols = Lists::default();
    }

    /// Stored numerical entries: diagonal, L, U and update files.
    pub fn entries(&self) -> usize {
        self.ft
            .as_ref()
            .map_or(self.diag.len() + self.u.value.len(), |f| f.entries())
            + self.l.value.len()
            + self.dense.as_ref().map_or(0, |d| d.data.len() - d.len())
            + self.eta_value.len()
            + self.etas.len()
    }

    /// Number of eta updates since factorization.
    pub fn updates(&self) -> usize {
        self.etas.len() + self.ft.as_ref().map_or(0, |f| f.updates())
    }

    /// Bound update-file work relative to the sparse base factor. The
    /// caller's interval and memory cap remain hard upper limits.
    pub fn needs_refactor(&self, interval: usize, cap: usize, adaptive: bool) -> bool {
        let dense_work = adaptive
            && self.m >= 2048
            && self.dense.as_ref().is_some_and(|d| d.len() >= self.m / 8);
        let interval = if adaptive && !dense_work {
            interval.min(64)
        } else {
            interval
        };
        self.force_refactor
            || (dense_work && self.updates() >= 32 && self.eta_work > self.factor_work / 4)
            || self.updates() >= interval
            || self.entries() > cap
            || (self.m >= 128
                && self.updates() >= 16
                && self.eta_value.len()
                    > 8 * (self.m
                        + self.l.value.len()
                        + self.u.value.len()
                        + self.dense.as_ref().map_or(0, |d| d.data.len() - d.len())))
    }

    fn materialize_dense(&mut self) {
        let Some(tail) = self.dense.take() else {
            return;
        };
        let n = tail.len();
        let prefix = self.m - n;
        self.l.start.truncate(prefix + 1);
        self.u.start.truncate(prefix + 1);
        for k in 0..n {
            for i in k + 1..n {
                let v = tail.data[i * n + k];
                if v != 0. {
                    self.l.index.push(tail.rows[i]);
                    self.l.value.push(v);
                }
            }
            self.l.start.push(self.l.index.len());
            for j in k + 1..n {
                let v = tail.data[k * n + j];
                if v != 0. {
                    self.u.index.push(tail.cols[j]);
                    self.u.value.push(v);
                }
            }
            self.u.start.push(self.u.index.len());
        }
        self.l_rows = self.l.transpose(self.m, &self.prow);
        self.u_cols = self.u.transpose(self.m, &self.prow);
    }
    fn ftran_dense(&mut self, v: &mut [f64]) {
        let _profile = crate::profile::enter("ftran_dense");
        let tail = self.dense.as_mut().unwrap();
        let prefix = self.m - tail.len();
        for k in 0..prefix {
            let t = v[self.prow[k]];
            if t != 0. {
                let (ix, val) = self.l.get(k);
                for (&i, &a) in ix.iter().zip(val) {
                    v[i] -= a * t;
                }
            }
        }
        tail.ftran(v, &mut self.work);
        for &p in tail.cols.iter().rev() {
            let t = self.work[p];
            if t != 0. {
                let (ix, val) = self.u_cols.get(p);
                for (&i, &a) in ix.iter().zip(val) {
                    v[i] -= a * t;
                }
            }
        }
        for k in (0..prefix).rev() {
            let p = self.pcol[k];
            let rhs = v[self.prow[k]];
            let t = if rhs == 0. {
                0.
            } else {
                rhs * self.diag_inv[k]
            };
            self.work[p] = t;
            if t != 0. {
                let (ix, val) = self.u_cols.get(p);
                for (&i, &a) in ix.iter().zip(val) {
                    v[i] -= a * t;
                }
            }
        }
        v.copy_from_slice(&self.work);
    }
    fn btran_dense(&mut self, v: &mut [f64]) {
        let _profile = crate::profile::enter("btran_dense");
        let tail = self.dense.as_mut().unwrap();
        let prefix = self.m - tail.len();
        for k in 0..prefix {
            let p = self.pcol[k];
            let t = v[p] * self.diag_inv[k];
            self.work[self.prow[k]] = t;
            if t != 0. {
                let (ix, val) = self.u.get(k);
                for (&q, &a) in ix.iter().zip(val) {
                    v[q] -= a * t;
                }
            }
        }
        tail.btran(v, &mut self.work);
        for &i in tail.rows.iter().rev() {
            let t = self.work[i];
            if t != 0. {
                let (ix, val) = self.l_rows.get(i);
                for (&r, &a) in ix.iter().zip(val) {
                    self.work[r] -= a * t;
                }
            }
        }
        for k in (0..prefix).rev() {
            let i = self.prow[k];
            let t = self.work[i];
            if t != 0. {
                let (ix, val) = self.l_rows.get(i);
                for (&r, &a) in ix.iter().zip(val) {
                    self.work[r] -= a * t;
                }
            }
        }
        self.sparse_btran = false;
        v.copy_from_slice(&self.work);
    }

    /// Solves `B x = v` in place: `v` is indexed by row on entry and by
    /// basis position on return.
    pub fn ftran(&mut self, v: &mut [f64]) {
        self.ftran_impl(v, false);
    }
    pub fn ftran_entering(&mut self, v: &mut [f64]) {
        self.ftran_impl(v, true);
    }
    pub fn ftran_entering_indexed(&mut self, v: &mut [f64], indices: &mut Vec<usize>) {
        self.ftran_entering(v);
        let _timer = crate::profile::enter("ftran_support_export");
        indices.clear();
        // FT already records exactly the nonzero final positions. Without
        // trailing conventional etas there is no support union to maintain.
        if self.dense.is_none() && self.etas.is_empty() {
            if let Some(ft) = &self.ft {
                indices.extend_from_slice(ft.column_support());
                return;
            }
        }
        if self.result_sparse {
            indices.extend(self.result_index.iter().copied().filter(|&i| v[i] != 0.));
        } else {
            indices.extend(
                v.iter()
                    .enumerate()
                    .filter_map(|(i, &x)| (x != 0.).then_some(i)),
            );
        }
    }
    fn ftran_impl(&mut self, v: &mut [f64], capture: bool) {
        let _triangular = crate::profile::enter("ftran_triangular");
        let _support = crate::profile::enter("ftran_support");
        for &i in &self.result_index {
            self.result_marked[i] = false;
        }
        self.result_index.clear();
        self.result_sparse = false;
        drop(_support);
        if self.dense.is_some() {
            self.work_sparse = false;
            self.ftran_dense(v);
        } else {
            let _lower = crate::profile::enter("ftran_lower");
            if !self.l.value.is_empty() {
                if self
                    .reach
                    .find(v, &self.l, Some(&self.row_step), &self.row_step, 0)
                {
                    for &i in &self.reach.nodes {
                        let t = v[i];
                        if t != 0. {
                            let (index, value) = self.l.get(self.row_step[i]);
                            for (&r, &l) in index.iter().zip(value) {
                                v[r] -= l * t;
                            }
                        }
                    }
                } else {
                    for k in 0..self.m {
                        let t = v[self.prow[k]];
                        if t != 0.0 {
                            let (index, value) = self.l.get(k);
                            for (&i, &l) in index.iter().zip(value) {
                                v[i] -= l * t;
                            }
                        }
                    }
                }
            }
            drop(_lower);
            let w = &mut self.work;
            if let Some(ft) = &mut self.ft {
                self.work_sparse = false;
                ft.ftran(v, w, capture);
                if capture && !self.etas.is_empty() {
                    let _timer = crate::profile::enter("ftran_support");
                    self.result_sparse = true;
                    self.result_index.extend_from_slice(ft.column_support());
                    for &p in &self.result_index {
                        self.result_marked[p] = true;
                    }
                }
            } else {
                if self
                    .reach
                    .find(v, &self.u_cols, Some(&self.row_position), &self.row_step, 1)
                {
                    let _clear = crate::profile::enter("ftran_workspace_clear");
                    if self.work_sparse {
                        for &p in &self.work_index {
                            w[p] = 0.;
                        }
                    } else {
                        w.fill(0.);
                    }
                    self.work_index.clear();
                    drop(_clear);
                    let _upper = crate::profile::enter("ftran_upper");
                    for &i in self.reach.nodes.iter().rev() {
                        let k = self.row_step[i];
                        let p = self.pcol[k];
                        let rhs = v[i];
                        let t = if rhs == 0. {
                            0.
                        } else {
                            rhs * self.diag_inv[k]
                        };
                        w[p] = t;
                        self.work_index.push(p);
                        if t != 0. {
                            let (index, value) = self.u_cols.get(p);
                            for (&r, &u) in index.iter().zip(value) {
                                v[r] -= u * t;
                            }
                        }
                    }
                    drop(_upper);
                    // Reach covers every old RHS entry and every triangular
                    // write. Clear row identities before scattering positions.
                    let _output = crate::profile::enter("ftran_output");
                    for &i in &self.reach.nodes {
                        v[i] = 0.;
                    }
                    for &p in &self.work_index {
                        v[p] = w[p];
                    }
                    drop(_output);
                    self.work_sparse = true;
                    if capture {
                        self.result_sparse = true;
                        self.result_index.extend_from_slice(&self.work_index);
                        for &p in &self.result_index {
                            self.result_marked[p] = true;
                        }
                    }
                } else {
                    self.work_sparse = false;
                    let _upper = crate::profile::enter("ftran_upper");
                    for k in (0..self.m).rev() {
                        let p = self.pcol[k];
                        let rhs = v[self.prow[k]];
                        let t = if rhs == 0. {
                            0.
                        } else {
                            rhs * self.diag_inv[k]
                        };
                        w[p] = t;
                        if t != 0.0 {
                            let (index, value) = self.u_cols.get(p);
                            for (&i, &u) in index.iter().zip(value) {
                                v[i] -= u * t;
                            }
                        }
                    }
                }
                if !self.work_sparse {
                    let _timer = crate::profile::enter("ftran_output");
                    v.copy_from_slice(w);
                }
            }
        }
        drop(_triangular);
        let _eta = crate::profile::enter("ftran_eta");
        for eta in &self.etas {
            let t = v[eta.position] / eta.pivot;
            v[eta.position] = t;
            if t != 0.0 {
                if capture && self.result_sparse {
                    for &i in &self.eta_index[eta.start..eta.end] {
                        if !self.result_marked[i] {
                            self.result_marked[i] = true;
                            self.result_index.push(i);
                        }
                    }
                }
                self.eta_work = self
                    .eta_work
                    .saturating_add((eta.end - eta.start + 1) as u64);
                let range = eta.start..eta.end;
                for (&i, &a) in self.eta_index[range.clone()]
                    .iter()
                    .zip(&self.eta_value[range])
                {
                    v[i] -= a * t;
                }
            }
        }
    }

    /// Solves `Bᵀ y = v` in place: `v` is indexed by basis position on entry
    /// and by row on return.
    pub fn btran(&mut self, v: &mut [f64]) {
        self.btran_masked(v, u64::MAX, false);
    }

    /// The caller supplies a unit vector at `position`.
    pub fn btran_unit(&mut self, position: usize, v: &mut [f64]) {
        for &i in &self.eta_support {
            self.eta_marked[i] = false;
        }
        self.eta_support.clear();
        self.eta_support.push(position);
        self.eta_marked[position] = true;
        self.btran_masked(v, 1u64 << (position & 63), true);
    }

    pub fn btran_unit_indexed(&mut self, position: usize, v: &mut [f64], indices: &mut Vec<usize>) {
        self.btran_unit(position, v);
        indices.clear();
        if self.sparse_btran {
            indices.extend(self.reach.nodes.iter().copied().filter(|&i| v[i] != 0.));
            indices.sort_unstable();
        } else {
            indices.extend(
                v.iter()
                    .enumerate()
                    .filter_map(|(i, &x)| (x != 0.).then_some(i)),
            );
        }
    }

    fn btran_masked(&mut self, v: &mut [f64], mut support: u64, unit: bool) {
        // BTRAN uses row identities and may expand support through L.
        self.work_sparse = false;
        let _eta = crate::profile::enter("btran_eta");
        for eta in self.etas.iter().rev() {
            if support & eta.support == 0 {
                continue;
            }
            self.eta_work = self
                .eta_work
                .saturating_add((eta.end - eta.start + 1) as u64);
            let range = eta.start..eta.end;
            let mut s = v[eta.position];
            for (&i, &a) in self.eta_index[range.clone()]
                .iter()
                .zip(&self.eta_value[range])
            {
                s -= a * v[i];
            }
            v[eta.position] = s / eta.pivot;
            if unit && v[eta.position] != 0. {
                support |= 1u64 << (eta.position & 63);
                if !self.eta_marked[eta.position] {
                    self.eta_marked[eta.position] = true;
                    self.eta_support.push(eta.position);
                }
            }
        }
        drop(_eta);
        let _triangular = crate::profile::enter("btran_triangular");
        if self.dense.is_some() {
            self.btran_dense(v);
            return;
        }
        let w = &mut self.work;
        if let Some(ft) = &mut self.ft {
            ft.btran(
                v,
                w,
                (unit && self.etas.is_empty()).then(|| self.eta_support[0]),
            );
        } else if self.reach.find_seeded(
            v,
            &self.u,
            Some(&self.col_step),
            &self.col_step,
            2,
            unit.then_some(&self.eta_support),
        ) {
            w.fill(0.);
            for &p in &self.reach.nodes {
                let k = self.col_step[p];
                let t = v[p] * self.diag_inv[k];
                w[self.prow[k]] = t;
                if t != 0. {
                    let (index, value) = self.u.get(k);
                    for (&q, &u) in index.iter().zip(value) {
                        v[q] -= u * t;
                    }
                }
            }
        } else {
            for k in 0..self.m {
                let t = v[self.pcol[k]] * self.diag_inv[k];
                w[self.prow[k]] = t;
                if t != 0.0 {
                    let (index, value) = self.u.get(k);
                    for (&q, &u) in index.iter().zip(value) {
                        v[q] -= u * t;
                    }
                }
            }
        }
        self.sparse_btran = self.reach.find_seeded(
            w,
            &self.l_rows,
            None,
            &self.row_step,
            3,
            self.ft.as_ref().and_then(|f| f.row_support()),
        );
        if self.sparse_btran {
            for &i in self.reach.nodes.iter().rev() {
                let t = w[i];
                if t != 0. {
                    let (index, value) = self.l_rows.get(i);
                    for (&r, &l) in index.iter().zip(value) {
                        w[r] -= l * t;
                    }
                }
            }
        } else {
            for k in (0..self.m).rev() {
                let i = self.prow[k];
                let t = w[i];
                if t != 0.0 {
                    let (index, value) = self.l_rows.get(i);
                    for (&r, &l) in index.iter().zip(value) {
                        w[r] -= l * t;
                    }
                }
            }
        }
        v.copy_from_slice(w);
    }

    /// Records the replacement of the column at `position` by a column whose
    /// representation in the current basis is `alpha` (`B⁻¹ a`, by position).
    #[cfg(test)]
    pub fn update(&mut self, position: usize, alpha: &[f64]) {
        self.update_sparse(position, alpha, 0..alpha.len());
    }
    pub fn update_sparse(
        &mut self,
        position: usize,
        alpha: &[f64],
        indices: impl ExactSizeIterator<Item = usize>,
    ) -> bool {
        let _profile = crate::profile::enter("basis_update");
        self.update_nnz += indices.len();
        if let Some(ft) = &mut self.ft {
            if !self.force_refactor && ft.update(position, alpha[position]) {
                return true;
            }
            self.force_refactor = true;
        }
        let start = self.eta_index.len();
        let mut support = 1u64 << (position & 63);
        for i in indices {
            let v = alpha[i];
            if i != position && v != 0.0 {
                support |= 1u64 << (i & 63);
                self.eta_index.push(i);
                self.eta_value.push(v);
            }
        }
        self.etas.push(Eta {
            support,
            position,
            pivot: alpha[position],
            start,
            end: self.eta_index.len(),
        });
        !self.force_refactor
    }
}

/// Markowitz search over columns and rows by increasing active count. Stops
/// at a zero-cost pivot, after the configured candidate budget once one is acceptable, or
/// when no remaining count class can beat the best cost found.
fn find_pivot(
    a: &mut Active,
    colb: &Buckets,
    rowb: &Buckets,
    m: usize,
    search: usize,
    rank: Option<&[usize]>,
) -> Option<(usize, usize)> {
    struct Best {
        at: Option<(usize, usize)>,
        cost: usize,
        value: f64,
        rank: usize,
    }
    impl Best {
        fn consider(&mut self, i: usize, p: usize, v: f64, max: f64, cost: usize, rank: usize) {
            let a = v.abs();
            if a > ABS_PIVOT
                && a >= THRESHOLD * max
                && (cost < self.cost
                    || (cost == self.cost
                        && (a > self.value || (a == self.value && rank < self.rank))))
            {
                *self = Self {
                    at: Some((i, p)),
                    cost,
                    value: a,
                    rank,
                };
            }
        }
        fn done(&self, searched: usize, search: usize) -> bool {
            self.at.is_some() && (self.cost == 0 || searched >= search)
        }
    }
    let mut best = Best {
        at: None,
        cost: usize::MAX,
        value: 0.0,
        rank: usize::MAX,
    };
    let mut searched = 0;
    for count in 1..=m {
        let mut p = colb.head[count];
        while p != NIL {
            let max = a.column_max(p);
            for e in &a.cols[p] {
                best.consider(
                    e.row as usize,
                    p,
                    e.value,
                    max,
                    (rowb.count[e.row as usize] - 1) * (count - 1),
                    rank.map_or(usize::MAX, |r| r[p]),
                );
            }
            searched += 1;
            if best.done(searched, search) {
                return best.at;
            }
            p = colb.next[p];
        }
        let mut i = rowb.head[count];
        while i != NIL {
            for slot in 0..a.rows[i].len() {
                let (p, k) = a.rows[i][slot];
                let (p, k) = (p as usize, k as usize);
                let max = a.column_max(p);
                best.consider(
                    i,
                    p,
                    a.cols[p][k].value,
                    max,
                    (count - 1) * (colb.count[p] - 1),
                    rank.map_or(usize::MAX, |r| r[p]),
                );
            }
            searched += 1;
            if best.done(searched, search) {
                return best.at;
            }
            i = rowb.next[i];
        }
        // Every remaining candidate has row and column counts above `count`.
        if best.at.is_some() && best.cost <= count * count {
            return best.at;
        }
    }
    best.at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factor(columns: &[Vec<(usize, f64)>], cap: usize) -> Result<Lu, FactorError> {
        Lu::factor(
            columns.len(),
            |p, out| out.extend_from_slice(&columns[p]),
            cap,
        )
    }

    fn residual(columns: &[Vec<(usize, f64)>], lu: &mut Lu) -> f64 {
        let m = columns.len();
        let rhs: Vec<f64> = (0..m).map(|i| (i % 7) as f64 - 3.0).collect();
        let mut worst = 0.0_f64;
        for transpose in [false, true] {
            let mut x = rhs.clone();
            if transpose {
                lu.btran(&mut x);
            } else {
                lu.ftran(&mut x);
            }
            let mut back = vec![0.0; m];
            for (p, col) in columns.iter().enumerate() {
                for &(i, a) in col {
                    if transpose {
                        back[p] += a * x[i];
                    } else {
                        back[i] += a * x[p];
                    }
                }
            }
            for (b, r) in back.iter().zip(&rhs) {
                worst = worst.max((b - r).abs());
            }
        }
        worst
    }

    fn random_columns(m: usize, seed: u64, density: usize) -> Vec<Vec<(usize, f64)>> {
        let mut s = seed;
        let mut next = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 33) as usize
        };
        (0..m)
            .map(|p| {
                let mut col = vec![((p * 7 + 3) % m, 4.0 + (next() % 5) as f64)];
                for _ in 0..density {
                    let i = next() % m;
                    if col.iter().all(|&(r, _)| r != i) {
                        col.push((i, (next() % 9) as f64 - 4.0));
                    }
                }
                col
            })
            .collect()
    }

    #[test]
    fn indexed_forward_support_survives_cancellation_updates_and_btran_reuse() {
        let m = 256;
        let mut cols: Vec<Vec<(usize, f64)>> = (0..m)
            .map(|p| vec![(p, 2.), ((p / 4 * 4 + (p + 1) % 4), 0.125)])
            .collect();
        let mut lu = factor(&cols, 100000).unwrap();
        let mut sparse_calls = 0;
        for step in 0..80 {
            let mut rhs = vec![0.; m];
            if step % 5 == 0 {
                for (i, v) in rhs.iter_mut().enumerate() {
                    *v = (i % 7) as f64 - 3.;
                }
            } else {
                rhs[(step * 7) % m] = 1.;
                rhs[(step * 7 + 1) % m] = -0.125;
            }
            let original = rhs.clone();
            let mut indices = Vec::new();
            lu.ftran_entering_indexed(&mut rhs, &mut indices);
            sparse_calls += usize::from(lu.result_sparse);
            let mut seen = vec![false; m];
            for &i in &indices {
                assert!(!seen[i]);
                seen[i] = true;
            }
            assert!((0..m).all(|i| rhs[i] == 0. || seen[i]));
            let mut back = vec![0.; m];
            for (p, col) in cols.iter().enumerate() {
                for &(i, a) in col {
                    back[i] += a * rhs[p];
                }
            }
            assert!(
                back.iter()
                    .zip(&original)
                    .all(|(a, b)| (a - b).abs() < 1e-9)
            );
            if step % 3 == 0 {
                let mut y = original.clone();
                lu.btran(&mut y);
            }
            if step % 7 == 0 {
                let p = step % m;
                let new = vec![(p, 2.5), ((p / 4 * 4 + (p + 1) % 4), 0.1)];
                let mut alpha = vec![0.; m];
                for &(i, a) in &new {
                    alpha[i] = a;
                }
                lu.ftran_entering_indexed(&mut alpha, &mut indices);
                lu.update_sparse(p, &alpha, indices.into_iter());
                cols[p] = new;
            }
        }
        assert!(sparse_calls > 10, "sparse branch must be exercised");
    }

    #[test]
    fn ft_indexed_output_handles_zero_rhs_cancellation_and_trailing_eta() {
        let m = 256;
        let mut columns: Vec<_> = (0..m).map(|p| vec![((p * 7 + 3) % m, 2.)]).collect();
        let mut lu = factor(&columns, usize::MAX).unwrap();
        lu.enable_ft();
        let mut indices = Vec::new();
        for step in 0..16 {
            // Contaminate the shared workspace in the other orientation.
            let mut transpose = vec![1.; m];
            lu.btran(&mut transpose);
            let expected: Vec<_> = (0..m)
                .map(|j| {
                    if step % 4 == 0 {
                        0.
                    } else if step % 4 == 1 {
                        (j % 7) as f64 - 3.
                    } else if j == step || j == step + 1 {
                        1.
                    } else {
                        0.
                    }
                })
                .collect();
            let mut rhs = vec![0.; m];
            for (p, col) in columns.iter().enumerate() {
                for &(i, a) in col {
                    rhs[i] += a * expected[p];
                }
            }
            lu.ftran_entering_indexed(&mut rhs, &mut indices);
            assert_eq!(indices.len(), rhs.iter().filter(|&&v| v != 0.).count());
            assert!(indices.iter().all(|&i| rhs[i] != 0.));
            assert!(
                rhs.iter()
                    .zip(&expected)
                    .all(|(a, b)| (a - b).abs() < 1e-10)
            );
            if step == 4 || step == 8 {
                let p = step;
                let new = vec![((p * 7 + 3) % m, 3.), (((p + 1) * 7 + 3) % m, -0.5)];
                let mut alpha = vec![0.; m];
                for &(i, a) in &new {
                    alpha[i] = a;
                }
                lu.ftran_entering_indexed(&mut alpha, &mut indices);
                if step == 8 {
                    lu.force_refactor = true;
                }
                let accepted = lu.update_sparse(p, &alpha, indices.iter().copied());
                assert_eq!(accepted, step == 4);
                columns[p] = new;
            }
        }
        assert!(!lu.etas.is_empty());
    }

    #[test]
    fn ft_accepts_general_sparse_lower_factor_without_materializing_dense_tails() {
        let m = 256;
        let columns: Vec<Vec<(usize, f64)>> = (0..m)
            .map(|p| {
                let mut c = vec![(p, 4.)];
                for d in 1..=2 {
                    c.push(((p + d) % m, 0.125));
                }
                c
            })
            .collect();
        let mut lu = factor(&columns, 100000).unwrap();
        assert!(lu.l.value.len() > m / 8);
        assert!(lu.ft_suitable());
        lu.enable_ft();
        assert!(residual(&columns, &mut lu) < 1e-10);
        let dense = random_columns(160, 341, 100);
        let lu = factor(&dense, 100000).unwrap();
        assert!(lu.dense.is_some());
        assert!(!lu.ft_suitable());
    }

    #[test]
    fn numerical_pivot_replay_checks_identity_changes_singularity_and_caps() {
        let mut ws = FactorWorkspace::default();
        let m = 128;
        let mut cols: Vec<Vec<(usize, f64)>> = (0..m)
            .map(|p| vec![(p, 4.), ((p + 1) % m, 0.125)])
            .collect();
        let options = FactorOptions {
            replay: true,
            ..Default::default()
        };
        let first = Lu::factor_with_workspace(
            m,
            |p, out| out.extend_from_slice(&cols[p]),
            100000,
            &mut ws,
            options,
        )
        .unwrap();
        assert_eq!(first.replayed_pivots(), 0);
        let mut next = Lu::factor_with_workspace(
            m,
            |p, out| out.extend_from_slice(&cols[p]),
            100000,
            &mut ws,
            options,
        )
        .unwrap();
        assert!(next.replayed_pivots() > 100);
        assert!(residual(&cols, &mut next) < 1e-10);
        cols[0][0].1 = 4.25;
        let mut changed = Lu::factor_with_workspace(
            m,
            |p, out| out.extend_from_slice(&cols[p]),
            100000,
            &mut ws,
            options,
        )
        .unwrap();
        assert_eq!(changed.replayed_pivots(), 0);
        assert!(residual(&cols, &mut changed) < 1e-10);
        cols[0].clear();
        assert!(matches!(
            Lu::factor_with_workspace(
                m,
                |p, out| out.extend_from_slice(&cols[p]),
                100000,
                &mut ws,
                options
            ),
            Err(FactorError::Singular { .. })
        ));
        assert!(matches!(
            Lu::factor_with_workspace(
                m,
                |p, out| out.extend_from_slice(&cols[p]),
                1,
                &mut ws,
                options
            ),
            Err(FactorError::TooLarge)
        ));
    }

    #[test]
    fn blocked_tail_singularity_and_tight_cap_use_valid_legacy_fallback() {
        let m = 97;
        let mut singular: Vec<Vec<_>> = (0..m)
            .map(|p| {
                (0..m)
                    .map(|i| (i, if i == p { 4. } else { 0.01 }))
                    .collect()
            })
            .collect();
        singular[71] = singular[7].clone();
        match factor(&singular, usize::MAX) {
            Err(FactorError::Singular { positions, rows }) => {
                assert_eq!(positions.len(), 1);
                assert_eq!(rows.len(), 1);
            }
            _ => panic!("dependent dense columns must be detected"),
        }
        let m = 96;
        let block: Vec<Vec<_>> = (0..m)
            .map(|p| {
                (0..m)
                    .filter(|&i| i / 48 == p / 48)
                    .map(|i| (i, if i == p { 4. } else { 0.01 }))
                    .collect()
            })
            .collect();
        let mut lu = factor(&block, 5000).unwrap();
        assert!(lu.dense.is_none());
        assert!(lu.entries() <= 5000);
        assert!(residual(&block, &mut lu) < 1e-10);
    }

    #[test]
    fn retained_dense_tail_and_updates_match_full_basis() {
        for m in [64, 97, 257] {
            let mut columns: Vec<Vec<_>> = (0..m)
                .map(|p| {
                    (0..m)
                        .map(|r| {
                            let row = (r * 7 + 3) % m;
                            (
                                row,
                                if r == p {
                                    5.
                                } else {
                                    ((r + p) % 11) as f64 * 0.002 - 0.01
                                },
                            )
                        })
                        .collect()
                })
                .collect();
            for ft in [false, true] {
                let mut lu = factor(&columns, usize::MAX).unwrap();
                assert!(lu.dense.is_some());
                assert!(residual(&columns, &mut lu) < 1e-10);
                if ft {
                    lu.enable_ft();
                    assert!(lu.dense.is_none());
                }
                for step in 0..5 {
                    let p = step * 11 % m;
                    for (_, v) in &mut columns[p] {
                        *v *= 1.01;
                    }
                    let mut a = vec![0.; m];
                    for &(i, v) in &columns[p] {
                        a[i] = v;
                    }
                    lu.ftran_entering(&mut a);
                    assert!(lu.update_sparse(p, &a, 0..m));
                    assert!(residual(&columns, &mut lu) < 1e-10);
                }
            }
        }
    }

    #[test]
    fn reused_workspace_preserves_old_factors_across_dimensions() {
        let mut workspace = FactorWorkspace::default();
        for m in [17, 128, 5, 256, 17] {
            let columns = random_columns(m, 3, 2);
            let mut old = Lu::factor_with_workspace(
                m,
                |p, out| out.extend_from_slice(&columns[p]),
                usize::MAX,
                &mut workspace,
                FactorOptions::default(),
            )
            .unwrap();
            let other: Vec<_> = (0..7).map(|i| vec![(i, 1.)]).collect();
            let _new = Lu::factor_with_workspace(
                7,
                |p, out| out.extend_from_slice(&other[p]),
                usize::MAX,
                &mut workspace,
                FactorOptions::default(),
            )
            .unwrap();
            assert!(residual(&columns, &mut old) < 1e-9);
        }
    }

    #[test]
    fn sparse_unit_solves_with_permutations_and_updates() {
        let m = 512;
        let mut cols: Vec<Vec<(usize, f64)>> = (0..m)
            .map(|p| {
                vec![
                    ((p * 7) % m, 4.),
                    (((p / 4 * 4 + (p + 1) % 4) * 7) % m, 0.25),
                ]
            })
            .collect();
        let mut lu = factor(&cols, usize::MAX).unwrap();
        for step in 0..3 {
            for transpose in [false, true] {
                for r in [0, 1, 7, 255, 511] {
                    let mut x = vec![0.; m];
                    x[r] = 1.;
                    if transpose {
                        lu.btran_unit(r, &mut x);
                    } else {
                        lu.ftran(&mut x);
                    }
                    let mut back = vec![0.; m];
                    for (p, col) in cols.iter().enumerate() {
                        for &(i, a) in col {
                            if transpose {
                                back[p] += a * x[i];
                            } else {
                                back[i] += a * x[p];
                            }
                        }
                    }
                    for (i, &v) in back.iter().enumerate() {
                        assert!((v - if i == r { 1. } else { 0. }).abs() < 1e-12);
                    }
                }
            }
            let p = step * 7;
            cols[p][0].1 += 1.;
            let mut alpha = vec![0.; m];
            for &(i, v) in &cols[p] {
                alpha[i] = v;
            }
            lu.ftran(&mut alpha);
            lu.update(p, &alpha);
        }
    }

    #[test]
    fn work_rebuild_trigger_respects_minimum_spacing_and_hard_caps() {
        let m = 2048;
        let columns: Vec<Vec<_>> = (0..m)
            .map(|p| {
                if p < 256 {
                    (0..256)
                        .map(|i| (i, if i == p { 4. } else { 0.001 }))
                        .collect()
                } else {
                    vec![(p, 1.)]
                }
            })
            .collect();
        let mut lu = factor(&columns, usize::MAX).unwrap();
        assert!(lu.dense.is_some());
        for p in 0..31 {
            let mut a = vec![0.; m];
            a[p] = 1.;
            lu.update(p, &a);
        }
        lu.eta_work = u64::MAX;
        assert!(!lu.needs_refactor(256, usize::MAX, true));
        let mut a = vec![0.; m];
        a[31] = 1.;
        lu.update(31, &a);
        assert!(lu.needs_refactor(256, usize::MAX, true));
        assert!(!lu.needs_refactor(256, usize::MAX, false));
        lu.eta_work = 0;
        assert!(lu.needs_refactor(32, usize::MAX, true));
        assert!(lu.needs_refactor(256, lu.entries() - 1, true));
    }

    #[test]
    fn update_fill_trigger_preserves_interval_and_entry_caps() {
        let columns: Vec<_> = (0..128).map(|i| vec![(i, 1.)]).collect();
        let mut lu = factor(&columns, usize::MAX).unwrap();
        let alpha = vec![1.; 128];
        for p in 0..15 {
            lu.update(p, &alpha);
        }
        assert!(!lu.needs_refactor(64, usize::MAX, true));
        assert!(lu.needs_refactor(1, usize::MAX, true));
        assert!(lu.needs_refactor(64, 128, true));
        lu.update(15, &alpha);
        assert!(lu.needs_refactor(64, usize::MAX, true));
    }

    #[test]
    fn factor_and_solve_random_sparse_bases() {
        let mut factored = 0;
        for m in [1, 2, 5, 17, 60, 200, 700] {
            for seed in 0..5 {
                let columns = random_columns(m, seed, 3);
                let Ok(mut lu) = factor(&columns, usize::MAX) else {
                    continue; // a random pattern may be singular
                };
                factored += 1;
                assert!(residual(&columns, &mut lu) < 1e-8, "m={m} seed={seed}");
            }
        }
        assert!(factored >= 25);
    }

    #[test]
    fn eta_updates_match_the_replaced_basis() {
        let m = 40;
        let mut columns = random_columns(m, 3, 2);
        let mut lu = factor(&columns, usize::MAX).unwrap();
        let mut updates = 0;
        for step in 0..15 {
            let p = (step * 11) % m;
            let new = vec![(p, 3.0), ((p + 5) % m, -1.0), ((p + 9) % m, 0.5)];
            let mut alpha = vec![0.0; m];
            for &(i, v) in &new {
                alpha[i] = v;
            }
            lu.ftran(&mut alpha);
            if alpha[p].abs() < 1e-6 {
                continue;
            }
            lu.update(p, &alpha);
            columns[p] = new;
            updates += 1;
            assert!(residual(&columns, &mut lu) < 1e-8, "step {step}");
        }
        assert_eq!(lu.updates(), updates);
    }

    #[test]
    fn forrest_tomlin_updates_match_rebuilt_bases() {
        for m in [2, 17, 64, 256] {
            let mut columns = random_columns(m, 11, 2);
            let mut lu = factor(&columns, usize::MAX).unwrap();
            lu.enable_ft();
            for step in 0..80 {
                let p = (step * 11 + 1) % m;
                let mut new = columns[p].clone();
                for (_, v) in &mut new {
                    *v *= 1.01;
                }
                let row = (step * 13 + 3) % m;
                if let Some((_, v)) = new.iter_mut().find(|(i, _)| *i == row) {
                    *v += 0.001;
                } else {
                    new.push((row, 0.001));
                }
                // Exercise cached unit BTRAN and the restoration path without it.
                if step % 2 == 0 {
                    let mut unit = vec![0.; m];
                    unit[p] = 1.;
                    lu.btran_unit(p, &mut unit);
                }
                let mut alpha = vec![0.; m];
                for &(i, v) in &new {
                    alpha[i] = v;
                }
                lu.ftran_entering(&mut alpha);
                // An intervening edge-weight FTRAN must not overwrite the spike.
                let mut other = vec![1.; m];
                lu.ftran(&mut other);
                assert!(lu.update_sparse(p, &alpha, 0..m), "m={m} step={step}");
                columns[p] = new;
                assert!(residual(&columns, &mut lu) < 1e-8, "m={m} step={step}");
            }
            assert_eq!(lu.updates(), 80);
        }
    }

    #[test]
    fn ft_replacements_move_spikes_across_permuted_bases() {
        for m in [17, 128, 257] {
            for seed in 0..6 {
                let mut columns: Vec<_> = (0..m).map(|p| vec![((p * 7 + 3) % m, 4.)]).collect();
                let mut lu = factor(&columns, usize::MAX).unwrap();
                lu.enable_ft();
                for step in 0..100 {
                    let p = (step * 19 + seed * 7) % m;
                    let new = vec![
                        ((p * 7 + 3) % m, 4. + (step % 3) as f64),
                        (((p + 1 + seed) * 7 + 3) % m, -0.75),
                        (((p + 7 + seed) * 7 + 3) % m, 0.5),
                    ];
                    let mut rhs = vec![0.; m];
                    let mut indices = Vec::new();
                    for &(i, v) in &new {
                        rhs[i] = v;
                    }
                    lu.ftran_entering_indexed(&mut rhs, &mut indices);
                    assert_eq!(indices.len(), rhs.iter().filter(|&&v| v != 0.).count());
                    assert!(lu.update_sparse(p, &rhs, indices.iter().copied()));
                    columns[p] = new;
                    assert!(residual(&columns, &mut lu) < 1e-9);
                    // Verify sparse transpose traversal, including canceled entries.
                    let mut unit = vec![0.; m];
                    unit[p] = 1.;
                    lu.btran_unit_indexed(p, &mut unit, &mut indices);
                    assert_eq!(indices.len(), unit.iter().filter(|&&v| v != 0.).count());
                    for (q, col) in columns.iter().enumerate() {
                        let back: f64 = col.iter().map(|&(i, v)| v * unit[i]).sum();
                        assert!((back - f64::from(q == p)).abs() < 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn rejected_ft_update_keeps_valid_eta_fallback() {
        let mut columns = vec![vec![(0, 1.)], vec![(1, 1.)]];
        let mut lu = factor(&columns, usize::MAX).unwrap();
        lu.enable_ft();
        columns[0] = vec![(0, 1e-12), (1, 1.)];
        let mut a = vec![1e-12, 1.];
        lu.ftran_entering(&mut a);
        assert!(!lu.update_sparse(0, &a, 0..2));
        assert!(lu.needs_refactor(64, usize::MAX, true));
        // Known exact RHS avoids an ill-conditioning-dependent residual threshold.
        let mut rhs = vec![1e-12, 2.];
        lu.ftran(&mut rhs);
        assert_eq!(rhs, vec![1., 1.]);
        let mut rhs = vec![1e-12 + 1., 1.];
        lu.btran(&mut rhs);
        assert!((rhs[0] - 1.).abs() < 1e-3);
        assert_eq!(rhs[1], 1.);
    }

    #[test]
    fn reciprocal_diagonals_preserve_scaled_forward_and_transpose_solves() {
        let m = 192;
        let mut columns = random_columns(m, 2371, 2);
        for (p, col) in columns.iter_mut().enumerate() {
            for (r, value) in col {
                *value *= 2f64.powi((*r % 13) as i32 - 6) * 2f64.powi((p % 11) as i32 - 5);
            }
        }
        for ft in [false, true] {
            let mut lu = factor(&columns, usize::MAX).unwrap();
            if ft {
                lu.enable_ft();
            }
            for sparse in [true, false, true, false] {
                let rhs: Vec<f64> = (0..m)
                    .map(|i| {
                        if sparse && i % 31 != 0 {
                            0.
                        } else {
                            (i % 7) as f64 - 3.
                        }
                    })
                    .collect();
                for transpose in [false, true] {
                    let mut x = rhs.clone();
                    if transpose {
                        lu.btran(&mut x);
                    } else {
                        lu.ftran(&mut x);
                    }
                    let mut back = vec![0.; m];
                    let mut scale = rhs.iter().map(|v| v.abs()).collect::<Vec<_>>();
                    for (p, col) in columns.iter().enumerate() {
                        for &(r, a) in col {
                            let (i, t) = if transpose {
                                (p, a * x[r])
                            } else {
                                (r, a * x[p])
                            };
                            back[i] += t;
                            scale[i] += t.abs();
                        }
                    }
                    for i in 0..m {
                        assert!((back[i] - rhs[i]).abs() <= 1e-10 * scale[i].max(1.));
                    }
                }
            }
        }
    }

    #[test]
    fn singular_bases_name_uncovered_rows_and_cap_is_enforced() {
        let columns = vec![vec![(0, 1.0)], vec![(0, 2.0)], vec![(2, 1.0)]];
        match factor(&columns, usize::MAX) {
            Err(FactorError::Singular { positions, rows }) => {
                assert_eq!(positions.len(), 1);
                assert_eq!(rows, vec![1]);
            }
            other => panic!("{:?}", other.err()),
        }
        let dense: Vec<Vec<(usize, f64)>> = (0..20)
            .map(|p| {
                (0..20)
                    .map(|i| (i, if i == p { 5.0 } else { 1.0 }))
                    .collect()
            })
            .collect();
        assert!(matches!(factor(&dense, 50), Err(FactorError::TooLarge)));
        let mut lu = factor(&dense, usize::MAX).unwrap();
        assert!(residual(&dense, &mut lu) < 1e-9);
        // A dependent pair inside the dense phase.
        let mut dependent = dense.clone();
        dependent[7] = dependent[3].clone();
        match factor(&dependent, usize::MAX) {
            Err(FactorError::Singular { positions, rows }) => {
                assert_eq!((positions.len(), rows.len()), (1, 1));
            }
            other => panic!("{:?}", other.err()),
        }
    }

    #[test]
    fn exact_cancellation_keeps_lists_consistent() {
        // The second pivot cancels the (2, 2) entry exactly.
        let columns = vec![
            vec![(0, 1.0), (1, 1.0), (2, 1.0)],
            vec![(0, 1.0), (1, 2.0)],
            vec![(0, 1.0), (2, 1.0), (1, 3.0)],
        ];
        let mut lu = factor(&columns, usize::MAX).unwrap();
        assert!(residual(&columns, &mut lu) < 1e-12);
    }
}
