//! Solver trait, options, warm starts and results.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result, ensure};
use crate::model::{Model, Sense};

/// How a solve ended: the termination reason, independent of candidate
/// quality.
///
/// A status says why the backend stopped. Whether a candidate exists and passed
/// the independent check is in [`SolveResult::candidate`] and
/// [`SolveResult::evaluation`]: a limit can end a solve with a feasible
/// incumbent, and a solve without candidate can still carry a proven bound.
/// Backends map every native status explicitly and keep the native name in
/// [`BackendReport::raw_status`]; they never report a more precise outcome
/// than the engine did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// The dummy backend evaluated one candidate and stopped. Not a claim of
    /// feasibility, optimality, infeasibility or unboundedness.
    Stub,
    /// Stopped by the cancellation token before the engine finished.
    Cancelled,
    /// The engine reported optimality under its effective tolerances, and the
    /// adapter's independent checks passed: the candidate is a feasible
    /// candidate in original units, and for an LP with duals the relative
    /// primal-dual objective gap, stationarity and dual signs are within the
    /// backend's documented thresholds. A numerical statement, not an exact
    /// certificate.
    Optimal,
    /// The engine reported the model infeasible.
    Infeasible,
    /// The engine reported the model unbounded. A returned candidate is only a
    /// feasible point.
    Unbounded,
    /// The engine established that the model is infeasible or unbounded, but
    /// not which.
    InfeasibleOrUnbounded,
    /// A time limit stopped the engine.
    TimeLimit,
    /// A branch-and-bound node limit stopped the engine.
    NodeLimit,
    /// An LP iteration limit stopped the engine.
    IterationLimit,
    /// A limit on the number of solutions (or a native limit the engine
    /// reports the same way) stopped the engine.
    SolutionLimit,
    /// An objective bound or target stopped the engine.
    ObjectiveLimit,
    /// A memory limit stopped the engine.
    MemoryLimit,
    /// The engine failed numerically, or it reported optimality but the
    /// independent checks rejected the result. The message says which.
    NumericalFailure,
    /// The engine stopped without a definite outcome.
    Unknown,
}

impl Status {
    /// Stable lowercase name, as used by the Python binding.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Stub => "stub",
            Status::Cancelled => "cancelled",
            Status::Optimal => "optimal",
            Status::Infeasible => "infeasible",
            Status::Unbounded => "unbounded",
            Status::InfeasibleOrUnbounded => "infeasible_or_unbounded",
            Status::TimeLimit => "time_limit",
            Status::NodeLimit => "node_limit",
            Status::IterationLimit => "iteration_limit",
            Status::SolutionLimit => "solution_limit",
            Status::ObjectiveLimit => "objective_limit",
            Status::MemoryLimit => "memory_limit",
            Status::NumericalFailure => "numerical_failure",
            Status::Unknown => "unknown",
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
    /// Maximum numerical kernel workers,1 through8. Backends may reject
    /// widths they do not implement; native defaults to one.
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
        if self.threads > 8 {
            return Err(Error::Unsupported(format!(
                "threads = {}: maximum numerical budget is8",
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
    /// `bound` (a lower bound when minimizing, an upper bound when maximizing),
    /// with IEEE arithmetic: an overflow shows up as an infinite value.
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

    /// [`Gap::new`] for external data: rejects non-finite inputs and an
    /// overflowing result with [`Error::Evaluation`]. The infinite relative gap
    /// at a zero objective stays intentional.
    pub fn try_new(sense: Sense, objective: f64, bound: f64) -> Result<Self> {
        crate::error::finite(objective, "gap objective")?;
        crate::error::finite(bound, "gap bound")?;
        let gap = Self::new(sense, objective, bound);
        crate::error::finite(gap.absolute, "absolute gap")?;
        if objective != 0.0 {
            crate::error::finite(gap.relative, "relative gap")?;
        }
        Ok(gap)
    }
}

/// Outcome of a solve that was not aborted by an error.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveResult {
    /// How the solve ended (see [`Status`]).
    pub status: Status,
    /// The returned point (for a MIP: the incumbent). `None` if the backend
    /// declared no usable primal solution. A solve stopped by a limit or by
    /// cancellation keeps the best candidate found so far.
    pub candidate: Option<Vec<f64>>,
    /// Independent evaluation of `candidate`; present exactly when it is.
    pub evaluation: Option<Evaluation>,
    /// LP duals, when the backend computes them (never for a MIP).
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
    /// Backend provenance, applied settings and phase timings.
    pub backend: BackendReport,
}

impl SolveResult {
    /// A result without candidate, duals or bound.
    pub fn empty(status: Status, message: impl Into<String>, backend: BackendReport) -> Self {
        Self {
            status,
            candidate: None,
            evaluation: None,
            duals: None,
            best_bound: None,
            nodes: 0,
            iterations: 0,
            message: message.into(),
            backend,
        }
    }

    /// The [`Gap`] between the candidate and `best_bound`, if both exist and
    /// the candidate passed the feasibility check. `sense` is the model's.
    ///
    /// Computed with [`Gap::try_new`]: an overflow is an [`Error::Evaluation`].
    pub fn gap(&self, sense: Sense) -> Result<Option<Gap>> {
        match (&self.evaluation, self.best_bound) {
            (Some(e), Some(bound)) if e.feasible_candidate => {
                Gap::try_new(sense, e.objective, bound).map(Some)
            }
            _ => Ok(None),
        }
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

/// What happened to a [`WarmStart`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StartStatus {
    /// No start was given.
    #[default]
    NotProvided,
    /// The engine accepted the start for consideration. This does not mean it
    /// became the incumbent.
    Submitted,
    /// The engine refused the start.
    Rejected,
    /// The backend has no use for a start (the dummy evaluates it directly).
    Evaluated,
}

impl StartStatus {
    /// Stable lowercase name, as used by the Python binding.
    pub fn as_str(self) -> &'static str {
        match self {
            StartStatus::NotProvided => "not_provided",
            StartStatus::Submitted => "submitted",
            StartStatus::Rejected => "rejected",
            StartStatus::Evaluated => "evaluated",
        }
    }
}

/// Provenance and timings of one backend invocation.
///
/// Timings are wall-clock measurements for reporting; correctness never
/// depends on them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendReport {
    /// Backend name (`dummy`, `native`).
    pub name: String,
    /// Version of the native engine, if there is one.
    pub version: Option<String>,
    /// The engine's own status name, unmodified.
    pub raw_status: Option<String>,
    /// What happened to the start.
    pub start_status: StartStatus,
    /// Settings applied to the engine, as `(native name, value)` in the order
    /// they were set. Not an export of every native default.
    pub effective_options: Vec<(String, String)>,
    /// Whole call after input validation: conversion, engine and checks.
    pub total_seconds: f64,
    /// Model conversion and engine setup.
    pub conversion_seconds: f64,
    /// Engine run.
    pub solve_seconds: f64,
    /// Result extraction and independent checks.
    pub check_seconds: f64,
    /// The call took longer than its time budget. Limits are cooperative: the
    /// status still reports why the engine stopped.
    pub time_budget_exceeded: bool,
}

impl BackendReport {
    /// A report for the backend `name`, without engine data yet.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}
