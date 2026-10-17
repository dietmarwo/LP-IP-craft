//! Sparse LU factorization of simplex bases with Markowitz pivot selection
//! and a relative threshold, plus product-form (eta) column replacements.
//!
//! The basis `B` has one column per basis position. Factorization chooses a
//! pivot sequence `(row r_k, position p_k)` and stores, per pivot `k`, the
//! multipliers `L_k` of the rows it eliminates and the remaining pivot row
//! `U_k` over positions pivoted later. Values that cancel to exactly zero
//! stay in the active submatrix as structural entries (pivot selection
//! skips them), so row and column lists never need rescanning. Once the
//! active submatrix is dense enough, elimination finishes with a dense
//! partial-pivoting kernel.
//!
//! L and U are kept both by pivot and transposed, so both triangular solves
//! of `ftran` and `btran` push updates from nonzero entries only. Solves use
//! dense work vectors of the basis dimension.

const NIL: usize = usize::MAX;
/// Relative pivot threshold: `|a_rp| >= THRESHOLD * max_i |a_ip|`.
const THRESHOLD: f64 = 0.1;
/// Absolute pivot tolerance on the (scaled) basis entries.
const ABS_PIVOT: f64 = 1e-11;
/// Candidate columns/rows examined after the first acceptable pivot.
const SEARCH: usize = 4;
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
    position: usize,
    pivot: f64,
    start: usize,
    end: usize,
}

/// Compressed lists: `index[start[k]..start[k + 1]]` with matching values.
#[derive(Default)]
struct Lists {
    start: Vec<usize>,
    index: Vec<usize>,
    value: Vec<f64>,
}

impl Lists {
    fn get(&self, k: usize) -> (&[usize], &[f64]) {
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

/// An LU factorization of a basis with an eta file of column replacements.
pub(crate) struct Lu {
    m: usize,
    prow: Vec<usize>,
    pcol: Vec<usize>,
    diag: Vec<f64>,
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
}

impl Lu {
    /// Factorizes the basis whose position `p` has the sparse column written
    /// by `column(p, out)` (entries `(row, value)`, rows `< m`, distinct).
    /// `cap` bounds the stored entries (diagonal, L and U).
    pub fn factor(
        m: usize,
        mut column: impl FnMut(usize, &mut Vec<(usize, f64)>),
        cap: usize,
    ) -> Result<Self, FactorError> {
        let mut cols: Vec<Vec<(usize, f64)>> = vec![Vec::new(); m];
        for (p, col) in cols.iter_mut().enumerate() {
            column(p, col);
            col.retain(|&(_, v)| v != 0.0);
        }
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); m];
        for (p, col) in cols.iter().enumerate() {
            for &(i, _) in col {
                rows[i].push(p);
            }
        }
        let mut colb = Buckets::new(m, m);
        let mut rowb = Buckets::new(m, m);
        let mut active = 0usize;
        for (p, col) in cols.iter().enumerate() {
            colb.insert(p, col.len());
            active += col.len();
        }
        for (i, row) in rows.iter().enumerate() {
            rowb.insert(i, row.len());
        }
        let mut col_done = vec![false; m];
        let mut row_done = vec![false; m];
        // Scatter map from row to slot in the column being updated.
        let mut slot = vec![NIL; m];
        let empty = || Lists {
            start: vec![0],
            ..Lists::default()
        };
        let mut lu = Self {
            m,
            prow: Vec::with_capacity(m),
            pcol: Vec::with_capacity(m),
            diag: Vec::with_capacity(m),
            l: empty(),
            l_rows: Lists::default(),
            u: empty(),
            u_cols: Lists::default(),
            etas: Vec::new(),
            eta_index: Vec::new(),
            eta_value: Vec::new(),
            work: vec![0.0; m],
        };
        let mut pivot_row: Vec<(usize, f64)> = Vec::new();
        let mut pivot_col: Vec<(usize, f64)> = Vec::new();
        let mut singular = false;
        while lu.prow.len() < m {
            let remaining = (m - lu.prow.len()) as f64;
            if active as f64 > DENSE_DENSITY * remaining * remaining {
                break;
            }
            let Some((r, p)) = find_pivot(&cols, &rows, &colb, &rowb, m) else {
                singular = true;
                break;
            };
            // Take row r out of the active columns; keep the U row.
            pivot_row.clear();
            let mut a = 0.0;
            active -= rows[r].len();
            for &q in &rows[r] {
                let k = cols[q]
                    .iter()
                    .position(|&(i, _)| i == r)
                    .expect("row and column lists agree");
                let (_, v) = cols[q].swap_remove(k);
                if q == p {
                    a = v;
                } else {
                    pivot_row.push((q, v));
                }
            }
            // Take column p out of the active rows; keep the multipliers.
            pivot_col.clear();
            active -= cols[p].len();
            for &(i, v) in &cols[p] {
                let k = rows[i]
                    .iter()
                    .position(|&q| q == p)
                    .expect("row and column lists agree");
                rows[i].swap_remove(k);
                pivot_col.push((i, v / a));
            }
            cols[p].clear();
            rows[r].clear();
            colb.remove(p);
            rowb.remove(r);
            col_done[p] = true;
            row_done[r] = true;
            // Schur complement update of every column of the pivot row.
            for &(q, u) in &pivot_row {
                if u == 0.0 {
                    continue;
                }
                for (k, &(i, _)) in cols[q].iter().enumerate() {
                    slot[i] = k;
                }
                for &(i, l) in &pivot_col {
                    let delta = -l * u;
                    if slot[i] != NIL {
                        cols[q][slot[i]].1 += delta;
                    } else {
                        slot[i] = cols[q].len();
                        cols[q].push((i, delta));
                        rows[i].push(q);
                        active += 1;
                    }
                }
                for &(i, _) in &cols[q] {
                    slot[i] = NIL;
                }
                colb.change(q, cols[q].len());
            }
            for &(i, _) in &pivot_col {
                rowb.change(i, rows[i].len());
            }
            lu.push_pivot(r, p, a, &pivot_col, &pivot_row);
            if lu.entries() > cap {
                return Err(FactorError::TooLarge);
            }
        }
        if !singular && lu.prow.len() < m {
            lu.dense_phase(&cols, &mut row_done, &mut col_done, cap)?;
        }
        if lu.prow.len() < m {
            return Err(FactorError::Singular {
                positions: (0..m).filter(|&p| !col_done[p]).collect(),
                rows: (0..m).filter(|&i| !row_done[i]).collect(),
            });
        }
        // L by eliminated row: entries name the pivot row of their step.
        lu.l_rows = lu.l.transpose(m, &lu.prow);
        // U by position: entries name the pivot row of their step.
        lu.u_cols = lu.u.transpose(m, &lu.prow);
        Ok(lu)
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
        cols: &[Vec<(usize, f64)>],
        row_done: &mut [bool],
        col_done: &mut [bool],
        cap: usize,
    ) -> Result<(), FactorError> {
        let m = self.m;
        let rows_left: Vec<usize> = (0..m).filter(|&i| !row_done[i]).collect();
        let cols_left: Vec<usize> = (0..m).filter(|&p| !col_done[p]).collect();
        let k = rows_left.len();
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
            for &(i, v) in &cols[p] {
                d[c * k + dense_row[i]] = v;
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

    /// Stored numerical entries: diagonal, L, U and etas.
    pub fn entries(&self) -> usize {
        self.diag.len()
            + self.l.value.len()
            + self.u.value.len()
            + self.eta_value.len()
            + self.etas.len()
    }

    /// Number of eta updates since factorization.
    pub fn updates(&self) -> usize {
        self.etas.len()
    }

    /// Solves `B x = v` in place: `v` is indexed by row on entry and by
    /// basis position on return.
    pub fn ftran(&mut self, v: &mut [f64]) {
        for k in 0..self.m {
            let t = v[self.prow[k]];
            if t != 0.0 {
                let (index, value) = self.l.get(k);
                for (&i, &l) in index.iter().zip(value) {
                    v[i] -= l * t;
                }
            }
        }
        let w = &mut self.work;
        for k in (0..self.m).rev() {
            let p = self.pcol[k];
            let t = v[self.prow[k]] / self.diag[k];
            w[p] = t;
            if t != 0.0 {
                let (index, value) = self.u_cols.get(p);
                for (&i, &u) in index.iter().zip(value) {
                    v[i] -= u * t;
                }
            }
        }
        v.copy_from_slice(w);
        for eta in &self.etas {
            let t = v[eta.position] / eta.pivot;
            v[eta.position] = t;
            if t != 0.0 {
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
        for eta in self.etas.iter().rev() {
            let range = eta.start..eta.end;
            let mut s = v[eta.position];
            for (&i, &a) in self.eta_index[range.clone()]
                .iter()
                .zip(&self.eta_value[range])
            {
                s -= a * v[i];
            }
            v[eta.position] = s / eta.pivot;
        }
        let w = &mut self.work;
        for k in 0..self.m {
            let t = v[self.pcol[k]] / self.diag[k];
            w[self.prow[k]] = t;
            if t != 0.0 {
                let (index, value) = self.u.get(k);
                for (&q, &u) in index.iter().zip(value) {
                    v[q] -= u * t;
                }
            }
        }
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
        v.copy_from_slice(w);
    }

    /// Records the replacement of the column at `position` by a column whose
    /// representation in the current basis is `alpha` (`B⁻¹ a`, by position).
    pub fn update(&mut self, position: usize, alpha: &[f64]) {
        let start = self.eta_index.len();
        for (i, &v) in alpha.iter().enumerate() {
            if i != position && v != 0.0 {
                self.eta_index.push(i);
                self.eta_value.push(v);
            }
        }
        self.etas.push(Eta {
            position,
            pivot: alpha[position],
            start,
            end: self.eta_index.len(),
        });
    }
}

/// Markowitz search over columns and rows by increasing active count. Stops
/// at a zero-cost pivot, after `SEARCH` candidates once one is acceptable, or
/// when no remaining count class can beat the best cost found.
fn find_pivot(
    cols: &[Vec<(usize, f64)>],
    rows: &[Vec<usize>],
    colb: &Buckets,
    rowb: &Buckets,
    m: usize,
) -> Option<(usize, usize)> {
    struct Best {
        at: Option<(usize, usize)>,
        cost: usize,
        value: f64,
    }
    impl Best {
        fn consider(&mut self, i: usize, p: usize, v: f64, max: f64, cost: usize) {
            let a = v.abs();
            if a > ABS_PIVOT
                && a >= THRESHOLD * max
                && (cost < self.cost || (cost == self.cost && a > self.value))
            {
                *self = Self {
                    at: Some((i, p)),
                    cost,
                    value: a,
                };
            }
        }
        fn done(&self, searched: usize) -> bool {
            self.at.is_some() && (self.cost == 0 || searched >= SEARCH)
        }
    }
    let mut best = Best {
        at: None,
        cost: usize::MAX,
        value: 0.0,
    };
    let mut searched = 0;
    let colmax = |p: usize| cols[p].iter().fold(0.0_f64, |a, &(_, v)| a.max(v.abs()));
    for count in 1..=m {
        let mut p = colb.head[count];
        while p != NIL {
            let max = colmax(p);
            for &(i, v) in &cols[p] {
                best.consider(i, p, v, max, (rowb.count[i] - 1) * (count - 1));
            }
            searched += 1;
            if best.done(searched) {
                return best.at;
            }
            p = colb.next[p];
        }
        let mut i = rowb.head[count];
        while i != NIL {
            for &p in &rows[i] {
                if let Some(&(_, v)) = cols[p].iter().find(|&&(row, _)| row == i) {
                    best.consider(i, p, v, colmax(p), (count - 1) * (colb.count[p] - 1));
                }
            }
            searched += 1;
            if best.done(searched) {
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
