//! HiGHS backend for the `lp-ip-craft` contracts.
//!
//! [`HighsSolver`] implements [`LpIpSolver`] through the `highs` Rust bindings
//! (pinned 2.4.0, HiGHS 1.15.0 built from source by `highs-sys`). HiGHS does
//! the LP/MIP search; this crate owns validation, conversion, settings,
//! cancellation, status mapping and the independent acceptance checks. It is
//! an adapter, not a native Rust LP/MIP engine.
//!
//! Per solve, the adapter
//!
//! 1. validates settings, backend limits and the start (before any work);
//! 2. builds a fresh HiGHS model from the validated [`Model`], with sense,
//!    offset and integrality, and applies only the typed settings below;
//! 3. submits the start, installs the interrupt callback and runs HiGHS;
//! 4. maps the native status explicitly (see [`Status`]), evaluates any
//!    declared primal solution with [`check::evaluate`], checks LP duals with
//!    [`check::lp_residuals`], and reads the MIP dual bound.
//!
//! A reported optimum that fails the independent checks becomes
//! [`Status::NumericalFailure`], with the reason in the message.
#![warn(missing_docs)]

mod ffi;
mod status;

use std::time::Instant;

use highs::{HighsOptionValue, HighsSolutionStatus, RowProblem};
use lp_ip_craft::check::{self, LpResiduals};
use lp_ip_craft::{
    BackendReport, CancellationToken, Duals, Error, LpIpSolver, Model, Options, Result, Sense,
    SolveResult, StartStatus, Status, WarmStart,
};
use status::Mapped;

/// HiGHS's threshold for infinite costs and bounds (`infinite_cost`,
/// `infinite_bound`): finite values at or above it would be read as infinite.
pub const HIGHS_INFINITY: f64 = 1e20;
/// Largest accepted matrix magnitude (exclusive): HiGHS's `large_matrix_value`.
pub const MAX_MATRIX_VALUE: f64 = 1e15;
/// Smallest accepted nonzero matrix magnitude (exclusive). The adapter sets
/// HiGHS's `small_matrix_value` to this minimum, so no stored coefficient is
/// dropped silently.
pub const MIN_MATRIX_VALUE: f64 = 1e-12;
/// Largest integer magnitude whose neighbours are representable in `f64`.
pub const MAX_INTEGER_MAGNITUDE: f64 = 9_007_199_254_740_991.0;
/// HiGHS's smallest accepted feasibility tolerance.
pub const MIN_TOLERANCE: f64 = 1e-10;

/// Settings of the HiGHS backend.
///
/// The defaults form a deterministic single-thread reference configuration.
/// Every field is applied to HiGHS or rejected; none is ignored. The applied
/// native names and values are listed in [`BackendReport::effective_options`].
#[derive(Clone, Debug, PartialEq)]
pub struct HighsOptions {
    /// Cooperative budget for the whole call, measured from the end of input
    /// validation: conversion, engine run and checks. HiGHS gets the remaining
    /// time as `time_limit`, and the interrupt callback polls the budget. Not a
    /// hard real-time deadline; an overrun is reported in
    /// [`BackendReport::time_budget_exceeded`].
    pub time_limit_seconds: Option<f64>,
    /// Branch-and-bound node limit (`mip_max_nodes`). MIP models only; an LP
    /// with a node limit is rejected because HiGHS would ignore it.
    pub node_limit: Option<u32>,
    /// Simplex and interior-point iteration limits (`simplex_iteration_limit`,
    /// `ipm_iteration_limit`). LP models only: HiGHS does not apply them to the
    /// LP subproblems of a MIP, so a MIP with an iteration limit is rejected.
    pub iteration_limit: Option<u32>,
    /// `random_seed`, `0..=i32::MAX`.
    pub seed: u32,
    /// HiGHS's MIP stopping criterion `mip_rel_gap`, `|ub - lb| / |ub|`.
    /// A native stopping rule only: the result's gap is always the facade's
    /// [`Gap`](lp_ip_craft::Gap). Default `0` (HiGHS's own default is `1e-4`).
    pub mip_relative_gap: f64,
    /// HiGHS's `mip_abs_gap`. Default `0` (HiGHS's own default is `1e-6`).
    pub mip_absolute_gap: f64,
    /// HiGHS's `dual_feasibility_tolerance`, and the threshold of the LP
    /// optimality checks: stationarity and dual signs up to
    /// `dual_tolerance * (1 + ‖c‖∞)`, relative duality gap up to
    /// `dual_tolerance`. At least `1e-10`.
    pub dual_tolerance: f64,
}

impl Default for HighsOptions {
    fn default() -> Self {
        Self {
            time_limit_seconds: None,
            node_limit: None,
            iteration_limit: None,
            seed: 0,
            mip_relative_gap: 0.0,
            mip_absolute_gap: 0.0,
            dual_tolerance: 1e-7,
        }
    }
}

impl HighsOptions {
    /// Rejects values HiGHS cannot take, naming the field.
    pub fn validate(&self) -> Result<()> {
        let invalid = |m: String| Err(Error::InvalidInput(m));
        if let Some(t) = self.time_limit_seconds {
            if !(t.is_finite() && t >= 0.0) {
                return invalid(format!(
                    "time_limit_seconds must be finite and >= 0, got {t}"
                ));
            }
        }
        for (name, value) in [
            ("mip_relative_gap", self.mip_relative_gap),
            ("mip_absolute_gap", self.mip_absolute_gap),
        ] {
            if !(value.is_finite() && value >= 0.0) {
                return invalid(format!("{name} must be finite and >= 0, got {value}"));
            }
        }
        if !(self.dual_tolerance.is_finite() && self.dual_tolerance >= MIN_TOLERANCE) {
            return invalid(format!(
                "dual_tolerance must be finite and >= {MIN_TOLERANCE}, got {}",
                self.dual_tolerance
            ));
        }
        for (name, value) in [
            ("seed", Some(self.seed)),
            ("node_limit", self.node_limit),
            ("iteration_limit", self.iteration_limit),
        ] {
            if value.is_some_and(|v| i32::try_from(v).is_err()) {
                return invalid(format!("{name} must be at most {}", i32::MAX));
            }
        }
        Ok(())
    }
}

/// The HiGHS backend. Each solve builds a fresh native model; the handle is
/// created, used and destroyed on the calling thread.
#[derive(Clone, Debug, Default)]
pub struct HighsSolver {
    /// Backend settings.
    pub settings: HighsOptions,
}

impl HighsSolver {
    /// A solver with the given settings.
    pub fn new(settings: HighsOptions) -> Self {
        Self { settings }
    }
}

fn engine<T, E: std::fmt::Debug>(r: std::result::Result<T, E>, op: &str) -> Result<T> {
    r.map_err(|e| Error::Backend(format!("HiGHS {op} failed: {e:?}")))
}

fn unsupported<T>(message: String) -> Result<T> {
    Err(Error::Unsupported(message))
}

/// Backend limits, checked before any work. Malformed data is
/// `InvalidInput`; well-formed data HiGHS cannot represent is `Unsupported`.
fn validate(
    model: &Model,
    options: &Options,
    settings: &HighsOptions,
    start: Option<&WarmStart>,
) -> Result<()> {
    options.validate()?;
    settings.validate()?;
    for (name, value) in [
        ("feasibility", options.tolerances.feasibility),
        ("integrality", options.tolerances.integrality),
    ] {
        if value < MIN_TOLERANCE {
            return unsupported(format!(
                "{name} tolerance {value} is below HiGHS's minimum {MIN_TOLERANCE}"
            ));
        }
    }
    if settings.node_limit.is_some() && !model.is_mip() {
        return unsupported("node_limit applies to MIP models only; this model is an LP".into());
    }
    if settings.iteration_limit.is_some() && model.is_mip() {
        return unsupported(
            "iteration_limit: HiGHS does not limit the LP subproblems of a MIP; \
             use node_limit or time_limit_seconds"
                .into(),
        );
    }
    for (name, size) in [
        ("variables", model.n()),
        ("rows", model.m()),
        ("matrix entries", model.nnz()),
    ] {
        if i32::try_from(size).is_err() {
            return unsupported(format!(
                "{size} {name} exceed HiGHS's index range ({})",
                i32::MAX
            ));
        }
    }
    for (k, &a) in model.matrix().values.iter().enumerate() {
        if a != 0.0 && !(MIN_MATRIX_VALUE < a.abs() && a.abs() < MAX_MATRIX_VALUE) {
            return unsupported(format!(
                "matrix value {k} is {a}; HiGHS needs nonzero magnitudes in \
                 ({MIN_MATRIX_VALUE}, {MAX_MATRIX_VALUE})"
            ));
        }
    }
    for (j, &c) in model.objective().iter().enumerate() {
        if c.abs() >= HIGHS_INFINITY {
            return unsupported(format!(
                "objective coefficient {j} is {c}; HiGHS reads |c| >= {HIGHS_INFINITY} as infinite"
            ));
        }
    }
    if model.objective_offset().abs() >= HIGHS_INFINITY {
        return unsupported(format!(
            "objective offset {} reaches HiGHS's infinity {HIGHS_INFINITY}",
            model.objective_offset()
        ));
    }
    for (what, bounds) in [("variable", model.variables()), ("row", model.rows())] {
        for (i, (&l, &u)) in bounds.lower.iter().zip(&bounds.upper).enumerate() {
            for v in [l, u] {
                if v.is_finite() && v.abs() >= HIGHS_INFINITY {
                    return unsupported(format!(
                        "{what} {i} has finite bound {v}; HiGHS reads |bound| >= \
                         {HIGHS_INFINITY} as infinite"
                    ));
                }
            }
        }
    }
    if let Some(start) = start {
        if start.primal.len() != model.n() {
            return Err(Error::InvalidInput(format!(
                "start: expected {} entries, got {}",
                model.n(),
                start.primal.len()
            )));
        }
        if let Some((j, v)) = start
            .primal
            .iter()
            .enumerate()
            .find(|(_, v)| !v.is_finite())
        {
            return Err(Error::InvalidInput(format!("start: entry {j} is {v}")));
        }
        if let Some((j, v)) = start
            .primal
            .iter()
            .enumerate()
            .find(|(_, v)| v.abs() >= HIGHS_INFINITY)
        {
            return unsupported(format!(
                "start: entry {j} is {v}; HiGHS reads |value| >= {HIGHS_INFINITY} as infinite"
            ));
        }
    }
    for (j, kind) in model.kinds().iter().enumerate() {
        if !kind.is_integral() {
            continue;
        }
        let bounds = model.variables();
        let values = [
            Some(bounds.lower[j]),
            Some(bounds.upper[j]),
            start.map(|s| s.primal[j]),
        ];
        if let Some(v) = values
            .into_iter()
            .flatten()
            .find(|v| v.is_finite() && v.abs() > MAX_INTEGER_MAGNITUDE)
        {
            return unsupported(format!(
                "integer variable {j}: value {v} exceeds 2^53 - 1 in magnitude"
            ));
        }
    }
    Ok(())
}

/// Applies one setting and records it.
fn set<V: HighsOptionValue + std::fmt::Display + Copy>(
    native: &mut highs::Model,
    report: &mut BackendReport,
    key: &str,
    value: V,
) -> Result<()> {
    engine(native.try_set_option(key, value), &format!("option {key}"))?;
    report
        .effective_options
        .push((key.to_owned(), value.to_string()));
    Ok(())
}

/// Builds the native model: columns with bounds and integrality, rows from
/// the CSR matrix, sense. The objective offset is set separately (C API).
fn convert(model: &Model) -> Result<highs::Model> {
    let mut problem = RowProblem::new();
    let bounds = model.variables();
    let columns: Vec<_> = (0..model.n())
        .map(|j| {
            problem.add_column_with_integrality(
                model.objective()[j],
                bounds.lower[j]..=bounds.upper[j],
                model.kinds()[j].is_integral(),
            )
        })
        .collect();
    let a = model.matrix();
    let rows = model.rows();
    for i in 0..model.m() {
        let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        let entries = range.map(|k| (columns[a.pattern.indices[k]], a.values[k]));
        problem.add_row(rows.lower[i]..=rows.upper[i], entries);
    }
    let sense = match model.sense() {
        Sense::Minimize => highs::Sense::Minimise,
        Sense::Maximize => highs::Sense::Maximise,
    };
    engine(problem.try_optimise(sense), "model load")
}

/// Why LP duals do not confirm a reported optimum, if they do not.
fn reject_duals(model: &Model, r: &LpResiduals, dual_tolerance: f64) -> Option<String> {
    let c_scale = 1.0
        + model
            .objective()
            .iter()
            .fold(0.0_f64, |m, c| m.max(c.abs()));
    let checks = [
        ("stationarity", r.stationarity, dual_tolerance * c_scale),
        ("dual sign residual", r.dual_sign, dual_tolerance * c_scale),
        ("relative duality gap", r.duality_gap, dual_tolerance),
    ];
    checks
        .into_iter()
        .find(|(_, value, limit)| value > limit)
        .map(|(what, value, limit)| format!("{what} {value:e} exceeds {limit:e}"))
}

impl LpIpSolver for HighsSolver {
    fn solve(
        &mut self,
        model: &Model,
        options: &Options,
        start: Option<&WarmStart>,
        cancel: &CancellationToken,
    ) -> Result<SolveResult> {
        validate(model, options, &self.settings, start)?;
        let began = Instant::now();
        // Declared before the native handle, so it outlives it on every path.
        let context = Box::new(ffi::Interrupt {
            token: cancel.clone(),
            began,
            seconds: self.settings.time_limit_seconds,
        });
        let mut report = BackendReport::new("highs");
        report.version = Some(ffi::version());

        if cancel.is_cancelled() {
            return Ok(SolveResult::empty(
                Status::Cancelled,
                "cancelled before the model was converted",
                report,
            ));
        }
        if context.timed_out() {
            report.time_budget_exceeded = true;
            return Ok(SolveResult::empty(
                Status::TimeLimit,
                "time budget used up before the model was converted",
                report,
            ));
        }

        let mut native = convert(model)?;
        let tolerances = options.tolerances;
        set(&mut native, &mut report, "output_flag", false)?;
        set(&mut native, &mut report, "threads", 1_i32)?;
        set(
            &mut native,
            &mut report,
            "random_seed",
            self.settings.seed as i32,
        )?;
        set(
            &mut native,
            &mut report,
            "small_matrix_value",
            MIN_MATRIX_VALUE,
        )?;
        set(
            &mut native,
            &mut report,
            "primal_feasibility_tolerance",
            tolerances.feasibility,
        )?;
        set(
            &mut native,
            &mut report,
            "dual_feasibility_tolerance",
            self.settings.dual_tolerance,
        )?;
        if model.is_mip() {
            // HiGHS uses one MIP tolerance for integrality and row feasibility.
            let mip = tolerances.integrality.min(tolerances.feasibility);
            set(&mut native, &mut report, "mip_feasibility_tolerance", mip)?;
            set(
                &mut native,
                &mut report,
                "mip_rel_gap",
                self.settings.mip_relative_gap,
            )?;
            set(
                &mut native,
                &mut report,
                "mip_abs_gap",
                self.settings.mip_absolute_gap,
            )?;
            if let Some(nodes) = self.settings.node_limit {
                set(&mut native, &mut report, "mip_max_nodes", nodes as i32)?;
            }
        } else if let Some(iterations) = self.settings.iteration_limit {
            set(
                &mut native,
                &mut report,
                "simplex_iteration_limit",
                iterations as i32,
            )?;
            set(
                &mut native,
                &mut report,
                "ipm_iteration_limit",
                iterations as i32,
            )?;
        }
        ffi::offset(&mut native, model.objective_offset())?;
        if let Some(start) = start {
            report.start_status =
                match native.try_set_solution(Some(&start.primal), None, None, None) {
                    Ok(()) => StartStatus::Submitted,
                    Err(_) => StartStatus::Rejected,
                };
        }
        context.install(&mut native)?;
        report.conversion_seconds = began.elapsed().as_secs_f64();

        if cancel.is_cancelled() {
            report.total_seconds = began.elapsed().as_secs_f64();
            return Ok(SolveResult::empty(
                Status::Cancelled,
                "cancelled before HiGHS started",
                report,
            ));
        }
        if let Some(budget) = self.settings.time_limit_seconds {
            let remaining = budget - began.elapsed().as_secs_f64();
            if remaining <= 0.0 {
                report.total_seconds = began.elapsed().as_secs_f64();
                report.time_budget_exceeded = true;
                return Ok(SolveResult::empty(
                    Status::TimeLimit,
                    "time budget used up before HiGHS started",
                    report,
                ));
            }
            set(&mut native, &mut report, "time_limit", remaining)?;
        }

        let solve_began = Instant::now();
        let solved = engine(native.try_solve(), "run")?;
        report.solve_seconds = solve_began.elapsed().as_secs_f64();
        let check_began = Instant::now();
        let raw = solved.status();
        report.raw_status = Some(format!("{raw:?}"));

        let mut status = match status::map(raw) {
            Mapped::Status(status) => status,
            Mapped::Interrupted if cancel.is_cancelled() => Status::Cancelled,
            Mapped::Interrupted if context.timed_out() => Status::TimeLimit,
            Mapped::Interrupted => Status::Unknown,
            Mapped::NotRun => {
                return Err(Error::Backend(format!(
                    "HiGHS did not solve the model: {raw:?}"
                )));
            }
        };
        let nodes = if model.is_mip() {
            u64::try_from(ffi::int64(&solved, c"mip_node_count")?).unwrap_or(0)
        } else {
            0
        };
        // HiGHS reports a reached `mip_max_nodes` as a solution limit.
        if status == Status::SolutionLimit
            && self
                .settings
                .node_limit
                .is_some_and(|limit| nodes >= u64::from(limit))
        {
            status = Status::NodeLimit;
        }
        let mut result = SolveResult::empty(status, "", report);
        result.nodes = nodes;
        result.iterations = u64::try_from(solved.simplex_iteration_count()).unwrap_or(0)
            + u64::try_from(solved.ipm_iteration_count()).unwrap_or(0);

        let mut rejection = None;
        if solved.primal_solution_status() == HighsSolutionStatus::Feasible {
            let solution = solved.get_solution();
            let x = solution.columns().to_vec();
            if let Some((j, v)) = x
                .iter()
                .enumerate()
                .find(|&(j, v)| model.kinds()[j].is_integral() && v.abs() > MAX_INTEGER_MAGNITUDE)
            {
                return Err(Error::Evaluation(format!(
                    "HiGHS returned {v} for integer variable {j}, beyond 2^53 - 1"
                )));
            }
            let evaluation = check::evaluate(model, &x, &tolerances)?;
            if !evaluation.feasible_candidate {
                rejection = Some(format!(
                    "the candidate violates bounds by {:e} (tolerance {:e}) and integrality \
                     by {:e} (tolerance {:e}), absolute in original units; HiGHS applies its \
                     tolerances to its scaled model",
                    evaluation.primal_violation,
                    tolerances.feasibility,
                    evaluation.integrality_violation,
                    tolerances.integrality
                ));
            }
            let dual_status = engine(
                solved.int_info_value(c"dual_solution_status"),
                "dual solution status",
            )?;
            if !model.is_mip() && dual_status == HighsSolutionStatus::Feasible as i64 {
                let duals = Duals {
                    row_duals: solution.dual_rows().to_vec(),
                    reduced_costs: solution.dual_columns().to_vec(),
                };
                let residuals = check::lp_residuals(model, &x, &duals)?;
                if rejection.is_none() {
                    rejection = reject_duals(model, &residuals, self.settings.dual_tolerance);
                }
                result.duals = Some(duals);
            }
            result.candidate = Some(x);
            result.evaluation = Some(evaluation);
        } else if status == Status::Optimal {
            rejection = Some("HiGHS declared no feasible primal solution".into());
        }

        if model.is_mip() {
            let bound = engine(
                solved.double_info_value(c"mip_dual_bound"),
                "mip_dual_bound",
            )?;
            if bound.is_finite() && bound.abs() < HIGHS_INFINITY {
                result.best_bound = Some(bound);
            }
        }
        if let (Some(gap), Some(bound)) = (result.gap(model.sense())?, result.best_bound) {
            let objective = result.evaluation.as_ref().map_or(0.0, |e| e.objective);
            if gap.absolute < -tolerances.feasibility * (1.0 + objective.abs()) {
                rejection.get_or_insert_with(|| {
                    format!(
                        "the bound {bound} lies on the wrong side of the incumbent objective \
                         {objective} (absolute gap {:e})",
                        gap.absolute
                    )
                });
            }
        }

        result.message = match (&rejection, status) {
            (Some(reason), Status::Optimal) => {
                result.status = Status::NumericalFailure;
                format!("HiGHS reported Optimal, but the independent check failed: {reason}")
            }
            (Some(reason), _) => format!("HiGHS {raw:?}; independent check: {reason}"),
            (None, _) => format!("HiGHS {raw:?}"),
        };
        result.backend.check_seconds = check_began.elapsed().as_secs_f64();
        result.backend.total_seconds = began.elapsed().as_secs_f64();
        result.backend.time_budget_exceeded = context.timed_out();
        Ok(result)
    }
}
