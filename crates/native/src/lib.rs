//! Owned, dependency-free Rust LP/MIP solver: a bounded-variable dual
//! revised simplex on a sparse LU basis factorization, inside best-bound
//! branch and bound with diving, pseudocost branching and independently
//! checked results.
#![forbid(unsafe_code)]
mod boxed;
mod components;
mod incumbent;
mod lp;
mod lu;
mod presolve;
pub mod profile;
mod propagate;

use lp::{Basis, Lp};
use lp_ip_craft::{
    BackendReport, Bounds, CancellationToken, Error, LpIpSolver, Model, Options, Result, Sense,
    SolveResult, StartStatus, Status, WarmStart, check, safe_dual_bound_with_bounds,
};
use std::sync::Arc;
use std::time::Instant;

/// Native controls.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeOptions {
    /// Whole-call cooperative wall-time limit, including conversion/checks.
    pub time_limit_seconds: Option<f64>,
    /// Number of LP relaxations processed in a MIP.
    pub node_limit: Option<u64>,
    /// Total simplex iterations across the solve, including all MIP nodes.
    pub iteration_limit: Option<u64>,
    /// Smallest pivot-row entry accepted by the scaled dual ratio test.
    pub pivot_tolerance: f64,
    /// Independent relative duality and dual-sign acceptance threshold.
    pub dual_tolerance: f64,
    /// Cap on stored LU and eta entries of one basis factorization.
    pub max_factor_entries: usize,
    /// Maximum basis updates before rebuilding LU; excessive eta fill rebuilds sooner.
    pub refactor_interval: usize,
    /// Dual steepest-edge pricing; false prices by largest infeasibility.
    pub steepest_edge: bool,
    /// Drop free rows and feasible empty rows before simplex.
    pub presolve: bool,
    /// Also drop rows whose activity over the root box is conservatively feasible.
    pub redundant_rows: bool,
    /// Scan row redundancy only when m/n reaches this threshold; zero forces scanning.
    pub redundant_row_min_ratio: f64,
    /// Reoptimize branch-and-bound children from the parent basis.
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
    /// Solve components serially unless their average size (nonzeros plus
    /// rows plus columns) reaches this value; zero forces workers.
    pub component_parallel_min_entries: usize,
}
impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            time_limit_seconds: None,
            node_limit: Some(100_000),
            iteration_limit: Some(1_000_000),
            pivot_tolerance: 1e-9,
            dual_tolerance: 1e-7,
            max_factor_entries: 4_000_000,
            refactor_interval: 64,
            steepest_edge: true,
            presolve: true,
            redundant_rows: true,
            redundant_row_min_ratio: 1.0,
            node_reoptimization: true,
            incumbent_heuristic: true,
            objective_lattice: true,
            decompose: true,
            decomposition_min_variables: 32,
            component_threads: 16,
            component_parallel_min_entries: 2048,
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
        if self.max_factor_entries == 0 || self.refactor_interval == 0 {
            return Err(Error::InvalidInput(
                "max_factor_entries and refactor_interval must be positive".into(),
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

/// Solver with a single-thread simplex kernel and optional parallel
/// components. Independent instances can share immutable input models.
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
fn objective_scale(model: &Model) -> f64 {
    1.0 + model
        .objective()
        .iter()
        .fold(0.0_f64, |a, &b| a.max(b.abs()))
}
fn accepted(
    model: &Model,
    bounds: &Bounds,
    x: &[f64],
    duals: &lp_ip_craft::Duals,
    options: &Options,
    settings: &NativeOptions,
) -> bool {
    let Ok(r) = check::lp_residuals_with_bounds(model, bounds, x, duals) else {
        return false;
    };
    let scale = objective_scale(model);
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
/// Re-solves the continuous part of the best tolerance-feasible incumbent
/// on a copy of the LP whose rows and continuous boxes are shrunk inward,
/// so that its point can pass the outward-rounded certification and justify
/// pruning. Each distinct candidate is tried once, at most
/// `MAX_CERTIFICATIONS` times per solve.
struct Certifier {
    lp: Option<Lp>,
    attempts: u64,
    certified: u64,
    last: Option<Vec<f64>>,
}
const MAX_CERTIFICATIONS: u64 = 64;

#[allow(clippy::too_many_arguments)]
fn certify(
    model: &Model,
    options: &Options,
    settings: &NativeOptions,
    keep: &[bool],
    certifier: &mut Certifier,
    pool: &mut Incumbents,
    result: &mut SolveResult,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let Some(x) = result.candidate.as_ref() else {
        return Ok(());
    };
    if pool.best >= pool.upper
        || certifier.attempts >= MAX_CERTIFICATIONS
        || certifier.last.as_ref() == Some(x)
        || budget.stop().is_some()
    {
        return Ok(());
    }
    certifier.last = Some(x.clone());
    certifier.attempts += 1;
    let integral: Vec<bool> = model.kinds().iter().map(|k| k.is_integral()).collect();
    let rounded: Vec<f64> = x
        .iter()
        .zip(&integral)
        .map(|(&v, &i)| if i { v.round() } else { v })
        .collect();
    let lp = match &mut certifier.lp {
        Some(lp) => lp,
        none => {
            let columns = presolve::kept_columns(model, model.variables(), settings);
            let mut lp = Lp::new(model, keep, &columns, controls(model, options, settings))?;
            lp.shrink_rows(model);
            none.insert(lp)
        }
    };
    lp.fix_for_certification(&rounded, &integral, model.variables())?;
    if lp.solve(budget, true)? != Status::Optimal {
        return Ok(());
    }
    let mut point = lp.point();
    for j in (0..model.n()).filter(|&j| integral[j]) {
        point[j] = rounded[j];
    }
    let before = pool.upper;
    consider(model, point, options, result, pool)?;
    if pool.upper < before {
        certifier.certified += 1;
    }
    Ok(())
}

fn heuristic(
    model: &Model,
    node_bounds: &Bounds,
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

/// One bound change on the path from the root; shared by all descendants,
/// so an open node costs O(1) memory for its box.
struct Branch {
    parent: Option<Arc<Branch>>,
    j: usize,
    lower: f64,
    upper: f64,
}

/// A branch-and-bound node: its box (as a chain of changes), the parent's
/// conservative bound and, when stored, the parent's optimal basis.
struct Node {
    branch: Option<Arc<Branch>>,
    depth: usize,
    bound: Option<f64>,
    basis: Option<Arc<Basis>>,
    origin: Option<Origin>,
}

/// How a child was created, to learn pseudocosts once it is solved.
#[derive(Clone, Copy)]
struct Origin {
    j: usize,
    up: bool,
    distance: f64,
    parent_objective: f64,
}

/// Average LP objective degradation per unit of bound change, by variable
/// and direction (down, up).
struct Pseudocosts {
    sum: Vec<[f64; 2]>,
    count: Vec<[u32; 2]>,
    total: [f64; 2],
    total_count: [u32; 2],
}
impl Pseudocosts {
    fn new(n: usize) -> Self {
        Self {
            sum: vec![[0.0; 2]; n],
            count: vec![[0; 2]; n],
            total: [0.0; 2],
            total_count: [0; 2],
        }
    }
    fn record(&mut self, origin: Origin, objective: f64) {
        let gain = (objective - origin.parent_objective).max(0.0) / origin.distance;
        if gain.is_finite() && origin.distance > 0.0 {
            let d = usize::from(origin.up);
            self.sum[origin.j][d] += gain;
            self.count[origin.j][d] += 1;
            self.total[d] += gain;
            self.total_count[d] += 1;
        }
    }
    fn known(&self) -> bool {
        self.total_count[0] + self.total_count[1] > 0
    }
    /// Variable average, else the average over all variables, else 1.
    fn estimate(&self, j: usize, up: bool) -> f64 {
        let d = usize::from(up);
        if self.count[j][d] > 0 {
            self.sum[j][d] / f64::from(self.count[j][d])
        } else if self.total_count[d] > 0 {
            self.total[d] / f64::from(self.total_count[d])
        } else {
            1.0
        }
    }
}

/// Open node ordered for best-bound selection: smaller bound first, then
/// deeper, then newer.
struct Open {
    key: f64,
    serial: u64,
    node: Node,
}
impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Open {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .key
            .total_cmp(&self.key)
            .then(self.node.depth.cmp(&other.node.depth))
            .then(self.serial.cmp(&other.serial))
    }
}

/// Memory for basis snapshots held by open nodes; beyond it, open nodes
/// start from the current basis (dual feasible for boxed MIP columns).
const SNAPSHOT_BUDGET_BYTES: usize = 256 << 20;

/// The box of the current node: root bounds plus the applied changes.
struct NodeBox {
    bounds: Bounds,
    changed: Vec<usize>,
    path: Vec<(usize, f64, f64)>,
}
impl NodeBox {
    /// Moves to `branch`, updating the LP bounds of every touched variable.
    fn enter(&mut self, root: &Bounds, branch: &Option<Arc<Branch>>, lp: &mut Lp) -> Result<()> {
        for &j in &self.changed {
            self.bounds.lower[j] = root.lower[j];
            self.bounds.upper[j] = root.upper[j];
        }
        self.path.clear();
        let mut at = branch.as_deref();
        while let Some(b) = at {
            self.path.push((b.j, b.lower, b.upper));
            at = b.parent.as_deref();
        }
        let previous = std::mem::take(&mut self.changed);
        for &(j, l, u) in self.path.iter().rev() {
            self.bounds.lower[j] = l;
            self.bounds.upper[j] = u;
            self.changed.push(j);
        }
        for &j in previous.iter().chain(&self.changed) {
            lp.set_bound(j, self.bounds.lower[j], self.bounds.upper[j])?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Counters {
    warm_nodes: u64,
    cold_solves: u64,
    fallbacks: u64,
    unverified_infeasible: u64,
    heuristic_candidates: u64,
    pruned_nodes: u64,
    propagated_infeasible: u64,
    implied_bounds: u64,
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
        if self.settings.presolve && model.m() == 0 && !model.is_mip() {
            return boxed::solve(model, options, &self.settings, start, cancel, call_started);
        }
        let original_model = model;
        let strengthened = if self.settings.presolve
            && !cancel.is_cancelled()
            && self.settings.time_limit_seconds != Some(0.)
        {
            presolve::strengthen_integer_rows(model)?
        } else {
            None
        };
        let model = strengthened.as_ref().unwrap_or(model);
        // A tolerance-feasible start can violate an exact MIP strengthening.
        // Preserve it under the original contract and classify any conflict
        // in the single solve, rather than losing it in component conversion.
        let preserve_start = match (strengthened.as_ref(), start) {
            (Some(_), Some(s)) => {
                check::evaluate(original_model, &s.primal, &options.tolerances)?.feasible_candidate
                    && !check::evaluate(model, &s.primal, &options.tolerances)?.feasible_candidate
            }
            _ => false,
        };
        if self.settings.decompose
            && !preserve_start
            && model.nnz() > 0
            && model.n() >= self.settings.decomposition_min_variables
            && !cancel.is_cancelled()
            && self.settings.time_limit_seconds != Some(0.)
        {
            if let Some(mut out) = components::solve_if_separable(
                &self.settings,
                model,
                options,
                start,
                cancel,
                call_started,
            )? {
                if strengthened.is_some() {
                    if let Some(x) = &out.candidate {
                        out.evaluation =
                            Some(check::evaluate(original_model, x, &options.tolerances)?);
                    }
                }
                return Ok(out);
            }
        }
        let settings = &self.settings;
        let mut budget = Budget {
            start: call_started,
            settings,
            cancel,
            pivots: 0,
        };
        let mut report = BackendReport::new("native");
        report.version = Some(env!("CARGO_PKG_VERSION").into());
        report.effective_options = effective_options(settings);
        report.start_status = match start {
            None => StartStatus::NotProvided,
            Some(_) if model.is_mip() => StartStatus::Evaluated,
            Some(_) => StartStatus::Rejected,
        };
        let mut result = SolveResult::empty(Status::Unknown, "", report);
        let mut pool = Incumbents {
            best: f64::INFINITY,
            upper: f64::INFINITY,
            certified: None,
        };
        let lattice = settings.objective_lattice && incumbent::integer_lattice(model);
        let mut counters = Counters::default();
        if model.is_mip() {
            if let Some(s) = start {
                consider(
                    original_model,
                    s.primal.clone(),
                    options,
                    &mut result,
                    &mut pool,
                )?;
            }
        }
        let domain = propagate::Domain::new(model, settings.presolve);
        let root = model.variables();
        let mut node_box = NodeBox {
            bounds: root.clone(),
            changed: Vec::new(),
            path: Vec::new(),
        };
        let mut open = std::collections::BinaryHeap::new();
        let (mut serial, mut snapshot_bytes) = (0u64, 0usize);
        let mut dive = Some(Node {
            branch: None,
            depth: 0,
            bound: None,
            basis: None,
            origin: None,
        });
        let mut pseudocosts = Pseudocosts::new(model.n());
        let mut certifier = Certifier {
            lp: None,
            attempts: 0,
            certified: 0,
            last: None,
        };
        let keep = presolve::kept_rows(model, root, settings);
        let mut closed_bound = f64::INFINITY;
        let mut all_bounds = true;
        let mut status = Status::Optimal;
        let mut lp: Option<Lp> = None;
        let (mut removed_rows, mut removed_columns) = (0usize, 0usize);
        loop {
            let node = match dive.take() {
                Some(node) => node,
                None => match open.pop() {
                    Some(Open { node, .. }) => {
                        if node.basis.is_some() {
                            snapshot_bytes -= lp.as_ref().map_or(0, Lp::snapshot_bytes);
                        }
                        node
                    }
                    None => break,
                },
            };
            let stop = budget.stop().or_else(|| {
                (model.is_mip() && settings.node_limit.is_some_and(|n| result.nodes >= n))
                    .then_some(Status::NodeLimit)
            });
            if let Some(s) = stop {
                status = s;
                serial += 1;
                open.push(Open {
                    key: node.bound.unwrap_or(f64::NEG_INFINITY),
                    serial,
                    node,
                });
                break;
            }
            if node.bound.is_some_and(|b| b >= pool.upper) {
                closed_bound = closed_bound.min(node.bound.unwrap());
                counters.pruned_nodes += 1;
                continue;
            }
            let conversion = Instant::now();
            let fresh = lp.is_none();
            let lp = match &mut lp {
                Some(lp) => lp,
                None => {
                    let columns = presolve::kept_columns(model, root, settings);
                    let built = lp.insert(Lp::new(
                        model,
                        &keep,
                        &columns,
                        controls(model, options, settings),
                    )?);
                    removed_rows = built.removed_rows(model);
                    removed_columns = built.removed_columns();
                    built
                }
            };
            node_box.enter(root, &node.branch, lp)?;
            let before = node_box.changed.len();
            if !domain.apply(&mut node_box.bounds, &mut node_box.changed, &budget) {
                counters.propagated_infeasible += 1;
                if node.depth == 0 {
                    status = if result.candidate.is_some() {
                        Status::NumericalFailure
                    } else {
                        Status::Infeasible
                    };
                    break;
                }
                continue;
            }
            for &j in &node_box.changed[before..] {
                lp.set_bound(j, node_box.bounds.lower[j], node_box.bounds.upper[j])?;
            }
            counters.implied_bounds += (node_box.changed.len() - before) as u64;
            let warm = if !settings.node_reoptimization {
                if !fresh {
                    lp.reset_to_slack();
                }
                false
            } else {
                if let Some(basis) = &node.basis {
                    lp.restore(basis);
                }
                !fresh
            };
            result.backend.conversion_seconds += conversion.elapsed().as_secs_f64();
            let solve = Instant::now();
            if warm {
                counters.warm_nodes += 1;
            } else {
                counters.cold_solves += 1;
            }
            let node_bounds = &node_box.bounds;
            let mut s = lp.solve(&mut budget, !warm)?;
            let mut checked = false;
            if warm && budget.stop().is_none() {
                let retry = match s {
                    Status::Optimal => {
                        checked = true;
                        !accepted(
                            model,
                            node_bounds,
                            &lp.point(),
                            &lp.duals(model),
                            options,
                            settings,
                        )
                    }
                    Status::Infeasible => !lp.infeasibility_verified(),
                    Status::NumericalFailure => true,
                    _ => false,
                };
                if retry {
                    counters.fallbacks += 1;
                    counters.cold_solves += 1;
                    checked = false;
                    lp.reset_to_slack();
                    s = lp.solve(&mut budget, true)?;
                }
            }
            if s == Status::Infeasible && !lp.infeasibility_verified() {
                counters.unverified_infeasible += 1;
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
                serial += 1;
                open.push(Open {
                    key: node.bound.unwrap_or(f64::NEG_INFINITY),
                    serial,
                    node: Node {
                        basis: None,
                        ..node
                    },
                });
                break;
            }
            let checks = Instant::now();
            let x = lp.point();
            let duals = lp.duals(model);
            if !checked && !accepted(model, node_bounds, &x, &duals, options, settings) {
                status = Status::NumericalFailure;
                all_bounds = false;
                break;
            }
            let bound = safe_dual_bound_with_bounds(model, node_bounds, &duals.row_duals)?
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
            let objective = oriented(
                model,
                model.objective().iter().zip(&x).map(|(c, v)| c * v).sum(),
            );
            if let Some(origin) = node.origin {
                pseudocosts.record(origin, objective);
            }
            let branch = branching_variable(model, node_bounds, &x, options, &pseudocosts);
            if branch.is_some()
                && settings.incumbent_heuristic
                && (result.nodes == 1 || !pool.upper.is_finite())
            {
                counters.heuristic_candidates += heuristic(
                    model,
                    node_bounds,
                    &x,
                    options,
                    &mut result,
                    &mut pool,
                    &budget,
                )?;
            }
            certify(
                model,
                options,
                settings,
                &keep,
                &mut certifier,
                &mut pool,
                &mut result,
                &mut budget,
            )?;
            if bound.is_some_and(|b| b >= pool.upper) {
                closed_bound = closed_bound.min(bound.unwrap());
                counters.pruned_nodes += 1;
                result.backend.check_seconds += checks.elapsed().as_secs_f64();
                continue;
            }
            if let Some(j) = branch {
                let (l, u) = (node_bounds.lower[j], node_bounds.upper[j]);
                let (floor, ceil) = (x[j].floor(), x[j].ceil());
                let child = |lower: f64, upper: f64, up: bool| {
                    (lower <= upper).then(|| Node {
                        branch: Some(Arc::new(Branch {
                            parent: node.branch.clone(),
                            j,
                            lower,
                            upper,
                        })),
                        depth: node.depth + 1,
                        bound,
                        basis: None,
                        origin: Some(Origin {
                            j,
                            up,
                            distance: if up { ceil - x[j] } else { x[j] - floor },
                            parent_objective: objective,
                        }),
                    })
                };
                let (down, up) = (child(l, u.min(floor), false), child(l.max(ceil), u, true));
                // Dive into the child nearer to the LP value, which starts
                // from the unchanged factorized basis; the other waits.
                let (next, other) = if x[j] - floor >= 0.5 {
                    (up, down)
                } else {
                    (down, up)
                };
                if let Some(mut other) = other {
                    let size = lp.snapshot_bytes();
                    if settings.node_reoptimization
                        && snapshot_bytes + size <= SNAPSHOT_BUDGET_BYTES
                    {
                        other.basis = Some(Arc::new(lp.snapshot()));
                        snapshot_bytes += size;
                    }
                    serial += 1;
                    open.push(Open {
                        key: bound.unwrap_or(f64::NEG_INFINITY),
                        serial,
                        node: other,
                    });
                }
                dive = next;
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
                certify(
                    model,
                    options,
                    settings,
                    &keep,
                    &mut certifier,
                    &mut pool,
                    &mut result,
                    &mut budget,
                )?;
                if let Some(b) = bound {
                    closed_bound = closed_bound.min(b);
                } else {
                    all_bounds = false;
                }
            }
            result.backend.check_seconds += checks.elapsed().as_secs_f64();
        }
        if model.is_mip() {
            for o in open.iter().map(|o| &o.node).chain(dive.as_ref()) {
                if let Some(b) = o.bound {
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
                if gap < -settings.dual_tolerance * (1.0 + e.objective.abs()) {
                    if let Some((x, e)) = pool.certified {
                        let gap = oriented(model, e.objective) - oriented(model, b);
                        if gap >= -settings.dual_tolerance * (1.0 + e.objective.abs()) {
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
        let stats = lp.as_ref().map(|lp| lp.stats).unwrap_or_default();
        result.backend.effective_options.extend(
            [
                ("factor_peak_entries", stats.peak_entries as u64),
                ("refactorizations", stats.refactors),
                ("basis_updates", stats.updates),
                ("bound_flips", stats.bound_flips),
                ("basis_repairs", stats.repairs),
                ("basis_restores", stats.restores),
                ("cost_perturbations", stats.perturbations),
                ("component_count", 1),
                ("component_workers", 1),
                ("reoptimized_nodes", counters.warm_nodes),
                ("presolve_removed_rows", removed_rows as u64),
                ("presolve_removed_columns", removed_columns as u64),
                ("cold_lp_solves", counters.cold_solves),
                ("cold_fallbacks", counters.fallbacks),
                (
                    "unverified_infeasible_nodes",
                    counters.unverified_infeasible,
                ),
                ("heuristic_candidates", counters.heuristic_candidates),
                ("pruned_nodes", counters.pruned_nodes),
                (
                    "propagated_infeasible_nodes",
                    counters.propagated_infeasible,
                ),
                ("implied_bounds", counters.implied_bounds),
                ("certification_solves", certifier.attempts),
                ("certified_incumbents", certifier.certified),
            ]
            .map(|(k, v)| (format!("diagnostics.{k}"), v.to_string())),
        );
        result
            .backend
            .effective_options
            .push(("diagnostics.objective_lattice".into(), lattice.to_string()));
        if let Some(x) = result.candidate.as_ref().filter(|_| strengthened.is_some()) {
            result.evaluation = Some(check::evaluate(original_model, x, &options.tolerances)?);
        }
        result.status = status;
        result.iterations = budget.pivots;
        result.message = format!(
            "Native dual simplex numerical solve: {} (not an exact certificate)",
            status.as_str()
        );
        result.backend.raw_status = Some(status.as_str().into());
        result.backend.total_seconds = budget.start.elapsed().as_secs_f64();
        result.backend.time_budget_exceeded = settings
            .time_limit_seconds
            .is_some_and(|t| result.backend.total_seconds > t);
        Ok(result)
    }
}

/// Branching variable among the fractional integer variables: best
/// pseudocost product score once any history exists, else most fractional.
/// If rounding an integral-looking LP point does not certify, any integer
/// variable with a nonzero fraction that can still be split inside its box.
fn branching_variable(
    model: &Model,
    bounds: &Bounds,
    x: &[f64],
    options: &Options,
    pseudocosts: &Pseudocosts,
) -> Option<usize> {
    let fraction = |j: usize| (j, (x[j] - x[j].round()).abs());
    let most = |a: &(usize, f64), b: &(usize, f64)| a.1.total_cmp(&b.1);
    let integral = (0..model.n()).filter(|&j| model.kinds()[j].is_integral());
    let fractional = integral
        .clone()
        .map(fraction)
        .filter(|&(_, f)| f > options.tolerances.integrality);
    let chosen = if pseudocosts.known() {
        fractional
            .map(|(j, f)| {
                let down = x[j] - x[j].floor();
                let score = (pseudocosts.estimate(j, false) * down).max(1e-6)
                    * (pseudocosts.estimate(j, true) * (1.0 - down)).max(1e-6);
                (j, score + 1e-12 * f)
            })
            .max_by(most)
    } else {
        fractional.max_by(most)
    };
    if let Some((j, _)) = chosen {
        return Some(j);
    }
    let mut rounded = x.to_vec();
    for j in integral.clone() {
        rounded[j] = rounded[j].round();
    }
    if incumbent::upper_with_bounds(model, bounds, &rounded).is_some() {
        return None;
    }
    integral
        .filter(|&j| x[j].floor() >= bounds.lower[j] && x[j].ceil() <= bounds.upper[j])
        .map(fraction)
        .filter(|&(_, f)| f > 0.0)
        .max_by(most)
        .map(|(j, _)| j)
}

fn controls(model: &Model, options: &Options, settings: &NativeOptions) -> lp::Controls {
    lp::Controls {
        pivot_tolerance: settings.pivot_tolerance,
        refactor_interval: settings.refactor_interval,
        max_factor_entries: settings.max_factor_entries,
        steepest_edge: settings.steepest_edge,
        feasibility: options.tolerances.feasibility,
        dual: settings.dual_tolerance * objective_scale(model),
    }
}

fn effective_options(s: &NativeOptions) -> Vec<(String, String)> {
    [
        (
            "algorithm",
            "dual_simplex+best_bound_diving_branch_and_bound".to_string(),
        ),
        (
            "linear_kernel",
            "bounded_dual_revised_simplex+markowitz_lu".into(),
        ),
        ("max_factor_entries", s.max_factor_entries.to_string()),
        ("refactor_interval", s.refactor_interval.to_string()),
        ("steepest_edge", s.steepest_edge.to_string()),
        ("component_threads", s.component_threads.to_string()),
        (
            "component_parallel_min_entries",
            s.component_parallel_min_entries.to_string(),
        ),
        ("decompose", s.decompose.to_string()),
        (
            "decomposition_min_variables",
            s.decomposition_min_variables.to_string(),
        ),
        ("time_limit_seconds", format!("{:?}", s.time_limit_seconds)),
        ("node_limit", format!("{:?}", s.node_limit)),
        ("iteration_limit", format!("{:?}", s.iteration_limit)),
        ("pivot_tolerance", s.pivot_tolerance.to_string()),
        ("dual_tolerance", s.dual_tolerance.to_string()),
        ("presolve", s.presolve.to_string()),
        ("redundant_rows", s.redundant_rows.to_string()),
        (
            "redundant_row_min_ratio",
            s.redundant_row_min_ratio.to_string(),
        ),
        ("node_reoptimization", s.node_reoptimization.to_string()),
        ("incumbent_heuristic", s.incumbent_heuristic.to_string()),
        ("objective_lattice", s.objective_lattice.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}
