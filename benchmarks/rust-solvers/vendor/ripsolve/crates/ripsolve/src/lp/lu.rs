//! Sparse LU factorization of a basis matrix, with Markowitz pivoting.
//!
//! The dense explicit inverse this replaces costs `O(m^2)` per solve and `O(m^3)`
//! to rebuild. At `m = 1000` the rebuild alone is ~10^9 operations every 50 pivots,
//! which measured out at 0.2 seconds per branch-and-bound node, 53x slower per
//! simplex iteration than a leading commercial solver on the same model.
//!
//! # Pivot choice
//!
//! Pivots are chosen by the Markowitz criterion: among the remaining entries,
//! minimize `(r_i - 1) * (c_j - 1)`, the number of positions the elimination could
//! turn nonzero. That single rule subsumes the special cases, a column singleton
//! scores zero and is taken immediately, so an LP basis's large triangular part
//! (every logical variable contributes one) is peeled off without any separate
//! triangularization pass.
//!
//! Sparsity alone is not safe, so a candidate must also satisfy a relative
//! threshold, `|a_ij| >= tau * max|a_.j|`. Pure Markowitz would happily pivot on an
//! arbitrarily small entry and produce enormous multipliers.
//!
//! The search is bounded: it examines columns in increasing order of count and
//! stops once a pivot cannot be beaten. Searching every remaining entry at every
//! step would itself be quadratic.

/// The factorization `P B Q = L U`, with `L` unit lower triangular.
///
/// Both factors are held flat, one array of entries with a start offset per pivot
/// step, rather than as a vector per step. A factorization produces its steps in
/// order, so flat storage is what it writes naturally, and a vector per step was two
/// allocations per row of the basis every time it was refactorized.
#[derive(Clone)]
pub struct Lu {
    m: usize,
    /// Original row index of pivot `k`.
    prow: Vec<usize>,
    /// Basis position of pivot `k`.
    pcol: Vec<usize>,
    /// `L` by pivot step, in permuted index space: step `k`'s entries are
    /// `l_entries[l_start[k]..l_start[k + 1]]`, each `(k', multiplier)` with `k' > k`.
    l_start: Vec<usize>,
    l_entries: Vec<(usize, f64)>,
    /// `U` by pivot step, in permuted index space, laid out the same way: `(k', value)`
    /// with `k' > k`.
    u_start: Vec<usize>,
    u_entries: Vec<(usize, f64)>,
    /// Diagonal of `U`.
    diag: Vec<f64>,
    /// What producing these factors cost, in the units [`Lu::cost`] describes.
    cost: usize,
}

/// Working memory for [`Lu::factor`], kept between factorizations.
///
/// A factorization's bookkeeping is a vector per row and a vector per column of the
/// active submatrix, and allocating them afresh cost more than the elimination on a
/// sparse basis: 160 microseconds to factorize the 1830-row identity, against 1.2
/// milliseconds for the basis an optimal vertex of the same model has. A simplex solve
/// refactorizes every few dozen pivots, so the memory is worth keeping.
#[derive(Default)]
pub struct Scratch {
    cols: Vec<Vec<(usize, f64)>>,
    row_cols: Vec<Vec<usize>>,
    row_nz: Vec<usize>,
    row_done: Vec<bool>,
    col_done: Vec<bool>,
    scatter: Vec<f64>,
    present: Vec<bool>,
    held: Vec<u32>,
    stamp: u32,
    updated: Vec<(usize, f64)>,
    buckets: Vec<Vec<usize>>,
    col_len: Vec<usize>,
    shortlist: Vec<usize>,
    pivot_col: Vec<(usize, f64)>,
    touching: Vec<usize>,
    row_pos: Vec<usize>,
    col_pos: Vec<usize>,
}

impl Clone for Scratch {
    /// Working memory, not state: a clone starts empty and grows its own.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Scratch {
    /// Size every buffer for a basis of `m` rows and clear what the last factorization
    /// left, keeping the allocations.
    fn reset(&mut self, m: usize) {
        fn lists<T>(v: &mut Vec<Vec<T>>, len: usize) {
            v.truncate(len);
            for inner in v.iter_mut() {
                inner.clear();
            }
            v.resize_with(len, Vec::new);
        }
        fn fill<T: Clone>(v: &mut Vec<T>, len: usize, value: T) {
            v.clear();
            v.resize(len, value);
        }
        lists(&mut self.cols, m);
        lists(&mut self.row_cols, m);
        lists(&mut self.buckets, m + 1);
        fill(&mut self.row_nz, m, 0);
        fill(&mut self.row_done, m, false);
        fill(&mut self.col_done, m, false);
        fill(&mut self.scatter, m, 0.0);
        fill(&mut self.present, m, false);
        // Stamps only ever have to differ from the live one, so they carry over.
        self.held.resize(m, 0);
        fill(&mut self.col_len, m, 0);
        self.updated.clear();
        self.shortlist.clear();
        self.pivot_col.clear();
        self.touching.clear();
    }
}

/// The basis could not be factored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactorError {
    /// A basis position with no acceptable pivot; the caller should replace it.
    Singular { position: usize },
    /// The caller's deadline passed mid-factorization.
    ///
    /// Factorizing is one call however long it takes, and on a large basis that is
    /// long: 255386 rows of MIPLIB's neos-4754521-awarau took 166 seconds, during
    /// which the simplex had not begun and so had nothing to check a clock between.
    /// A five second limit ran for 193.
    OutOfTime,
}

/// Relative threshold a pivot must meet against the largest entry in its column.
///
/// Relative to the column maximum, which makes the test exactly invariant to column
/// scaling, both sides scale together. That is worth knowing before reaching for
/// equilibration: scaling the basis before factorizing was implemented and
/// measured, and changed nothing. It reduced the coefficient range of a badly
/// scaled basis from 2e15 to 8e1 and left the residual where it was (1.4e-7 against
/// 3.3e-7, i.e. marginally worse), because the pivoting was never scale-naive.
///
/// Two things it does not address, either:
///
/// - A basis of independently random wide-range entries cannot be equilibrated at
///   all. Diagonal scaling has 2m degrees of freedom against m^2 entries, so the
///   residuals measured in `scale_tests` are ill-conditioning rather than bad
///   scaling, and no scaling step will fix them.
/// - Scaling the whole *model* rather than the basis is a different thing and
///   remains untried. It is what production solvers do, and the benefit is that
///   feasibility and optimality tolerances then mean the same thing in every row,
///   which this, operating inside the factorization, never touches.
const PIVOT_THRESHOLD: f64 = 0.01;

/// Columns examined per pivot search before settling for the best seen.
///
/// Four was enough for a basis averaging one or two nonzeros per column, and far too
/// few for a dense one. On MIPLIB's hypothyroid-k1, whose basis carries 126000
/// nonzeros over 5195 rows, 92% of the solve is factorization, and the narrow search
/// settles on pivots in dense rows that then propagate work: 85.8 million column
/// eliminations, 149 columns touched per pivot against an average row of 24.
///
/// Widening it takes that relaxation from 295s to 174s and *reduces* the pivot count,
/// 19081 to 15389, so the pivots are better conditioned as well as sparser. Sixteen is
/// better still there, 167s, and worse on the sparse models, so eight is the balance.
///
/// The dense models in this solver's own range are unaffected either way: they are
/// small enough that a zero-cost pivot turns up before the width binds, and their node
/// and iteration counts do not move at all.
const MAX_COLUMN_SEARCH: usize = 8;

/// Elimination steps between clock reads during a factorization.
const FACTOR_CLOCK_INTERVAL: usize = 1024;

/// A factorization's cost in eta replays: per entry an elimination touches, and per
/// row of the basis.
///
/// Measured rather than reasoned, because the two sides of the comparison they feed run
/// at very different speeds per entry. An eta entry replays in about 1.5ns, a scatter
/// and a multiply. An elimination step costs about 10ns per entry it touches, the stamp
/// and the column rebuild included, and a factorization pays about 100ns per row before
/// it eliminates anything. Fitted on three bases, from 823 to 5904 rows: the clique
/// partitioning model behind `alta_divisions` (1830 rows, 56000 entries touched, 740us),
/// `cod105` (823, 248000, 2.6ms) and `air04` (5904, 220000, 2.6ms).
const FACTOR_ENTRY_COST: usize = 7;
const FACTOR_ROW_COST: usize = 70;

impl Lu {
    pub fn dimension(&self) -> usize {
        self.m
    }

    /// Factor the basis whose columns are given as `(rows, values)` pairs.
    pub fn factor(
        m: usize,
        columns: &[(Vec<usize>, Vec<f64>)],
        zero_tol: f64,
        deadline: Option<std::time::Instant>,
    ) -> Result<Lu, FactorError> {
        Self::factor_with(m, columns, zero_tol, deadline, &mut Scratch::default())
    }

    /// [`Lu::factor`], in working memory kept from an earlier factorization.
    pub fn factor_with(
        m: usize,
        columns: &[(Vec<usize>, Vec<f64>)],
        zero_tol: f64,
        deadline: Option<std::time::Instant>,
        scratch: &mut Scratch,
    ) -> Result<Lu, FactorError> {
        debug_assert_eq!(columns.len(), m);
        scratch.reset(m);
        let Scratch {
            cols,
            row_cols,
            row_nz,
            row_done,
            col_done,
            scatter,
            present,
            held,
            stamp,
            updated,
            buckets,
            col_len,
            shortlist,
            pivot_col,
            touching,
            row_pos,
            col_pos,
        } = scratch;

        // Active submatrix, column-major by basis position.
        for (col, (rows, vals)) in cols.iter_mut().zip(columns) {
            col.extend(
                rows.iter()
                    .copied()
                    .zip(vals.iter().copied())
                    .filter(|&(_, v)| v != 0.0),
            );
        }
        // Which columns touch each row. Entries may go stale; readers filter.
        for (j, col) in cols.iter().enumerate() {
            for &(i, _) in col {
                row_cols[i].push(j);
                row_nz[i] += 1;
            }
        }

        let mut prow = Vec::with_capacity(m);
        let mut pcol = Vec::with_capacity(m);
        // Collected in original index space; permuted once the order is known.
        let mut l_start = Vec::with_capacity(m + 1);
        let mut l_entries: Vec<(usize, f64)> = Vec::new();
        let mut u_start = Vec::with_capacity(m + 1);
        let mut u_entries: Vec<(usize, f64)> = Vec::new();
        let mut diag = Vec::with_capacity(m);
        l_start.push(0);
        u_start.push(0);

        // `held` records which rows the column being updated already holds, stamped
        // per column so that it never has to be cleared. See the fill-in step below
        // for why.

        // Columns bucketed by nonzero count, so the search reaches the sparsest few
        // without ordering the rest.
        //
        // Sorting every remaining column at every step, which is what this replaced,
        // costs `O(m^2 log m)` over a factorization and exists only to read off the
        // first `MAX_COLUMN_SEARCH` entries. On a 3186-row basis that was a third of
        // the solve. Entries are left in place when a column's length changes and
        // skipped on the way past, so an update costs a push rather than a search.
        for j in 0..m {
            col_len[j] = cols[j].len();
            buckets[col_len[j].min(m)].push(j);
        }
        // Lower bound on any live column's length. Scanning from zero each step would
        // walk `m` empty buckets per pivot and cost exactly what the sort did.
        // Elimination can only shorten a column by removing the pivot row, so this
        // moves down by one at a time and is corrected on the way past.
        let mut min_len = 0usize;
        // Entries the elimination reads or writes, for [`Lu::cost`].
        let mut touched = 0usize;

        for step in 0..m {
            // One factorization is a single call to the caller, so the deadline has to
            // be read here or not at all until it finishes.
            if step.is_multiple_of(FACTOR_CLOCK_INTERVAL)
                && deadline.is_some_and(|d| std::time::Instant::now() >= d)
            {
                return Err(FactorError::OutOfTime);
            }
            shortlist.clear();
            let mut shortest_live = None;
            // Candidates are *popped* rather than scanned. Walking a bucket to compact
            // it costs its whole length at every step, and with `-I` every column sits
            // in the length-one bucket, so that was quadratic: factorizing the
            // all-logical basis of MIPLIB's neos-4754521-awarau, 255386 rows of
            // nothing but a diagonal, took 65 seconds. Popping stops at the few
            // candidates actually wanted, and stale entries are discarded as they
            // surface.
            // Indexing rather than iterating: the loop pops from the bucket it reads,
            // which an iterator would hold borrowed.
            #[allow(clippy::needless_range_loop)]
            'buckets: for len in min_len..=m {
                while shortlist.len() < MAX_COLUMN_SEARCH {
                    let Some(j) = buckets[len].pop() else {
                        break;
                    };
                    // The column has since been pivoted out, or moved bucket.
                    if col_done[j] || col_len[j] != len {
                        continue;
                    }
                    if shortest_live.is_none() {
                        shortest_live = Some(j);
                    }
                    shortlist.push(j);
                }
                if !shortlist.is_empty() {
                    // Everything below this length is exhausted, so later steps start
                    // here rather than walking the empty buckets again.
                    min_len = len;
                    break 'buckets;
                }
            }

            let mut best: Option<(usize, usize, f64, usize)> = None; // (row, col, value, cost)
            for (examined, &j) in shortlist.iter().enumerate() {
                if cols[j].is_empty() {
                    return Err(FactorError::Singular { position: j });
                }
                let max_abs = cols[j].iter().map(|&(_, v)| v.abs()).fold(0.0f64, f64::max);
                if max_abs <= zero_tol {
                    return Err(FactorError::Singular { position: j });
                }
                for &(i, v) in &cols[j] {
                    // Stability first: a sparser pivot is worthless if it is tiny.
                    if v.abs() < PIVOT_THRESHOLD * max_abs {
                        continue;
                    }
                    let cost = (row_nz[i].saturating_sub(1)) * (cols[j].len().saturating_sub(1));
                    if best.is_none_or(|(_, _, _, c)| cost < c) {
                        best = Some((i, j, v, cost));
                    }
                }
                // A zero-cost pivot cannot be improved on, and the search is bounded
                // regardless so that finding a pivot stays sub-quadratic.
                if best.is_some_and(|(_, _, _, c)| c == 0) || examined + 1 >= MAX_COLUMN_SEARCH {
                    break;
                }
            }

            let Some((pi, pj, pivot, _)) = best else {
                // The sparsest column still active is the one to report, matching what
                // the caller repairs by swapping in a logical.
                let position = shortest_live.unwrap_or(0);
                return Err(FactorError::Singular { position });
            };

            // Candidates that were looked at but not pivoted are still live and were
            // taken out of their bucket by the pop, so they go back.
            for &j in shortlist.iter() {
                if j != pj {
                    buckets[col_len[j].min(m)].push(j);
                }
            }

            row_done[pi] = true;
            col_done[pj] = true;
            prow.push(pi);
            pcol.push(pj);
            diag.push(pivot);

            // Scatter the pivot column, dropping its own pivot row.
            pivot_col.clear();
            pivot_col.extend(cols[pj].iter().copied().filter(|&(i, _)| i != pi));
            for &(i, v) in pivot_col.iter() {
                scatter[i] = v;
                present[i] = true;
            }

            // L takes the multipliers below the pivot.
            l_entries.extend(pivot_col.iter().map(|&(i, v)| (i, v / pivot)));
            l_start.push(l_entries.len());

            // Eliminate the pivot row from every other active column.
            touching.clear();
            touching.extend(row_cols[pi].iter().copied().filter(|&j| !col_done[j]));
            for &j in touching.iter() {
                let Some(pos) = cols[j].iter().position(|&(i, _)| i == pi) else {
                    continue;
                };
                let alpha = cols[j][pos].1;
                if alpha == 0.0 {
                    continue;
                }
                touched += cols[j].len() + pivot_col.len();
                u_entries.push((j, alpha));
                let factor = alpha / pivot;

                // col_j <- col_j - factor * pivot_col, with the pivot row removed.
                updated.clear();
                // Stamp the rows this column already has, so the fill-in step below can
                // ask in constant time. Wrapping is safe because a wrap can only make a
                // stale stamp collide with the live one, and every row of the live
                // column is restamped before it is read.
                *stamp = stamp.wrapping_add(1);
                for &(i, _) in &cols[j] {
                    held[i] = *stamp;
                }
                for &(i, v) in &cols[j] {
                    if i == pi {
                        continue;
                    }
                    let value = if present[i] {
                        v - factor * scatter[i]
                    } else {
                        v
                    };
                    if value.abs() > zero_tol {
                        updated.push((i, value));
                    } else {
                        row_nz[i] -= 1;
                    }
                }
                // Fill-in: pivot-column rows this column did not already have, asked
                // of the stamp rather than by searching the column.
                //
                // Searching it is what this did, and it is quadratic in the wrong two
                // things: the pivot column's length times the updated column's, for
                // every column the pivot row touches. It costs nothing while the basis
                // stays sparse and everything once fill-in makes it dense, which is
                // exactly the case a factorization is expensive in. On MIPLIB's
                // `cod105` -- 1024 rows, 56 nonzeros a column, and a basis that fills
                // in almost completely -- it was **182 billion** operations against 822
                // million in the elimination it belongs to, and 92% of the whole
                // relaxation. The stamp makes it 689 million and the same 8000
                // iterations go from 57.8 seconds to 9.0.
                for &(i, v) in pivot_col.iter() {
                    if held[i] != *stamp {
                        let value = -factor * v;
                        if value.abs() > zero_tol {
                            updated.push((i, value));
                            row_cols[i].push(j);
                            row_nz[i] += 1;
                        }
                    }
                }
                row_nz[pi] -= 1;
                // Swap rather than assign: `cols[j]`'s allocation becomes the next
                // iteration's buffer instead of being freed.
                std::mem::swap(&mut cols[j], updated);
                if cols[j].len() != col_len[j] {
                    col_len[j] = cols[j].len();
                    let bucket = col_len[j].min(m);
                    buckets[bucket].push(j);
                    min_len = min_len.min(bucket);
                }
            }
            u_start.push(u_entries.len());

            for &(i, _) in pivot_col.iter() {
                scatter[i] = 0.0;
                present[i] = false;
                row_nz[i] -= 1;
            }
            row_nz[pi] = 0;
            cols[pj].clear();
        }

        let cost = FACTOR_ENTRY_COST * touched + FACTOR_ROW_COST * m;

        // Translate to permuted index space, now that the pivot order is known.
        row_pos.clear();
        row_pos.resize(m, usize::MAX);
        col_pos.clear();
        col_pos.resize(m, usize::MAX);
        for k in 0..m {
            row_pos[prow[k]] = k;
            col_pos[pcol[k]] = k;
        }
        for (i, _) in l_entries.iter_mut() {
            *i = row_pos[*i];
        }
        for (j, _) in u_entries.iter_mut() {
            *j = col_pos[*j];
        }

        Ok(Lu {
            m,
            prow,
            pcol,
            l_start,
            l_entries,
            u_start,
            u_entries,
            diag,
            cost,
        })
    }

    /// What these factors cost to produce, counted in eta entries replayed.
    ///
    /// This is the number a refactorization policy has to weigh, and the factors' own
    /// size is not it: the 1830-row basis above touches 56000 entries to produce 8000,
    /// so a policy that refactorized once the eta file outgrew the factors did so every
    /// six pivots, and spent 56% of a relaxation factorizing.
    pub fn cost(&self) -> usize {
        self.cost
    }

    fn l_col(&self, k: usize) -> &[(usize, f64)] {
        &self.l_entries[self.l_start[k]..self.l_start[k + 1]]
    }

    fn u_row(&self, k: usize) -> &[(usize, f64)] {
        &self.u_entries[self.u_start[k]..self.u_start[k + 1]]
    }

    /// Solve `B d = a`, writing `d` (indexed by basis position) into `out`.
    pub fn ftran(&self, a: &[f64], out: &mut Vec<f64>) {
        let m = self.m;
        debug_assert_eq!(a.len(), m);
        // Permute the right-hand side into pivot order, then solve L z = b.
        let mut work: Vec<f64> = (0..m).map(|k| a[self.prow[k]]).collect();
        for k in 0..m {
            let z = work[k];
            if z != 0.0 {
                for &(k2, mult) in self.l_col(k) {
                    work[k2] -= mult * z;
                }
            }
        }
        // Back-substitute U w = z.
        for k in (0..m).rev() {
            let mut acc = work[k];
            for &(k2, v) in self.u_row(k) {
                acc -= v * work[k2];
            }
            work[k] = acc / self.diag[k];
        }
        out.clear();
        out.resize(m, 0.0);
        for k in 0..m {
            out[self.pcol[k]] = work[k];
        }
    }

    /// Solve `B' y = c`, writing `y` (indexed by row) into `out`.
    pub fn btran(&self, c: &[f64], out: &mut Vec<f64>) {
        let m = self.m;
        debug_assert_eq!(c.len(), m);
        // Permute by column order, then solve U' v = c (U' is lower triangular).
        let mut work: Vec<f64> = (0..m).map(|k| c[self.pcol[k]]).collect();
        for k in 0..m {
            let v = work[k] / self.diag[k];
            work[k] = v;
            if v != 0.0 {
                for &(k2, u) in self.u_row(k) {
                    work[k2] -= u * v;
                }
            }
        }
        // Then L' u = v, which is upper triangular in this ordering.
        for k in (0..m).rev() {
            let mut acc = work[k];
            for &(k2, mult) in self.l_col(k) {
                acc -= mult * work[k2];
            }
            work[k] = acc;
        }
        out.clear();
        out.resize(m, 0.0);
        for k in 0..m {
            out[self.prow[k]] = work[k];
        }
    }

    /// Stored nonzeros in both factors, for diagnostics and refactorization policy.
    pub fn nnz(&self) -> usize {
        self.m + self.l_entries.len() + self.u_entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic PRNG, so a failing case is reproducible from its seed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
        fn value(&mut self) -> f64 {
            (self.next() % 19) as f64 - 9.0
        }
    }

    /// Dense columns from sparse ones, for the reference solves.
    fn densify(m: usize, columns: &[(Vec<usize>, Vec<f64>)]) -> Vec<Vec<f64>> {
        columns
            .iter()
            .map(|(rows, vals)| {
                let mut col = vec![0.0; m];
                for (&i, &v) in rows.iter().zip(vals) {
                    col[i] = v;
                }
                col
            })
            .collect()
    }

    /// Check `B (B^-1 a) == a` and `(B' y) == c`, which pins both solves without
    /// needing a reference inverse.
    fn assert_solves(m: usize, columns: &[(Vec<usize>, Vec<f64>)], lu: &Lu, tol: f64) {
        let dense = densify(m, columns);
        let mut rng = Rng(7);
        for _ in 0..4 {
            let a: Vec<f64> = (0..m).map(|_| rng.value()).collect();

            let mut d = Vec::new();
            lu.ftran(&a, &mut d);
            // B d must reproduce a.
            for i in 0..m {
                let got: f64 = (0..m).map(|p| dense[p][i] * d[p]).sum();
                assert!((got - a[i]).abs() < tol, "ftran row {i}: {got} vs {}", a[i]);
            }

            let c: Vec<f64> = (0..m).map(|_| rng.value()).collect();
            let mut y = Vec::new();
            lu.btran(&c, &mut y);
            // y' B must reproduce c.
            for p in 0..m {
                let got: f64 = (0..m).map(|i| dense[p][i] * y[i]).sum();
                assert!(
                    (got - c[p]).abs() < tol,
                    "btran column {p}: {got} vs {}",
                    c[p]
                );
            }
        }
    }

    fn sparse(m: usize, dense: &[Vec<f64>]) -> Vec<(Vec<usize>, Vec<f64>)> {
        dense
            .iter()
            .map(|col| {
                let mut rows = Vec::new();
                let mut vals = Vec::new();
                for (i, &v) in col.iter().enumerate().take(m) {
                    if v != 0.0 {
                        rows.push(i);
                        vals.push(v);
                    }
                }
                (rows, vals)
            })
            .collect()
    }

    #[test]
    fn factors_the_logical_basis() {
        // The all-logical starting basis is -I, the case every solve begins from.
        let m = 5;
        let columns: Vec<(Vec<usize>, Vec<f64>)> = (0..m).map(|i| (vec![i], vec![-1.0])).collect();
        let lu = Lu::factor(m, &columns, 1e-12, None).unwrap();
        assert_solves(m, &columns, &lu, 1e-9);
        // Perfectly triangular: no fill beyond the diagonal.
        assert_eq!(lu.nnz(), m);
    }

    #[test]
    fn factors_a_permuted_triangular_basis() {
        // Markowitz should peel this off with zero fill, choosing singletons first.
        let m = 4;
        let dense = vec![
            vec![0.0, 0.0, 2.0, 0.0],
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 3.0, 1.0, 0.0],
            vec![0.0, 0.0, 0.0, 4.0],
        ];
        let columns = sparse(m, &dense);
        let lu = Lu::factor(m, &columns, 1e-12, None).unwrap();
        assert_solves(m, &columns, &lu, 1e-9);
    }

    #[test]
    fn factors_a_dense_basis() {
        let m = 6;
        let mut rng = Rng(11);
        loop {
            let dense: Vec<Vec<f64>> = (0..m)
                .map(|_| (0..m).map(|_| rng.value()).collect())
                .collect();
            let columns = sparse(m, &dense);
            if let Ok(lu) = Lu::factor(m, &columns, 1e-12, None) {
                assert_solves(m, &columns, &lu, 1e-6);
                return;
            }
        }
    }

    #[test]
    fn factors_random_sparse_bases() {
        // The broad net: many shapes, each verified by round-tripping both solves.
        let mut rng = Rng(2024);
        let mut factored = 0;
        for _ in 0..60 {
            let m = 8 + rng.below(20);
            // Start from the identity so the basis is generally nonsingular, then
            // scatter extra entries to force fill-in and real pivot choices.
            let mut dense: Vec<Vec<f64>> = (0..m)
                .map(|j| {
                    let mut col = vec![0.0; m];
                    col[j] = 1.0 + rng.below(4) as f64;
                    col
                })
                .collect();
            for _ in 0..(m * 3) {
                let (j, i) = (rng.below(m), rng.below(m));
                let v = rng.value();
                if v != 0.0 {
                    dense[j][i] = v;
                }
            }
            let columns = sparse(m, &dense);
            if let Ok(lu) = Lu::factor(m, &columns, 1e-12, None) {
                assert_solves(m, &columns, &lu, 1e-5);
                factored += 1;
            }
        }
        assert!(factored > 40, "only {factored} of 60 bases factored");
    }

    #[test]
    fn reports_a_singular_basis() {
        // Third column is the sum of the first two.
        let m = 3;
        let dense = vec![
            vec![1.0, 0.0, 1.0],
            vec![0.0, 1.0, 1.0],
            vec![1.0, 1.0, 2.0],
        ];
        assert!(Lu::factor(m, &sparse(m, &dense), 1e-12, None).is_err());
    }

    #[test]
    fn reports_an_empty_column_as_singular() {
        let m = 3;
        let columns = vec![
            (vec![0], vec![1.0]),
            (Vec::new(), Vec::new()),
            (vec![2], vec![1.0]),
        ];
        let err = Lu::factor(m, &columns, 1e-12, None)
            .err()
            .expect("empty column is singular");
        assert_eq!(err, FactorError::Singular { position: 1 });
    }

    #[test]
    fn markowitz_keeps_an_arrow_matrix_sparse() {
        // An arrow with the dense row and column last: pivoting on the diagonal
        // spokes first costs no fill, while pivoting on the hub first would make the
        // whole remaining matrix dense.
        let m = 30;
        let mut dense: Vec<Vec<f64>> = (0..m)
            .map(|j| {
                let mut col = vec![0.0; m];
                col[j] = 2.0;
                col
            })
            .collect();
        for col in dense.iter_mut().take(m - 1) {
            col[m - 1] = 1.0;
        }
        let hub = &mut dense[m - 1];
        for entry in hub.iter_mut().take(m - 1) {
            *entry = 1.0;
        }
        let columns = sparse(m, &dense);
        let lu = Lu::factor(m, &columns, 1e-12, None).unwrap();
        assert_solves(m, &columns, &lu, 1e-7);
        // Fill-free would be 3m - 2 nonzeros; allow slack but not densification.
        assert!(
            lu.nnz() < 4 * m,
            "fill blew up: {} nonzeros for m = {m}",
            lu.nnz()
        );
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    /// SplitMix64, so a failure is reproducible from its seed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
        fn value(&mut self) -> f64 {
            let v = (self.next() % 2001) as f64 / 100.0 - 10.0;
            if v.abs() < 0.5 { 1.0 } else { v }
        }
    }

    /// A basis shaped like one the simplex actually holds: mostly unit columns from
    /// logical variables, with a minority of sparse structural columns mixed in.
    ///
    /// Structure is the point. A dense random matrix makes every pivot choice
    /// equivalent, so it cannot distinguish a good Markowitz search from a bad one.
    fn realistic_basis(
        m: usize,
        structural: usize,
        per_column: usize,
        seed: u64,
    ) -> Vec<(Vec<usize>, Vec<f64>)> {
        let mut rng = Rng(seed);
        let mut columns: Vec<(Vec<usize>, Vec<f64>)> =
            (0..m).map(|i| (vec![i], vec![-1.0])).collect();
        for _ in 0..structural {
            let target = rng.below(m);
            let mut rows: Vec<usize> = Vec::new();
            // Keep the diagonal entry so the basis stays comfortably nonsingular.
            rows.push(target);
            for _ in 0..per_column.saturating_sub(1) {
                let r = rng.below(m);
                if !rows.contains(&r) {
                    rows.push(r);
                }
            }
            let mut pairs: Vec<(usize, f64)> = rows.into_iter().map(|r| (r, rng.value())).collect();
            pairs.sort_unstable_by_key(|&(r, _)| r);
            // A strong diagonal keeps the basis well conditioned, so a residual this
            // test rejects is the factorization's fault and not the matrix's.
            for (r, v) in &mut pairs {
                if *r == target {
                    *v = 25.0;
                }
            }
            columns[target] = (
                pairs.iter().map(|&(r, _)| r).collect(),
                pairs.iter().map(|&(_, v)| v).collect(),
            );
        }
        columns
    }

    fn dense_column(m: usize, column: &(Vec<usize>, Vec<f64>)) -> Vec<f64> {
        let mut out = vec![0.0; m];
        for (&r, &v) in column.0.iter().zip(&column.1) {
            out[r] = v;
        }
        out
    }

    /// The largest relative residual of `B (B^-1 a) = a` over random right-hand
    /// sides, how much accuracy the factorization actually delivers.
    ///
    /// Checking that a solve *runs* says nothing. A factorization built on bad
    /// pivots still produces numbers; they are simply wrong enough, far enough
    /// downstream, to make the simplex conclude something false.
    fn worst_residual(m: usize, columns: &[(Vec<usize>, Vec<f64>)], lu: &Lu) -> f64 {
        let dense: Vec<Vec<f64>> = columns.iter().map(|c| dense_column(m, c)).collect();
        let mut rng = Rng(99);
        let mut worst: f64 = 0.0;
        let mut d = Vec::new();
        for _ in 0..3 {
            let a: Vec<f64> = (0..m).map(|_| rng.value()).collect();
            lu.ftran(&a, &mut d);
            let scale = a.iter().fold(0.0f64, |acc, v| acc.max(v.abs())).max(1.0);
            for i in 0..m {
                let got: f64 = (0..m).map(|p| dense[p][i] * d[p]).sum();
                worst = worst.max((got - a[i]).abs() / scale);
            }
        }
        worst
    }

    /// Coefficients spanning `10^spread` either side of one, which is what makes a
    /// basis hard to factorize accurately. A matrix of ones and twos does not
    /// distinguish a good factorization from a bad one.
    fn spread_basis(
        m: usize,
        structural: usize,
        per_column: usize,
        spread: f64,
        seed: u64,
    ) -> Vec<(Vec<usize>, Vec<f64>)> {
        let mut rng = Rng(seed);
        let mut columns: Vec<(Vec<usize>, Vec<f64>)> =
            (0..m).map(|i| (vec![i], vec![-1.0])).collect();
        for _ in 0..structural {
            let target = rng.below(m);
            let mut rows = vec![target];
            for _ in 0..per_column.saturating_sub(1) {
                let r = rng.below(m);
                if !rows.contains(&r) {
                    rows.push(r);
                }
            }
            let mut pairs: Vec<(usize, f64)> = rows
                .into_iter()
                .map(|r| {
                    let exponent = ((rng.next() % 2001) as f64 / 1000.0 - 1.0) * spread;
                    let magnitude = 10f64.powf(exponent);
                    (
                        r,
                        if rng.next().is_multiple_of(2) {
                            magnitude
                        } else {
                            -magnitude
                        },
                    )
                })
                .collect();
            pairs.sort_unstable_by_key(|&(r, _)| r);
            columns[target] = (
                pairs.iter().map(|&(r, _)| r).collect(),
                pairs.iter().map(|&(_, v)| v).collect(),
            );
        }
        columns
    }

    #[test]
    fn stays_accurate_on_a_basis_large_enough_for_pivoting_to_matter() {
        // The gap this closes: every other LU test here uses a handful of rows,
        // where any pivot order works and a Markowitz search cannot be wrong. A
        // change to pivot selection once passed the whole suite and then reported a
        // feasible LP infeasible on a 402-row model.
        //
        // Checked as a *residual*, not as "the solve ran". A factorization built on
        // bad pivots still returns numbers; they are simply wrong enough, far enough
        // downstream, for the simplex to conclude something false.
        for (m, structural) in [(200, 60), (500, 150), (900, 250)] {
            let columns = spread_basis(m, structural, 8, 0.5, 7 + m as u64);
            let lu =
                Lu::factor(m, &columns, 1e-12, None).unwrap_or_else(|e| panic!("m = {m}: {e:?}"));
            let residual = worst_residual(m, &columns, &lu);
            assert!(
                residual < 1e-7,
                "m = {m}: residual {residual:.3e}, the factorization is losing accuracy"
            );
        }
    }

    /// What the factorization does as the data gets worse conditioned.
    ///
    /// Not an assertion of quality, a record of a known limitation, measured. The
    /// factorization has no scaling step, so accuracy tracks the spread of the
    /// coefficients directly:
    ///
    /// | coefficient range | residual |
    /// |---|---|
    /// | 1x        | 6e-14 |
    /// | 10x       | 6e-10 |
    /// | 100x      | 7e-8  |
    /// | 1000x     | 2e-3  |
    /// | 10000x    | 9e0   |
    ///
    /// By a range of 1e4 the result is meaningless, and real models routinely carry
    /// 1e4 to 1e6. Equilibrating the matrix before factorizing is the standard
    /// remedy and is not implemented. This test asserts only the part that holds
    /// today, so the boundary is recorded rather than assumed away.
    #[test]
    fn accuracy_tracks_how_badly_scaled_the_data_is() {
        let m = 400;
        let well = worst_residual(
            m,
            &spread_basis(m, 120, 8, 0.0, 3),
            &Lu::factor(m, &spread_basis(m, 120, 8, 0.0, 3), 1e-12, None).unwrap(),
        );
        assert!(
            well < 1e-10,
            "uniform coefficients should factorize cleanly, got {well:.3e}"
        );

        // And the documented failure, so a future scaling step has something to beat.
        let bad_columns = spread_basis(m, 120, 8, 2.0, 3);
        let bad = worst_residual(
            m,
            &bad_columns,
            &Lu::factor(m, &bad_columns, 1e-12, None).unwrap(),
        );
        assert!(
            bad > well,
            "a 1e4 coefficient range is expected to be worse than a uniform one; \
             if this fails, scaling has been added and the bound above should be tightened"
        );
    }

    #[test]
    fn markowitz_keeps_fill_bounded_at_scale() {
        // What the pivot search is *for*. A search that picks poorly still produces
        // a usable factorization on a small matrix, and buries the solver in fill on
        // a real one, so this is checked where the difference shows.
        for (m, structural) in [(300, 90), (800, 240)] {
            let columns = spread_basis(m, structural, 8, 0.5, 11 + m as u64);
            let original: usize = columns.iter().map(|c| c.0.len()).sum();
            let lu = Lu::factor(m, &columns, 1e-12, None).unwrap();
            assert!(
                lu.nnz() < 6 * original,
                "m = {m}: {} nonzeros from {original}, fill has blown up",
                lu.nnz()
            );
        }
    }

    #[test]
    fn accuracy_survives_a_long_run_of_updates() {
        // The product-form file grows between refactorizations, and its error grows
        // with it. This is the regime the simplex spends its life in.
        let m = 400;
        let mut columns = realistic_basis(m, 120, 8, 31);
        let mut basis = crate::lp::basis::Basis::all_logical(m);
        basis.refactorize(&columns, 1e-9, None).unwrap();

        let mut rng = Rng(5);
        for step in 0..60 {
            let position = rng.below(m);
            let replacement = realistic_basis(m, 1, 6, 100 + step)[0].clone();
            let dense = dense_column(m, &replacement);
            let mut d = Vec::new();
            basis.ftran(&dense, &mut d);
            // Skip a pivot too small to be safe, exactly as the simplex would.
            if d[position].abs() < 1e-7 {
                continue;
            }
            basis.update(&d, position);
            columns[position] = replacement;
        }

        // B^-1 B must still be the identity after all of that.
        let dense: Vec<Vec<f64>> = columns.iter().map(|c| dense_column(m, c)).collect();
        let mut out = Vec::new();
        let mut worst: f64 = 0.0;
        for (j, column) in dense.iter().enumerate() {
            basis.ftran(column, &mut out);
            for (i, &got) in out.iter().enumerate() {
                let expected = if i == j { 1.0 } else { 0.0 };
                worst = worst.max((got - expected).abs());
            }
        }
        assert!(
            worst < 1e-6,
            "after 60 updates the inverse is off by {worst:.3e}"
        );
    }
}
