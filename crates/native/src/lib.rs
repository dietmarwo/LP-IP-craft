//! Owned, dependency-free Rust LP/MIP solver: a bounded-variable dual
//! revised simplex on a sparse LU basis factorization, inside best-bound
//! branch and bound with diving, pseudocost branching and independently
//! checked results.
#![forbid(unsafe_code)]
mod active;
mod batch;
mod boxed;
mod certified_cuts;
mod components;
mod cuts;
mod dense;
mod diagnostics;
mod ft;
mod incumbent;
mod jump;
mod lp;
mod lu;
mod mip_presolve;
mod mixed;
mod ordering;
mod packing;
mod presolve;
mod primal;
pub mod profile;
mod propagate;
mod rational;
#[cfg(feature = "basis-replay")]
pub mod replay;
mod root;
mod rootcuts;
mod tree;
mod witness;

use lp::{Basis, Lp};
use lp_ip_craft::{
    BackendReport, Bounds, CancellationToken, Duals, Error, LpIpSolver, Model, Options, Result,
    Sense, SolveResult, StartStatus, Status, WarmStart, check, safe_dual_bound_with_bounds,
};
use std::sync::Arc;
use std::time::Instant;

/// Basis-update strategy. Auto chooses from measured factor/update sparsity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BasisUpdate {
    #[default]
    Auto,
    /// Product-form column eta file.
    Eta,
    /// Forrest–Tomlin row updates; unsafe updates fall back to eta and rebuild.
    ForrestTomlin,
}
impl BasisUpdate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Eta => "eta",
            Self::ForrestTomlin => "forrest_tomlin",
        }
    }
}

/// Symbolic column-ordering advice for threshold Markowitz LU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FactorOrdering {
    #[default]
    Auto,
    Natural,
    MinDegree,
}
impl FactorOrdering {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Natural => "natural",
            Self::MinDegree => "min_degree",
        }
    }
}

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
    /// Rebuild large dense bases using accumulated update-work estimates.
    pub adaptive_refactor: bool,
    /// Basis-update strategy; auto adapts to observed update and factor fill.
    pub basis_update: BasisUpdate,
    /// Markowitz candidates examined after a feasible pivot is found (1..=64).
    pub factor_pivot_search: usize,
    /// Optional bounded minimum-degree advice for sparse pivot selection.
    pub factor_ordering: FactorOrdering,
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
    /// Bounded root lookahead on eligible multi-constraint MIPs.
    pub strong_branching: bool,
    /// Bounded early-node probes until directional pseudocosts are reliable.
    pub reliability_branching: bool,
    pub reliability_threshold: u32,
    pub reliability_max_probes: u64,
    /// Fix integer bound values using conservative opposite-domain dual bounds.
    pub reduced_cost_fixing: bool,
    /// Independently checked rounding and bounded unit-move incumbents.
    pub incumbent_heuristic: bool,
    /// Exact rational fixed-integer boundary witnesses, bounded by dimension/bit caps.
    pub rational_certification: bool,
    /// Bounded serial root Feasibility Jump prototype.
    pub feasibility_jump: bool,
    pub feasibility_jump_steps: u64,
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
    /// Add bounded exact binary cover cuts before building LP caches.
    pub root_cuts: bool,
    /// LP-directed root cover separation, rebuilding caches before search.
    pub root_separation: bool,
    pub mip_presolve: bool,
    pub coefficient_tightening: bool,
    pub dual_presolve: bool,
    pub mip_presolve_rounds: usize,
    pub gmi_cuts: bool,
    pub mir_cuts: bool,
    pub mip_cut_rounds: usize,
    pub mip_cut_limit: usize,
    pub cut_exact_work: u64,
    pub root_heuristics: bool,
    pub heuristic_lp_calls: usize,
    pub branching_score: String,
    pub pseudocost_min_step: f64,
    pub branch_trace_limit: usize,
    pub root_cut_rounds: usize,
    pub cover_lifting: bool,
    /// Infer sound mixed MIP root bounds before finite-domain admission.
    pub root_propagation: bool,
    /// Conservative node propagation for finite mixed/nonintegral rows.
    pub mixed_propagation: bool,
    /// Remove exactly reversible singleton rows before factorization.
    pub singleton_presolve: bool,
    /// Keep large near-monolithic models in a single numerical solve.
    pub avoid_dominant_components: bool,
    /// Use bounded Devex-style approximate weights instead of exact DSE maintenance.
    pub devex: bool,
    /// Choose column pricing for dense BTRAN support when estimated work is lower.
    pub adaptive_pricing: bool,
    /// Replay validated numerical pivots for identical basis signatures.
    pub factor_replay: bool,
    /// Collect bounded observational MIP search diagnostics in backend metadata.
    pub mip_diagnostics: bool,
    /// Maximum retained search events (0 means summaries only, at most 4096).
    pub mip_trace_limit: usize,
    // Opt-in controls for binary unit-packing models (maximize, nonnegative
    // weights, rows `sum x <= 1`), bounded probing and the disjoint frontier.
    // All are off by default; docs/spoc4-025 and docs/spoc4-026 give details.
    /// Greedy incumbent plus a factorization-free primal-dual root pass whose
    /// prices give a checked upper bound. Also enables the packing driver.
    pub packing_root: bool,
    /// First-order root steps, at most 100000.
    pub packing_root_iterations: usize,
    /// Root work cap in matrix/vector units, `2*nnz+4*n+4*m` per step.
    pub packing_root_work: u64,
    /// Residual repair tasks per round, at most 4096; positive enables the
    /// packing driver.
    pub packing_neighborhoods: usize,
    /// Maximum columns per residual, 4..=1024.
    pub packing_neighborhood_size: usize,
    /// Residual workers, 1..=16. Fixed-work results do not depend on it.
    pub packing_workers: usize,
    /// Node ceiling per residual task, at most 100000.
    pub packing_neighborhood_nodes: u64,
    /// Hint-directed triangle and odd-cycle cuts before the proof search.
    pub packing_cuts: bool,
    /// Edge-visit cap of that separator, at most 100000000.
    pub packing_cut_work: u64,
    /// Merge compatible improving repairs of one snapshot batch.
    pub packing_combine_repairs: bool,
    /// Repair rounds, 1..=32; `packing_neighborhoods * packing_rounds <= 4096`.
    pub packing_rounds: usize,
    /// Extra residual columns per round, at most 1024 (capacity saturates at 1024).
    pub packing_neighborhood_growth: usize,
    /// Repair seeds: `"incumbent"`, `"rejected"` or `"mixed"`.
    pub packing_seed_strategy: String,
    /// Local clique cuts inside each residual before it is solved.
    pub packing_residual_cliques: bool,
    /// Operation cap of the local clique separator per task, at most 1000000.
    pub packing_clique_work: u64,
    /// Report the best checked root prices as `diagnostics.packing_root_prices`.
    pub packing_price_diagnostics: bool,
    /// Wall-time cap of the repair phase (finite, nonnegative; 0 skips it).
    pub packing_repair_seconds: Option<f64>,
    /// Wall-time cap of the exact search after repairs (finite,
    /// nonnegative; 0 skips it and returns `time_limit`).
    pub packing_proof_seconds: Option<f64>,
    /// Conservative bound from current prices of an interrupted simplex solve.
    pub partial_dual_bounds: bool,
    /// Reliability probing on a bounded shortlist, without the old size gate.
    pub scalable_branching: bool,
    /// Shortlist size of scalable probing, 1..=16.
    pub branch_probe_candidates: usize,
    /// Pivots reserved per probe direction, at most 1024.
    pub branch_probe_pivots: u64,
    /// Cumulative probe-work reservation per serial search, at most 1e10.
    pub branch_probe_work: u64,
    /// Workers for the disjoint binary frontier, 1..=16 (1 disables it).
    pub tree_threads: usize,
    /// Frontier depth, at most 8: `2^depth` disjoint subtrees (0 disables it).
    pub tree_split_depth: usize,
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
            refactor_interval: 256,
            adaptive_refactor: true,
            basis_update: BasisUpdate::Auto,
            factor_pivot_search: 4,
            factor_ordering: FactorOrdering::Auto,
            steepest_edge: true,
            presolve: true,
            redundant_rows: true,
            redundant_row_min_ratio: 1.0,
            node_reoptimization: true,
            strong_branching: false,
            reliability_branching: false,
            reliability_threshold: 2,
            reliability_max_probes: 64,
            reduced_cost_fixing: true,
            incumbent_heuristic: true,
            rational_certification: true,
            feasibility_jump: false,
            feasibility_jump_steps: 2000,
            objective_lattice: true,
            decompose: true,
            decomposition_min_variables: 32,
            component_threads: 16,
            component_parallel_min_entries: 2048,
            root_cuts: false,
            root_separation: false,
            mip_presolve: false,
            coefficient_tightening: false,
            dual_presolve: false,
            mip_presolve_rounds: 8,
            gmi_cuts: false,
            mir_cuts: false,
            mip_cut_rounds: 8,
            mip_cut_limit: 16,
            cut_exact_work: 4_000_000,
            root_heuristics: false,
            heuristic_lp_calls: 16,
            branching_score: "weighted_sum".into(),
            pseudocost_min_step: 0.,
            branch_trace_limit: 0,
            root_cut_rounds: 2,
            cover_lifting: true,
            root_propagation: true,
            mixed_propagation: false,
            singleton_presolve: true,
            avoid_dominant_components: true,
            devex: false,
            adaptive_pricing: true,
            factor_replay: false,
            mip_diagnostics: false,
            mip_trace_limit: 256,
            packing_root: false,
            packing_root_iterations: 1000,
            packing_root_work: 200_000_000,
            packing_neighborhoods: 0,
            packing_neighborhood_size: 192,
            packing_workers: 1,
            packing_neighborhood_nodes: 128,
            packing_cuts: false,
            packing_cut_work: 2_000_000,
            packing_combine_repairs: false,
            packing_rounds: 1,
            packing_neighborhood_growth: 0,
            packing_seed_strategy: "incumbent".into(),
            packing_residual_cliques: false,
            packing_clique_work: 50_000,
            packing_price_diagnostics: false,
            packing_repair_seconds: None,
            packing_proof_seconds: None,
            partial_dual_bounds: false,
            scalable_branching: false,
            branch_probe_candidates: 4,
            branch_probe_pivots: 32,
            branch_probe_work: 2_000_000,
            tree_threads: 1,
            tree_split_depth: 2,
        }
    }
}
impl NativeOptions {
    /// Validate settings before cancellation or numerical work.
    pub fn validate(&self) -> Result<()> {
        for (valid, message) in [
            (
                self.mip_presolve_rounds <= 32,
                "mip_presolve_rounds must be at most 32",
            ),
            (
                self.mip_cut_rounds <= 50,
                "mip_cut_rounds must be at most 50",
            ),
            (
                (1..=64).contains(&self.mip_cut_limit),
                "mip_cut_limit must lie in 1..=64",
            ),
            (
                self.cut_exact_work <= 100_000_000,
                "cut_exact_work must be at most 100000000",
            ),
            (
                self.heuristic_lp_calls <= 128,
                "heuristic_lp_calls must be at most 128",
            ),
            (
                self.branch_trace_limit <= 256,
                "branch_trace_limit must be at most 256",
            ),
            (
                ["weighted_sum", "product"].contains(&self.branching_score.as_str()),
                "branching_score must be weighted_sum or product",
            ),
            (
                self.pseudocost_min_step.is_finite()
                    && (0.0..=0.5).contains(&self.pseudocost_min_step),
                "pseudocost_min_step must be finite in 0..=0.5",
            ),
        ] {
            if !valid {
                return Err(Error::InvalidInput(message.into()));
            }
        }
        let phase = |t: Option<f64>| t.is_none_or(|t| t.is_finite() && t >= 0.);
        for (ok, message) in [
            (
                (1..=32).contains(&self.packing_rounds),
                "packing_rounds must lie in 1..=32",
            ),
            (
                self.packing_neighborhoods
                    .saturating_mul(self.packing_rounds)
                    <= 4096,
                "packing_neighborhoods * packing_rounds must be at most 4096",
            ),
            (
                self.packing_neighborhood_growth <= 1024,
                "packing_neighborhood_growth must be at most 1024",
            ),
            (
                self.packing_clique_work <= 1_000_000,
                "packing_clique_work must be at most 1000000",
            ),
            (
                ["incumbent", "rejected", "mixed"].contains(&self.packing_seed_strategy.as_str()),
                "packing_seed_strategy must be incumbent, rejected or mixed",
            ),
            (
                phase(self.packing_repair_seconds),
                "packing_repair_seconds must be finite and nonnegative",
            ),
            (
                phase(self.packing_proof_seconds),
                "packing_proof_seconds must be finite and nonnegative",
            ),
            (
                self.packing_root_iterations <= 100_000,
                "packing_root_iterations must be at most 100000",
            ),
            (
                self.packing_root_work <= 10_000_000_000,
                "packing_root_work must be at most 10000000000",
            ),
            (
                self.packing_neighborhoods <= 4096,
                "packing_neighborhoods must be at most 4096",
            ),
            (
                (4..=1024).contains(&self.packing_neighborhood_size),
                "packing_neighborhood_size must lie in 4..=1024",
            ),
            (
                (1..=16).contains(&self.packing_workers),
                "packing_workers must lie in 1..=16",
            ),
            (
                self.packing_neighborhood_nodes <= 100_000,
                "packing_neighborhood_nodes must be at most 100000",
            ),
            (
                self.packing_cut_work <= 100_000_000,
                "packing_cut_work must be at most 100000000",
            ),
            (
                (1..=16).contains(&self.branch_probe_candidates),
                "branch_probe_candidates must lie in 1..=16",
            ),
            (
                self.branch_probe_pivots <= 1024,
                "branch_probe_pivots must be at most 1024",
            ),
            (
                self.branch_probe_work <= 10_000_000_000,
                "branch_probe_work must be at most 10000000000",
            ),
            (
                (1..=16).contains(&self.tree_threads),
                "tree_threads must lie in 1..=16",
            ),
            (
                self.tree_split_depth <= 8,
                "tree_split_depth must be at most 8",
            ),
        ] {
            if !ok {
                return Err(Error::InvalidInput(message.into()));
            }
        }
        if !(1..=16).contains(&self.reliability_threshold) || self.reliability_max_probes > 1024 {
            return Err(Error::InvalidInput(
                "reliability threshold must be 1..=16 and max probes at most 1024".into(),
            ));
        }
        if self.feasibility_jump_steps > 100_000 {
            return Err(Error::InvalidInput(
                "feasibility_jump_steps must be at most 100000".into(),
            ));
        }
        if self.root_cut_rounds > 4 {
            return Err(Error::InvalidInput(
                "root_cut_rounds must be at most 4".into(),
            ));
        }
        if self.mip_trace_limit > 4096 {
            return Err(Error::InvalidInput(
                "mip_trace_limit must be at most 4096".into(),
            ));
        }
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
        if !(1..=64).contains(&self.factor_pivot_search) {
            return Err(Error::InvalidInput(
                "factor_pivot_search must be between 1 and 64".into(),
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

struct Incumbents<'a> {
    best: f64,
    upper: f64,
    certified: Option<(Vec<f64>, lp_ip_craft::Evaluation)>,
    trace: Option<diagnostics::SearchDiagnostics>,
    original_model: &'a Model,
    pivots: u64,
}
fn consider(
    model: &Model,
    x: Vec<f64>,
    options: &Options,
    result: &mut SolveResult,
    pool: &mut Incumbents<'_>,
    source: &'static str,
) -> Result<bool> {
    let e = check::evaluate(model, &x, &options.tolerances)?;
    if !e.feasible_candidate {
        return Ok(false);
    }
    let previous_upper = pool.upper;
    if let Some(upper) = incumbent::upper(model, &x) {
        if upper < pool.upper {
            pool.upper = upper;
            pool.certified = Some((x.clone(), e.clone()));
        }
    }
    if let Some(trace) = &mut pool.trace {
        // Root reductions do not change columns. Check the user's model too;
        // a diagnostic must not confuse reduced-model and original acceptance.
        if let Ok(original) = check::evaluate(pool.original_model, &x, &options.tolerances) {
            if original.feasible_candidate {
                trace.candidate(
                    original.objective,
                    (pool.upper < previous_upper).then(|| oriented(model, pool.upper)),
                    source,
                    result.nodes,
                    pool.pivots,
                );
            } else {
                trace.original_check_rejections += 1;
            }
        } else {
            trace.original_check_rejections += 1;
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
    lp_attempts: u64,
    witness_attempts: u64,
    witness_successes: u64,
    witness_work: u64,
}
const MAX_CERTIFICATIONS: u64 = 64;
/// Exact-arithmetic limb operations for all rational witness attempts of one
/// solve; see [`witness::ATTEMPT_WORK`] for the per-attempt cap.
const WITNESS_WORK: u64 = 64_000_000;

#[allow(clippy::too_many_arguments)]
fn certify(
    model: &Model,
    options: &Options,
    settings: &NativeOptions,
    keep: &[bool],
    certifier: &mut Certifier,
    pool: &mut Incumbents<'_>,
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
    let remaining = WITNESS_WORK.saturating_sub(certifier.witness_work);
    let witness = if settings.rational_certification && remaining > 0 {
        certifier.witness_attempts += 1;
        let limit = remaining.min(witness::ATTEMPT_WORK);
        let (witness, work) = witness::upper(pool.original_model, x, budget, limit);
        certifier.witness_work += work;
        witness
    } else {
        None
    };
    if let Some(witness) = witness {
        let point = witness.point;
        let upper = witness.upper;
        let evaluation = check::evaluate(model, &point, &options.tolerances)?;
        if upper < pool.upper
            && evaluation.feasible_candidate
            && check::evaluate(pool.original_model, &point, &options.tolerances)?.feasible_candidate
        {
            pool.upper = upper;
            pool.certified = Some((point.clone(), evaluation.clone()));
            let objective = oriented(model, evaluation.objective);
            if objective < pool.best {
                pool.best = objective;
                result.candidate = Some(point);
                result.evaluation = Some(evaluation);
            }
            certifier.witness_successes += 1;
            if let Some(trace) = &mut pool.trace {
                trace.candidate(
                    oriented(model, objective),
                    Some(oriented(model, upper)),
                    "rational_witness",
                    result.nodes,
                    budget.pivots,
                );
            }
            return Ok(());
        }
    }
    let integral: Vec<bool> = model.kinds().iter().map(|k| k.is_integral()).collect();
    let rounded: Vec<f64> = x
        .iter()
        .zip(&integral)
        .map(|(&v, &i)| if i { v.round() } else { v })
        .collect();
    certifier.lp_attempts += 1;
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
    pool.pivots = budget.pivots;
    consider(model, point, options, result, pool, "certification")?;
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
    pool: &mut Incumbents<'_>,
    budget: &Budget<'_>,
) -> Result<u64> {
    let mut found = 0;
    if let Some(trace) = &mut pool.trace {
        trace.heuristic_calls += 1;
    }
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
        if let Some(trace) = &mut pool.trace {
            trace.rounding_attempts += 1;
        }
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
        if !consider(model, point.clone(), options, result, pool, "rounding")? {
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
                || !consider(model, point.clone(), options, result, pool, "unit_move")?
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
    min_step: f64,
}
impl Pseudocosts {
    fn new(n: usize) -> Self {
        Self {
            sum: vec![[0.0; 2]; n],
            count: vec![[0; 2]; n],
            total: [0.0; 2],
            total_count: [0; 2],
            min_step: 0.,
        }
    }
    fn record(&mut self, origin: Origin, objective: f64) {
        let gain =
            (objective - origin.parent_objective).max(0.0) / origin.distance.max(self.min_step);
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
    /// Variable observations smoothed by two observations of the global mean.
    fn estimate(&self, j: usize, up: bool) -> f64 {
        let d = usize::from(up);
        let prior = if self.total_count[d] > 0 {
            self.total[d] / f64::from(self.total_count[d])
        } else {
            1.
        };
        // Two global observations keep a single degenerate child from making
        // a variable appear permanently free to branch on in one direction.
        (self.sum[j][d] + 2. * prior) / (f64::from(self.count[j][d]) + 2.)
    }
}

/// Open node ordered for best-bound selection: smaller bound first, then
/// deeper, then newer.
struct Open {
    key: f64,
    serial: u64,
    node: Node,
}

/// Observational lower bound in internally minimized objective units. The
/// heap puts every missing bound first; no scan or estimate is needed.
fn diagnostic_frontier_bound(
    closed: f64,
    all_closed_known: bool,
    open: &std::collections::BinaryHeap<Open>,
    current: Option<Option<f64>>,
) -> Option<f64> {
    if !all_closed_known {
        return None;
    }
    let mut bound = closed;
    if let Some(node) = open.peek() {
        bound = bound.min(node.node.bound?);
    }
    if let Some(node_bound) = current {
        bound = bound.min(node_bound?);
    }
    bound.is_finite().then_some(bound)
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
    strong_probes: u64,
    strong_pivots: u64,
    strong_work: u64,
    strong_work_exits: u64,
    reduced_cost_fixings: u64,
    branch_events: Vec<String>,
}

impl NativeSolver {
    fn solve_internal(
        &mut self,
        model: &Model,
        options: &Options,
        start: Option<&WarmStart>,
        cancel: &CancellationToken,
        diagnostic_original: &Model,
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
            trace: (settings.mip_diagnostics && model.is_mip()).then(|| {
                diagnostics::SearchDiagnostics::new(model, call_started, settings.mip_trace_limit)
            }),
            original_model: diagnostic_original,
            pivots: 0,
        };
        let lattice = settings.objective_lattice && incumbent::integer_lattice(model);
        let mut counters = Counters::default();
        let mut jump_stats = jump::Stats::default();
        if model.is_mip() {
            if let Some(s) = start {
                consider(
                    original_model,
                    s.primal.clone(),
                    options,
                    &mut result,
                    &mut pool,
                    "start",
                )?;
            }
        }
        let mut domain = propagate::Domain::new(model, settings.presolve);
        let mut mixed = mixed::Domain::new(model, settings.presolve && settings.mixed_propagation);
        if pool.trace.is_some() && settings.presolve {
            domain.enable_statistics();
        }
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
        pseudocosts.min_step = settings.pseudocost_min_step;
        let mut certifier = Certifier {
            lp: None,
            attempts: 0,
            certified: 0,
            last: None,
            lp_attempts: 0,
            witness_attempts: 0,
            witness_successes: 0,
            witness_work: 0,
        };
        let keep = presolve::kept_rows(model, root, settings);
        let mut closed_bound = f64::INFINITY;
        let mut all_bounds = true;
        let mut status = Status::Optimal;
        let mut lp: Option<Lp> = None;
        let (mut removed_rows, mut removed_columns) = (0usize, 0usize);
        loop {
            if let Some(trace) = &mut pool.trace {
                trace.checkpoint(
                    result.nodes,
                    budget.pivots,
                    diagnostic_frontier_bound(
                        closed_bound,
                        all_bounds,
                        &open,
                        dive.as_ref().map(|n| n.bound),
                    )
                    .map(|b| oriented(model, b)),
                    open.len() + usize::from(dive.is_some()),
                );
            }
            let mut node = match dive.take() {
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
                if let Some(trace) = &mut pool.trace {
                    trace.enter_node(node.depth, open.len() + 1);
                    trace.inherited_prunes += 1;
                }
                closed_bound = closed_bound.min(node.bound.unwrap());
                counters.pruned_nodes += 1;
                continue;
            }
            if let Some(trace) = &mut pool.trace {
                trace.enter_node(node.depth, open.len() + 1);
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
            let mut feasible = domain.apply(&mut node_box.bounds, &mut node_box.changed, &budget);
            if feasible && settings.mixed_propagation {
                for _ in 0..2 {
                    let previous = node_box.changed.len();
                    feasible =
                        mixed.apply(model, &mut node_box.bounds, &mut node_box.changed, &budget)
                            && domain.apply(&mut node_box.bounds, &mut node_box.changed, &budget);
                    if !feasible || previous == node_box.changed.len() {
                        break;
                    }
                }
            }
            if !feasible {
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
            let before_pivots = budget.pivots;
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
                    if let Some(trace) = &mut pool.trace {
                        trace.node_lp_solves += 1;
                    }
                }
            }
            if s == Status::Infeasible && !lp.infeasibility_verified() {
                counters.unverified_infeasible += 1;
            }
            let node_seconds = solve.elapsed().as_secs_f64();
            result.backend.solve_seconds += node_seconds;
            if let Some(trace) = &mut pool.trace {
                trace.node_lp_solves += 1;
                trace.node_lp_pivots += budget.pivots - before_pivots;
                trace.node_lp_seconds += node_seconds;
                if node.depth == 0 {
                    trace.root_lp(s, budget.pivots - before_pivots, node_seconds);
                }
                if s == Status::Infeasible {
                    trace.infeasible_lps += 1;
                }
            }
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
                if settings.partial_dual_bounds {
                    let duals = lp.duals(model);
                    if duals.row_duals.iter().all(|v| v.is_finite()) {
                        if let Some(value) =
                            safe_dual_bound_with_bounds(model, node_bounds, &duals.row_duals)?.bound
                        {
                            let b = oriented(model, value);
                            let b = if lattice { b.ceil() } else { b };
                            node.bound = Some(node.bound.map_or(b, |old| old.max(b)));
                            if !model.is_mip() {
                                result.best_bound = Some(oriented(model, b));
                            }
                        }
                    }
                }
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
                #[cfg(feature = "kernel-profile")]
                if std::env::var_os("LPIP_TRACE_SIMPLEX").is_some() {
                    eprintln!(
                        "acceptance failure {:?}",
                        check::lp_residuals_with_bounds(model, node_bounds, &x, &duals)
                    );
                }
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
            pool.pivots = budget.pivots;
            if let Some(trace) = &mut pool.trace {
                trace.checkpoint(
                    result.nodes,
                    budget.pivots,
                    diagnostic_frontier_bound(closed_bound, all_bounds, &open, Some(bound))
                        .map(|b| oriented(model, b)),
                    open.len() + 1,
                );
                if node.depth == 0 {
                    let fractional = (0..model.n())
                        .filter(|&j| {
                            model.kinds()[j].is_integral()
                                && (x[j] - x[j].round()).abs() > options.tolerances.integrality
                        })
                        .count();
                    trace.root_solution(
                        result.nodes,
                        budget.pivots,
                        model.objective_offset() + oriented(model, objective),
                        bound.map(|b| oriented(model, b)),
                        fractional,
                    );
                }
            }
            if let Some(origin) = node.origin {
                pseudocosts.record(origin, objective);
            }
            let branch =
                branching_variable(model, node_bounds, &x, options, &pseudocosts, settings);
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
            if node.depth == 0
                && branch.is_some()
                && settings.feasibility_jump
                && budget.stop().is_none()
            {
                let (proposal, stats) = jump::search(
                    model,
                    node_bounds,
                    &x,
                    &budget,
                    settings.feasibility_jump_steps,
                );
                jump_stats = stats;
                if let Some(point) = proposal {
                    consider(
                        model,
                        point,
                        options,
                        &mut result,
                        &mut pool,
                        "feasibility_jump",
                    )?;
                }
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
                if let Some(trace) = &mut pool.trace {
                    trace.solved_prunes += 1;
                }
                closed_bound = closed_bound.min(bound.unwrap());
                counters.pruned_nodes += 1;
                result.backend.check_seconds += checks.elapsed().as_secs_f64();
                continue;
            }
            let mut probe_seconds = 0.;
            let branch = if branch.is_some()
                && (((settings.reliability_branching || settings.scalable_branching)
                    && node.depth <= 8
                    && counters.strong_probes < settings.reliability_max_probes)
                    || (node.depth == 0 && settings.strong_branching))
                && settings.node_reoptimization
                && (if settings.scalable_branching {
                    true
                } else if settings.reliability_branching {
                    model.n() <= 1024 && model.m() <= 1024 && model.nnz() <= 20000
                } else {
                    eligible_strong_branching(model)
                }) {
                let probe_started = Instant::now();
                let chosen = strong_branch(
                    model,
                    node_bounds,
                    &x,
                    objective,
                    options,
                    lp,
                    &mut pseudocosts,
                    &mut budget,
                    &mut result.nodes,
                    &mut counters,
                )?;
                probe_seconds = probe_started.elapsed().as_secs_f64();
                result.backend.solve_seconds += probe_seconds;
                chosen.or(branch)
            } else {
                branch
            };
            if let Some(j) = branch {
                if counters.branch_events.len() < settings.branch_trace_limit {
                    let mut columns = vec![j];
                    columns.extend(
                        (0..model.n())
                            .filter(|&k| {
                                k != j
                                    && model.kinds()[k].is_integral()
                                    && (x[k] - x[k].round()).abs() > options.tolerances.integrality
                            })
                            .take(15),
                    );
                    let candidates=columns.iter().map(|&k|format!("{{\"column\":{k},\"value\":{},\"down_sum\":{},\"up_sum\":{},\"down_count\":{},\"up_count\":{},\"down_estimate\":{},\"up_estimate\":{}}}",x[k],pseudocosts.sum[k][0],pseudocosts.sum[k][1],pseudocosts.count[k][0],pseudocosts.count[k][1],pseudocosts.estimate(k,false),pseudocosts.estimate(k,true))).collect::<Vec<_>>().join(",");
                    counters.branch_events.push(format!(
                        "{{\"node\":{},\"chosen\":{j},\"candidates\":[{candidates}]}}",
                        result.nodes
                    ));
                }
                let fixes = if settings.reduced_cost_fixing && lattice && node.depth <= 8 {
                    reduced_cost_fixes(model, node_bounds, &x, &duals, pool.upper, bound, &budget)?
                } else {
                    Vec::new()
                };
                let mut child_parent = node.branch.clone();
                if !fixes.is_empty() {
                    closed_bound = closed_bound.min(pool.upper);
                    counters.reduced_cost_fixings += fixes.len() as u64;
                    for (column, value) in fixes {
                        child_parent = Some(Arc::new(Branch {
                            parent: child_parent,
                            j: column,
                            lower: value,
                            upper: value,
                        }));
                    }
                }
                let (l, u) = (node_bounds.lower[j], node_bounds.upper[j]);
                let (floor, ceil) = (x[j].floor(), x[j].ceil());
                let child = |lower: f64, upper: f64, up: bool| {
                    (lower <= upper).then(|| Node {
                        branch: Some(Arc::new(Branch {
                            parent: child_parent.clone(),
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
                pool.pivots = budget.pivots;
                if let Some(trace) = &mut pool.trace {
                    trace.integral_leaves += 1;
                }
                let mut point = x;
                for (j, v) in point.iter_mut().enumerate() {
                    if model.kinds()[j].is_integral() {
                        *v = v.round();
                    }
                }
                if !consider(
                    model,
                    point,
                    options,
                    &mut result,
                    &mut pool,
                    "integral_leaf",
                )? {
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
            result.backend.check_seconds +=
                (checks.elapsed().as_secs_f64() - probe_seconds).max(0.);
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
                ("phase_one_pivots", stats.phase_one_pivots),
                ("degenerate_pivots", stats.degenerate_pivots),
                ("weight_resets", stats.weight_resets),
                ("row_pricings", stats.row_pricings),
                ("column_pricings", stats.column_pricings),
                ("replayed_pivots", stats.replayed_pivots),
                ("basis_updates", stats.updates),
                ("forrest_tomlin_updates", stats.ft_updates),
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
                ("mixed_propagation_rows", mixed.stats.rows),
                ("mixed_propagation_visits", mixed.stats.visits),
                ("mixed_propagation_terms", mixed.stats.terms),
                ("mixed_propagation_changes", mixed.stats.changes),
                ("mixed_propagation_infeasibilities", mixed.stats.infeasible),
                ("mixed_propagation_work_limits", mixed.stats.limits),
                ("feasibility_jump_steps", jump_stats.steps),
                ("feasibility_jump_work", jump_stats.work),
                ("feasibility_jump_candidates", jump_stats.feasible),
                ("feasibility_jump_weight_bumps", jump_stats.bumps),
                ("pruned_nodes", counters.pruned_nodes),
                (
                    "propagated_infeasible_nodes",
                    counters.propagated_infeasible,
                ),
                ("implied_bounds", counters.implied_bounds),
                ("strong_branch_probes", counters.strong_probes),
                ("strong_branch_pivots", counters.strong_pivots),
                ("strong_branch_work", counters.strong_work),
                ("strong_branch_work_exits", counters.strong_work_exits),
                ("reduced_cost_fixings", counters.reduced_cost_fixings),
                ("certification_solves", certifier.lp_attempts),
                ("rational_witness_attempts", certifier.witness_attempts),
                ("rational_witness_successes", certifier.witness_successes),
                ("rational_witness_work", certifier.witness_work),
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
        if let Some(trace) = pool.trace.take() {
            trace.finish(&mut result, domain.statistics(), diagnostic_original, model);
        }
        if settings.branch_trace_limit > 0 {
            result.backend.effective_options.push((
                "diagnostics.branch_trace".into(),
                format!("[{}]", counters.branch_events.join(",")),
            ));
        }
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

/// Each removed opposite domain has a conservative bound >= the certified
/// incumbent. Combining these fixings therefore preserves every improving
/// integer point. No floating-point reduced cost is used as a pruning proof.
fn reduced_cost_fixes(
    model: &Model,
    bounds: &Bounds,
    x: &[f64],
    duals: &Duals,
    upper: f64,
    parent_bound: Option<f64>,
    budget: &Budget<'_>,
) -> Result<Vec<(usize, f64)>> {
    let Some(parent_bound) = parent_bound.filter(|_| upper.is_finite()) else {
        return Ok(Vec::new());
    };
    let _fixing = crate::profile::enter("reduced_cost_fixing");
    let gap = (upper - parent_bound).max(0.);
    let mut trials = Vec::new();
    for (j, &value) in x.iter().enumerate() {
        if !model.kinds()[j].is_integral()
            || value.fract() != 0.
            || bounds.upper[j] - bounds.lower[j] < 1.
        {
            continue;
        }
        let rc = oriented(model, duals.reduced_costs[j]);
        let gain = if value == bounds.lower[j] {
            rc
        } else if value == bounds.upper[j] {
            -rc
        } else {
            continue;
        };
        if gain.is_finite() && gain > 0. && gain >= 0.9 * gap {
            trials.push((j, value, gain));
        }
    }
    trials.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
    trials.truncate(16);
    if trials.is_empty() {
        return Ok(Vec::new());
    }
    let mut prepared = if trials.len() >= 2 {
        let _prepare = crate::profile::enter("fixing_prepare");
        Some(lp_ip_craft::PreparedDualBound::new(
            model,
            bounds,
            &duals.row_duals,
        )?)
    } else {
        None
    };
    let mut opposite = bounds.clone();
    let mut fixes = Vec::new();
    for (j, value, _) in trials {
        if budget.stop().is_some() {
            break;
        }
        let (lo, hi) = (opposite.lower[j], opposite.upper[j]);
        if value == lo {
            opposite.lower[j] = lo + 1.;
        } else {
            opposite.upper[j] = hi - 1.;
        }
        let proof = {
            let _bound = crate::profile::enter("fixing_bound");
            let report = if let Some(cache) = &mut prepared {
                cache.set_bound(j, opposite.lower[j], opposite.upper[j])?;
                cache.bound()
            } else {
                safe_dual_bound_with_bounds(model, &opposite, &duals.row_duals)?
            };
            report.bound.map(|v| oriented(model, v))
        };
        if proof.is_some_and(|b| b >= upper) {
            opposite.lower[j] = value;
            opposite.upper[j] = value;
            fixes.push((j, value));
        } else {
            opposite.lower[j] = lo;
            opposite.upper[j] = hi;
        }
        if let Some(cache) = &mut prepared {
            cache.set_bound(j, opposite.lower[j], opposite.upper[j])?;
        }
    }
    Ok(fixes)
}

/// Limit lookahead to models where several coupled long rows can justify it.
fn eligible_strong_branching(model: &Model) -> bool {
    (12..=256).contains(&model.n())
        && model.m() <= 512
        && model.nnz() <= 10000
        && model
            .matrix()
            .pattern
            .indptr
            .windows(2)
            .filter(|w| w[1] - w[0] > 8)
            .take(2)
            .count()
            == 2
}

#[allow(clippy::too_many_arguments)]
fn strong_branch(
    model: &Model,
    bounds: &Bounds,
    x: &[f64],
    objective: f64,
    options: &Options,
    lp: &mut Lp,
    pseudocosts: &mut Pseudocosts,
    budget: &mut Budget<'_>,
    nodes: &mut u64,
    counters: &mut Counters,
) -> Result<Option<usize>> {
    let reliable = budget.settings.reliability_branching || budget.settings.scalable_branching;
    let eligible = |j: usize| {
        model.kinds()[j].is_integral()
            && (x[j] - x[j].round()).abs() > options.tolerances.integrality
            && (!reliable
                || pseudocosts.count[j]
                    .iter()
                    .any(|&n| n < budget.settings.reliability_threshold))
    };
    let mut candidates = Vec::new();
    if budget.settings.scalable_branching {
        let mut structure = vec![0.; model.n()];
        let a = model.matrix();
        for range in a.pattern.indptr.windows(2) {
            let degree = (range[1] - range[0]).max(1) as f64;
            for &j in &a.pattern.indices[range[0]..range[1]] {
                structure[j] += 1. / degree;
            }
        }
        let score = |j: usize| (x[j] - x[j].round()).abs() * (1. + structure[j]);
        for j in (0..model.n()).filter(|&j| eligible(j)) {
            let at = candidates
                .iter()
                .position(|&k| score(j) > score(k))
                .unwrap_or(candidates.len());
            if at < budget.settings.branch_probe_candidates {
                candidates.insert(at, j);
                candidates.truncate(budget.settings.branch_probe_candidates);
            }
        }
    } else {
        candidates = (0..model.n()).filter(|&j| eligible(j)).collect();
        candidates.sort_by(|&a, &b| {
            (x[b] - x[b].round())
                .abs()
                .total_cmp(&(x[a] - x[a].round()).abs())
                .then(a.cmp(&b))
        });
        candidates.truncate(2);
    }
    // One unreliable candidate is worth probing for its pseudocosts; the
    // legacy root rule keeps its 0.23 behaviour and probes only a real choice.
    if candidates.is_empty() || (!reliable && candidates.len() < 2) {
        return Ok(None);
    }
    let basis = lp.snapshot();
    let mut best = None;
    let mut best_score = -1.;
    for j in candidates {
        let mut gains = [0.; 2];
        let mut complete = true;
        for (d, gain) in gains.iter_mut().enumerate() {
            if budget.stop().is_some()
                || (reliable && counters.strong_probes >= budget.settings.reliability_max_probes)
                || budget.settings.node_limit.is_some_and(|cap| *nodes >= cap)
            {
                complete = false;
                break;
            }
            let up = d == 1;
            let (lo, hi) = if up {
                (bounds.lower[j].max(x[j].ceil()), bounds.upper[j])
            } else {
                (bounds.lower[j], bounds.upper[j].min(x[j].floor()))
            };
            if lo > hi {
                complete = false;
                break;
            }
            let probe_cap = if budget.settings.scalable_branching {
                let unit = (model.nnz() + model.n() + model.m()).max(1) as u64;
                let left = budget
                    .settings
                    .branch_probe_work
                    .saturating_sub(counters.strong_work);
                let cap = budget.settings.branch_probe_pivots.min(left / unit);
                if cap == 0 {
                    counters.strong_work_exits += 1;
                    complete = false;
                    break;
                }
                counters.strong_work += cap * unit;
                cap
            } else {
                32
            };
            lp.set_bound(j, lo, hi)?;
            let mut settings = budget.settings.clone();
            settings.iteration_limit = Some(
                settings
                    .iteration_limit
                    .unwrap_or(u64::MAX)
                    .min(budget.pivots.saturating_add(probe_cap)),
            );
            let mut probe = Budget {
                start: budget.start,
                settings: &settings,
                cancel: budget.cancel,
                pivots: budget.pivots,
            };
            let status = lp.solve(&mut probe, false);
            counters.strong_pivots += probe.pivots - budget.pivots;
            budget.pivots = probe.pivots;
            *nodes += 1;
            counters.strong_probes += 1;
            if status.as_ref().is_ok_and(|s| *s == Status::Optimal) {
                let point = lp.point();
                let value = oriented(
                    model,
                    model
                        .objective()
                        .iter()
                        .zip(&point)
                        .map(|(c, v)| c * v)
                        .sum(),
                );
                *gain = (value - objective).max(0.);
                pseudocosts.record(
                    Origin {
                        j,
                        up,
                        distance: if up {
                            x[j].ceil() - x[j]
                        } else {
                            x[j] - x[j].floor()
                        },
                        parent_objective: objective,
                    },
                    value,
                );
            } else {
                complete = false;
            }
            // Restore bounds and basis even on a failed/limited probe. Its
            // status is a branching hint only, never a pruning certificate.
            lp.set_bound(j, bounds.lower[j], bounds.upper[j])?;
            lp.restore(&basis);
            // Numerical probe failures do not fail the main search.
            let _ = status;
        }
        if complete {
            let score = gains[0].max(1e-6) * gains[1].max(1e-6);
            if score > best_score {
                best_score = score;
                best = Some(j);
            }
        }
    }
    if reliable {
        Ok(branching_variable(model, bounds, x, options, pseudocosts, budget.settings).or(best))
    } else {
        Ok(best)
    }
}

/// Branching variable among the fractional integer variables: best
/// balanced pseudocost score once any history exists, else most fractional.
/// If rounding an integral-looking LP point does not certify, any integer
/// variable with a nonzero fraction that can still be split inside its box.
fn branching_variable(
    model: &Model,
    bounds: &Bounds,
    x: &[f64],
    options: &Options,
    pseudocosts: &Pseudocosts,
    settings: &NativeOptions,
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
                let a = (pseudocosts.estimate(j, false) * down).max(1e-6);
                let b = (pseudocosts.estimate(j, true) * (1.0 - down)).max(1e-6);
                let score = if settings.branching_score == "product" {
                    a.sqrt() * b.sqrt()
                } else {
                    0.8 * a.min(b) + 0.2 * a.max(b)
                };
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
        adaptive_refactor: settings.adaptive_refactor,
        basis_update: settings.basis_update,
        factor_pivot_search: settings.factor_pivot_search,
        factor_ordering: settings.factor_ordering,
        max_factor_entries: settings.max_factor_entries,
        factor_replay: settings.factor_replay,
        adaptive_pricing: settings.adaptive_pricing,
        devex: settings.devex,
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
        ("packing_root", s.packing_root.to_string()),
        (
            "packing_root_iterations",
            s.packing_root_iterations.to_string(),
        ),
        ("packing_root_work", s.packing_root_work.to_string()),
        ("packing_neighborhoods", s.packing_neighborhoods.to_string()),
        (
            "packing_neighborhood_size",
            s.packing_neighborhood_size.to_string(),
        ),
        ("packing_workers", s.packing_workers.to_string()),
        (
            "packing_neighborhood_nodes",
            s.packing_neighborhood_nodes.to_string(),
        ),
        ("packing_cuts", s.packing_cuts.to_string()),
        ("packing_cut_work", s.packing_cut_work.to_string()),
        (
            "packing_combine_repairs",
            s.packing_combine_repairs.to_string(),
        ),
        ("packing_rounds", s.packing_rounds.to_string()),
        (
            "packing_neighborhood_growth",
            s.packing_neighborhood_growth.to_string(),
        ),
        ("packing_seed_strategy", s.packing_seed_strategy.to_string()),
        (
            "packing_residual_cliques",
            s.packing_residual_cliques.to_string(),
        ),
        ("packing_clique_work", s.packing_clique_work.to_string()),
        (
            "packing_price_diagnostics",
            s.packing_price_diagnostics.to_string(),
        ),
        (
            "packing_repair_seconds",
            format!("{:?}", s.packing_repair_seconds),
        ),
        (
            "packing_proof_seconds",
            format!("{:?}", s.packing_proof_seconds),
        ),
        ("partial_dual_bounds", s.partial_dual_bounds.to_string()),
        ("scalable_branching", s.scalable_branching.to_string()),
        (
            "branch_probe_candidates",
            s.branch_probe_candidates.to_string(),
        ),
        ("branch_probe_pivots", s.branch_probe_pivots.to_string()),
        ("branch_probe_work", s.branch_probe_work.to_string()),
        ("tree_threads", s.tree_threads.to_string()),
        ("tree_split_depth", s.tree_split_depth.to_string()),
        ("refactor_interval", s.refactor_interval.to_string()),
        ("adaptive_refactor", s.adaptive_refactor.to_string()),
        ("basis_update", s.basis_update.as_str().to_string()),
        ("factor_pivot_search", s.factor_pivot_search.to_string()),
        ("factor_ordering", s.factor_ordering.as_str().to_string()),
        ("steepest_edge", s.steepest_edge.to_string()),
        ("root_cuts", s.root_cuts.to_string()),
        ("root_separation", s.root_separation.to_string()),
        ("mip_presolve", s.mip_presolve.to_string()),
        (
            "coefficient_tightening",
            s.coefficient_tightening.to_string(),
        ),
        ("dual_presolve", s.dual_presolve.to_string()),
        ("mip_presolve_rounds", s.mip_presolve_rounds.to_string()),
        ("gmi_cuts", s.gmi_cuts.to_string()),
        ("mir_cuts", s.mir_cuts.to_string()),
        ("mip_cut_rounds", s.mip_cut_rounds.to_string()),
        ("mip_cut_limit", s.mip_cut_limit.to_string()),
        ("cut_exact_work", s.cut_exact_work.to_string()),
        ("root_heuristics", s.root_heuristics.to_string()),
        ("heuristic_lp_calls", s.heuristic_lp_calls.to_string()),
        ("branching_score", s.branching_score.to_string()),
        ("pseudocost_min_step", s.pseudocost_min_step.to_string()),
        ("branch_trace_limit", s.branch_trace_limit.to_string()),
        ("root_cut_rounds", s.root_cut_rounds.to_string()),
        ("cover_lifting", s.cover_lifting.to_string()),
        ("root_propagation", s.root_propagation.to_string()),
        ("mixed_propagation", s.mixed_propagation.to_string()),
        ("singleton_presolve", s.singleton_presolve.to_string()),
        (
            "avoid_dominant_components",
            s.avoid_dominant_components.to_string(),
        ),
        ("devex", s.devex.to_string()),
        ("adaptive_pricing", s.adaptive_pricing.to_string()),
        ("factor_replay", s.factor_replay.to_string()),
        ("mip_diagnostics", s.mip_diagnostics.to_string()),
        ("mip_trace_limit", s.mip_trace_limit.to_string()),
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
        ("strong_branching", s.strong_branching.to_string()),
        ("reliability_branching", s.reliability_branching.to_string()),
        ("reliability_threshold", s.reliability_threshold.to_string()),
        (
            "reliability_max_probes",
            s.reliability_max_probes.to_string(),
        ),
        ("reduced_cost_fixing", s.reduced_cost_fixing.to_string()),
        ("incumbent_heuristic", s.incumbent_heuristic.to_string()),
        (
            "rational_certification",
            s.rational_certification.to_string(),
        ),
        ("feasibility_jump", s.feasibility_jump.to_string()),
        (
            "feasibility_jump_steps",
            s.feasibility_jump_steps.to_string(),
        ),
        ("objective_lattice", s.objective_lattice.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
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
        if self.settings.packing_root
            || self.settings.packing_neighborhoods > 0
            || self.settings.packing_cuts
        {
            if let Some(result) = packing::solve(self, model, options, start, cancel)? {
                return Ok(result);
            }
        }
        if self.settings.tree_threads > 1 {
            if let Some(result) = tree::solve(self, model, options, start, cancel, None, None)? {
                return Ok(result);
            }
        }
        root::solve(self, model, options, start, cancel)
    }
}

#[cfg(test)]
mod option_validation_tests {
    use super::*;
    #[test]
    fn invalid_packing_controls_name_the_field() {
        let cases: Vec<(NativeOptions, &str)> = vec![
            (
                NativeOptions {
                    packing_rounds: 0,
                    ..Default::default()
                },
                "packing_rounds",
            ),
            (
                NativeOptions {
                    packing_neighborhoods: 4096,
                    packing_rounds: 2,
                    ..Default::default()
                },
                "packing_neighborhoods * packing_rounds",
            ),
            (
                NativeOptions {
                    packing_neighborhood_growth: 1025,
                    ..Default::default()
                },
                "packing_neighborhood_growth",
            ),
            (
                NativeOptions {
                    packing_clique_work: 1_000_001,
                    ..Default::default()
                },
                "packing_clique_work",
            ),
            (
                NativeOptions {
                    packing_seed_strategy: "best".into(),
                    ..Default::default()
                },
                "packing_seed_strategy",
            ),
            (
                NativeOptions {
                    packing_repair_seconds: Some(f64::NAN),
                    ..Default::default()
                },
                "packing_repair_seconds",
            ),
            (
                NativeOptions {
                    packing_proof_seconds: Some(-1.),
                    ..Default::default()
                },
                "packing_proof_seconds",
            ),
            (
                NativeOptions {
                    packing_root_iterations: 100_001,
                    ..Default::default()
                },
                "packing_root_iterations",
            ),
            (
                NativeOptions {
                    packing_neighborhood_size: 3,
                    ..Default::default()
                },
                "packing_neighborhood_size",
            ),
            (
                NativeOptions {
                    packing_workers: 17,
                    ..Default::default()
                },
                "packing_workers",
            ),
            (
                NativeOptions {
                    branch_probe_candidates: 0,
                    ..Default::default()
                },
                "branch_probe_candidates",
            ),
            (
                NativeOptions {
                    tree_threads: 0,
                    ..Default::default()
                },
                "tree_threads",
            ),
            (
                NativeOptions {
                    tree_split_depth: 9,
                    ..Default::default()
                },
                "tree_split_depth",
            ),
        ];
        for (settings, field) in cases {
            match settings.validate() {
                Err(Error::InvalidInput(message)) => {
                    assert!(message.starts_with(field), "{message} should name {field}")
                }
                other => panic!("{field}: expected InvalidInput, got {other:?}"),
            }
        }
        assert!(
            NativeOptions {
                packing_neighborhoods: 4096,
                packing_rounds: 1,
                packing_repair_seconds: Some(0.),
                packing_proof_seconds: Some(0.),
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }
}

#[cfg(test)]
mod branching_policy_tests {
    use super::*;
    use lp_ip_craft::{CsrMatrix, ModelData, VariableKind};
    #[test]
    fn product_and_weighted_sum_choose_distinct_planted_candidates() {
        let m = Model::new(ModelData {
            objective: vec![1.; 2],
            objective_offset: 0.,
            sense: Sense::Minimize,
            variables: Bounds::new(vec![0.; 2], vec![2.; 2]),
            kinds: vec![VariableKind::Integer; 2],
            matrix: CsrMatrix::from_dense(0, 2, &[]).unwrap(),
            rows: Bounds::free(0),
        })
        .unwrap();
        let mut p = Pseudocosts::new(2);
        p.sum = vec![[200., 20000.], [3000., 3000.]];
        p.count = vec![[1000, 1000]; 2];
        p.total = [3200., 23000.];
        p.total_count = [2000, 2000];
        for (score, expected) in [("weighted_sum", 0), ("product", 1)] {
            let s = NativeOptions {
                branching_score: score.into(),
                ..Default::default()
            };
            assert_eq!(
                branching_variable(&m, m.variables(), &[0.5, 0.5], &Options::default(), &p, &s),
                Some(expected)
            );
        }
    }
    #[test]
    fn pseudocost_step_floor_is_opt_in_and_does_not_inflate_tiny_steps() {
        let origin = Origin {
            j: 0,
            up: false,
            distance: 1e-8,
            parent_objective: 0.,
        };
        let mut raw = Pseudocosts::new(1);
        raw.record(origin, 1.);
        let mut clamped = Pseudocosts::new(1);
        clamped.min_step = 0.01;
        clamped.record(origin, 1.);
        assert_eq!(raw.sum[0][0], 1e8);
        assert_eq!(clamped.sum[0][0], 100.);
    }
}
