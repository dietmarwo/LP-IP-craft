//! Owned, dependency-free dense and sparse Rust LP/MIP numerical pilots.
#![forbid(unsafe_code)]
mod components;
mod dictionary;
mod incumbent;
mod presolve;
mod revised;
mod sparse_basis;
mod standard;
mod tableau;

use lp_ip_craft::{
    BackendReport, CancellationToken, Duals, Error, LpIpSolver, Model, Options, Result, Sense,
    SolveResult, StartStatus, Status, WarmStart, check, safe_dual_bound_with_bounds,
};
use standard::Standard;
use std::sync::Arc;
use std::time::Instant;

/// Native controls. Dense and sparse memory/update controls are kernel-specific.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeOptions {
    /// Whole-call cooperative wall-time limit, including conversion/checks.
    pub time_limit_seconds: Option<f64>,
    /// Number of LP relaxations processed in a MIP.
    pub node_limit: Option<u64>,
    /// Total simplex pivots across the solve, including all MIP nodes.
    pub iteration_limit: Option<u64>,
    /// Scaled tableau pricing/pivot tolerance.
    pub pivot_tolerance: f64,
    /// Independent relative duality and dual-sign acceptance threshold.
    pub dual_tolerance: f64,
    /// Select sparse revised simplex instead of the dense reference kernel.
    pub sparse: bool,
    /// Numerical entry cap per sparse state, including refactor overlap.
    pub max_sparse_entries: usize,
    /// Maximum product-form/border updates before sparse LU refactorization.
    pub sparse_refactor_interval: usize,
    /// Dense storage cap, in f64 entries, checked before allocation.
    pub max_tableau_entries: usize,
    /// Eliminate fixed columns and feasible empty rows before simplex.
    pub presolve: bool,
    /// Omit rows whose complete box activity is conservatively feasible.
    pub redundant_rows: bool,
    /// Scan row redundancy only when m/n reaches this threshold; zero forces scanning.
    pub redundant_row_min_ratio: f64,
    /// Dense only: in-place pivots; false retains the allocating reference kernel.
    pub fast_pivots: bool,
    /// Most-negative pricing with Bland fallback on repeated stalls.
    pub dantzig_pricing: bool,
    /// Reoptimize child dictionaries with dual simplex; cold-retry failures.
    pub node_reoptimization: bool,
    /// Independently checked rounding and bounded unit-move incumbents.
    pub incumbent_heuristic: bool,
    /// Tighten conservative bounds to a verified integral objective lattice.
    pub objective_lattice: bool,
    /// Split independent restricted-row components before solving.
    pub decompose: bool,
    /// Skip graph discovery below this variable count.
    pub decomposition_min_variables: usize,
    /// Maximum workers for independent components, 1 through 16.
    pub component_threads: usize,
    /// Keep smaller average component dictionaries serial; zero forces workers.
    pub component_parallel_min_entries: usize,
}
impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            time_limit_seconds: None,
            node_limit: Some(100_000),
            iteration_limit: Some(100_000),
            pivot_tolerance: 1e-10,
            dual_tolerance: 1e-7,
            sparse: false,
            max_sparse_entries: 2_000_000,
            sparse_refactor_interval: 64,
            max_tableau_entries: 8_000_000,
            presolve: true,
            redundant_rows: true,
            redundant_row_min_ratio: 1.0,
            fast_pivots: true,
            dantzig_pricing: true,
            node_reoptimization: true,
            incumbent_heuristic: true,
            objective_lattice: true,
            decompose: true,
            decomposition_min_variables: 32,
            component_threads: 16,
            component_parallel_min_entries: 4096,
        }
    }
}
impl NativeOptions {
    /// Validate settings before cancellation or numerical work.
    pub fn validate(&self) -> Result<()> {
        if self
            .time_limit_seconds
            .is_some_and(|v| !v.is_finite() || v < 0.0)
        {
            return Err(Error::InvalidInput(
                "time_limit_seconds must be finite and nonnegative".into(),
            ));
        }
        if !self.pivot_tolerance.is_finite()
            || self.pivot_tolerance <= 0.0
            || self.pivot_tolerance >= 0.01
        {
            return Err(Error::InvalidInput(
                "pivot_tolerance must lie in (0, .01)".into(),
            ));
        }
        if !self.dual_tolerance.is_finite() || self.dual_tolerance <= 0.0 {
            return Err(Error::InvalidInput(
                "dual_tolerance must be positive and finite".into(),
            ));
        }
        if !self.redundant_row_min_ratio.is_finite() || self.redundant_row_min_ratio < 0. {
            return Err(Error::InvalidInput(
                "redundant_row_min_ratio must be finite and nonnegative".into(),
            ));
        }
        if !(1..=16).contains(&self.component_threads) {
            return Err(Error::InvalidInput(
                "component_threads must lie in 1..=16".into(),
            ));
        }
        if self.decomposition_min_variables == 0 {
            return Err(Error::InvalidInput(
                "decomposition_min_variables must be positive".into(),
            ));
        }
        if self.max_sparse_entries < 4 || self.sparse_refactor_interval == 0 {
            return Err(Error::InvalidInput(
                "max_sparse_entries must be >= 4 and sparse_refactor_interval positive".into(),
            ));
        }
        if self.max_tableau_entries < 4 {
            return Err(Error::InvalidInput(
                "max_tableau_entries must be at least 4".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) struct Budget<'a> {
    pub start: Instant,
    pub settings: &'a NativeOptions,
    pub cancel: &'a CancellationToken,
    pub pivots: u64,
}
impl Budget<'_> {
    fn stop(&self) -> Option<Status> {
        if self.cancel.is_cancelled() {
            Some(Status::Cancelled)
        } else if self
            .settings
            .time_limit_seconds
            .is_some_and(|t| self.start.elapsed().as_secs_f64() >= t)
        {
            Some(Status::TimeLimit)
        } else if self
            .settings
            .iteration_limit
            .is_some_and(|p| self.pivots >= p)
        {
            Some(Status::IterationLimit)
        } else {
            None
        }
    }
}

/// Solver with single-thread dictionaries and optional parallel components.
/// Independent instances can share immutable input models.
#[derive(Clone, Debug, Default)]
pub struct NativeSolver {
    /// Engine controls.
    pub settings: NativeOptions,
}
impl NativeSolver {
    /// Construct a solver with explicit controls.
    pub fn new(settings: NativeOptions) -> Self {
        Self { settings }
    }
}

fn oriented(model: &Model, v: f64) -> f64 {
    if model.sense() == Sense::Minimize {
        v
    } else {
        -v
    }
}
fn accepted(
    model: &Model,
    bounds: &lp_ip_craft::Bounds,
    x: &[f64],
    duals: &Duals,
    options: &Options,
    settings: &NativeOptions,
) -> bool {
    let Ok(r) = check::lp_residuals_with_bounds(model, bounds, x, duals) else {
        return false;
    };
    let scale = 1.0
        + model
            .objective()
            .iter()
            .fold(0.0_f64, |a, &b| a.max(b.abs()));
    r.primal <= options.tolerances.feasibility
        && r.dual_sign <= settings.dual_tolerance * scale
        && r.stationarity <= settings.dual_tolerance * scale
        && r.duality_gap <= settings.dual_tolerance
}

struct Incumbents {
    best: f64,
    upper: f64,
    certified: Option<(Vec<f64>, lp_ip_craft::Evaluation)>,
}
fn consider(
    model: &Model,
    x: Vec<f64>,
    options: &Options,
    result: &mut SolveResult,
    pool: &mut Incumbents,
) -> Result<bool> {
    let e = check::evaluate(model, &x, &options.tolerances)?;
    if !e.feasible_candidate {
        return Ok(false);
    }
    if let Some(upper) = incumbent::upper(model, &x) {
        if upper < pool.upper {
            pool.upper = upper;
            pool.certified = Some((x.clone(), e.clone()));
        }
    }
    let obj = oriented(model, e.objective);
    if obj < pool.best {
        pool.best = obj;
        result.candidate = Some(x);
        result.evaluation = Some(e);
    }
    Ok(true)
}
fn heuristic(
    model: &Model,
    node_bounds: &lp_ip_craft::Bounds,
    x: &[f64],
    options: &Options,
    result: &mut SolveResult,
    pool: &mut Incumbents,
    budget: &Budget<'_>,
) -> Result<u64> {
    let mut found = 0;
    let mut order: Vec<_> = (0..model.n())
        .filter(|&j| model.kinds()[j].is_integral())
        .collect();
    order.sort_by(|&a, &b| {
        model.objective()[b]
            .abs()
            .total_cmp(&model.objective()[a].abs())
            .then(a.cmp(&b))
    });
    for mode in 0..3 {
        if budget.stop().is_some() {
            break;
        }
        let mut point = x.to_vec();
        for &j in &order {
            point[j] = match mode {
                0 => point[j].round(),
                1 => point[j].floor(),
                _ => point[j].ceil(),
            };
            point[j] = point[j]
                .max(node_bounds.lower[j].ceil())
                .min(node_bounds.upper[j].floor());
        }
        if !consider(model, point.clone(), options, result, pool)? {
            continue;
        }
        found += 1;
        for &j in order.iter().take(64) {
            if budget.stop().is_some() {
                break;
            }
            let c = oriented(model, model.objective()[j]);
            if c == 0.0 {
                continue;
            }
            let old = point[j];
            point[j] += if c < 0.0 { 1.0 } else { -1.0 };
            if point[j] < node_bounds.lower[j]
                || point[j] > node_bounds.upper[j]
                || !consider(model, point.clone(), options, result, pool)?
            {
                point[j] = old;
            } else {
                found += 1;
            }
        }
    }
    Ok(found)
}

#[derive(Clone)]
struct Node {
    lower: Vec<f64>,
    upper: Vec<f64>,
    bound: Option<f64>,
    parent: Option<Arc<Standard>>,
    branch: Option<(usize, f64, bool)>,
}

impl LpIpSolver for NativeSolver {
    fn solve(
        &mut self,
        model: &Model,
        options: &Options,
        start: Option<&WarmStart>,
        cancel: &CancellationToken,
    ) -> Result<SolveResult> {
        options.validate()?;
        self.settings.validate()?;
        if let Some(s) = start {
            if s.primal.len() != model.n() || s.primal.iter().any(|v| !v.is_finite()) {
                return Err(Error::InvalidInput(
                    "start must contain n finite values".into(),
                ));
            }
        }
        if model.is_mip()
            && model
                .variables()
                .lower
                .iter()
                .chain(&model.variables().upper)
                .any(|v| !v.is_finite() || v.abs() > 9_007_199_254_740_991.0)
        {
            return Err(Error::Unsupported(
                "native MIP requires finite variable bounds of magnitude <= 2^53-1".into(),
            ));
        }
        let call_started = Instant::now();
        if self.settings.decompose
            && model.n() >= self.settings.decomposition_min_variables
            && !cancel.is_cancelled()
            && self.settings.time_limit_seconds != Some(0.)
        {
            if let Some(out) = components::solve_if_separable(
                &self.settings,
                model,
                options,
                start,
                cancel,
                call_started,
            )? {
                return Ok(out);
            }
        }
        let mut budget = Budget {
            start: call_started,
            settings: &self.settings,
            cancel,
            pivots: 0,
        };
        let mut report = BackendReport::new("native");
        report.version = Some(env!("CARGO_PKG_VERSION").into());
        report.effective_options = vec![(
            "algorithm".into(),
            "two_phase_simplex+depth_first_branch_and_bound".into(),
        )];
        report.effective_options.extend([
            (
                "linear_kernel".into(),
                if self.settings.sparse {
                    "sparse_revised_simplex"
                } else {
                    "dense_tableau"
                }
                .into(),
            ),
            ("sparse".into(), self.settings.sparse.to_string()),
            (
                "max_sparse_entries".into(),
                self.settings.max_sparse_entries.to_string(),
            ),
            (
                "sparse_refactor_interval".into(),
                self.settings.sparse_refactor_interval.to_string(),
            ),
            (
                "component_threads".into(),
                self.settings.component_threads.to_string(),
            ),
            (
                "component_parallel_min_entries".into(),
                self.settings.component_parallel_min_entries.to_string(),
            ),
            ("decompose".into(), self.settings.decompose.to_string()),
            (
                "decomposition_min_variables".into(),
                self.settings.decomposition_min_variables.to_string(),
            ),
            (
                "time_limit_seconds".into(),
                format!("{:?}", self.settings.time_limit_seconds),
            ),
            (
                "node_limit".into(),
                format!("{:?}", self.settings.node_limit),
            ),
            (
                "iteration_limit".into(),
                format!("{:?}", self.settings.iteration_limit),
            ),
            (
                "pivot_tolerance".into(),
                self.settings.pivot_tolerance.to_string(),
            ),
            (
                "dual_tolerance".into(),
                self.settings.dual_tolerance.to_string(),
            ),
            (
                "max_tableau_entries".into(),
                self.settings.max_tableau_entries.to_string(),
            ),
            ("presolve".into(), self.settings.presolve.to_string()),
            (
                "redundant_rows".into(),
                self.settings.redundant_rows.to_string(),
            ),
            (
                "redundant_row_min_ratio".into(),
                self.settings.redundant_row_min_ratio.to_string(),
            ),
            ("fast_pivots".into(), self.settings.fast_pivots.to_string()),
            (
                "dantzig_pricing".into(),
                self.settings.dantzig_pricing.to_string(),
            ),
            (
                "node_reoptimization".into(),
                self.settings.node_reoptimization.to_string(),
            ),
            (
                "incumbent_heuristic".into(),
                self.settings.incumbent_heuristic.to_string(),
            ),
            (
                "objective_lattice".into(),
                self.settings.objective_lattice.to_string(),
            ),
        ]);
        report.start_status = if start.is_some() {
            if model.is_mip() {
                StartStatus::Evaluated
            } else {
                StartStatus::Rejected
            }
        } else {
            StartStatus::NotProvided
        };
        let mut result = SolveResult::empty(Status::Unknown, "", report);
        let mut pool = Incumbents {
            best: f64::INFINITY,
            upper: f64::INFINITY,
            certified: None,
        };
        let lattice = self.settings.objective_lattice && incumbent::integer_lattice(model);
        let (mut heuristic_candidates, mut pruned_nodes) = (0u64, 0u64);
        if model.is_mip() {
            if let Some(s) = start {
                consider(model, s.primal.clone(), options, &mut result, &mut pool)?;
            }
        }
        let mut stack = vec![Node {
            lower: model.variables().lower.clone(),
            upper: model.variables().upper.clone(),
            bound: None,
            parent: None,
            branch: None,
        }];
        let mut closed_bound = f64::INFINITY;
        let mut all_bounds = true;
        let mut status = Status::Optimal;
        let mut removed_row_instances = 0u64;
        let (mut sparse_peak, mut sparse_refactors, mut sparse_updates) = (0usize, 0u64, 0u64);
        let (mut warm_nodes, mut cold_solves, mut fallbacks) = (0u64, 0u64, 0u64);
        while let Some(node) = stack.pop() {
            if let Some(s) = budget.stop() {
                status = s;
                stack.push(node);
                break;
            }
            if model.is_mip() && self.settings.node_limit.is_some_and(|n| result.nodes >= n) {
                status = Status::NodeLimit;
                stack.push(node);
                break;
            }
            if node.bound.is_some_and(|b| b >= pool.upper) {
                closed_bound = closed_bound.min(node.bound.unwrap());
                pruned_nodes += 1;
                continue;
            }
            let node_bounds = lp_ip_craft::Bounds::new(node.lower, node.upper);
            let conversion = Instant::now();
            let reused = if self.settings.node_reoptimization {
                if let (Some(parent), Some((j, value, upper))) = (&node.parent, node.branch) {
                    let mut s = (**parent).clone();
                    if s.branch(j, value, upper, &self.settings).is_ok() {
                        Some(s)
                    } else {
                        fallbacks += 1;
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let warm = reused.is_some();
            let mut standard = if let Some(s) = reused {
                warm_nodes += 1;
                s
            } else {
                cold_solves += 1;
                Standard::new(model, &node_bounds, &self.settings)?
            };
            result.backend.conversion_seconds += conversion.elapsed().as_secs_f64();
            let prior_stats = standard.table.diagnostics();
            let solve = Instant::now();
            let mut s = if warm {
                standard
                    .table
                    .dual_simplex(&mut budget, self.settings.pivot_tolerance)
            } else {
                standard
                    .table
                    .solve(&mut budget, self.settings.pivot_tolerance)
            };
            let mut cold_retry = false;
            let warm_rejected = warm
                && s == Status::Optimal
                && !accepted(
                    model,
                    &node_bounds,
                    &standard.point(),
                    &standard.duals(model),
                    options,
                    &self.settings,
                );
            if warm
                && (matches!(s, Status::NumericalFailure | Status::Infeasible) || warm_rejected)
                && budget.stop().is_none()
            {
                cold_retry = true;
                fallbacks += 1;
                cold_solves += 1;
                let stats = standard.table.diagnostics();
                sparse_peak = sparse_peak.max(stats.0);
                sparse_refactors += stats.1.saturating_sub(prior_stats.1);
                sparse_updates += stats.2.saturating_sub(prior_stats.2);
                standard = Standard::new(model, &node_bounds, &self.settings)?;
                s = standard
                    .table
                    .solve(&mut budget, self.settings.pivot_tolerance);
            }
            let stats = standard.table.diagnostics();
            sparse_peak = sparse_peak.max(stats.0);
            sparse_refactors += stats
                .1
                .saturating_sub(if cold_retry { 0 } else { prior_stats.1 });
            sparse_updates += stats
                .2
                .saturating_sub(if cold_retry { 0 } else { prior_stats.2 });
            if !warm || cold_retry {
                removed_row_instances += standard.removed_rows as u64;
            }
            result.backend.solve_seconds += solve.elapsed().as_secs_f64();
            if model.is_mip() {
                result.nodes += 1;
            }
            if s == Status::Infeasible {
                if model.is_mip() && result.nodes == 1 {
                    status = if result.candidate.is_some() {
                        Status::NumericalFailure
                    } else {
                        Status::Infeasible
                    };
                    break;
                }
                if !model.is_mip() {
                    status = s;
                    break;
                }
                continue;
            }
            if s != Status::Optimal {
                status = if model.is_mip() && s == Status::Unbounded {
                    Status::Unknown
                } else {
                    s
                };
                stack.push(Node {
                    lower: node_bounds.lower.clone(),
                    upper: node_bounds.upper.clone(),
                    bound: node.bound,
                    parent: None,
                    branch: None,
                });
                break;
            }
            let checks = Instant::now();
            let x = standard.point();
            let duals = standard.duals(model);
            if (!warm || cold_retry)
                && !accepted(model, &node_bounds, &x, &duals, options, &self.settings)
            {
                status = Status::NumericalFailure;
                all_bounds = false;
                break;
            }
            let bound = safe_dual_bound_with_bounds(model, &node_bounds, &duals.row_duals)?
                .bound
                .map(|b| oriented(model, b))
                .map(|b| if lattice { b.ceil() } else { b });
            if !model.is_mip() {
                result.evaluation = Some(check::evaluate(model, &x, &options.tolerances)?);
                result.candidate = Some(x);
                result.duals = Some(duals);
                result.best_bound = bound.map(|b| oriented(model, b));
                result.backend.check_seconds += checks.elapsed().as_secs_f64();
                break;
            }
            let mut branch = (0..model.n())
                .filter(|&j| model.kinds()[j].is_integral())
                .map(|j| (j, (x[j] - x[j].round()).abs()))
                .filter(|&(_, f)| f > options.tolerances.integrality)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if branch.is_none() {
                let mut rounded = x.clone();
                for (j, v) in rounded.iter_mut().enumerate() {
                    if model.kinds()[j].is_integral() {
                        *v = v.round();
                    }
                }
                if incumbent::upper_with_bounds(model, &node_bounds, &rounded).is_none() {
                    branch = (0..model.n())
                        .filter(|&j| {
                            model.kinds()[j].is_integral()
                                && x[j].floor() >= node_bounds.lower[j]
                                && x[j].ceil() <= node_bounds.upper[j]
                        })
                        .map(|j| (j, (x[j] - x[j].round()).abs()))
                        .filter(|&(_, f)| f > 0.0)
                        .max_by(|a, b| a.1.total_cmp(&b.1));
                }
            }
            if branch.is_some()
                && self.settings.incumbent_heuristic
                && (result.nodes == 1 || !pool.upper.is_finite())
            {
                heuristic_candidates += heuristic(
                    model,
                    &node_bounds,
                    &x,
                    options,
                    &mut result,
                    &mut pool,
                    &budget,
                )?;
            }
            if bound.is_some_and(|b| b >= pool.upper) {
                closed_bound = closed_bound.min(bound.unwrap());
                pruned_nodes += 1;
                result.backend.check_seconds += checks.elapsed().as_secs_f64();
                continue;
            }
            if let Some((j, _)) = branch {
                let parent = if self.settings.node_reoptimization {
                    Some(Arc::new(standard))
                } else {
                    None
                };
                let mut left = Node {
                    lower: node_bounds.lower.clone(),
                    upper: node_bounds.upper.clone(),
                    bound,
                    parent,
                    branch: Some((j, x[j].floor(), true)),
                };
                let mut right = left.clone();
                right.branch = Some((j, x[j].ceil(), false));
                left.upper[j] = left.upper[j].min(x[j].floor());
                right.lower[j] = right.lower[j].max(x[j].ceil());
                if right.lower[j] <= right.upper[j] {
                    stack.push(right);
                }
                if left.lower[j] <= left.upper[j] {
                    stack.push(left);
                }
            } else {
                let mut point = x;
                for (j, v) in point.iter_mut().enumerate() {
                    if model.kinds()[j].is_integral() {
                        *v = v.round();
                    }
                }
                if !consider(model, point, options, &mut result, &mut pool)? {
                    status = Status::NumericalFailure;
                    all_bounds = false;
                    break;
                }
                if let Some(b) = bound {
                    closed_bound = closed_bound.min(b);
                } else {
                    all_bounds = false;
                }
            }
            result.backend.check_seconds += checks.elapsed().as_secs_f64();
        }
        if model.is_mip() {
            for node in &stack {
                if let Some(b) = node.bound {
                    closed_bound = closed_bound.min(b);
                } else {
                    all_bounds = false;
                }
            }
            if all_bounds && closed_bound.is_finite() {
                result.best_bound = Some(oriented(model, closed_bound));
            }
            if status == Status::Optimal && result.candidate.is_none() {
                status = Status::Infeasible;
            }
        }
        if status == Status::Optimal {
            if let (Some(e), Some(b)) = (&result.evaluation, result.best_bound) {
                let gap = oriented(model, e.objective) - oriented(model, b);
                if gap < -self.settings.dual_tolerance * (1.0 + e.objective.abs()) {
                    if let Some((x, e)) = pool.certified {
                        let gap = oriented(model, e.objective) - oriented(model, b);
                        if gap >= -self.settings.dual_tolerance * (1.0 + e.objective.abs()) {
                            result.candidate = Some(x);
                            result.evaluation = Some(e);
                        } else {
                            status = Status::NumericalFailure;
                        }
                    } else {
                        status = Status::NumericalFailure;
                    }
                }
            }
        }
        result.backend.effective_options.extend([
            (
                "diagnostics.sparse_peak_entries".into(),
                sparse_peak.to_string(),
            ),
            (
                "diagnostics.sparse_refactors".into(),
                sparse_refactors.to_string(),
            ),
            (
                "diagnostics.sparse_updates".into(),
                sparse_updates.to_string(),
            ),
            ("diagnostics.component_count".into(), "1".into()),
            ("diagnostics.component_workers".into(), "1".into()),
            (
                "diagnostics.reoptimized_nodes".into(),
                warm_nodes.to_string(),
            ),
            (
                "diagnostics.presolve_removed_row_instances".into(),
                removed_row_instances.to_string(),
            ),
            ("diagnostics.cold_lp_solves".into(), cold_solves.to_string()),
            ("diagnostics.cold_fallbacks".into(), fallbacks.to_string()),
            (
                "diagnostics.heuristic_candidates".into(),
                heuristic_candidates.to_string(),
            ),
            ("diagnostics.pruned_nodes".into(), pruned_nodes.to_string()),
            ("diagnostics.objective_lattice".into(), lattice.to_string()),
        ]);
        result.status = status;
        result.iterations = budget.pivots;
        result.message = format!(
            "Native {} numerical pilot: {} (not an exact certificate)",
            if self.settings.sparse {
                "sparse"
            } else {
                "dense"
            },
            status.as_str()
        );
        result.backend.raw_status = Some(status.as_str().into());
        result.backend.total_seconds = budget.start.elapsed().as_secs_f64();
        result.backend.time_budget_exceeded = self
            .settings
            .time_limit_seconds
            .is_some_and(|t| result.backend.total_seconds > t);
        Ok(result)
    }
}
