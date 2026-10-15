//! Solver trait, options, warm starts and results.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result, ensure};
use crate::model::{Model, Sense};

/// How a solve ended. More variants (optimal, infeasible, unbounded, limits)
/// will be added together with real backends, each with a precise definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// The dummy backend evaluated one candidate and stopped. Not a claim of
    /// feasibility, optimality, infeasibility or unboundedness.
    Stub,
    /// Stopped by the cancellation token.
    Cancelled,
}

impl Status {
    /// Stable lowercase name, as used by the Python binding.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Stub => "stub",
            Status::Cancelled => "cancelled",
        }
    }
}

/// Tolerances of the candidate check, absolute and in original, unscaled units.
///
/// These define [`Evaluation::feasible_candidate`]. Backends have their own,
/// differently scaled tolerances; comparisons between backends use these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerances {
    /// Largest accepted violation of a variable or row bound. Positive, finite.
    pub feasibility: f64,
    /// Largest accepted distance of an integer variable to the nearest
    /// integer. In `(0, 0.5)`.
    pub integrality: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            feasibility: 1e-7,
            integrality: 1e-7,
        }
    }
}

impl Tolerances {
    /// Rejects non-finite, non-positive or (for integrality) `>= 0.5` values.
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.feasibility.is_finite() && self.feasibility > 0.0,
            || {
                format!(
                    "feasibility tolerance must be positive and finite, got {}",
                    self.feasibility
                )
            },
        )?;
        ensure(
            self.integrality.is_finite() && self.integrality > 0.0 && self.integrality < 0.5,
            || {
                format!(
                    "integrality tolerance must lie in (0, 0.5), got {}",
                    self.integrality
                )
            },
        )
    }
}

/// Settings accepted by the solvers in this crate.
///
/// Only settings that a backend actually honours are exposed. Time, node and
/// gap limits, presolve, scaling and seeds will be added together with the
/// backend that implements them, so no setting is ever silently ignored.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Tolerances of the candidate check.
    pub tolerances: Tolerances,
    /// Threads used inside one solve. Only `1` is supported; run independent
    /// solves for parallelism.
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tolerances: Tolerances::default(),
            threads: 1,
        }
    }
}

impl Options {
    /// Rejects malformed values (`InvalidInput`) and settings no backend
    /// supports yet (`Unsupported`).
    pub fn validate(&self) -> Result<()> {
        self.tolerances.validate()?;
        ensure(self.threads > 0, || "threads must be at least 1".into())?;
        if self.threads != 1 {
            return Err(Error::Unsupported(format!(
                "threads = {}: only single-threaded solves are implemented; \
                 run independent solves for parallelism",
                self.threads
            )));
        }
        Ok(())
    }
}

/// A cooperative cancellation flag that can be shared across threads.
///
/// Solvers check it at documented checkpoints; it is not a hard interrupt or a
/// time limit. Cancellation is one-way: use a new token for a new run.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Requests cancellation. Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Information a solve may start from.
///
/// Currently a complete primal point only. Partial MIP starts, simplex bases
/// and dual starts will be added (this struct is `#[non_exhaustive]`) together
/// with a backend that uses them.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct WarmStart {
    /// A complete primal point, length `n`. It need not be feasible or
    /// integral, and is never rounded or repaired.
    pub primal: Vec<f64>,
}

impl WarmStart {
    /// A start from a complete primal point.
    pub fn from_primal(primal: Vec<f64>) -> Self {
        Self { primal }
    }
}

/// Independently recomputed values at a candidate point (see
/// [`evaluate`](crate::check::evaluate)).
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    /// `objective_offset + c^T x`, in the original sense.
    pub objective: f64,
    /// `A x`, length `m`.
    pub row_activity: Vec<f64>,
    /// Largest bound violation over variables and rows, absolute and
    /// unscaled; `0.0` if none.
    pub primal_violation: f64,
    /// Largest distance of an integer or binary variable to the nearest
    /// integer; `0.0` for a pure LP.
    pub integrality_violation: f64,
    /// Both violations are within the [`Tolerances`]. A numerical statement
    /// about this point only: neither optimality nor (if false) infeasibility
    /// of the model follows from it.
    pub feasible_candidate: bool,
}

/// LP dual values.
///
/// Sign convention, in the original objective sense: at an optimal LP solution
///
/// ```text
/// c = A^T row_duals + reduced_costs
/// ```
///
/// When minimizing, a positive dual belongs to an active lower bound and a
/// negative one to an active upper bound; when maximizing the signs are
/// reversed. Zero means inactive; equalities accept either sign. This is the
/// convention of Gurobi's `Pi`/`RC` and of HiGHS's `row_dual`/`col_dual`;
/// [`lp_residuals`](crate::check::lp_residuals) checks it.
///
/// Duals refer to an LP. For a MIP they can only describe an LP relaxation or
/// a fixed-integer LP, and a backend must say which.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Duals {
    /// One per row (length `m`).
    pub row_duals: Vec<f64>,
    /// One per variable (length `n`).
    pub reduced_costs: Vec<f64>,
}

/// Optimality gap between a feasible candidate and a bound, both in the
/// original objective sense and including the offset.
///
/// One definition for every backend (solvers define the relative gap
/// differently, so a backend's own value is not reported):
///
/// ```text
/// absolute = objective - bound   (minimize)      bound - objective   (maximize)
/// relative = absolute / |objective|;  if objective == 0: 0 if absolute == 0, else ±inf
/// ```
///
/// The quotient is the one of Gurobi's `MIPGap`; SCIP's default gap divides by
/// the smaller of the two magnitudes instead. `absolute` keeps its sign: it is
/// non-negative for a valid bound and a feasible candidate, and a negative
/// value (an invalid bound) is reported, not clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    /// Objective distance to the bound.
    pub absolute: f64,
    /// `absolute` relative to the candidate objective.
    pub relative: f64,
}

impl Gap {
    /// The gap of a candidate with objective value `objective` against
    /// `bound` (a lower bound when minimizing, an upper bound when maximizing).
    pub fn new(sense: Sense, objective: f64, bound: f64) -> Self {
        let absolute = match sense {
            Sense::Minimize => objective - bound,
            Sense::Maximize => bound - objective,
        };
        let relative = if objective != 0.0 {
            absolute / objective.abs()
        } else if absolute == 0.0 {
            0.0
        } else {
            f64::INFINITY.copysign(absolute)
        };
        Self { absolute, relative }
    }
}

/// Outcome of a solve that was not aborted by an error.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveResult {
    /// How the solve ended.
    pub status: Status,
    /// The returned point (for a MIP: the incumbent). `None` if no complete
    /// evaluation happened. A cancelled solve keeps the best candidate found
    /// so far, as MIP solvers keep their incumbent when interrupted.
    pub candidate: Option<Vec<f64>>,
    /// Independent evaluation of `candidate`; present exactly when it is.
    pub evaluation: Option<Evaluation>,
    /// LP duals, when the backend computes them.
    pub duals: Option<Duals>,
    /// A proven bound on the optimal objective in the original sense (a lower
    /// bound when minimizing, an upper bound when maximizing), including the
    /// offset. `None` unless the backend proves one; never estimated.
    pub best_bound: Option<f64>,
    /// Branch-and-bound nodes processed.
    pub nodes: u64,
    /// LP iterations (simplex pivots or interior-point steps), all nodes.
    pub iterations: u64,
    /// Human-readable explanation of `status`.
    pub message: String,
}

impl SolveResult {
    /// The [`Gap`] between the candidate and `best_bound`, if both exist and
    /// the candidate passed the feasibility check. `sense` is the model's.
    pub fn gap(&self, sense: Sense) -> Option<Gap> {
        let evaluation = self.evaluation.as_ref()?;
        let bound = self.best_bound?;
        evaluation
            .feasible_candidate
            .then(|| Gap::new(sense, evaluation.objective, bound))
    }
}

/// An LP / MIP solver: the replacement point for backends (HiGHS, SCIP, a
/// native component).
///
/// `&mut self` lets a solver keep reusable workspace across solves; a solver
/// instance serves one solve at a time. Independent solves may share one
/// [`Model`].
pub trait LpIpSolver {
    /// Solves `model`, optionally from `start`.
    ///
    /// Malformed options or start data are rejected with
    /// [`Error::InvalidInput`] before any work, even if `cancel` is already set.
    fn solve(
        &mut self,
        model: &Model,
        options: &Options,
        start: Option<&WarmStart>,
        cancel: &CancellationToken,
    ) -> Result<SolveResult>;
}
