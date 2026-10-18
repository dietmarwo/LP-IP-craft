//! Primal heuristics: find good feasible solutions early.
//!
//! Branch and bound prunes by comparing a node's bound against the best solution
//! found so far, so it cannot prune anything at all until it holds one. A search
//! that spends its first thousand nodes without an incumbent is doing exhaustive
//! work no matter how good its bound is, which is exactly what `v064c1000n020`
//! did, running out its time limit having never found a feasible point.
//!
//! Two heuristics, cheapest first:
//!
//! - **Rounding.** Round the relaxation and test it. Costs one pass over the rows
//!   and no LP at all, and on models whose relaxation is nearly integral it simply
//!   works.
//! - **Diving.** Repeatedly fix the *least* fractional column to its nearest
//!   integer and re-solve, until the relaxation comes out integral or the dive
//!   hits infeasibility. Each re-solve differs from the last by one bound, so the
//!   warm-started dual simplex settles it in a handful of pivots, the same
//!   machinery the search itself runs on.
//!
//! Least fractional, not most: the aim here is a feasible point rather than a
//! strong bound, so the column already closest to deciding itself is the one whose
//! rounding is least likely to make the LP infeasible. Branching wants the
//! opposite, which is why it scores columns by pseudocost instead.
//!
//! A feasibility pump would be the next addition, for models where diving keeps
//! hitting infeasibility. It is a genuinely different mechanism, alternating
//! projection and rounding, rather than a refinement of this one.

use crate::cuts::Conflicts;
use crate::lp::{BasisState, Lp, LpStatus};
use crate::model::Problem;
use crate::sparse::SparseMatrix;

/// When to try a heuristic again, based on whether the last attempts worked.
///
/// A fixed interval is wrong in both directions. On a model where diving keeps
/// landing, the search wants it often. On one where it keeps failing (and it fails
/// on entire instance families, not just occasional nodes) every attempt is a
/// short chain of LPs spent for nothing. Running unconditionally every 100 nodes
/// measurably cost `v064c1000n100` its incumbent quality, because the time went to
/// dives instead of nodes.
///
/// So: double the interval on each failure up to a cap, and snap back to the base
/// on any success. A heuristic that never works costs a vanishing share of the
/// search; one that starts working again is picked back up immediately.
#[derive(Clone, Debug)]
pub struct Schedule {
    base: usize,
    interval: usize,
    next: usize,
    calls: usize,
    successes: usize,
}

/// How far the interval may grow. Past this a heuristic is effectively retired for
/// the run, though never quite: a search that changes character still gets one look.
const MAX_INTERVAL_FACTOR: usize = 64;

impl Schedule {
    /// `base` is the node interval between attempts while they keep succeeding.
    /// Zero disables the heuristic entirely.
    pub fn new(base: usize) -> Self {
        Self {
            base,
            interval: base,
            next: base,
            calls: 0,
            successes: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.base > 0
    }

    /// Is an attempt due at this node?
    pub fn due(&self, node_index: usize) -> bool {
        self.enabled() && node_index >= self.next
    }

    /// Record what an attempt achieved and set the next one.
    pub fn record(&mut self, node_index: usize, improved: bool) {
        self.calls += 1;
        if improved {
            self.successes += 1;
            self.interval = self.base;
        } else {
            self.interval = (self.interval * 2).min(self.base * MAX_INTERVAL_FACTOR);
        }
        self.next = node_index + self.interval;
    }

    pub fn calls(&self) -> usize {
        self.calls
    }

    pub fn successes(&self) -> usize {
        self.successes
    }
}

/// A feasible binary assignment and its objective, in the internal minimization
/// form.
#[derive(Clone, Debug)]
pub struct Incumbent {
    pub x: Vec<f64>,
    pub objective: f64,
}

/// Limits on how much work the heuristics may do.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// A relaxation value this close to an integer counts as integral.
    pub integrality_tolerance: f64,
    /// Row activities may violate a bound by this much.
    pub feasibility_tolerance: f64,
    /// Columns a dive may fix before giving up.
    pub max_dive_steps: usize,
    /// Simplex iterations per dive or pump re-solve.
    pub max_iterations_per_solve: usize,
    /// Projection rounds the feasibility pump may take before giving up.
    pub max_pump_rounds: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            integrality_tolerance: 1e-6,
            feasibility_tolerance: 1e-6,
            max_dive_steps: 200,
            max_iterations_per_solve: 2_000,
            max_pump_rounds: 60,
        }
    }
}

/// How much more iteration budget a pump re-solve gets than a dive step.
const PUMP_ITERATION_FACTOR: usize = 4;
/// Unfinished re-solves in a row before the pump gives up.
const MAX_SOLVE_FAILURES: usize = 3;
/// Total pump work, as a multiple of one re-solve's budget.
///
/// Rounds alone do not bound this: once a round is allowed to finish its re-solve, a
/// round on a large model can cost thousands of pivots, and sixty of those is most of a
/// search's budget spent on a heuristic that may well fail. On MIPLIB's piperout-27 the
/// pump ran 26 seconds of a 45 second limit and found nothing.
const PUMP_TOTAL_BUDGET: usize = 8;

/// Cycles tolerated before the pump restarts somewhere else entirely.
const RESTART_AFTER_CYCLES: usize = 3;
/// One in this many integer columns is moved by a restart.
const RESTART_FRACTION: usize = 10;

/// SplitMix64 again, for the same reason it is used in the instance generator: a
/// heuristic that perturbs randomly must still give the same answer on the same model
/// every time, or a benchmark measures the seed rather than the solver.
struct Perturb(u64);

impl Perturb {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as usize
        }
    }
}

/// A fingerprint of the integer part of a candidate, for detecting a repeat.
///
/// The pump cycles, and it does not only cycle with period one. Comparing against the
/// previous target catches the shortest case and misses every longer one, which then
/// runs until the round limit doing nothing.
fn fingerprint(problem: &Problem, x: &[f64]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for j in problem.integer_columns() {
        let bits = (x[j].round() as i64) as u64;
        hash ^= bits.wrapping_add(j as u64);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// Is this binary assignment feasible for every row and column bound?
pub fn is_feasible(problem: &Problem, x: &[f64], tolerance: f64) -> bool {
    for (j, &v) in x.iter().enumerate() {
        if v < problem.col_lb[j] - tolerance || v > problem.col_ub[j] + tolerance {
            return false;
        }
    }
    let csr = problem.matrix.to_csr();
    (0..problem.n_rows()).all(|i| {
        let (cols, vals) = csr.column(i);
        let activity: f64 = cols.iter().zip(vals).map(|(&j, &a)| a * x[j]).sum();
        activity >= problem.row_lb[i] - tolerance && activity <= problem.row_ub[i] + tolerance
    })
}

/// The internal-form objective of an assignment.
fn objective_of(problem: &Problem, x: &[f64]) -> f64 {
    problem.obj.iter().zip(x).map(|(c, v)| c * v).sum()
}

/// Round the integer columns of a relaxation and keep the point if it is feasible.
///
/// Continuous columns are left exactly as the relaxation put them: they are already
/// allowed to take those values, and moving them would only break feasibility.
///
/// Deliberately does not try to repair an infeasible rounding: that is diving's
/// job, and doing it here would duplicate the machinery badly.
pub fn round(problem: &Problem, x: &[f64], limits: &Limits) -> Option<Incumbent> {
    let rounded = snap(problem, x);
    is_feasible(problem, &rounded, limits.feasibility_tolerance).then(|| Incumbent {
        objective: objective_of(problem, &rounded),
        x: rounded,
    })
}

/// Round every integer column to the nearest integer inside its bounds.
fn snap(problem: &Problem, x: &[f64]) -> Vec<f64> {
    x.iter()
        .enumerate()
        .map(|(j, &v)| {
            if problem.is_integer(j) {
                v.round().clamp(problem.col_lb[j], problem.col_ub[j])
            } else {
                v
            }
        })
        .collect()
}

/// How much more iteration budget a polish solve gets than a dive or pump re-solve.
const POLISH_ITERATION_FACTOR: usize = 20;

/// Re-optimize the continuous columns of an integer-feasible point.
///
/// Every heuristic here produces its point by moving the *integer* columns and
/// leaving the rest where the relaxation put them. Those values were optimal for the
/// relaxed integers, not for the ones finally chosen, so on a model that is mostly
/// continuous the result can be far from the best solution with those integers.
/// MIPLIB's australia-abs-cta is the case in point: 918 of its 49758 columns are
/// integer, and the incumbent came out at 10865 against an optimum of 106.9.
///
/// Fixing the integers and solving the remaining LP costs one solve and gives the best
/// completion of the choice already made. On a model with no continuous columns there
/// is nothing to re-optimize and this returns the point unchanged.
///
/// `lp`'s column bounds are restored before returning, whatever the outcome.
pub fn polish(
    problem: &Problem,
    lp: &mut Lp,
    basis: &BasisState,
    x: &[f64],
    limits: &Limits,
    iterations: &mut usize,
) -> Option<Incumbent> {
    if problem.integer_columns().count() == problem.n_cols() {
        return None;
    }
    for (j, &value) in x.iter().enumerate().take(problem.n_cols()) {
        if problem.is_integer(j) {
            lp.set_column_bounds(j, value, value);
        }
    }
    // Warm-started from the caller's basis, and given room to finish. Fixing the
    // integers only tightens bounds, so that basis stays dual feasible and the dual
    // simplex repairs it in far fewer pivots than a cold solve would take. The budget
    // is per-solve limit times the dive allowance because this happens once per
    // incumbent, not once per node, and half-finishing it wastes the whole attempt.
    let solved = lp.solve_warm(
        basis,
        None,
        limits.max_iterations_per_solve * POLISH_ITERATION_FACTOR,
    );
    *iterations += solved.iterations;
    for j in 0..problem.n_cols() {
        lp.set_column_bounds(j, problem.col_lb[j], problem.col_ub[j]);
    }

    if solved.status != LpStatus::Optimal {
        return None;
    }
    // The fixed columns come back as they were fixed, but rounding them again costs
    // nothing and keeps the result exactly integral.
    let polished = snap(problem, &solved.x);
    if !is_feasible(problem, &polished, limits.feasibility_tolerance) {
        return None;
    }
    let objective = objective_of(problem, &polished);
    Some(Incumbent {
        objective,
        x: polished,
    })
}

/// Dive from a relaxation towards a feasible binary point.
///
/// `lp`'s column bounds are restored before returning, whatever the outcome, so
/// the caller's node is left exactly as it was found.
pub fn dive(
    problem: &Problem,
    lp: &mut Lp,
    basis: &BasisState,
    start: &[f64],
    cutoff: Option<f64>,
    limits: &Limits,
    iterations: &mut usize,
) -> Option<Incumbent> {
    let n = problem.n_cols();
    let saved: Vec<(f64, f64)> = (0..n).map(|j| lp.column_bounds(j)).collect();

    let result = dive_inner(problem, lp, basis, start, cutoff, limits, iterations);

    for (j, &(lo, hi)) in saved.iter().enumerate() {
        lp.set_column_bounds(j, lo, hi);
    }
    result
}

fn dive_inner(
    problem: &Problem,
    lp: &mut Lp,
    basis: &BasisState,
    start: &[f64],
    cutoff: Option<f64>,
    limits: &Limits,
    iterations: &mut usize,
) -> Option<Incumbent> {
    let tol = limits.integrality_tolerance;
    let mut x = start.to_vec();
    // Warm start each step from the previous step's basis rather than always from
    // the node's. Consecutive dive LPs differ by a single bound, so the previous
    // basis is nearly optimal for the next one; the node's grows staler with depth.
    let mut warm = basis.clone();

    let mut steps = 0usize;
    let trace = std::env::var_os("RIPSOLVE_TRACE").is_some();
    let fractional = |x: &[f64]| {
        (0..problem.n_cols())
            .filter(|&j| problem.is_integer(j) && (x[j] - x[j].round()).abs() > tol)
            .count()
    };
    if trace {
        eprintln!(
            "  dive: starting, {} of {} integer columns fractional, budget {} steps",
            fractional(&x),
            (0..problem.n_cols()).filter(|&j| problem.is_integer(j)).count(),
            limits.max_dive_steps
        );
    }
    for _ in 0..limits.max_dive_steps {
        steps += 1;
        // The column closest to already being decided.
        let next = x
            .iter()
            .enumerate()
            .filter(|&(j, _)| problem.is_integer(j))
            .filter_map(|(j, &v)| {
                let distance = (v - v.round()).abs();
                (distance > tol).then_some((j, v, distance))
            })
            .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        let Some((j, value, _)) = next else {
            // Every integer column has landed on an integer.
            let rounded = snap(problem, &x);
            if !is_feasible(problem, &rounded, limits.feasibility_tolerance) {
                return None;
            }
            return Some(Incumbent {
                objective: objective_of(problem, &rounded),
                x: rounded,
            });
        };

        // Greedy rounding regularly paints the dive into a corner, so when the
        // preferred value turns out infeasible, try the other one before giving up.
        // Without this the dive fails on essentially every model tested; with it,
        // one extra LP rescues the descent.
        // Fix towards the nearer integer, and if that proves infeasible try the
        // other side of the split rather than abandoning the dive.
        let (lo, hi) = lp.column_bounds(j);
        let nearer = value.round().clamp(lo, hi);
        let other = if nearer > value {
            value.floor()
        } else {
            value.ceil()
        };
        let mut advanced = false;
        for target in [nearer, other.clamp(lo, hi)] {
            lp.set_column_bounds(j, target, target);
            let solved = lp.solve_warm(&warm, cutoff, limits.max_iterations_per_solve);
            *iterations += solved.iterations;
            if solved.status == LpStatus::Optimal {
                x = solved.x;
                warm = solved.basis;
                advanced = true;
                break;
            }
            // Cut off or truncated says nothing about the other direction being
            // better, only that this dive is not worth continuing.
            if solved.status != LpStatus::Infeasible {
                return None;
            }
        }
        if !advanced {
            // Neither value works, so the fixings made so far are jointly infeasible.
            if trace {
                eprintln!("  dive: dead-ended after {steps} steps, {} still fractional", fractional(&x));
            }
            return None;
        }
    }
    if trace {
        eprintln!("  dive: ran out of steps after {steps}, {} still fractional", fractional(&x));
    }
    None
}

/// Find a feasible point by alternating projection and rounding.
///
/// The *feasibility pump*. It keeps two points: a relaxation-feasible `x` that is
/// fractional, and an integral `x~` that is generally infeasible. Each round
/// re-optimizes the original constraint set under a new objective, the distance to
/// `x~`, which pulls `x` toward integrality, and then re-rounds to get the next
/// `x~`. When the two coincide, that point is both integral and feasible.
///
/// The distance objective is linear on binaries: `|x_j - t_j|` is `x_j` when
/// `t_j = 0` and `1 - x_j` when `t_j = 1`, so minimizing it means costs of
/// `1 - 2*t_j` and a dropped constant.
///
/// This is the right tool where diving is the wrong one. Diving commits to a
/// rounding and re-solves a *smaller* LP each step, so on a model whose feasible
/// set is sparse it walks into infeasibility and cannot recover, measured, it
/// failed on every instance in this benchmark set. The pump never fixes anything,
/// so its LP is always feasible; it can wander, but it cannot dead-end.
pub fn feasibility_pump(
    problem: &Problem,
    lp: &mut Lp,
    basis: &BasisState,
    start: &[f64],
    limits: &Limits,
    iterations: &mut usize,
) -> Option<Incumbent> {
    let n = problem.n_cols();
    let saved_costs = lp.costs().to_vec();
    let result = pump_inner(problem, lp, basis, start, limits, iterations, n);
    lp.set_costs(&saved_costs);
    result
}

#[allow(clippy::too_many_arguments)]
fn pump_inner(
    problem: &Problem,
    lp: &mut Lp,
    basis: &BasisState,
    start: &[f64],
    limits: &Limits,
    iterations: &mut usize,
    n: usize,
) -> Option<Incumbent> {
    let mut x = start.to_vec();
    // Only the objective changes between rounds, so the previous basis stays primal
    // feasible and re-optimizing from it is far cheaper than a cold solve.
    let mut warm = basis.clone();
    // Every target seen since the last restart. A one-step memory catches only a cycle
    // of period one and leaves every longer one to run out the round limit.
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    // Deterministic tie-breaking for perturbation, so a run is reproducible.
    let mut tick = 0usize;
    let mut rng = Perturb(0x5EED_1234_ABCD_0001);
    let mut cycles = 0usize;
    let mut failures = 0usize;
    let ceiling = limits.max_iterations_per_solve * PUMP_TOTAL_BUDGET;
    let mut spent = 0usize;

    for _ in 0..limits.max_pump_rounds {
        let mut target = snap(problem, &x);

        if is_feasible(problem, &target, limits.feasibility_tolerance) {
            return Some(Incumbent {
                objective: objective_of(problem, &target),
                x: target,
            });
        }

        let repeated = !seen.insert(fingerprint(problem, &target));
        if repeated {
            cycles += 1;
        }

        // Small flips get out of a short cycle. They do not get out of a basin: once
        // the walk keeps returning to the same neighbourhood, nudging a handful of
        // columns returns it there again. After a few cycles the pump restarts
        // instead, perturbing a tenth of the integer columns to somewhere else
        // entirely and forgetting where it has been.
        if cycles >= RESTART_AFTER_CYCLES {
            cycles = 0;
            seen.clear();
            let integers: Vec<usize> = problem.integer_columns().collect();
            let disturb = (integers.len() / RESTART_FRACTION).max(1);
            for _ in 0..disturb {
                let j = integers[rng.below(integers.len())];
                let lb = problem.col_lb[j];
                let ub = problem.col_ub[j];
                if !lb.is_finite() || !ub.is_finite() || ub <= lb {
                    continue;
                }
                let span = (ub - lb) as usize + 1;
                target[j] = lb + rng.below(span.max(1)) as f64;
            }
        } else if repeated {
            let mut by_distance: Vec<usize> = (0..n).filter(|&j| problem.is_integer(j)).collect();
            by_distance.sort_by(|&a, &b| {
                let d = |j: usize| (x[j] - x[j].round()).abs();
                d(b).partial_cmp(&d(a)).unwrap_or(std::cmp::Ordering::Equal)
            });
            tick += 1;
            let flips = (1 + tick % 5).min(n);
            for &j in by_distance.iter().take(flips) {
                // Push the column to the other side of its fractional value, staying
                // inside its bounds.
                let away = if target[j] > x[j] {
                    x[j].floor()
                } else {
                    x[j].ceil()
                };
                target[j] = away.clamp(problem.col_lb[j], problem.col_ub[j]);
            }
        }

        // Minimize the distance to the target over the original constraint set.
        //
        // For a column at one of its bounds the distance is linear in one direction,
        // so a cost of +/-1 expresses it exactly. A general integer sitting strictly
        // inside its range has a V-shaped distance that no single linear cost can
        // represent; those columns are left uncosted rather than pulled the wrong
        // way, which weakens the pump but never misdirects it.
        let costs: Vec<f64> = target
            .iter()
            .enumerate()
            .map(|(j, &t)| {
                if !problem.is_integer(j) {
                    0.0
                } else if t <= problem.col_lb[j] + 0.5 {
                    1.0
                } else if t >= problem.col_ub[j] - 0.5 {
                    -1.0
                } else {
                    0.0
                }
            })
            .collect();
        lp.set_costs(&costs);
        // The distance LP is a full re-optimization, not the handful of pivots a dive
        // step takes, so it gets room to finish. At the ordinary per-solve limit the
        // first round of this pump hit the cap on MIPLIB's nursesched-sprint02 and
        // piperout-27 and the whole heuristic gave up having done nothing, which looked
        // from outside exactly like a pump that had run and failed.
        let solved = lp.solve_warm(
            &warm,
            None,
            limits.max_iterations_per_solve * PUMP_ITERATION_FACTOR,
        );
        *iterations += solved.iterations;
        spent += solved.iterations;
        if spent > ceiling {
            return None;
        }
        if solved.status != LpStatus::Optimal {
            // One unfinished re-solve is a reason to try elsewhere, not to abandon the
            // search. The walk restarts from a perturbed point, and only repeated
            // failures end it.
            failures += 1;
            if failures >= MAX_SOLVE_FAILURES {
                return None;
            }
            cycles = RESTART_AFTER_CYCLES;
            continue;
        }
        failures = 0;
        x = solved.x;
        warm = solved.basis;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{Kind, Spec};
    use lp_parser_rs::problem::LpProblem;

    fn instance(kind: Kind, n_cols: usize, n_rows: usize, seed: u64) -> Problem {
        let spec = Spec {
            kind,
            n_cols,
            n_rows,
            seed,
        };
        Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap()
    }

    /// Whatever a heuristic returns must actually be feasible and correctly costed.
    /// A heuristic that returns an infeasible point does not fail loudly, it
    /// installs a bogus incumbent and the search prunes the real optimum away.
    fn assert_valid(problem: &Problem, found: &Incumbent, label: &str) {
        let x: Vec<f64> = found.x.clone();
        assert_eq!(x.len(), problem.n_cols(), "{label}: wrong length");
        assert!(
            is_feasible(problem, &x, 1e-6),
            "{label}: returned an infeasible point"
        );
        let recomputed = objective_of(problem, &x);
        assert!(
            (recomputed - found.objective).abs() < 1e-6,
            "{label}: reported {} but the point costs {recomputed}",
            found.objective
        );
    }

    /// The same guarantee the propagating heuristic carries, for the same reason: a
    /// weighted local search has no notion of validity, so what it returns is checked
    /// against the model before it counts as anything.
    #[test]
    fn feasibility_jump_never_returns_an_infeasible_point() {
        for seed in 0..40u64 {
            for kind in [Kind::Covering, Kind::Signed, Kind::Knapsack] {
                let p = instance(kind, 30, 25, seed);
                let relaxed = Lp::relaxation(&p).solve();
                if relaxed.status != LpStatus::Optimal {
                    continue;
                }
                if let Some(found) =
                    feasibility_jump(&p, &relaxed.x, &Limits::default(), 20_000, None)
                {
                    assert_valid(&p, &found, &format!("jump {kind:?} seed {seed}"));
                }
            }
        }
    }

    /// The point of it: a feasible point with no LP solved at all. Covering instances
    /// have one trivially, so the check that matters is that it is reached from a
    /// starting assignment that is itself infeasible.
    #[test]
    fn feasibility_jump_repairs_an_infeasible_start() {
        let p = instance(Kind::Covering, 40, 30, 7);
        // Everything off, which violates every covering row at once.
        let start = vec![0.0; p.n_cols()];
        assert!(
            !is_feasible(&p, &start, 1e-6),
            "the starting point was supposed to be infeasible"
        );
        let found = feasibility_jump(&p, &start, &Limits::default(), 200_000, None)
            .expect("covering rows are always satisfiable by switching enough on");
        assert_valid(&p, &found, "jump repair");
    }

    /// Weight bumping is what distinguishes this from a rounding pass: without it the
    /// search stops at the first assignment no single flip improves. This model has
    /// exactly that shape, one flip away from feasible in a direction that looks
    /// neutral until the violated row is made expensive.
    #[test]
    fn feasibility_jump_escapes_a_local_minimum() {
        let lp = "Minimize\n obj: x1 + x2 + x3\nSubject To\n \
                  r1: x1 + x2 >= 1\n r2: x2 + x3 >= 1\n r3: x1 + x3 >= 1\n\
                  Binary\n x1\n x2\n x3\nEnd\n";
        let p = Problem::from_lp(&LpProblem::parse(lp).unwrap()).unwrap();
        let start = vec![0.0; p.n_cols()];
        let found = feasibility_jump(&p, &start, &Limits::default(), 100_000, None)
            .expect("two of the three switched on satisfies every row");
        assert_valid(&p, &found, "jump local minimum");
    }

    /// The property that makes this heuristic safe to add: propagation may be wrong
    /// about where the feasible points are, and cannot be wrong about what it returns,
    /// because the completed assignment is checked before it is handed back.
    #[test]
    fn fix_and_propagate_never_returns_an_infeasible_point() {
        for seed in 0..40u64 {
            for kind in [Kind::Covering, Kind::Signed, Kind::Knapsack] {
                let p = instance(kind, 30, 25, seed);
                let relaxed = Lp::relaxation(&p).solve();
                if relaxed.status != LpStatus::Optimal {
                    continue;
                }
                let conflicts = Conflicts::of(&p);
                if let Some(found) =
                    fix_and_propagate(&p, &conflicts, &relaxed.x, &Limits::default())
                {
                    assert_valid(&p, &found, &format!("{kind:?} seed {seed}"));
                }
            }
        }
    }

    /// Set partitioning is the structure this exists for: one fixing to one settles
    /// every other column of the row, so the assignment completes without ever asking
    /// the relaxation a second question.
    #[test]
    fn fix_and_propagate_completes_a_partitioning_model() {
        // Three disjoint rows, each demanding exactly one of its three columns.
        let mut lp = String::from("Minimize
 obj: x1 + 2 x2 + 3 x3 + x4 + 2 x5 + 3 x6
");
        lp.push_str("Subject To
");
        lp.push_str(" r1: x1 + x2 + x3 = 1
");
        lp.push_str(" r2: x4 + x5 + x6 = 1
");
        lp.push_str("Binary
 x1
 x2
 x3
 x4
 x5
 x6
End
");
        let p = Problem::from_lp(&LpProblem::parse(&lp).unwrap()).unwrap();
        let relaxed = Lp::relaxation(&p).solve();
        let conflicts = Conflicts::of(&p);
        let found = fix_and_propagate(&p, &conflicts, &relaxed.x, &Limits::default())
            .expect("a partitioning model this small must complete");
        assert_valid(&p, &found, "partitioning");
    }

    /// Only binaries are decided, so a general integer column is left where the
    /// relaxation put it. A fractional value there is not a solution, however well it
    /// sits in the rows.
    #[test]
    fn fix_and_propagate_never_returns_a_fractional_integer() {
        use crate::model::{Builder, RowSense, Sense};
        let mut model = Builder::new(Sense::Minimize);
        let b = model.binary("b");
        let n = model.integer("n", 0.0, 3.0);
        model.objective(&[(b, -1.0), (n, -1.0)]);
        model.row(&[(b, 1.0), (n, 1.0)], RowSense::Le, 2.5);
        let p = model.build();
        assert!(p.is_integer(n) && !is_binary_column(&p, n));
        let conflicts = Conflicts::of(&p);
        let mut relaxation = vec![0.0; 2];
        relaxation[b] = 1.0;
        relaxation[n] = 1.5;
        if let Some(found) = fix_and_propagate(&p, &conflicts, &relaxation, &Limits::default()) {
            assert!(
                (found.x[n] - found.x[n].round()).abs() < 1e-9,
                "fractional point {:?}",
                found.x
            );
        }
    }

    /// A model with no feasible point must produce none, rather than a point that
    /// propagation merely failed to refute.
    #[test]
    fn fix_and_propagate_finds_nothing_when_nothing_is_feasible() {
        let lp = "Minimize
 obj: x1 + x2
Subject To
 r1: x1 + x2 >= 2
                   r2: x1 + x2 <= 1
Binary
 x1
 x2
End
";
        let p = Problem::from_lp(&LpProblem::parse(lp).unwrap()).unwrap();
        let conflicts = Conflicts::of(&p);
        let x = vec![0.5, 0.5];
        assert!(
            fix_and_propagate(&p, &conflicts, &x, &Limits::default()).is_none(),
            "an infeasible model yielded a point"
        );
    }

    #[test]
    fn rounding_accepts_an_already_integral_relaxation() {
        let p = instance(Kind::Covering, 25, 30, 1);
        let relaxed = Lp::relaxation(&p).solve();
        // This instance's relaxation is integral, so rounding it must succeed.
        let found = round(&p, &relaxed.x, &Limits::default()).expect("integral relaxation");
        assert_valid(&p, &found, "covering rounding");
    }

    #[test]
    fn rounding_rejects_an_infeasible_rounding() {
        // Rounding down a tight `>=` row breaks it, and round() must not claim it.
        let p = instance(Kind::Knapsack, 20, 10, 1);
        let x = vec![0.4; p.n_cols()];
        assert!(round(&p, &x, &Limits::default()).is_none());
    }

    #[test]
    fn the_pump_finds_feasible_points_where_diving_does_not() {
        // The measured reason the pump exists. Diving commits to a rounding and can
        // walk into infeasibility with no way back; the pump never fixes anything,
        // so its LP stays feasible throughout.
        let limits = Limits::default();
        let mut pumped = 0;
        for seed in 0..6u64 {
            let p = instance(Kind::Knapsack, 30, 15, seed);
            let mut lp = Lp::relaxation(&p);
            let root = lp.solve();
            let mut iterations = 0;
            if let Some(found) =
                feasibility_pump(&p, &mut lp, &root.basis, &root.x, &limits, &mut iterations)
            {
                assert_valid(&p, &found, &format!("pump seed {seed}"));
                pumped += 1;
            }
        }
        assert!(pumped >= 4, "the pump found only {pumped} of 6");
    }

    #[test]
    fn heuristics_leave_the_lp_exactly_as_they_found_it() {
        // Both dive and pump mutate the LP, bounds and costs respectively, and
        // both must restore it, or the node they were called from is corrupted.
        let p = instance(Kind::Knapsack, 25, 12, 3);
        let mut lp = Lp::relaxation(&p);
        let root = lp.solve();
        let limits = Limits::default();

        let bounds: Vec<(f64, f64)> = (0..p.n_cols()).map(|j| lp.column_bounds(j)).collect();
        let costs = lp.costs().to_vec();

        let mut iterations = 0;
        let _ = dive(
            &p,
            &mut lp,
            &root.basis,
            &root.x,
            None,
            &limits,
            &mut iterations,
        );
        let _ = feasibility_pump(&p, &mut lp, &root.basis, &root.x, &limits, &mut iterations);

        let after: Vec<(f64, f64)> = (0..p.n_cols()).map(|j| lp.column_bounds(j)).collect();
        assert_eq!(bounds, after, "column bounds were not restored");
        assert_eq!(costs, lp.costs(), "objective costs were not restored");

        // And the LP still solves to the same relaxation value.
        let again = lp.solve();
        assert!((again.objective - root.objective).abs() < 1e-9);
    }

    #[test]
    fn a_dive_that_succeeds_returns_a_feasible_point() {
        let limits = Limits::default();
        for seed in 0..8u64 {
            let p = instance(Kind::Covering, 30, 40, seed);
            let mut lp = Lp::relaxation(&p);
            let root = lp.solve();
            let mut iterations = 0;
            if let Some(found) = dive(
                &p,
                &mut lp,
                &root.basis,
                &root.x,
                None,
                &limits,
                &mut iterations,
            ) {
                assert_valid(&p, &found, &format!("dive seed {seed}"));
            }
        }
    }

    #[test]
    fn feasibility_check_catches_a_violated_row() {
        let p = instance(Kind::Knapsack, 15, 8, 2);
        let all_zero = vec![0.0; p.n_cols()];
        // Knapsack rows are `>=` with a positive right-hand side, so the origin fails.
        assert!(!is_feasible(&p, &all_zero, 1e-6));
        let all_one = vec![1.0; p.n_cols()];
        assert!(is_feasible(&p, &all_one, 1e-6));
    }
}

#[cfg(test)]
mod schedule_tests {
    use super::Schedule;

    #[test]
    fn a_zero_base_disables_the_heuristic() {
        let s = Schedule::new(0);
        assert!(!s.enabled());
        assert!(!s.due(0));
        assert!(!s.due(1_000_000));
    }

    #[test]
    fn failures_back_the_interval_off_geometrically() {
        let mut s = Schedule::new(10);
        assert!(!s.due(9));
        assert!(s.due(10));

        s.record(10, false); // next at 10 + 20
        assert!(!s.due(29));
        assert!(s.due(30));

        s.record(30, false); // next at 30 + 40
        assert!(!s.due(69));
        assert!(s.due(70));
    }

    #[test]
    fn a_success_snaps_the_interval_back() {
        // A heuristic that starts working again must be picked straight back up,
        // not left at whatever interval its earlier failures had grown to.
        let mut s = Schedule::new(10);
        for node in [10, 30, 70] {
            s.record(node, false);
        }
        s.record(150, true);
        assert!(s.due(160), "interval did not reset after a success");
        assert_eq!(s.successes(), 1);
        assert_eq!(s.calls(), 4);
    }

    #[test]
    fn the_interval_is_capped() {
        // Never retired outright: a search that changes character still gets a look.
        let mut s = Schedule::new(10);
        let mut node = 0;
        for _ in 0..40 {
            node += 1 << 20;
            s.record(node, false);
        }
        // Capped at base * 64, so an attempt is still due a bounded distance later.
        assert!(s.due(node + 10 * 64), "interval grew past its cap");
    }
}

/// How much a propagation sweep may read before it gives up, in literals and
/// coefficients.
///
/// Each forced column re-queues the rows it appears in, so a sweep is bounded by the
/// model rather than by a count, and the guard is against a cycle rather than against
/// depth.
const MAX_PROPAGATIONS: usize = 100_000_000;

/// How many times a fixing may be found infeasible and retried the other way.
///
/// A conflict says the last fixing was wrong, and flipping it is the cheapest repair
/// that keeps the work already done. Repeated conflicts mean the trouble is further
/// back than the last decision, which this does not chase.
const MAX_CONFLICTS: usize = 64;

/// A partial assignment of the columns, with everything a sweep needs to extend it and
/// to take it back again.
///
/// Both callers fix a column tentatively, force what follows, and then keep the result
/// or discard it. What makes that affordable is that discarding rebuilds nothing: a
/// sweep writes down every value it overwrote, and undoing is that record walked
/// backwards. Copying the assignment instead costs a pass over the columns per trial,
/// and probing tries two per column, so the model is copied twice for every column in
/// it before any inference is drawn at all.
///
/// Row activities are carried the same way, and for the same reason. Recomputing a
/// row's activity range from its coefficients is what made a sweep cost the rows it
/// touched rather than the columns it fixed, and on a set partitioning model one row is
/// a large part of the matrix.
pub(crate) struct Propagator<'a> {
    conflicts: &'a Conflicts,
    csr: &'a SparseMatrix,
    /// Copied out of the model rather than borrowed from it, so that a caller mirroring
    /// what is proven back into the model does not have to give this up to do it.
    row_lb: Vec<f64>,
    row_ub: Vec<f64>,
    binary: Vec<bool>,
    /// The matrix by column: `col_start` indexes `col_rows` and `col_values`.
    col_start: Vec<usize>,
    col_rows: Vec<u32>,
    col_values: Vec<f64>,
    lb: Vec<f64>,
    ub: Vec<f64>,
    /// The least and greatest activity each row can still take.
    min_activity: Vec<f64>,
    max_activity: Vec<f64>,
    /// The most any single column can move a row's activity, over the bounds the model
    /// started with. A row with more slack than this at both ends can force nothing,
    /// whatever else is fixed in it, and is not read.
    reach: Vec<f64>,
    trail: Vec<Undo>,
    queue: Vec<usize>,
    /// Reused between sweeps so a row's forcings cost no allocation to collect.
    forced: Vec<u32>,
    /// Literals and coefficients looked at since this was built, which is what a caller
    /// rationing propagation has to ration. Counting sweeps or fixings instead prices
    /// a column in a clique of four the same as one in a clique of four thousand.
    work: usize,
    tolerance: f64,
}

/// How a propagation sweep ended.
///
/// The third case is the one worth naming. A sweep that ran out of work has proved
/// nothing either way, and a caller that treats that as "proved nothing" — the same as
/// a sweep that finished and found nothing — concludes the model has no more to give
/// and stops, when what actually happened is that it was not allowed to look.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Sweep {
    /// The assignment cannot be completed.
    Refuted,
    /// A fixed point was reached with the assignment intact.
    Settled,
    /// The work ran out first.
    Abandoned,
}

/// One overwritten value, kept so it can be put back exactly.
///
/// Exactly, rather than by re-adding what was subtracted: an activity walked forwards
/// and backwards through a million floating point additions does not come back to
/// where it started, and probing draws conclusions from these numbers that fix columns
/// for good.
enum Undo {
    Column { j: u32, lb: f64, ub: f64 },
    Activity { i: u32, min: f64, max: f64 },
}

impl<'a> Propagator<'a> {
    pub(crate) fn new(
        problem: &Problem,
        conflicts: &'a Conflicts,
        csr: &'a SparseMatrix,
        tolerance: f64,
    ) -> Self {
        let (n, m) = (problem.n_cols(), problem.n_rows());
        let mut counts = vec![0usize; n + 1];
        for i in 0..m {
            for &j in csr.column(i).0 {
                counts[j + 1] += 1;
            }
        }
        for j in 0..n {
            counts[j + 1] += counts[j];
        }
        let col_start = counts.clone();
        let mut col_rows = vec![0u32; col_start[n]];
        let mut col_values = vec![0.0f64; col_start[n]];
        let mut min_activity = vec![0.0f64; m];
        let mut max_activity = vec![0.0f64; m];
        let mut reach = vec![0.0f64; m];
        for i in 0..m {
            let (cols, values) = csr.column(i);
            for (&j, &a) in cols.iter().zip(values) {
                col_rows[counts[j]] = i as u32;
                col_values[counts[j]] = a;
                counts[j] += 1;
                let (lo, hi) = (a * problem.col_lb[j], a * problem.col_ub[j]);
                min_activity[i] += lo.min(hi);
                max_activity[i] += lo.max(hi);
                reach[i] = reach[i].max((hi - lo).abs());
            }
        }
        Self {
            conflicts,
            csr,
            row_lb: problem.row_lb.clone(),
            row_ub: problem.row_ub.clone(),
            binary: (0..n).map(|j| is_binary_column(problem, j)).collect(),
            col_start,
            col_rows,
            col_values,
            lb: problem.col_lb.clone(),
            ub: problem.col_ub.clone(),
            min_activity,
            max_activity,
            reach,
            trail: Vec::new(),
            queue: Vec::new(),
            forced: Vec::new(),
            work: 0,
            tolerance,
        }
    }

    /// How much has been looked at so far, in literals and coefficients.
    pub(crate) fn work(&self) -> usize {
        self.work
    }

    pub(crate) fn fixed(&self, j: usize) -> bool {
        self.lb[j] >= self.ub[j] - 1e-9
    }

    /// What column `j` is pinned to, which means anything only where `fixed` holds.
    pub(crate) fn value(&self, j: usize) -> f64 {
        self.lb[j]
    }

    /// Column `j`'s bounds as they stand.
    pub(crate) fn lower(&self, j: usize) -> f64 {
        self.lb[j]
    }

    pub(crate) fn upper(&self, j: usize) -> f64 {
        self.ub[j]
    }

    /// Where the columns stand now, to hand back to `rewind`.
    pub(crate) fn mark(&self) -> usize {
        self.trail.len()
    }

    /// Put everything back as it stood at `mark`.
    pub(crate) fn rewind(&mut self, mark: usize) {
        while self.trail.len() > mark {
            match self.trail.pop().expect("length checked") {
                Undo::Column { j, lb, ub } => {
                    self.lb[j as usize] = lb;
                    self.ub[j as usize] = ub;
                }
                Undo::Activity { i, min, max } => {
                    self.min_activity[i as usize] = min;
                    self.max_activity[i as usize] = max;
                }
            }
        }
        self.queue.clear();
    }

    /// Make everything on the trail permanent, so no later rewind reaches it.
    pub(crate) fn commit(&mut self) {
        self.trail.clear();
    }

    /// The columns fixed since `mark`, in the order they were fixed.
    pub(crate) fn fixed_since(&self, mark: usize, out: &mut Vec<usize>) {
        out.clear();
        for entry in &self.trail[mark.min(self.trail.len())..] {
            if let Undo::Column { j, .. } = entry {
                out.push(*j as usize);
            }
        }
    }

    /// Queue every column the model already pins, so the first sweep starts from them.
    pub(crate) fn seed_fixed(&mut self) {
        for j in 0..self.lb.len() {
            if self.fixed(j) {
                self.queue.push(j);
            }
        }
    }

    /// Pin `j` to `value`, reporting false if that contradicts what is already known.
    ///
    /// The change is recorded on the trail and carried into the activity range of every
    /// row `j` appears in, so the rows stay current without being read again.
    pub(crate) fn fix(&mut self, j: usize, value: f64) -> bool {
        if value < self.lb[j] - 1e-9 || value > self.ub[j] + 1e-9 {
            return false;
        }
        if self.lb[j] == value && self.ub[j] == value {
            return true;
        }
        self.trail.push(Undo::Column {
            j: j as u32,
            lb: self.lb[j],
            ub: self.ub[j],
        });
        let (start, end) = (self.col_start[j], self.col_start[j + 1]);
        self.work += end - start;
        for position in start..end {
            let i = self.col_rows[position] as usize;
            let a = self.col_values[position];
            let (lo, hi) = (a * self.lb[j], a * self.ub[j]);
            let held = a * value;
            self.trail.push(Undo::Activity {
                i: i as u32,
                min: self.min_activity[i],
                max: self.max_activity[i],
            });
            // An infinite bound leaves nothing to subtract, so that row is rebuilt from
            // its coefficients. Only a column the model left unbounded reaches this.
            if lo.min(hi).is_finite() {
                self.min_activity[i] += held - lo.min(hi);
            } else {
                self.min_activity[i] = f64::NAN;
            }
            if lo.max(hi).is_finite() {
                self.max_activity[i] += held - lo.max(hi);
            } else {
                self.max_activity[i] = f64::NAN;
            }
        }
        self.lb[j] = value;
        self.ub[j] = value;
        for position in start..end {
            let i = self.col_rows[position] as usize;
            if self.min_activity[i].is_nan() || self.max_activity[i].is_nan() {
                self.rebuild_activity(i);
            }
        }
        self.queue.push(j);
        true
    }

    /// Recompute one row's activity range from its coefficients.
    fn rebuild_activity(&mut self, i: usize) {
        let (cols, values) = self.csr.column(i);
        let (mut min, mut max) = (0.0f64, 0.0f64);
        for (&k, &a) in cols.iter().zip(values) {
            let (lo, hi) = (a * self.lb[k], a * self.ub[k]);
            min += lo.min(hi);
            max += lo.max(hi);
        }
        self.min_activity[i] = min;
        self.max_activity[i] = max;
    }

    /// Suppose `j` takes `value`, and work out what follows.
    ///
    /// The supposition is left in place on every outcome, refutation included, so the
    /// caller can keep what a surviving sweep proved or hand it back with `rewind`.
    pub(crate) fn probe(&mut self, j: usize, value: f64, max_work: usize) -> Sweep {
        if !self.fix(j, value) {
            return Sweep::Refuted;
        }
        self.propagate(max_work)
    }

    /// Assert `j = value` without sweeping, so several assertions share one sweep.
    ///
    /// `probe` propagates each supposition on its own because each is a separate
    /// question. A caller that has already *proved* a batch of values is not asking
    /// anything, and paying for one sweep per column would price a proof like a guess.
    /// Returns false when the assertion contradicts the bounds already held.
    pub(crate) fn assert_value(&mut self, j: usize, value: f64) -> bool {
        self.fix(j, value)
    }

    /// Force everything that follows from the columns already queued.
    ///
    /// Two inferences run to a common fixed point. The conflict graph gives the logical
    /// one: a literal that holds excludes every literal it conflicts with, and excluding
    /// `x_k = 1` is fixing `x_k = 0`. This is where a set partitioning row pays, since
    /// fixing one of its columns to one forces every other column in the row to zero at
    /// once, without an LP solve and without looking at the row again.
    ///
    /// Row activities give the arithmetic one: a row whose remaining slack cannot absorb
    /// a column's coefficient forces that column to the end that fits. This catches what
    /// the conflict graph does not, the rows that exclude nothing pairwise but still
    /// leave only one value open once enough of their columns are pinned.
    ///
    /// `max_work` is the literals and coefficients this sweep may read before giving
    /// up, which is what a sweep costs. Bounding the number of fixings instead prices a
    /// column in a clique of four the same as one in a clique of four thousand, and the
    /// second is a thousand times the work.
    ///
    /// Returns false when the two together prove the assignment cannot be completed.
    /// Running out of work returns true: a sweep that stopped early has refuted
    /// nothing, while every fixing it did make is still implied by what it started from.
    pub(crate) fn propagate(&mut self, max_work: usize) -> Sweep {
        let ceiling = self.work.saturating_add(max_work);
        while let Some(j) = self.queue.pop() {
            if self.work > ceiling {
                return Sweep::Abandoned;
            }
            if !self.fixed(j) {
                continue;
            }
            if !self.propagate_conflicts(j) {
                return Sweep::Refuted;
            }
            let (start, end) = (self.col_start[j], self.col_start[j + 1]);
            for position in start..end {
                let i = self.col_rows[position] as usize;
                if !self.propagate_row(i) {
                    return Sweep::Refuted;
                }
            }
        }
        Sweep::Settled
    }

    /// Fix everything excluded by the literal `j` now asserts.
    fn propagate_conflicts(&mut self, j: usize) -> bool {
        if !self.binary[j] {
            return true;
        }
        let node = if self.lb[j] > 0.5 { 2 * j } else { 2 * j + 1 };
        let conflicts = self.conflicts;
        self.work += conflicts.degree(node as u32);
        let mut ok = true;
        conflicts.for_each_adjacent(node as u32, |excluded| {
            let k = excluded as usize / 2;
            // Excluding `x_k = 1` means fixing zero, and excluding `x_k = 0` one.
            let value = if excluded.is_multiple_of(2) { 0.0 } else { 1.0 };
            ok = self.fix(k, value);
            ok
        });
        ok
    }

    /// Fix whatever row `i` now leaves only one value for.
    fn propagate_row(&mut self, i: usize) -> bool {
        let (min, max) = (self.min_activity[i], self.max_activity[i]);
        let (row_lb, row_ub) = (self.row_lb[i], self.row_ub[i]);
        let tolerance = self.tolerance;
        // A row can neither force nor refute while both its slacks exceed what any one
        // column could contribute, and most rows of most models stay that way for a
        // whole sweep. Reading the row to find that out is the cost being avoided; this
        // is two comparisons against numbers the row already carries.
        //
        // A row still holding an unbounded column compares against an infinity and
        // gives NaN, which fails both tests and skips the row. That is the right answer:
        // an activity range open at one end refutes nothing at that end, and it cannot
        // be a binary column that opened it.
        let above = min + self.reach[i] > row_ub + tolerance;
        let below = max - self.reach[i] < row_lb - tolerance;
        if !above && !below {
            return true;
        }
        if min > row_ub + tolerance || max < row_lb - tolerance {
            return false;
        }
        let (cols, values) = self.csr.column(i);
        self.work += cols.len();
        let mut forced = std::mem::take(&mut self.forced);
        forced.clear();
        for (&k, &a) in cols.iter().zip(values) {
            if a == 0.0 || self.fixed(k) || !self.binary[k] {
                continue;
            }
            // What this row leaves open for `k` once every other column is at its
            // worst, which for a binary is a choice between two values.
            let (lo, hi) = (a * self.lb[k], a * self.ub[k]);
            let rest_min = min - lo.min(hi);
            let rest_max = max - lo.max(hi);
            let mut only: Option<f64> = None;
            for value in [0.0f64, 1.0] {
                if value < self.lb[k] - 1e-9 || value > self.ub[k] + 1e-9 {
                    continue;
                }
                let with = a * value;
                let feasible =
                    rest_min + with <= row_ub + tolerance && rest_max + with >= row_lb - tolerance;
                if feasible {
                    // Two values fit, so the row forces nothing here.
                    if only.is_some() {
                        only = None;
                        break;
                    }
                    only = Some(value);
                }
            }
            if let Some(value) = only {
                // Packed as a literal so the fixing pass does not need the coefficient
                // again. Fixing inside the scan instead would leave the rest of the row
                // reading an activity range that has already moved.
                forced.push(2 * k as u32 + u32::from(value < 0.5));
            }
        }
        let mut ok = true;
        for &literal in &forced {
            let value = if literal.is_multiple_of(2) { 1.0 } else { 0.0 };
            if !self.fix(literal as usize / 2, value) {
                ok = false;
                break;
            }
        }
        self.forced = forced;
        ok
    }
}

/// Is this column a binary, including one already pinned to an end?
pub(crate) fn is_binary_column(problem: &Problem, j: usize) -> bool {
    problem.is_integer(j) && problem.col_lb[j] >= 0.0 && problem.col_ub[j] <= 1.0
}

/// Fix the binary columns one at a time, forcing everything each choice implies.
///
/// Diving asks the relaxation what to do next and pays an LP solve for the answer. On a
/// model whose feasible set is sparse the answer is usually another fractional vertex,
/// and the chain of solves ends without a point: across the pure binary instances this
/// solver loses on, the whole heuristic chain takes 71% of the root's iterations and
/// returns nothing on every one of them.
///
/// This asks the model instead. A fixing propagates through the conflict graph and the
/// row activities, and on set partitioning structure one choice settles a whole row at
/// once. Propagation is logical rather than numerical, so a step costs a sweep over the
/// rows the fixed columns touch rather than a factorization.
///
/// The relaxation still chooses the order and the values, being the best guess available
/// as to where the feasible points are. What it does not do is get asked again after
/// every fixing.
pub fn fix_and_propagate(
    problem: &Problem,
    conflicts: &Conflicts,
    x: &[f64],
    limits: &Limits,
) -> Option<Incumbent> {
    let n = problem.n_cols();
    let csr = problem.matrix.to_csr();
    let mut state = Propagator::new(problem, conflicts, &csr, limits.feasibility_tolerance);
    // Columns the model already pins are part of the assignment from the start.
    state.seed_fixed();
    if state.propagate(MAX_PROPAGATIONS) == Sweep::Refuted {
        return None;
    }
    // Nothing after this point is allowed to take those back.
    state.commit();

    // Most nearly decided first: a column the relaxation has already pushed to an end
    // is the one it is most confident about, and fixing it forces the most for the
    // least risk of being wrong.
    let mut order: Vec<usize> = (0..n)
        .filter(|&j| is_binary_column(problem, j) && !state.fixed(j))
        .collect();
    order.sort_by(|&a, &b| {
        let (da, db) = ((x[a] - 0.5).abs(), (x[b] - 0.5).abs());
        db.total_cmp(&da)
    });

    let mut conflicts_seen = 0usize;
    for &j in &order {
        if state.fixed(j) {
            continue;
        }
        let prefer = if x[j] > 0.5 { 1.0 } else { 0.0 };
        let mut settled = false;
        for value in [prefer, 1.0 - prefer] {
            let mark = state.mark();
            if state.fix(j, value) && state.propagate(MAX_PROPAGATIONS) != Sweep::Refuted {
                // Kept, so the fixings it made are as permanent as the choice itself.
                state.commit();
                settled = true;
                break;
            }
            state.rewind(mark);
            // The preferred value is refuted; the other one is the whole of the repair
            // this attempts, and counting the refutations bounds how long it may go on
            // being wrong.
            conflicts_seen += 1;
            if conflicts_seen > MAX_CONFLICTS {
                return None;
            }
        }
        if !settled {
            return None;
        }
    }

    // Every binary is decided. Continuous columns stay where the relaxation left them
    // when that is still within their bounds, which polish may improve on afterwards.
    let mut point = vec![0.0f64; n];
    for (j, slot) in point.iter_mut().enumerate() {
        *slot = if state.fixed(j) {
            state.value(j)
        } else {
            x[j].clamp(state.lower(j), state.upper(j))
        };
    }
    // Only binaries are decided above, so a general integer column left free is still
    // at its relaxation value. `is_feasible` reads rows and bounds, not integrality, and
    // let such a point through as an incumbent: on MIPLIB's `neos-3381206-awhea`, 259
    // integer columns fractional, reported optimal at 452 against an optimum of 453.
    let integral = problem
        .integer_columns()
        .all(|j| (point[j] - point[j].round()).abs() <= limits.feasibility_tolerance);
    (integral && is_feasible(problem, &point, limits.feasibility_tolerance)).then(|| Incumbent {
        objective: objective_of(problem, &point),
        x: point,
    })
}

/// How much a violated row's weight grows when the search has nowhere left to go.
const JUMP_WEIGHT_BUMP: f64 = 1.0;

/// Moves without reducing the least violation seen before the search is abandoned.
///
/// Generous rather than tight, because weight bumping is meant to make things worse for
/// a while: a run climbing out of a local minimum raises violation deliberately and the
/// cutoff must not mistake that for failure. What it catches is the other case, a run
/// that has settled and will not move again however long it is left.
const JUMP_STALL: usize = 30_000;

/// How far the candidate queue may outgrow the columns before it is rebuilt.
const STALE_FACTOR: usize = 8;

/// A row's distance from being satisfied.
fn row_violation(activity: f64, lb: f64, ub: f64) -> f64 {
    let below = if activity < lb { lb - activity } else { 0.0 };
    let above = if activity > ub { activity - ub } else { 0.0 };
    below + above
}

/// A column and the gain from flipping it, ordered so the best is popped first.
#[derive(PartialEq)]
struct Candidate {
    gain: f64,
    column: usize,
}

impl Eq for Candidate {}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.gain
            .total_cmp(&other.gain)
            .then_with(|| other.column.cmp(&self.column))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The corners of the box, tried before anything else.
///
/// Putting every column on one of its bounds costs a single pass over the matrix and is
/// feasible more often than it has any right to be: on the pure binary set the all-zero
/// point satisfies `mine-166-5`, `neos-953928`, `neos-957143` and `neos-960392`, and the
/// all-upper point satisfies `neos-787933`. A reference solver reports these as its first
/// incumbent on eight of the thirty instances this solver loses.
///
/// Cheap enough that it is unconditional. Anything it finds is a genuine feasible point,
/// usually a poor one, and everything downstream is free to improve on it.
pub fn corners(problem: &Problem, limits: &Limits) -> Option<Incumbent> {
    let n = problem.n_cols();
    let mut best: Option<Incumbent> = None;
    for pick_upper in [false, true] {
        let x: Vec<f64> = (0..n)
            .map(|j| {
                let (lo, hi) = (problem.col_lb[j], problem.col_ub[j]);
                // A corner of an unbounded column is not a corner; zero stands in.
                let value = if pick_upper { hi } else { lo };
                if value.is_finite() {
                    value
                } else {
                    0.0f64.clamp(lo, hi)
                }
            })
            .collect();
        if !is_feasible(problem, &x, limits.feasibility_tolerance) {
            continue;
        }
        let objective = objective_of(problem, &x);
        if best.as_ref().is_none_or(|held| objective < held.objective) {
            best = Some(Incumbent { objective, x });
        }
    }
    best
}

/// Search for a feasible point without solving a single LP.
///
/// Every heuristic above this one is a way of asking the relaxation where to look, and
/// on a model whose feasible set is sparse the relaxation does not know. Across the pure
/// binary instances this solver loses on, the whole LP-driven chain takes 71% of the
/// root's iterations and returns nothing, and on seven of them the relaxation does not
/// even finish, so nothing downstream of it ever runs at all.
///
/// This asks the constraints instead. Each row carries a weight, the objective being the
/// weighted sum of how far the rows are from satisfied, and each step flips whichever
/// column reduces that sum the most. Reaching a point where no single flip improves
/// anything is not the end: the weights of the rows still violated go up, which reshapes
/// the surface until a flip helps again. That is what carries it out of the local minima
/// that stop a rounding heuristic dead.
///
/// The published form of this is Luteberget and Sandvik's Feasibility Jump. What is here
/// is its feasibility half: no objective term, because the measured blocker is finding
/// any feasible point at all rather than finding a good one, and because whatever this
/// returns is handed to polish and to the improvement search afterwards.
///
/// Binary columns only. General integers would need a jump to the best value in their
/// range rather than a flip, and the models this is aimed at do not have any. Continuous
/// columns stay where they were put and contribute a constant to every row they are in.
pub fn feasibility_jump(
    problem: &Problem,
    start: &[f64],
    limits: &Limits,
    max_moves: usize,
    deadline: Option<std::time::Instant>,
) -> Option<Incumbent> {
    // Checked on a stride: the clock is far more expensive than a flip, and a stride
    // this size is well under a second's worth of work on the largest model here.
    const CLOCK_STRIDE: usize = 512;
    let n = problem.n_cols();
    let m = problem.n_rows();
    // A general integer column has no flip, and standing in for one badly is worse than
    // declining the model.
    if (0..n).any(|j| problem.is_integer(j) && !is_binary_column(problem, j)) {
        return None;
    }
    // The matrix is held by column, so a column's rows and coefficients are already
    // adjacent; only the row-wise view has to be built.
    let csr = problem.matrix.to_csr();

    // Start from the relaxation, rounded, which is a better guess than either bound.
    let mut assign: Vec<f64> = (0..n)
        .map(|j| {
            if is_binary_column(problem, j) {
                let rounded: f64 = if start[j] > 0.5 { 1.0 } else { 0.0 };
                rounded.clamp(problem.col_lb[j], problem.col_ub[j])
            } else {
                start[j].clamp(problem.col_lb[j], problem.col_ub[j])
            }
        })
        .collect();

    let mut activity = vec![0.0f64; m];
    for i in 0..m {
        let (cols, vals) = csr.column(i);
        activity[i] = cols.iter().zip(vals).map(|(&j, &a)| a * assign[j]).sum();
    }
    let mut weight = vec![1.0f64; m];
    let mut violation: Vec<f64> = (0..m)
        .map(|i| row_violation(activity[i], problem.row_lb[i], problem.row_ub[i]))
        .collect();

    let tolerance = limits.feasibility_tolerance;
    let movable: Vec<usize> = (0..n)
        .filter(|&j| is_binary_column(problem, j) && problem.col_lb[j] < problem.col_ub[j])
        .collect();
    if movable.is_empty() {
        return None;
    }

    // What flipping `j` would do to the weighted violation, positive being an
    // improvement.
    let gain_of = |j: usize,
                   assign: &[f64],
                   activity: &[f64],
                   violation: &[f64],
                   weight: &[f64]|
     -> f64 {
        let step = 1.0 - 2.0 * assign[j];
        let (rows, vals) = problem.matrix.column(j);
        let mut gain = 0.0;
        for (&i, &a) in rows.iter().zip(vals) {
            let moved = activity[i] + a * step;
            let after = row_violation(moved, problem.row_lb[i], problem.row_ub[i]);
            gain += weight[i] * (violation[i] - after);
        }
        gain
    };

    let mut gain: Vec<f64> = vec![0.0; n];
    let mut heap: std::collections::BinaryHeap<Candidate> = std::collections::BinaryHeap::new();
    for &j in &movable {
        gain[j] = gain_of(j, &assign, &activity, &violation, &weight);
        heap.push(Candidate {
            gain: gain[j],
            column: j,
        });
    }

    let mut total: f64 = violation.iter().sum();
    // The least violation seen, and how long since it last fell. Where this heuristic
    // works it works quickly, closing out in about a second on every instance it wins;
    // where it does not, it flips until its budget runs out and returns nothing. The
    // difference is visible while it runs, so it is worth watching rather than waiting
    // out: a run that has stopped reducing violation at all is not about to start.
    let mut best = total;
    let mut since_improved = 0usize;
    let mut moves = 0usize;
    while moves < max_moves {
        if total <= tolerance {
            break;
        }
        // A move budget alone does not bound the time this takes: a flip costs the
        // columns of the rows it touches, which on a dense model is thousands of them.
        // Without this `mitre` spends 34 seconds here against a limit of 20 and never
        // reaches the relaxation at all.
        if moves.is_multiple_of(CLOCK_STRIDE)
            && deadline.is_some_and(|d| std::time::Instant::now() >= d)
        {
            break;
        }
        if total < best - 1e-9 {
            best = total;
            since_improved = 0;
        } else {
            since_improved += 1;
            if since_improved > JUMP_STALL {
                break;
            }
        }
        // Entries are superseded rather than removed, so the queue grows by every column
        // a flip touches and fills with readings that no longer hold. Left alone it
        // becomes most of the running time: each step then pops through the accumulated
        // staleness to reach anything current, which took `f2gap201600` from 0.7 seconds
        // to nearly seven. Rebuilding from the live scores once it outgrows the columns
        // several times over keeps that bounded.
        if heap.len() > STALE_FACTOR * movable.len() {
            heap.clear();
            for &k in &movable {
                if gain[k] > 1e-12 {
                    heap.push(Candidate {
                        gain: gain[k],
                        column: k,
                    });
                }
            }
        }
        // The best flip available, discarding heap entries a later update has stale.
        let mut chosen: Option<usize> = None;
        while let Some(candidate) = heap.pop() {
            if (candidate.gain - gain[candidate.column]).abs() > 1e-12 {
                continue;
            }
            if candidate.gain > 1e-12 {
                chosen = Some(candidate.column);
            } else {
                // Nothing improves: put it back for the next pass, which will see the
                // reweighted surface rather than this one.
                heap.push(candidate);
            }
            break;
        }

        let Some(j) = chosen else {
            // A local minimum. Every row still violated becomes more expensive to leave
            // violated, which is what lets the next step move somewhere this one could
            // not.
            let mut touched: Vec<usize> = Vec::new();
            for i in 0..m {
                if violation[i] > tolerance {
                    weight[i] += JUMP_WEIGHT_BUMP;
                    let (cols, _) = csr.column(i);
                    touched.extend_from_slice(cols);
                }
            }
            if touched.is_empty() {
                break;
            }
            touched.sort_unstable();
            touched.dedup();
            for &k in &touched {
                if problem.col_lb[k] >= problem.col_ub[k] || !is_binary_column(problem, k) {
                    continue;
                }
                gain[k] = gain_of(k, &assign, &activity, &violation, &weight);
                heap.push(Candidate {
                    gain: gain[k],
                    column: k,
                });
            }
            moves += 1;
            continue;
        };

        // Apply the flip and repair everything it touched.
        let step = 1.0 - 2.0 * assign[j];
        assign[j] += step;
        let mut touched: Vec<usize> = Vec::new();
        let (rows, vals) = problem.matrix.column(j);
        for (&i, &a) in rows.iter().zip(vals) {
            activity[i] += a * step;
            let after = row_violation(activity[i], problem.row_lb[i], problem.row_ub[i]);
            total += after - violation[i];
            violation[i] = after;
            let (cols, _) = csr.column(i);
            touched.extend_from_slice(cols);
        }
        touched.sort_unstable();
        touched.dedup();
        for &k in &touched {
            if problem.col_lb[k] >= problem.col_ub[k] || !is_binary_column(problem, k) {
                continue;
            }
            gain[k] = gain_of(k, &assign, &activity, &violation, &weight);
            heap.push(Candidate {
                gain: gain[k],
                column: k,
            });
        }
        moves += 1;
    }

    let found = is_feasible(problem, &assign, tolerance);
    if std::env::var("RIPSOLVE_JUMP_TRACE").is_ok() {
        eprintln!(
            "JUMP moves={moves} cols={} nnz={} violation={total:.6} found={found}",
            problem.n_cols(),
            problem.matrix.nnz(),
        );
    }
    found.then(|| Incumbent {
        objective: objective_of(problem, &assign),
        x: assign,
    })
}
