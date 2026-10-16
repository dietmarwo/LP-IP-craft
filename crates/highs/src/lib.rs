//! Real LP/MIP backend. Core contracts remain independent of the native engine.
mod ffi;
use highs::{HighsModelStatus as H, HighsSolutionStatus, RowProblem};
use lp_ip_craft::{
    BackendReport, CancellationToken, Duals, Error, LpIpSolver, Model, Options, Result, Sense,
    SolveResult, Status, WarmStart, check,
};
use std::time::Instant;

/// Supported HiGHS settings. Limits are cooperative; only one engine thread is
/// enabled in this iteration. Gap settings use HiGHS's stopping definition.
#[derive(Clone, Debug, PartialEq)]
pub struct HighsOptions {
    pub time_limit_seconds: Option<f64>,
    pub node_limit: Option<u32>,
    pub iteration_limit: Option<u32>,
    pub seed: u32,
    pub mip_relative_gap: f64,
    pub mip_absolute_gap: f64,
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
    pub fn validate(&self) -> Result<()> {
        if self
            .time_limit_seconds
            .is_some_and(|t| !t.is_finite() || t < 0.0)
            || !self.mip_relative_gap.is_finite()
            || self.mip_relative_gap < 0.0
            || !self.mip_absolute_gap.is_finite()
            || self.mip_absolute_gap < 0.0
            || !self.dual_tolerance.is_finite()
            || self.dual_tolerance < 1e-10
            || self.seed > i32::MAX as u32
            || self.node_limit.is_some_and(|v| v > i32::MAX as u32)
            || self.iteration_limit.is_some_and(|v| v > i32::MAX as u32)
        {
            return Err(Error::InvalidInput(
                "invalid HiGHS options or limit exceeds i32 range".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct HighsSolver {
    pub settings: HighsOptions,
}

fn engine<T, E: std::fmt::Debug>(r: std::result::Result<T, E>, op: &str) -> Result<T> {
    r.map_err(|e| Error::Backend(format!("HiGHS {op}: {e:?}")))
}
fn validate(model: &Model, options: &Options, start: Option<&WarmStart>) -> Result<()> {
    options.validate()?;
    if options.tolerances.feasibility < 1e-10 || options.tolerances.integrality < 1e-10 {
        return Err(Error::Unsupported(
            "HiGHS tolerances must be >= 1e-10".into(),
        ));
    }
    for size in [model.n(), model.m(), model.matrix().values.len()] {
        i32::try_from(size).map_err(|_| Error::InvalidInput("HiGHS index overflow".into()))?;
    }
    for &a in &model.matrix().values {
        if a.abs() >= 1e15 || (a != 0.0 && a.abs() <= 1e-12) {
            return Err(Error::Unsupported(
                "matrix coefficient outside HiGHS supported magnitude (1e-12,1e15)".into(),
            ));
        }
    }
    for &v in model
        .objective()
        .iter()
        .chain(std::iter::once(&model.objective_offset()))
    {
        if v.abs() >= 1e20 {
            return Err(Error::Unsupported(
                "objective magnitude collides with HiGHS infinity".into(),
            ));
        }
    }
    for b in [model.variables(), model.rows()] {
        for &v in b.lower.iter().chain(&b.upper) {
            if v.is_finite() && v.abs() >= 1e20 {
                return Err(Error::Unsupported(
                    "finite bound collides with HiGHS infinity".into(),
                ));
            }
        }
    }
    if let Some(w) = start {
        if w.primal.len() != model.n() || w.primal.iter().any(|v| !v.is_finite() || v.abs() >= 1e20)
        {
            return Err(Error::InvalidInput(
                "start: expected n finite supported entries".into(),
            ));
        }
    }
    for (j, kind) in model.kinds().iter().enumerate() {
        if kind.is_integral() {
            for v in [
                Some(model.variables().lower[j]),
                Some(model.variables().upper[j]),
                start.map(|w| w.primal[j]),
            ]
            .into_iter()
            .flatten()
            {
                if v.is_finite() && v.abs() > 9_007_199_254_740_991.0 {
                    return Err(Error::Unsupported(
                        "integer magnitude exceeds 2^53-1".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}
fn status(raw: H, node_limit: Option<u32>, nodes: u64) -> Status {
    match raw {
        H::Optimal => Status::Optimal,
        H::Infeasible => Status::Infeasible,
        H::Unbounded => Status::Unbounded,
        H::UnboundedOrInfeasible => Status::InfeasibleOrUnbounded,
        H::ReachedTimeLimit => Status::TimeLimit,
        // Pinned HiGHS reports its node cap as kSolutionLimit. Other
        // solution limits retain that generic status when no node cap hit.
        H::ReachedSolutionLimit if node_limit.is_some_and(|v| nodes >= u64::from(v)) => {
            Status::NodeLimit
        }
        H::ReachedIterationLimit => Status::IterationLimit,
        H::ReachedSolutionLimit => Status::SolutionLimit,
        H::ObjectiveBound | H::ObjectiveTarget => Status::ObjectiveLimit,
        H::ReachedInterrupt => Status::Cancelled,
        H::LoadError | H::ModelError | H::PresolveError | H::SolveError | H::PostsolveError => {
            Status::NumericalFailure
        }
        _ => Status::Unknown,
    }
}
impl LpIpSolver for HighsSolver {
    fn solve(
        &mut self,
        model: &Model,
        options: &Options,
        start: Option<&WarmStart>,
        cancel: &CancellationToken,
    ) -> Result<SolveResult> {
        self.settings.validate()?;
        validate(model, options, start)?;
        let began = Instant::now();
        // Context is allocated before the native handle, so every return path
        // destroys the handle before its callback data.
        let mut context = Box::new(ffi::Interrupt {
            token: cancel.clone(),
            began,
            seconds: self.settings.time_limit_seconds,
        });
        let mut result = SolveResult::empty(Status::Cancelled, "cancelled before engine entry");
        result.backend = BackendReport {
            name: "highs".into(),
            version: Some(ffi::version()),
            ..Default::default()
        };
        if cancel.is_cancelled() {
            return Ok(result);
        }
        if context.timed_out() {
            result.status = Status::TimeLimit;
            result.message = "budget exhausted before conversion".into();
            return Ok(result);
        }
        let mut p = RowProblem::new();
        let cols: Vec<_> = (0..model.n())
            .map(|j| {
                p.add_column_with_integrality(
                    model.objective()[j],
                    model.variables().lower[j]..=model.variables().upper[j],
                    model.kinds()[j].is_integral(),
                )
            })
            .collect();
        let a = model.matrix();
        for i in 0..model.m() {
            let entries = (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
                .map(|k| (cols[a.pattern.indices[k]], a.values[k]));
            p.add_row(model.rows().lower[i]..=model.rows().upper[i], entries);
        }
        let sense = match model.sense() {
            Sense::Minimize => highs::Sense::Minimise,
            Sense::Maximize => highs::Sense::Maximise,
        };
        let mut native = engine(p.try_optimise(sense), "load model")?;
        macro_rules! set {
            ($key:expr,$value:expr) => {{
                let value = $value;
                engine(native.try_set_option($key, value), $key)?;
                result
                    .backend
                    .effective_options
                    .push(($key.into(), value.to_string()));
            }};
        }
        set!("output_flag", false);
        set!("threads", 1_i32);
        set!("random_seed", self.settings.seed as i32);
        set!("small_matrix_value", 1e-12);
        set!(
            "primal_feasibility_tolerance",
            options.tolerances.feasibility
        );
        set!("dual_feasibility_tolerance", self.settings.dual_tolerance);
        set!(
            "mip_feasibility_tolerance",
            options
                .tolerances
                .integrality
                .min(options.tolerances.feasibility)
        );
        set!("mip_rel_gap", self.settings.mip_relative_gap);
        set!("mip_abs_gap", self.settings.mip_absolute_gap);
        if let Some(v) = self.settings.node_limit {
            set!("mip_max_nodes", v as i32);
        }
        if let Some(v) = self.settings.iteration_limit {
            set!("simplex_iteration_limit", v as i32);
            set!("ipm_iteration_limit", v as i32);
        }
        ffi::offset(&mut native, model.objective_offset())?;
        if let Some(w) = start {
            result.backend.start_status =
                match native.try_set_solution(Some(&w.primal), None, None, None) {
                    Ok(_) => "submitted",
                    Err(_) => "rejected",
                }
                .into();
        }
        context.install(&mut native)?;
        result.backend.conversion_seconds = began.elapsed().as_secs_f64();
        if cancel.is_cancelled() || context.timed_out() {
            result.status = if cancel.is_cancelled() {
                Status::Cancelled
            } else {
                Status::TimeLimit
            };
            result.backend.total_seconds = began.elapsed().as_secs_f64();
            result.message = "stopped before engine entry".into();
            return Ok(result);
        }
        if let Some(seconds) = self.settings.time_limit_seconds {
            set!(
                "time_limit",
                (seconds - began.elapsed().as_secs_f64()).max(0.0)
            );
        }
        let solve_began = Instant::now();
        let solved = engine(native.try_solve(), "run")?;
        result.backend.solve_seconds = solve_began.elapsed().as_secs_f64();
        let check_began = Instant::now();
        let raw = solved.status();
        result.backend.raw_status = Some(format!("{raw:?}"));
        result.nodes = if model.is_mip() {
            ffi::int64(&solved, c"mip_node_count")?.max(0) as u64
        } else {
            0
        };
        result.iterations =
            (solved.simplex_iteration_count().max(0) + solved.ipm_iteration_count().max(0)) as u64;
        result.status = status(raw, self.settings.node_limit, result.nodes);
        if raw == H::ReachedInterrupt {
            result.status = if cancel.is_cancelled() {
                Status::Cancelled
            } else if context.timed_out() {
                Status::TimeLimit
            } else {
                Status::Unknown
            };
        }
        if solved.primal_solution_status() == HighsSolutionStatus::Feasible {
            let solution = solved.get_solution();
            let x = solution.columns().to_vec();
            for (j, kind) in model.kinds().iter().enumerate() {
                if kind.is_integral() && x[j].abs() > 9_007_199_254_740_991.0 {
                    return Err(Error::Evaluation(
                        "returned integer outside safe magnitude".into(),
                    ));
                }
            }
            let evaluation = check::evaluate(model, &x, &options.tolerances)?;
            if result.status == Status::Optimal && !evaluation.feasible_candidate {
                result.status = Status::NumericalFailure;
            }
            if !model.is_mip() && solved.int_info_value(c"dual_solution_status").ok() == Some(2) {
                let duals = Duals {
                    row_duals: solution.dual_rows().to_vec(),
                    reduced_costs: solution.dual_columns().to_vec(),
                };
                let k = check::lp_residuals(model, &x, &duals)?;
                if result.status == Status::Optimal
                    && (k.stationarity > self.settings.dual_tolerance
                        || k.complementarity > self.settings.dual_tolerance
                        || k.dual_sign > self.settings.dual_tolerance)
                {
                    result.status = Status::NumericalFailure;
                }
                result.duals = Some(duals);
            }
            result.candidate = Some(x);
            result.evaluation = Some(evaluation);
        } else if result.status == Status::Optimal {
            result.status = Status::NumericalFailure;
        }
        if model.is_mip() {
            let bound = engine(solved.double_info_value(c"mip_dual_bound"), "MIP bound")?;
            if bound.is_finite() && bound.abs() < 1e20 {
                result.best_bound = Some(bound);
            }
        }
        if let Some(g) = result.checked_gap(model.sense())? {
            if !g.absolute.is_finite()
                || g.absolute < -1e-7 * (1.0 + result.evaluation.as_ref().unwrap().objective.abs())
            {
                result.status = Status::NumericalFailure;
            }
        }
        // Late requests do not discard a checked incumbent. A call budget
        // includes extraction/checking even if the native run was optimal.
        if result.status != Status::NumericalFailure {
            if cancel.is_cancelled() {
                result.status = Status::Cancelled;
            } else if context.timed_out() {
                result.status = Status::TimeLimit;
            }
        }
        result.message = format!("HiGHS {raw:?}; facade {}", result.status.as_str());
        result.backend.check_seconds = check_began.elapsed().as_secs_f64();
        result.backend.total_seconds = began.elapsed().as_secs_f64();
        Ok(result)
    }
}
