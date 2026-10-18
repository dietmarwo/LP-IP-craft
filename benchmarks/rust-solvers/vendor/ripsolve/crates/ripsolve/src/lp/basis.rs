//! The basis inverse, and the solves the simplex needs against it.
//!
//! Representation: a sparse LU factorization (see [`crate::lp::lu`]) plus a
//! *product form* update, an "eta file" of rank-one corrections, one per pivot,
//! replayed on top of the factors. Periodically the etas are discarded and the
//! basis refactorized, which both bounds the replay cost and stops error
//! accumulating.
//!
//! # Why product form rather than Forrest-Tomlin
//!
//! Forrest-Tomlin updates the `U` factor in place and keeps the eta file shorter,
//! which is what a mature solver does. Product form is a few lines by comparison
//! and is what makes the correctness argument easy to see: after a pivot on row
//! `r`, the new basis is `B_new = B_old E`, where `E` is the identity with column
//! `r` replaced by `d = B_old^-1 a_q`. So `B_new^-1 = E^-1 B_old^-1`, and after `k`
//! pivots `B_k^-1 = E_k^-1 ... E_1^-1 B_0^-1`. FTRAN therefore applies the factors
//! first and the etas in order; BTRAN applies the transposed etas in reverse and
//! the factors last.
//!
//! The interface ([`Basis::ftran`], [`Basis::btran`], [`Basis::update`],
//! [`Basis::refactorize`]) is unchanged from the dense explicit inverse this
//! replaced, so Forrest-Tomlin can supersede product form later without the simplex
//! driver noticing, exactly as this change did.

use crate::lp::lu::{FactorError, Lu, Scratch};

/// Why a refactorization could not produce a usable basis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasisError {
    /// The basis is singular at this position; the caller must repair it (typically
    /// by swapping in a logical) and retry.
    Singular { row: usize },
    /// The caller's deadline passed while factorizing.
    OutOfTime,
}

/// One product-form update: the transformed entering column and its pivot row.
#[derive(Clone)]
struct Eta {
    row: usize,
    pivot: f64,
    /// The column's other nonzeros, `(row, value)` with `row != self.row`.
    column: Vec<(usize, f64)>,
}

/// One row appended to a basis after it was factorized.
///
/// Adding a cut grows the basis by a row and a logical. The logical starts basic in
/// its own row, so the new basis matrix is block lower triangular:
///
/// ```text
///     B' = [ B    0 ]
///          [ R_B  S ]
/// ```
///
/// with `S` the appended logicals' own coefficients, `-1` each in `[A | -I]`. A block
/// triangular matrix is solved by substitution, so `B'` never needs factorizing:
/// `B'^-1` is the existing `B^-1` plus a sparse correction of size `k`.
#[derive(Clone)]
struct Extension {
    /// The appended row's coefficients against the *base* basis positions.
    row: Vec<(usize, f64)>,
    /// This row's own logical, the diagonal of `S`.
    diagonal: f64,
}

/// A factorized basis.
#[derive(Clone)]
pub struct Basis {
    /// Full dimension, appended rows included.
    m: usize,
    lu: Lu,
    /// Pivots against the factorized base, applied beneath any extension.
    etas: Vec<Eta>,
    /// Rows appended since the last refactorization.
    ///
    /// The extension wraps the whole base operator, the LU *and* its etas, because
    /// the correction needs `B^-1` applied, not `LU^-1`. Pivots taken after it
    /// therefore cannot join `etas`; they go in `post`.
    ext: Vec<Extension>,
    /// Pivots recorded after the extension, applied on top of it.
    post: Vec<Eta>,
    /// Working memory for the next refactorization.
    scratch: Scratch,
    /// Nonzeros in `etas` and `post`, and the replay work they have cost since the last
    /// refactorization; see [`Basis::eta_file_is_expensive`].
    eta_nnz: usize,
    replayed: usize,
}

/// Solves a pivot asks of the basis, each of which replays the whole eta file: the
/// entering column, the pivot row, and the pricing update's own.
const SOLVES_PER_PIVOT: usize = 3;

/// Entries smaller than this are treated as structurally absent.
const ZERO_TOL: f64 = 1e-12;

impl Basis {
    /// The all-logical starting basis, whose matrix is `-I`.
    pub fn all_logical(m: usize) -> Self {
        let columns: Vec<(Vec<usize>, Vec<f64>)> = (0..m).map(|i| (vec![i], vec![-1.0])).collect();
        let lu = Lu::factor(m, &columns, ZERO_TOL, None).expect("-I is nonsingular");
        Self {
            m,
            lu,
            etas: Vec::new(),
            ext: Vec::new(),
            post: Vec::new(),
            scratch: Scratch::default(),
            eta_nnz: 0,
            replayed: 0,
        }
    }

    /// Dimension of the factorized base, before any appended rows.
    fn base_dim(&self) -> usize {
        self.m - self.ext.len()
    }

    /// Grow the basis by the given rows, each with its own logical basic in it.
    ///
    /// `rows` gives each appended row's coefficients over the structural columns, and
    /// `basic` the variables currently in the basis, in position order. The result
    /// inverts the grown basis exactly, with no refactorization: this is the whole
    /// point of the block form, since refactorizing at every node that separates is
    /// what made node-local cuts too expensive to run often.
    pub fn extend(&mut self, rows: &[Vec<(usize, f64)>], basic: &[usize], n_structural: usize) {
        if rows.is_empty() {
            return;
        }
        debug_assert_eq!(basic.len(), self.m);
        // Scatter each row once and read it off per basis position, rather than
        // searching the row for every column.
        let mut dense = vec![0.0; n_structural];
        for row in rows {
            for &(j, v) in row {
                dense[j] = v;
            }
            let mut against_basis = Vec::new();
            for (position, &j) in basic.iter().enumerate() {
                // Logicals of the pre-existing rows have no entry in an appended row.
                if j < n_structural && dense[j] != 0.0 {
                    against_basis.push((position, dense[j]));
                }
            }
            self.ext.push(Extension {
                row: against_basis,
                diagonal: -1.0,
            });
            self.m += 1;
            for &(j, _) in row {
                dense[j] = 0.0;
            }
        }
    }

    /// FTRAN against the base only: the LU and the etas beneath any extension.
    fn ftran_base(&self, a: &[f64], out: &mut Vec<f64>) {
        self.lu.ftran(a, out);
        for eta in &self.etas {
            let scaled = out[eta.row] / eta.pivot;
            out[eta.row] = scaled;
            if scaled != 0.0 {
                for &(i, v) in &eta.column {
                    out[i] -= v * scaled;
                }
            }
        }
    }

    /// BTRAN against the base only.
    fn btran_base(&self, c: &[f64], out: &mut Vec<f64>) {
        out.clear();
        out.extend_from_slice(c);
        for eta in self.etas.iter().rev() {
            let mut acc = out[eta.row];
            for &(i, v) in &eta.column {
                acc -= v * out[i];
            }
            out[eta.row] = acc / eta.pivot;
        }
        let work = std::mem::take(out);
        self.lu.btran(&work, out);
    }

    pub fn dimension(&self) -> usize {
        self.m
    }

    /// Pivots applied since the last refactorization.
    pub fn updates(&self) -> usize {
        self.etas.len() + self.post.len()
    }

    /// Whether the eta file has cost enough to be worth rebuilding.
    ///
    /// Every solve replays the whole eta file, and the file grows by a pivot's worth each
    /// iteration, so the work it adds over a run of `k` pivots grows as `k^2` while a
    /// refactorization costs the same whenever it happens. The average cost per pivot is
    /// least when the replay work accumulated since the last refactorization reaches
    /// that refactorization's own cost, which is the test, at `growth` 1.
    ///
    /// Self-tuning, where a fixed pivot count cannot be: the same interval that suits
    /// one model starves another. On MIPLIB's neos-555001, 3474 rows, an interval of 200
    /// pivots managed 328 nodes in twenty seconds where 50 managed 700, and on
    /// neos-3048764-nadi, 3186 rows, the ordering is the other way round.
    ///
    /// It compared the eta file's size with the factors' until the factors' size turned
    /// out to say little about their cost; see [`Lu::cost`].
    pub fn eta_file_is_expensive(&self, growth: f64) -> bool {
        self.replayed as f64 > growth * self.lu.cost() as f64
    }

    /// Nonzeros held in the factors and the updates on top of them, for diagnostics.
    pub fn nnz(&self) -> usize {
        self.lu.nnz()
            + self
                .etas
                .iter()
                .chain(&self.post)
                .map(|e| e.column.len() + 1)
                .sum::<usize>()
            + self.ext.iter().map(|e| e.row.len() + 1).sum::<usize>()
    }

    /// FTRAN: solve `B d = a`, returning `d = B^-1 a`.
    pub fn ftran(&self, a: &[f64], out: &mut Vec<f64>) {
        let b = self.base_dim();
        self.ftran_base(&a[..b], out);
        // Forward substitution through the block: `S d2 = a2 - R_B d1`.
        for (i, e) in self.ext.iter().enumerate() {
            let correction: f64 = e.row.iter().map(|&(p, v)| v * out[p]).sum();
            out.push((a[b + i] - correction) / e.diagonal);
        }
        for eta in &self.post {
            let scaled = out[eta.row] / eta.pivot;
            out[eta.row] = scaled;
            if scaled != 0.0 {
                for &(i, v) in &eta.column {
                    out[i] -= v * scaled;
                }
            }
        }
    }

    /// BTRAN: solve `B^T y = c`, returning `y = B^-T c`.
    ///
    /// Transposing the block form turns it upper triangular, so the appended rows are
    /// solved first and their result corrects the right-hand side of the base solve --
    /// the mirror of the forward substitution in [`Basis::ftran`].
    pub fn btran(&self, c: &[f64], out: &mut Vec<f64>) {
        if self.ext.is_empty() && self.post.is_empty() {
            self.btran_base(c, out);
            return;
        }
        let mut work = c.to_vec();
        // Transposed etas, newest first: only the pivot row's entry changes.
        for eta in self.post.iter().rev() {
            let mut acc = work[eta.row];
            for &(i, v) in &eta.column {
                acc -= v * work[i];
            }
            work[eta.row] = acc / eta.pivot;
        }
        if self.ext.is_empty() {
            self.btran_base(&work, out);
            return;
        }
        let b = self.base_dim();
        let mut base_rhs = work[..b].to_vec();
        let mut appended = Vec::with_capacity(self.ext.len());
        for (i, e) in self.ext.iter().enumerate() {
            let y = work[b + i] / e.diagonal;
            if y != 0.0 {
                for &(p, v) in &e.row {
                    base_rhs[p] -= v * y;
                }
            }
            appended.push(y);
        }
        self.btran_base(&base_rhs, out);
        out.extend_from_slice(&appended);
    }

    /// BTRAN against a unit vector: row `r` of `B^-1`.
    ///
    /// The dual simplex prices a whole pivot row at once and needs this. With an
    /// explicit inverse it was a row copy; here it is an ordinary BTRAN, which is
    /// why it belongs behind the interface rather than being read off the
    /// representation by the caller.
    pub fn btran_unit(&self, r: usize, out: &mut Vec<f64>) {
        let mut unit = vec![0.0; self.m];
        unit[r] = 1.0;
        self.btran(&unit, out);
    }

    /// Record a pivot on row `r`, where `d = B^-1 a_q` for the entering column.
    ///
    /// The caller is responsible for having rejected a pivot too small to be safe.
    pub fn update(&mut self, d: &[f64], r: usize) {
        debug_assert_eq!(d.len(), self.m);
        if self.m == 0 {
            return;
        }
        let pivot = d[r];
        debug_assert!(pivot != 0.0, "pivot on a zero element");
        let column = d
            .iter()
            .enumerate()
            .filter(|&(i, &v)| i != r && v.abs() > ZERO_TOL)
            .map(|(i, &v)| (i, v))
            .collect();
        let eta = Eta {
            row: r,
            pivot,
            column,
        };
        self.eta_nnz += eta.column.len() + 1;
        self.replayed += SOLVES_PER_PIVOT * self.eta_nnz;
        // A pivot taken after the basis grew sits above the extension, not beneath it.
        if self.ext.is_empty() {
            self.etas.push(eta);
        } else {
            self.post.push(eta);
        }
    }

    /// Refactorize from the basis columns, given as `(rows, values)` pairs.
    pub fn refactorize(
        &mut self,
        columns: &[(Vec<usize>, Vec<f64>)],
        _pivot_tol: f64,
        deadline: Option<std::time::Instant>,
    ) -> Result<(), BasisError> {
        if self.m == 0 {
            self.etas.clear();
            self.ext.clear();
            self.post.clear();
            self.eta_nnz = 0;
            self.replayed = 0;
            return Ok(());
        }
        match Lu::factor_with(self.m, columns, ZERO_TOL, deadline, &mut self.scratch) {
            Ok(lu) => {
                self.lu = lu;
                self.etas.clear();
                // A fresh factorization covers the appended rows like any other.
                self.ext.clear();
                self.post.clear();
                self.eta_nnz = 0;
                self.replayed = 0;
                Ok(())
            }
            Err(FactorError::Singular { position }) => Err(BasisError::Singular { row: position }),
            Err(FactorError::OutOfTime) => Err(BasisError::OutOfTime),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Build the grown basis matrix explicitly: the old columns gain their entries in
    /// the appended rows, and each appended row gets a logical column of -1.
    fn grown(dense: &[Vec<f64>], rows: &[Vec<(usize, f64)>], basic: &[usize]) -> Vec<Vec<f64>> {
        let b = dense.len();
        let k = rows.len();
        let mut out: Vec<Vec<f64>> = dense
            .iter()
            .enumerate()
            .map(|(p, col)| {
                let mut c = col.clone();
                c.resize(b + k, 0.0);
                for (i, row) in rows.iter().enumerate() {
                    // `basic[p]` is the variable in position `p`; structural columns
                    // are numbered below `dense.len()` in these tests.
                    for &(j, v) in row {
                        if j == basic[p] {
                            c[b + i] = v;
                        }
                    }
                }
                c
            })
            .collect();
        for i in 0..k {
            let mut logical = vec![0.0; b + k];
            logical[b + i] = -1.0;
            out.push(logical);
        }
        out
    }

    #[test]
    fn an_extended_basis_inverts_the_grown_matrix() {
        // A base that is not triangular, so the LU actually has to do something.
        let dense = vec![
            vec![2.0, 1.0, 0.0],
            vec![1.0, 3.0, 1.0],
            vec![0.0, 1.0, 2.0],
        ];
        let basic = vec![0usize, 1, 2];
        let mut basis = Basis::all_logical(3);
        basis.refactorize(&sparse(3, &dense), 1e-9, None).unwrap();
        assert_inverts(&basis, &dense);

        // Two appended rows touching different subsets of the basis.
        let rows = vec![vec![(0usize, 1.0), (2, 3.0)], vec![(1usize, -2.0)]];
        basis.extend(&rows, &basic, 3);
        assert_eq!(basis.dimension(), 5);
        assert_inverts(&basis, &grown(&dense, &rows, &basic));
    }

    /// The extension has to survive later pivots, which is why they layer above it
    /// rather than joining the etas beneath.
    #[test]
    fn an_extended_basis_still_inverts_after_a_pivot() {
        let dense = vec![vec![2.0, 1.0], vec![1.0, 3.0]];
        let basic = vec![0usize, 1];
        let mut basis = Basis::all_logical(2);
        basis.refactorize(&sparse(2, &dense), 1e-9, None).unwrap();

        let rows = vec![vec![(0usize, 1.0), (1, 1.0)]];
        basis.extend(&rows, &basic, 2);
        let mut grown_cols = grown(&dense, &rows, &basic);

        // Swap a new column into position 1, exactly as a pivot would.
        let entering = vec![1.0, -1.0, 2.0];
        let mut d = Vec::new();
        basis.ftran(&entering, &mut d);
        basis.update(&d, 1);
        grown_cols[1] = entering;

        assert_inverts(&basis, &grown_cols);
    }

    /// `B^-1 B` must be the identity, checked column by column.
    fn assert_inverts(basis: &Basis, dense: &[Vec<f64>]) {
        let m = basis.dimension();
        let mut out = Vec::new();
        for (j, col) in dense.iter().enumerate() {
            basis.ftran(col, &mut out);
            for (i, &got) in out.iter().enumerate().take(m) {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((got - expected).abs() < 1e-9, "B^-1 B [{i}][{j}] = {got}");
            }
        }
    }

    #[test]
    fn logical_basis_is_negative_identity() {
        let b = Basis::all_logical(3);
        let mut out = Vec::new();
        b.ftran(&[1.0, 2.0, 3.0], &mut out);
        assert_eq!(out, vec![-1.0, -2.0, -3.0]);
        b.btran(&[1.0, 2.0, 3.0], &mut out);
        assert_eq!(out, vec![-1.0, -2.0, -3.0]);
    }

    #[test]
    fn refactorize_inverts() {
        let dense = vec![
            vec![2.0, 1.0, 1.0],
            vec![1.0, 3.0, 2.0],
            vec![1.0, 0.0, 4.0],
        ];
        let mut b = Basis::all_logical(3);
        b.refactorize(&sparse(3, &dense), 1e-9, None).unwrap();
        assert_inverts(&b, &dense);
        assert_eq!(b.updates(), 0);
    }

    #[test]
    fn refactorize_reports_a_singular_basis() {
        let dense = vec![
            vec![1.0, 0.0, 1.0],
            vec![0.0, 1.0, 1.0],
            vec![1.0, 1.0, 2.0],
        ];
        let mut b = Basis::all_logical(3);
        assert!(matches!(
            b.refactorize(&sparse(3, &dense), 1e-9, None),
            Err(BasisError::Singular { .. })
        ));
    }

    #[test]
    fn update_matches_a_refactorization_of_the_same_basis() {
        // Replacing a basis column must give the same inverse whether reached by a
        // product-form update or by refactorizing the new basis outright. This is
        // the property the whole eta file rests on.
        let mut dense = vec![
            vec![2.0, 1.0, 1.0],
            vec![1.0, 3.0, 2.0],
            vec![1.0, 0.0, 4.0],
        ];
        let entering = vec![3.0, -1.0, 2.0];

        let mut updated = Basis::all_logical(3);
        updated.refactorize(&sparse(3, &dense), 1e-9, None).unwrap();
        let mut d = Vec::new();
        updated.ftran(&entering, &mut d);
        updated.update(&d, 1);

        dense[1] = entering;
        assert_inverts(&updated, &dense);
        assert_eq!(updated.updates(), 1);

        let mut fresh = Basis::all_logical(3);
        fresh.refactorize(&sparse(3, &dense), 1e-9, None).unwrap();
        let probe = [1.0, -2.0, 0.5];
        let (mut a, mut b) = (Vec::new(), Vec::new());
        updated.ftran(&probe, &mut a);
        fresh.ftran(&probe, &mut b);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-9, "{x} vs {y}");
        }
        updated.btran(&probe, &mut a);
        fresh.btran(&probe, &mut b);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-9, "{x} vs {y}");
        }
    }

    #[test]
    fn many_updates_stay_consistent() {
        // The eta file must remain correct as it grows, not just for one pivot.
        let mut dense = vec![
            vec![4.0, 1.0, 0.0, 1.0],
            vec![1.0, 3.0, 1.0, 0.0],
            vec![0.0, 1.0, 5.0, 1.0],
            vec![1.0, 0.0, 1.0, 2.0],
        ];
        let mut basis = Basis::all_logical(4);
        basis.refactorize(&sparse(4, &dense), 1e-9, None).unwrap();

        for (step, replacement) in [
            (0usize, vec![1.0, 2.0, 0.0, 1.0]),
            (2, vec![0.0, 1.0, 3.0, 1.0]),
            (1, vec![2.0, 0.0, 1.0, 1.0]),
            (3, vec![1.0, 1.0, 1.0, 4.0]),
        ] {
            let mut d = Vec::new();
            basis.ftran(&replacement, &mut d);
            basis.update(&d, step);
            dense[step] = replacement;
            assert_inverts(&basis, &dense);
        }
        assert_eq!(basis.updates(), 4);
    }

    #[test]
    fn btran_unit_is_a_row_of_the_inverse() {
        let dense = vec![vec![2.0, 1.0], vec![1.0, 3.0]];
        let mut b = Basis::all_logical(2);
        b.refactorize(&sparse(2, &dense), 1e-9, None).unwrap();
        for r in 0..2 {
            let mut rho = Vec::new();
            b.btran_unit(r, &mut rho);
            for (j, col) in dense.iter().enumerate() {
                let dot: f64 = rho.iter().zip(col).map(|(a, b)| a * b).sum();
                let expected = if j == r { 1.0 } else { 0.0 };
                assert!((dot - expected).abs() < 1e-9, "row {r}, column {j}: {dot}");
            }
        }
    }

    #[test]
    fn a_zero_row_basis_is_handled() {
        // A model with no rows has an empty basis; nothing here may panic on it.
        let mut b = Basis::all_logical(0);
        assert!(b.refactorize(&[], 1e-9, None).is_ok());
        let mut out = Vec::new();
        b.ftran(&[], &mut out);
        assert!(out.is_empty());
        b.update(&[], 0);
        assert_eq!(b.updates(), 0);
    }
}
