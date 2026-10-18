//! Sparse matrix storage.
//!
//! The constraint matrix is held column-major (CSC) because that is what the
//! simplex wants: pricing and the FTRAN of an entering column both walk a single
//! column. Presolve and cut separation walk rows instead, so a CSR transpose is
//! built on demand via [`SparseMatrix::to_csr`] rather than kept permanently in
//! sync.

/// A matrix in compressed sparse column form.
///
/// Within each column the entries are sorted by row index and no row index
/// appears twice; [`SparseMatrix::from_triplets`] establishes this and every
/// other constructor preserves it.
#[derive(Clone, Debug, PartialEq)]
pub struct SparseMatrix {
    n_rows: usize,
    n_cols: usize,
    /// Offsets into `row_idx`/`values`; length `n_cols + 1`, last entry = nnz.
    col_start: Vec<usize>,
    row_idx: Vec<usize>,
    values: Vec<f64>,
}

impl SparseMatrix {
    /// Build from `(row, col, value)` triplets.
    ///
    /// Duplicate `(row, col)` pairs are summed, matching the convention of LP and
    /// MPS files where a variable may legally appear more than once in a row.
    /// Entries that are exactly zero, including ones that cancel, are dropped.
    pub fn from_triplets(
        n_rows: usize,
        n_cols: usize,
        triplets: impl IntoIterator<Item = (usize, usize, f64)>,
    ) -> Self {
        let mut per_col: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n_cols];
        for (r, c, v) in triplets {
            debug_assert!(r < n_rows && c < n_cols, "triplet ({r},{c}) out of bounds");
            per_col[c].push((r, v));
        }

        let mut col_start = Vec::with_capacity(n_cols + 1);
        let mut row_idx = Vec::new();
        let mut values = Vec::new();
        for col in &mut per_col {
            col_start.push(row_idx.len());
            col.sort_unstable_by_key(|&(r, _)| r);
            // Sum runs of equal row index, then drop anything that came out zero.
            let mut i = 0;
            while i < col.len() {
                let r = col[i].0;
                let mut sum = 0.0;
                while i < col.len() && col[i].0 == r {
                    sum += col[i].1;
                    i += 1;
                }
                if sum != 0.0 {
                    row_idx.push(r);
                    values.push(sum);
                }
            }
        }
        col_start.push(row_idx.len());

        Self {
            n_rows,
            n_cols,
            col_start,
            row_idx,
            values,
        }
    }

    pub fn n_rows(&self) -> usize {
        self.n_rows
    }

    pub fn n_cols(&self) -> usize {
        self.n_cols
    }

    /// Number of stored (structurally nonzero) entries.
    pub fn nnz(&self) -> usize {
        self.row_idx.len()
    }

    /// The `(row_indices, values)` of column `c`, sorted by row index.
    pub fn column(&self, c: usize) -> (&[usize], &[f64]) {
        let s = self.col_start[c];
        let e = self.col_start[c + 1];
        (&self.row_idx[s..e], &self.values[s..e])
    }

    /// A copy with `n_new` extra rows, whose entries are supplied per column as
    /// `(row_offset, value)` with `row_offset` counted from the first new row.
    ///
    /// Every new entry sits below every existing one, so it belongs at the end of its
    /// column and nothing needs re-sorting: this is one linear pass, not a rebuild
    /// through triplets.
    pub fn with_rows_appended(
        &self,
        n_new: usize,
        by_column: &[Vec<(usize, f64)>],
    ) -> SparseMatrix {
        debug_assert_eq!(by_column.len(), self.n_cols);
        let added: usize = by_column.iter().map(|c| c.len()).sum();
        let mut col_start = Vec::with_capacity(self.n_cols + 1);
        let mut row_idx = Vec::with_capacity(self.nnz() + added);
        let mut values = Vec::with_capacity(self.nnz() + added);

        col_start.push(0);
        for (c, extra) in by_column.iter().enumerate() {
            let (rows, vals) = self.column(c);
            row_idx.extend_from_slice(rows);
            values.extend_from_slice(vals);
            for &(offset, v) in extra {
                debug_assert!(offset < n_new);
                row_idx.push(self.n_rows + offset);
                values.push(v);
            }
            col_start.push(row_idx.len());
        }

        SparseMatrix {
            n_rows: self.n_rows + n_new,
            n_cols: self.n_cols,
            col_start,
            row_idx,
            values,
        }
    }

    /// Density as a fraction of `n_rows * n_cols`; 0.0 for an empty matrix.
    ///
    /// The simplex uses this to decide between dense and sparse kernels.
    pub fn density(&self) -> f64 {
        let cells = self.n_rows * self.n_cols;
        if cells == 0 {
            0.0
        } else {
            self.nnz() as f64 / cells as f64
        }
    }

    /// Transpose into compressed sparse *row* form, returned as a `SparseMatrix`
    /// whose "columns" are this matrix's rows. Row indices within each are sorted.
    pub fn to_csr(&self) -> SparseMatrix {
        // Counting sort by row: count, prefix-sum, then scatter. Walking columns in
        // order and rows within a column in order means each row's entries come out
        // sorted by column index for free.
        let mut counts = vec![0usize; self.n_rows + 1];
        for &r in &self.row_idx {
            counts[r + 1] += 1;
        }
        for i in 0..self.n_rows {
            counts[i + 1] += counts[i];
        }
        let row_start = counts;

        let mut next = row_start.clone();
        let mut col_idx = vec![0usize; self.nnz()];
        let mut values = vec![0.0f64; self.nnz()];
        for c in 0..self.n_cols {
            let (rows, vals) = self.column(c);
            for (&r, &v) in rows.iter().zip(vals) {
                let pos = next[r];
                col_idx[pos] = c;
                values[pos] = v;
                next[r] += 1;
            }
        }

        SparseMatrix {
            n_rows: self.n_cols,
            n_cols: self.n_rows,
            col_start: row_start,
            row_idx: col_idx,
            values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triplets_sum_duplicates_and_drop_zeros() {
        let m = SparseMatrix::from_triplets(
            3,
            2,
            [
                (0, 0, 1.0),
                (0, 0, 2.0),
                (2, 0, 5.0),
                (1, 1, 4.0),
                (2, 1, -4.0),
                (2, 1, 4.0),
            ],
        );
        // (0,0) summed to 3.0; (2,1) cancelled to zero and was dropped.
        assert_eq!(m.nnz(), 3);
        assert_eq!(m.column(0), (&[0usize, 2][..], &[3.0, 5.0][..]));
        assert_eq!(m.column(1), (&[1usize][..], &[4.0][..]));
    }

    #[test]
    fn csr_round_trips() {
        let m = SparseMatrix::from_triplets(
            3,
            4,
            [(0, 1, 1.5), (2, 0, -2.0), (1, 3, 7.0), (0, 3, 0.5)],
        );
        let back = m.to_csr().to_csr();
        assert_eq!(m, back);
    }

    #[test]
    fn csr_rows_are_sorted_by_column() {
        let m = SparseMatrix::from_triplets(2, 3, [(0, 2, 1.0), (0, 0, 2.0), (1, 1, 3.0)]);
        let csr = m.to_csr();
        assert_eq!(csr.column(0), (&[0usize, 2][..], &[2.0, 1.0][..]));
        assert_eq!(csr.column(1), (&[1usize][..], &[3.0][..]));
    }

    #[test]
    fn density_of_empty_matrix_is_zero() {
        let m = SparseMatrix::from_triplets(0, 0, []);
        assert_eq!(m.density(), 0.0);
        assert_eq!(m.nnz(), 0);
    }
}
