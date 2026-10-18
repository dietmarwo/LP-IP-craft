//! The problem model: a mixed-integer program.
//!
//! Rows are stored in *range* form (`lb <= a'x <= ub`, with infinite bounds
//! allowed) rather than as a sense plus a right-hand side. That is what HiGHS and
//! other solvers keep internally, and it pays off twice: `<=`, `>=`, `=` and true range
//! rows all become one case for the simplex, and presolve's bound tightening is
//! just an update to `lb`/`ub` instead of a change of sense.
//!
//! Columns carry a type (continuous or integer) and their own bounds. A binary
//! variable is simply an integer one bounded to `[0, 1]`, which is why the solver
//! has no separate notion of it: the branching rule `x <= floor(v)` / `x >= ceil(v)`
//! degenerates to fixing at 0 or 1 on its own.
//!
//! Bounds are carried explicitly per column so that presolve fixing and branching
//! both express themselves as bound changes, with no separate mechanism.

use crate::sparse::SparseMatrix;

/// Whether a column must take an integer value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VarType {
    /// May take any value within its bounds.
    Continuous,
    /// Must be integral. A column bounded to `[0, 1]` is what other solvers call
    /// binary; nothing here treats that as a distinct case.
    #[default]
    Integer,
}

/// Optimization direction of the *original* problem as the user stated it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Minimize,
    Maximize,
}

/// A mixed-integer program.
///
/// The solver works internally in minimization form. [`Problem::objective_value`]
/// converts an internal objective back to the user's original sense and offset, so
/// callers never have to remember whether a negation happened.
#[derive(Clone, Debug)]
pub struct Problem {
    pub name: String,
    /// Direction the user asked for. Coefficients in `obj` are always in
    /// minimization form; for a maximization they were negated on the way in.
    pub sense: Sense,
    /// Objective coefficients, minimization form, one per column.
    pub obj: Vec<f64>,
    /// Constant term of the internal (minimization) objective.
    pub obj_offset: f64,
    /// Constraint matrix, `n_rows x n_cols`, column-major.
    pub matrix: SparseMatrix,
    pub row_lb: Vec<f64>,
    pub row_ub: Vec<f64>,
    pub col_lb: Vec<f64>,
    pub col_ub: Vec<f64>,
    /// Per-column integrality requirement.
    pub col_type: Vec<VarType>,
    pub col_names: Vec<String>,
    pub row_names: Vec<String>,
}

/// How a row was written before conversion to range form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowSense {
    /// `a'x <= rhs`
    Le,
    /// `a'x >= rhs`
    Ge,
    /// `a'x == rhs`
    Eq,
}

impl RowSense {
    /// The `(lb, ub)` range-form bounds for this sense and right-hand side.
    pub fn bounds(self, rhs: f64) -> (f64, f64) {
        match self {
            RowSense::Le => (f64::NEG_INFINITY, rhs),
            RowSense::Ge => (rhs, f64::INFINITY),
            RowSense::Eq => (rhs, rhs),
        }
    }
}

/// A row held while the model is being assembled: terms, then its two bounds.
type PendingRow = (Vec<(usize, f64)>, f64, f64);

/// Assembles a [`Problem`] a column and a row at a time.
///
/// [`Problem`] is a plain struct and can be filled in directly, but doing so means
/// keeping nine parallel vectors consistent and writing the objective in minimization
/// form by hand. This does both for you.
///
/// ```
/// use ripsolve::model::{Builder, RowSense, Sense};
/// use ripsolve::search;
///
/// // Maximize 3b + 2n subject to 2b + n <= 12, b binary and n integer in [0, 10].
/// let mut model = Builder::new(Sense::Maximize);
/// let b = model.binary("b");
/// let n = model.integer("n", 0.0, 10.0);
/// model.objective(&[(b, 3.0), (n, 2.0)]);
/// model.row(&[(b, 2.0), (n, 1.0)], RowSense::Le, 12.0);
/// let problem = model.build();
///
/// let solution = search::solve(&problem, search::Options::default());
/// assert_eq!(solution.objective, Some(23.0));
/// ```
pub struct Builder {
    sense: Sense,
    obj: Vec<f64>,
    col_lb: Vec<f64>,
    col_ub: Vec<f64>,
    col_type: Vec<VarType>,
    col_names: Vec<String>,
    rows: Vec<PendingRow>,
    row_names: Vec<String>,
    name: String,
}

impl Builder {
    pub fn new(sense: Sense) -> Self {
        Self {
            sense,
            obj: Vec::new(),
            col_lb: Vec::new(),
            col_ub: Vec::new(),
            col_type: Vec::new(),
            col_names: Vec::new(),
            rows: Vec::new(),
            row_names: Vec::new(),
            name: String::new(),
        }
    }

    /// Name the model, as it appears in [`Problem::name`].
    pub fn named(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Add a column, returning its index for use in rows and the objective.
    pub fn column(&mut self, name: &str, lb: f64, ub: f64, kind: VarType) -> usize {
        self.obj.push(0.0);
        self.col_lb.push(lb);
        self.col_ub.push(ub);
        self.col_type.push(kind);
        self.col_names.push(name.to_string());
        self.obj.len() - 1
    }

    /// A column restricted to `{0, 1}`.
    pub fn binary(&mut self, name: &str) -> usize {
        self.column(name, 0.0, 1.0, VarType::Integer)
    }

    /// A column restricted to the integers in `[lb, ub]`.
    pub fn integer(&mut self, name: &str, lb: f64, ub: f64) -> usize {
        self.column(name, lb, ub, VarType::Integer)
    }

    /// A column free to take any value in `[lb, ub]`.
    pub fn continuous(&mut self, name: &str, lb: f64, ub: f64) -> usize {
        self.column(name, lb, ub, VarType::Continuous)
    }

    /// Set the objective from `(column, coefficient)` pairs.
    ///
    /// Written in the sense given to [`Builder::new`]; the conversion to the solver's
    /// internal minimization form happens in [`Builder::build`].
    pub fn objective(&mut self, terms: &[(usize, f64)]) {
        for c in self.obj.iter_mut() {
            *c = 0.0;
        }
        for &(j, c) in terms {
            self.obj[j] += c;
        }
    }

    /// Add a row `terms <sense> rhs`, returning its index.
    pub fn row(&mut self, terms: &[(usize, f64)], sense: RowSense, rhs: f64) -> usize {
        let (lb, ub) = sense.bounds(rhs);
        self.range(terms, lb, ub)
    }

    /// Add a row in range form, `lb <= terms <= ub`, returning its index.
    pub fn range(&mut self, terms: &[(usize, f64)], lb: f64, ub: f64) -> usize {
        let i = self.rows.len();
        self.rows.push((terms.to_vec(), lb, ub));
        self.row_names.push(format!("c{i}"));
        i
    }

    /// Rename the most recently added row.
    pub fn row_named(&mut self, name: &str) {
        if let Some(last) = self.row_names.last_mut() {
            *last = name.to_string();
        }
    }

    /// Finish the model.
    ///
    /// The result is not validated; call [`Problem::validate`] for that.
    pub fn build(self) -> Problem {
        let n = self.obj.len();
        let m = self.rows.len();
        // The solver minimizes, so a maximization is negated on the way in and
        // `Problem::objective_value` negates the answer back.
        let flip = if self.sense == Sense::Maximize {
            -1.0
        } else {
            1.0
        };
        let triplets = self
            .rows
            .iter()
            .enumerate()
            .flat_map(|(i, (terms, _, _))| terms.iter().map(move |&(j, a)| (i, j, a)));
        Problem {
            name: self.name,
            sense: self.sense,
            obj: self.obj.iter().map(|c| c * flip).collect(),
            obj_offset: 0.0,
            matrix: SparseMatrix::from_triplets(m, n, triplets),
            row_lb: self.rows.iter().map(|r| r.1).collect(),
            row_ub: self.rows.iter().map(|r| r.2).collect(),
            col_lb: self.col_lb,
            col_ub: self.col_ub,
            col_type: self.col_type,
            col_names: self.col_names,
            row_names: self.row_names,
        }
    }
}

impl Problem {
    pub fn n_cols(&self) -> usize {
        self.obj.len()
    }

    pub fn n_rows(&self) -> usize {
        self.row_lb.len()
    }

    /// Convert an internal (minimization) objective value back to the value in the
    /// user's original sense, including the offset introduced during construction.
    pub fn objective_value(&self, internal: f64) -> f64 {
        let with_offset = internal + self.obj_offset;
        match self.sense {
            Sense::Minimize => with_offset,
            Sense::Maximize => -with_offset,
        }
    }

    /// Is this column required to be integral?
    pub fn is_integer(&self, j: usize) -> bool {
        self.col_type[j] == VarType::Integer
    }

    /// Is this column integral and bounded to `[0, 1]`?
    ///
    /// Several reductions (cover cuts, coefficient tightening) are stated for
    /// binary columns specifically, and check this rather than assuming it.
    pub fn is_binary(&self, j: usize) -> bool {
        self.is_integer(j) && self.col_lb[j] >= 0.0 && self.col_ub[j] <= 1.0
    }

    /// The columns whose values the search must drive to integers.
    pub fn integer_columns(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.n_cols()).filter(|&j| self.is_integer(j))
    }

    /// Check the internal invariants a freshly built problem must satisfy.
    ///
    /// Readers and presolve both go through this, so a malformed model is caught at
    /// the boundary rather than as a confusing failure deep in the simplex.
    pub fn validate(&self) -> Result<(), ModelError> {
        let (n, m) = (self.n_cols(), self.n_rows());
        if self.matrix.n_cols() != n || self.matrix.n_rows() != m {
            return Err(ModelError::ShapeMismatch {
                matrix: (self.matrix.n_rows(), self.matrix.n_cols()),
                vectors: (m, n),
            });
        }
        for (len, what) in [
            (self.col_lb.len(), "col_lb"),
            (self.col_ub.len(), "col_ub"),
            (self.col_type.len(), "col_type"),
            (self.col_names.len(), "col_names"),
        ] {
            if len != n {
                return Err(ModelError::LengthMismatch {
                    what,
                    got: len,
                    expected: n,
                });
            }
        }
        for (len, what) in [
            (self.row_ub.len(), "row_ub"),
            (self.row_names.len(), "row_names"),
        ] {
            if len != m {
                return Err(ModelError::LengthMismatch {
                    what,
                    got: len,
                    expected: m,
                });
            }
        }
        // A NaN bound would silently poison every comparison in the simplex, so it is
        // rejected here rather than allowed to propagate.
        for j in 0..n {
            let (lo, hi) = (self.col_lb[j], self.col_ub[j]);
            if lo.is_nan() || hi.is_nan() || lo > hi {
                return Err(ModelError::InvalidBounds {
                    what: "column",
                    index: j,
                    lb: lo,
                    ub: hi,
                });
            }
            // An integer column's bounds must themselves be integral, or branching
            // could never reach them: `x <= floor(2.5)` and `x >= ceil(2.5)` would
            // both exclude values the bounds admit.
            if self.col_type[j] == VarType::Integer {
                for bound in [lo, hi] {
                    if bound.is_finite() && (bound - bound.round()).abs() > 1e-9 {
                        return Err(ModelError::FractionalIntegerBound { index: j, bound });
                    }
                }
            }
        }
        for i in 0..m {
            let (lo, hi) = (self.row_lb[i], self.row_ub[i]);
            if lo.is_nan() || hi.is_nan() || lo > hi {
                return Err(ModelError::InvalidBounds {
                    what: "row",
                    index: i,
                    lb: lo,
                    ub: hi,
                });
            }
        }
        if self.obj.iter().any(|v| !v.is_finite()) || !self.obj_offset.is_finite() {
            return Err(ModelError::NonFiniteObjective);
        }
        Ok(())
    }
}

/// A malformed [`Problem`].
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("matrix is {}x{} but the model describes {}x{}", matrix.0, matrix.1, vectors.0, vectors.1)]
    ShapeMismatch {
        matrix: (usize, usize),
        vectors: (usize, usize),
    },
    #[error("{what} has length {got}, expected {expected}")]
    LengthMismatch {
        what: &'static str,
        got: usize,
        expected: usize,
    },
    #[error("{what} {index} has invalid bounds [{lb}, {ub}]")]
    InvalidBounds {
        what: &'static str,
        index: usize,
        lb: f64,
        ub: f64,
    },
    #[error("integer column {index} has the fractional bound {bound}")]
    FractionalIntegerBound { index: usize, bound: f64 },
    #[error("objective contains a non-finite coefficient")]
    NonFiniteObjective,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builder_negates_a_maximization_and_reports_it_back() {
        // The solver minimizes, so a maximization is stored negated. What the caller
        // wrote must come back out of `objective_value`, which is the part that is easy
        // to get backwards when filling the struct by hand.
        let mut model = Builder::new(Sense::Maximize);
        let x = model.binary("x");
        model.objective(&[(x, 7.0)]);
        let p = model.build();

        assert_eq!(p.obj, vec![-7.0], "stored in minimization form");
        assert_eq!(
            p.objective_value(-7.0),
            7.0,
            "reported in the caller's sense"
        );

        let mut same = Builder::new(Sense::Minimize);
        let y = same.binary("y");
        same.objective(&[(y, 7.0)]);
        assert_eq!(same.build().obj, vec![7.0], "a minimization is left alone");
    }

    #[test]
    fn the_builder_places_coefficients_where_it_says() {
        let mut model = Builder::new(Sense::Minimize).named("m");
        let a = model.continuous("a", 0.0, 10.0);
        let b = model.integer("b", -3.0, 3.0);
        model.row(&[(a, 2.0), (b, -1.0)], RowSense::Ge, 4.0);
        model.row_named("first");
        model.range(&[(b, 1.0)], -1.0, 2.0);
        let p = model.build();

        assert_eq!((p.n_cols(), p.n_rows()), (2, 2));
        assert_eq!(p.row_names, vec!["first".to_string(), "c1".to_string()]);
        // `Ge` becomes a range open at the top, and the explicit range is kept as given.
        assert_eq!(p.row_lb, vec![4.0, -1.0]);
        assert_eq!(p.row_ub, vec![f64::INFINITY, 2.0]);
        assert_eq!((p.col_lb[b], p.col_ub[b]), (-3.0, 3.0));
        assert!(p.is_integer(b) && !p.is_integer(a));
        assert!(!p.is_binary(b), "an integer in [-3, 3] is not binary");

        let csr = p.matrix.to_csr();
        let (cols, vals) = csr.column(0);
        assert_eq!(cols, &[a, b]);
        assert_eq!(vals, &[2.0, -1.0]);
        p.validate().expect("the assembled model is well formed");
    }

    #[test]
    fn setting_the_objective_twice_replaces_it() {
        // `objective` is a setter, not an accumulator, so a second call is a correction
        // rather than a doubling. Repeated columns within one call do accumulate.
        let mut model = Builder::new(Sense::Minimize);
        let x = model.binary("x");
        model.objective(&[(x, 1.0)]);
        model.objective(&[(x, 2.0), (x, 3.0)]);
        assert_eq!(model.build().obj, vec![5.0]);
    }

    fn tiny(sense: Sense, offset: f64) -> Problem {
        Problem {
            name: "tiny".into(),
            sense,
            obj: vec![1.0, 2.0],
            obj_offset: offset,
            matrix: SparseMatrix::from_triplets(1, 2, [(0, 0, 1.0), (0, 1, 1.0)]),
            row_lb: vec![1.0],
            row_ub: vec![f64::INFINITY],
            col_lb: vec![0.0, 0.0],
            col_ub: vec![1.0, 1.0],
            col_type: vec![VarType::Integer; 2],
            col_names: vec!["x0".into(), "x1".into()],
            row_names: vec!["c0".into()],
        }
    }

    #[test]
    fn row_sense_to_range_bounds() {
        assert_eq!(RowSense::Le.bounds(3.0), (f64::NEG_INFINITY, 3.0));
        assert_eq!(RowSense::Ge.bounds(3.0), (3.0, f64::INFINITY));
        assert_eq!(RowSense::Eq.bounds(3.0), (3.0, 3.0));
    }

    #[test]
    fn objective_value_undoes_negation_and_offset() {
        // A maximization is stored negated, so the round trip must flip it back.
        assert_eq!(tiny(Sense::Minimize, 5.0).objective_value(2.0), 7.0);
        assert_eq!(tiny(Sense::Maximize, 5.0).objective_value(2.0), -7.0);
    }

    #[test]
    fn validate_accepts_a_well_formed_problem() {
        assert!(tiny(Sense::Minimize, 0.0).validate().is_ok());
    }

    #[test]
    fn validate_accepts_a_general_integer_column() {
        let mut p = tiny(Sense::Minimize, 0.0);
        p.col_ub[1] = 4.0;
        assert!(
            p.validate().is_ok(),
            "a [0, 4] integer column is legitimate"
        );
    }

    #[test]
    fn validate_rejects_a_fractional_integer_bound() {
        // Branching splits at an integer, so a bound of 2.5 is unreachable from
        // either side and the model is ill-posed rather than merely unusual.
        let mut p = tiny(Sense::Minimize, 0.0);
        p.col_ub[1] = 2.5;
        assert!(matches!(
            p.validate(),
            Err(ModelError::FractionalIntegerBound { index: 1, .. })
        ));
        // The same bound on a continuous column is fine.
        p.col_type[1] = VarType::Continuous;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validate_rejects_crossed_and_nan_bounds() {
        let mut p = tiny(Sense::Minimize, 0.0);
        p.col_lb[0] = 1.0;
        p.col_ub[0] = 0.0;
        assert!(matches!(
            p.validate(),
            Err(ModelError::InvalidBounds { .. })
        ));

        let mut p = tiny(Sense::Minimize, 0.0);
        p.row_lb[0] = f64::NAN;
        assert!(matches!(
            p.validate(),
            Err(ModelError::InvalidBounds { .. })
        ));
    }

    #[test]
    fn validate_rejects_shape_and_length_errors() {
        let mut p = tiny(Sense::Minimize, 0.0);
        p.obj.push(3.0);
        assert!(matches!(
            p.validate(),
            Err(ModelError::ShapeMismatch { .. })
        ));

        let mut p = tiny(Sense::Minimize, 0.0);
        p.col_names.pop();
        assert!(matches!(
            p.validate(),
            Err(ModelError::LengthMismatch {
                what: "col_names",
                ..
            })
        ));
    }
}
