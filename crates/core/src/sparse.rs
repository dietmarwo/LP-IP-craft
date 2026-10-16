//! Bounds and sparse matrices in zero-based CSR form.

use crate::error::{Error, Result, ensure, ensure_finite};

/// Elementwise bounds `lower[i] <= v[i] <= upper[i]`.
///
/// Equal finite bounds express an equality. `-inf` lower or `+inf` upper
/// bounds express one-sided or free entries. NaN, `lower = +inf`,
/// `upper = -inf` and `lower > upper` are invalid.
#[derive(Clone, Debug, PartialEq)]
pub struct Bounds {
    /// Lower bounds; `f64::NEG_INFINITY` for none.
    pub lower: Vec<f64>,
    /// Upper bounds; `f64::INFINITY` for none.
    pub upper: Vec<f64>,
}

impl Bounds {
    /// Bounds from two vectors. Validation happens where the bounds are used.
    pub fn new(lower: Vec<f64>, upper: Vec<f64>) -> Self {
        Self { lower, upper }
    }

    /// `n` free entries.
    pub fn free(n: usize) -> Self {
        Self::new(vec![f64::NEG_INFINITY; n], vec![f64::INFINITY; n])
    }

    /// `n` entries `0 <= v[i] < +inf`.
    pub fn non_negative(n: usize) -> Self {
        Self::new(vec![0.0; n], vec![f64::INFINITY; n])
    }

    /// Equalities `v[i] = values[i]`.
    pub fn fixed(values: Vec<f64>) -> Self {
        Self::new(values.clone(), values)
    }

    /// Number of entries (length of `lower`).
    pub fn len(&self) -> usize {
        self.lower.len()
    }

    /// Whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.lower.is_empty()
    }

    /// Checks lengths against `n` and every pair for consistency. `what` names
    /// the bounds in the error message.
    pub fn validate(&self, n: usize, what: &str) -> Result<()> {
        ensure(self.lower.len() == n && self.upper.len() == n, || {
            format!(
                "{what}: expected {n} entries, got lower={} and upper={}",
                self.lower.len(),
                self.upper.len()
            )
        })?;
        for (i, (&l, &u)) in self.lower.iter().zip(&self.upper).enumerate() {
            let ok = !l.is_nan()
                && !u.is_nan()
                && l != f64::INFINITY
                && u != f64::NEG_INFINITY
                && l <= u;
            ensure(ok, || {
                format!(
                    "{what}: entry {i} has lower={l}, upper={u}; need lower <= upper, \
                     no NaN, lower < +inf and upper > -inf"
                )
            })?;
        }
        Ok(())
    }

    /// Largest amount by which `values` exceed the bounds, or `0.0` when all
    /// entries are within them. Absolute and in the units of `values`.
    ///
    /// # Panics
    /// If bounds/point are invalid or a finite-bound distance overflows. Use
    /// [`Self::checked_violation`] for a fallible call.
    pub fn violation(&self, values: &[f64]) -> f64 {
        self.checked_violation(values)
            .expect("invalid bounds/point or nonfinite violation")
    }

    /// Checked bound violation, rejecting malformed inputs and arithmetic overflow.
    pub fn checked_violation(&self, values: &[f64]) -> Result<f64> {
        self.validate(values.len(), "bounds")?;
        let mut result = 0.0_f64;
        for (i, &v) in values.iter().enumerate() {
            ensure(v.is_finite(), || format!("point entry {i} must be finite"))?;
            if self.lower[i].is_finite() {
                result = result.max(crate::error::finite(
                    self.lower[i] - v,
                    "lower-bound distance",
                )?);
            }
            if self.upper[i].is_finite() {
                result = result.max(crate::error::finite(
                    v - self.upper[i],
                    "upper-bound distance",
                )?);
            }
        }
        Ok(result)
    }
}

/// Structure of a sparse matrix in zero-based compressed sparse row (CSR) form.
///
/// Row `r` holds the column indices `indices[indptr[r]..indptr[r + 1]]`, which
/// must be strictly increasing (sorted, no duplicates) and smaller than `cols`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsrPattern {
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
    /// Row pointers, `rows + 1` entries, starting at 0 and ending at `indices.len()`.
    pub indptr: Vec<usize>,
    /// Column index of every stored entry.
    pub indices: Vec<usize>,
}

impl CsrPattern {
    /// Number of stored entries.
    pub fn nnz(&self) -> usize {
        self.indices.len()
    }

    /// Column indices of row `r`.
    ///
    /// # Panics
    /// If `r >= self.rows` or the pattern is invalid.
    pub fn row(&self, r: usize) -> &[usize] {
        &self.indices[self.indptr[r]..self.indptr[r + 1]]
    }

    /// Checks the CSR invariants. `what` names the matrix in the error message.
    pub fn validate(&self, what: &str) -> Result<()> {
        ensure(self.rows.checked_add(1) == Some(self.indptr.len()), || {
            format!(
                "{what}: indptr has {} entries, expected rows + 1 with rows = {}",
                self.indptr.len(),
                self.rows
            )
        })?;
        ensure(
            self.indptr[0] == 0 && self.indptr[self.rows] == self.indices.len(),
            || {
                format!(
                    "{what}: indptr must start at 0 and end at the number of indices ({}), \
                     got {} and {}",
                    self.indices.len(),
                    self.indptr[0],
                    self.indptr[self.rows]
                )
            },
        )?;
        for r in 0..self.rows {
            let (start, end) = (self.indptr[r], self.indptr[r + 1]);
            ensure(start <= end && end <= self.indices.len(), || {
                format!("{what}: indptr decreases at row {r} ({start} > {end})")
            })?;
            let row = &self.indices[start..end];
            if let Some(&j) = row.iter().find(|&&j| j >= self.cols) {
                return Err(Error::InvalidInput(format!(
                    "{what}: row {r} has column index {j}, but there are only {} columns",
                    self.cols
                )));
            }
            ensure(row.windows(2).all(|p| p[0] < p[1]), || {
                format!(
                    "{what}: row {r} column indices must be strictly increasing \
                     (sorted, no duplicates), got {row:?}"
                )
            })?;
        }
        Ok(())
    }
}

/// A sparse matrix: a [`CsrPattern`] plus one value per stored entry.
///
/// Duplicate entries are rejected, not summed: combine and sort them before
/// building the matrix. Explicitly stored zeros are allowed.
#[derive(Clone, Debug, PartialEq)]
pub struct CsrMatrix {
    /// Structure.
    pub pattern: CsrPattern,
    /// Values in the order of `pattern.indices`.
    pub values: Vec<f64>,
}

impl CsrMatrix {
    /// A matrix from its CSR arrays. Validation happens where it is used.
    pub fn new(
        rows: usize,
        cols: usize,
        indptr: Vec<usize>,
        indices: Vec<usize>,
        values: Vec<f64>,
    ) -> Self {
        Self {
            pattern: CsrPattern {
                rows,
                cols,
                indptr,
                indices,
            },
            values,
        }
    }

    /// The nonzero entries of a dense `rows x cols` matrix given row by row.
    /// Exact zeros are not stored.
    pub fn from_dense(rows: usize, cols: usize, row_major: &[f64]) -> Result<Self> {
        ensure(rows.checked_mul(cols) == Some(row_major.len()), || {
            format!(
                "dense matrix: {} values for a {rows} x {cols} matrix",
                row_major.len()
            )
        })?;
        ensure_finite(row_major, "dense matrix")?;
        let mut indptr = Vec::with_capacity(rows + 1);
        let mut indices = Vec::new();
        let mut values = Vec::new();
        indptr.push(0);
        for r in 0..rows {
            for (j, &a) in row_major[r * cols..(r + 1) * cols].iter().enumerate() {
                if a != 0.0 {
                    indices.push(j);
                    values.push(a);
                }
            }
            indptr.push(indices.len());
        }
        Ok(Self::new(rows, cols, indptr, indices, values))
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.pattern.rows
    }

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.pattern.cols
    }

    /// Number of stored entries.
    pub fn nnz(&self) -> usize {
        self.pattern.nnz()
    }

    /// Checks the pattern, the number of values and their finiteness.
    pub fn validate(&self, what: &str) -> Result<()> {
        self.pattern.validate(what)?;
        ensure(self.values.len() == self.pattern.nnz(), || {
            format!(
                "{what}: {} values for {} stored entries",
                self.values.len(),
                self.pattern.nnz()
            )
        })?;
        ensure_finite(&self.values, &format!("{what} values"))
    }

    /// `y = A x`.
    ///
    /// # Panics
    /// On length mismatches.
    pub fn multiply(&self, x: &[f64], y: &mut [f64]) {
        let p = &self.pattern;
        assert_eq!(self.values.len(), p.nnz(), "values length");
        assert_eq!(x.len(), p.cols, "x length");
        assert_eq!(y.len(), p.rows, "y length");
        for (r, out) in y.iter_mut().enumerate() {
            let range = p.indptr[r]..p.indptr[r + 1];
            *out = p.indices[range.clone()]
                .iter()
                .zip(&self.values[range])
                .map(|(&j, &a)| a * x[j])
                .sum();
        }
    }

    /// `y = A^T x`.
    ///
    /// # Panics
    /// On length mismatches.
    pub fn multiply_transpose(&self, x: &[f64], y: &mut [f64]) {
        let p = &self.pattern;
        assert_eq!(self.values.len(), p.nnz(), "values length");
        assert_eq!(x.len(), p.rows, "x length");
        assert_eq!(y.len(), p.cols, "y length");
        y.fill(0.0);
        for (r, &xr) in x.iter().enumerate() {
            let range = p.indptr[r]..p.indptr[r + 1];
            for (&j, &a) in p.indices[range.clone()].iter().zip(&self.values[range]) {
                y[j] += a * xr;
            }
        }
    }
}
