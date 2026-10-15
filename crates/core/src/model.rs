//! The validated LP / mixed-integer LP model.

use crate::error::{Result, ensure, ensure_finite};
use crate::sparse::{Bounds, CsrMatrix};

/// Direction of optimization. Objective values, bounds and duals are always
/// reported in this original sense; there is no hidden sign change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sense {
    /// Minimize the objective.
    #[default]
    Minimize,
    /// Maximize the objective.
    Maximize,
}

impl Sense {
    /// Stable lowercase name, as used by the Python binding.
    pub fn as_str(self) -> &'static str {
        match self {
            Sense::Minimize => "minimize",
            Sense::Maximize => "maximize",
        }
    }
}

/// Domain of a variable within its bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VariableKind {
    /// Any real value.
    #[default]
    Continuous,
    /// An integer value.
    Integer,
    /// An integer value with bounds inside `[0, 1]`.
    Binary,
}

impl VariableKind {
    /// Stable lowercase name, as used by the Python binding.
    pub fn as_str(self) -> &'static str {
        match self {
            VariableKind::Continuous => "continuous",
            VariableKind::Integer => "integer",
            VariableKind::Binary => "binary",
        }
    }

    /// Whether the variable must take an integer value.
    pub fn is_integral(self) -> bool {
        self != VariableKind::Continuous
    }
}

/// The parts of a model, before validation. Turn it into a [`Model`] with
/// [`Model::new`].
///
/// ```text
/// minimize or maximize   objective_offset + objective^T x
/// subject to             rows.lower      <= matrix x <= rows.upper
///                        variables.lower <=    x     <= variables.upper
///                        x[j] integer where kinds[j] is Integer or Binary
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ModelData {
    /// Optimization direction.
    pub sense: Sense,
    /// Objective coefficients `c`, length `n`.
    pub objective: Vec<f64>,
    /// Constant added to the objective.
    pub objective_offset: f64,
    /// Variable bounds, length `n`.
    pub variables: Bounds,
    /// Variable domains, length `n`.
    pub kinds: Vec<VariableKind>,
    /// Constraint matrix `A`, `m x n`.
    pub matrix: CsrMatrix,
    /// Row bounds, length `m`.
    pub rows: Bounds,
}

/// A validated LP or mixed-integer LP.
///
/// A `Model` can only be obtained through [`Model::new`], so every instance
/// satisfies the invariants below and solvers do not re-validate it. It is
/// immutable and can be shared (for example behind an `Arc`) by any number of
/// independent solves.
///
/// Invariants: `n >= 1` variables and `m >= 0` rows; finite objective
/// coefficients, offset and matrix values; valid [`Bounds`] (infinite values
/// only as `-inf` lower or `+inf` upper bounds); binary bounds inside `[0, 1]`;
/// a canonical CSR matrix of shape `m x n` (see [`CsrPattern`](crate::CsrPattern)).
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    data: ModelData,
}

impl Model {
    /// Validates and wraps `data`. Errors name the offending field and entry.
    pub fn new(data: ModelData) -> Result<Self> {
        let n = data.objective.len();
        let m = data.rows.len();
        ensure(n > 0, || "at least one variable is required".into())?;
        ensure_finite(&data.objective, "objective")?;
        ensure(data.objective_offset.is_finite(), || {
            format!("objective offset is {}", data.objective_offset)
        })?;
        data.variables.validate(n, "variable bounds")?;
        ensure(data.kinds.len() == n, || {
            format!(
                "variable kinds: expected {n} entries, got {}",
                data.kinds.len()
            )
        })?;
        for (j, kind) in data.kinds.iter().enumerate() {
            if *kind == VariableKind::Binary {
                let (l, u) = (data.variables.lower[j], data.variables.upper[j]);
                ensure(l >= 0.0 && u <= 1.0, || {
                    format!("variable {j} is binary but has bounds [{l}, {u}] outside [0, 1]")
                })?;
            }
        }
        data.rows.validate(m, "row bounds")?;
        ensure(data.matrix.rows() == m && data.matrix.cols() == n, || {
            format!(
                "matrix: shape is {} x {}, expected rows x variables = {m} x {n}",
                data.matrix.rows(),
                data.matrix.cols()
            )
        })?;
        data.matrix.validate("matrix")?;
        Ok(Self { data })
    }

    /// Number of variables `n`.
    pub fn n(&self) -> usize {
        self.data.objective.len()
    }

    /// Number of rows `m`.
    pub fn m(&self) -> usize {
        self.data.rows.len()
    }

    /// Number of stored matrix entries.
    pub fn nnz(&self) -> usize {
        self.data.matrix.nnz()
    }

    /// Optimization direction.
    pub fn sense(&self) -> Sense {
        self.data.sense
    }

    /// Objective coefficients.
    pub fn objective(&self) -> &[f64] {
        &self.data.objective
    }

    /// Constant objective term.
    pub fn objective_offset(&self) -> f64 {
        self.data.objective_offset
    }

    /// Variable bounds.
    pub fn variables(&self) -> &Bounds {
        &self.data.variables
    }

    /// Variable domains.
    pub fn kinds(&self) -> &[VariableKind] {
        &self.data.kinds
    }

    /// Constraint matrix (`m x n`).
    pub fn matrix(&self) -> &CsrMatrix {
        &self.data.matrix
    }

    /// Row bounds.
    pub fn rows(&self) -> &Bounds {
        &self.data.rows
    }

    /// Whether any variable is integer or binary.
    pub fn is_mip(&self) -> bool {
        self.data.kinds.iter().any(|k| k.is_integral())
    }

    /// The validated parts.
    pub fn data(&self) -> &ModelData {
        &self.data
    }

    /// Gives the parts back, for example to modify and re-validate them.
    pub fn into_data(self) -> ModelData {
        self.data
    }
}
