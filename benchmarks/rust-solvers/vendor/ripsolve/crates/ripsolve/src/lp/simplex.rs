//! A bounded-variable revised primal simplex, with a phase-1 for feasibility.
//!
//! # Computational form
//!
//! The model's range rows are turned into equalities by giving every row a logical
//! (slack) variable:
//!
//! ```text
//!   min  c'z    subject to    [A | -I] z = 0,    l <= z <= u
//! ```
//!
//! where `z = [x; s]` puts the `n` structural variables first and the `m` logicals
//! after. A logical carries its row's bounds, so `<=`, `>=`, `=` and range rows all
//! reduce to a bound pair and the simplex never branches on row sense. Structural
//! bounds start at `[0, 1]`; branching later narrows them to `[0,0]` or `[1,1]`.
//!
//! The starting basis is all-logical, whose matrix is `-I`, trivially invertible,
//! and generally primal infeasible, which is what phase 1 is for.
//!
//! # Phase 1
//!
//! Phase 1 minimizes the sum of bound violations of the basic variables. Its
//! gradient is `-1` for a basic below its lower bound and `+1` for one above its
//! upper bound, and zero elsewhere; nonbasic variables sit on a bound and so never
//! contribute. The ratio test stops at the first breakpoint, the first basic
//! variable to reach a bound, whether it is becoming feasible or losing
//! feasibility. That is the textbook short-step rule: monotone and easy to verify,
//! but it takes more iterations than the long-step piecewise test a mature solver
//! uses. Replacing it is an M1b concern, not a correctness one.

use crate::lp::basis::{Basis, BasisError};
use crate::model::Problem;
use std::time::Instant;

use crate::sparse::SparseMatrix;

/// Pivots between clock checks. Small enough to bound the overrun, large enough
/// that reading the clock does not show up in a profile.
/// Singular-basis repairs between clock reads. Each repair costs a factorization, so
/// this is far smaller than the interval between simplex iterations.
const REPAIR_CLOCK_INTERVAL: usize = 8;

/// Refactorize once the eta file's nonzeros reach this multiple of the factors' own.
///
/// The pivot-count interval remains as a ceiling; this is what usually fires.
const ETA_GROWTH_LIMIT: f64 = 1.0;

const CLOCK_INTERVAL: usize = 256;

/// Primal iterations after which pricing switches to devex; see [`Solver::consider_devex`].
///
/// Above what an ordinary solve takes and far below what a stalled one does. On the binary
/// set the relaxations that finish do so in a few thousand iterations, and the ones that do
/// not run to six figures.
const DEVEX_AFTER: usize = 5_000;


/// Share of the row count at or below which `B^-1 a_q` counts as hyper-sparse.
///
/// The usual threshold in the literature, and the one the reverted implementation was
/// judged against: below roughly a tenth of the rows it pays to walk only the reachable
/// part of the elimination graph instead of every row.
pub const HYPERSPARSE_SHARE: f64 = 0.10;

/// Factorizations kept per LP.
///
/// Swept over 1, 4, 8, 32, 64, 128 and 256: solve time falls all the way up, but so
/// does memory, and each worker in a parallel search holds its own cache. 64 keeps
/// most of the gain (v064c1000n100 11.2s to 9.2s, v128c1000n100 5.1s to 4.1s) at
/// 50MB on one thread and 200MB across sixteen. Larger is faster if memory is free.
const FACTOR_CACHE: usize = 64;

/// Numerical tolerances. Defaults follow the usual simplex conventions.
#[derive(Clone, Copy, Debug)]
pub struct Tolerances {
    /// A bound violation at or below this is not a violation.
    pub primal_feasibility: f64,
    /// A reduced cost between `-dual_feasibility` and `+dual_feasibility` is zero,
    /// so a column that far from profitable is not worth entering.
    pub dual_feasibility: f64,
    /// Pivots smaller than this are rejected as numerically unsafe.
    pub pivot: f64,
    /// Product-form updates to accumulate before refactorizing.
    ///
    /// Trades two costs against each other: every solve replays the whole eta file,
    /// so a long interval makes solves expensive, while refactorizing is a fixed
    /// cost amortized over the interval.
    ///
    /// The balance moves with the row count, because refactorizing is `O(m * fill)`
    /// while replaying an eta is closer to the width of one column. Fifty was tuned on
    /// models of a few hundred rows, where an LP finishes in under a hundred pivots
    /// and the interval never binds. On MIPLIB's neos-3048764-nadi (3186 rows) it made
    /// factorization a third of the solve: raising it to two hundred takes that LP
    /// from 1.75s to 1.09s and leaves every small model unchanged.
    pub refactor_interval: usize,
    /// Price the primal method's entering column by devex rather than by the largest
    /// reduced cost.
    ///
    /// Dantzig's rule takes the steepest reduced cost, which is the steepest *per unit
    /// of that variable* and says nothing about how far the solve actually moves. On a
    /// degenerate model it takes an enormous number of very short steps. Devex divides
    /// by an approximate norm of the step direction, so the score is improvement per
    /// unit of distance travelled, and the reference is updated across each pivot from
    /// the pivot row.
    ///
    /// **Off by default, because over the whole binary set it loses more than it wins.**
    /// The case for it was iteration counts on the models whose root relaxation does not
    /// converge -- cod105 3801 against 6337, cvs16r106-72 9914 against 22835 -- and none
    /// of those close at sixty seconds either way, so that gain converts nothing. What it
    /// cost is measured: `neos-787933` and `neos-913984` both close under Dantzig's rule
    /// and neither closes under devex, at either stall threshold, three runs each. It is
    /// kept because the relaxations it speeds up are real and a later change may make them
    /// reachable; see "Devex is off, and what the measurement was worth" in
    /// `docs/design-notes.md`.
    pub primal_devex: bool,
    /// Count how sparse the entering column's solve `B^-1 a_q` comes out.
    ///
    /// Off by default and read once per iteration when on, because the count itself is
    /// a pass over `alpha` and belongs in a diagnostic rather than in every solve. It
    /// exists to answer whether a hyper-sparse triangular solve would pay on the
    /// row-heavy models, which needs *this* solve isolated: an earlier attempt counted
    /// every FTRAN including the dense `B^-1 b` and read 95% everywhere, which decides
    /// nothing. See `examples/spikedensity.rs`.
    pub entering_density_stats: bool,
    /// Price a warm-started dual solve's leaving row by steepest edge, with every
    /// weight starting at one, rather than by the largest violation.
    ///
    /// A cold dual entry always does; this is the warm one, which repairs a node's
    /// branched bound or a cut's violation from its parent's basis. The largest
    /// violation was thought good enough there, on the grounds that the repair takes a
    /// few pivots and the row choice barely matters. On the clique partitioning model
    /// behind `alta_divisions` it takes thousands, 4000 to 11000 per node, and steepest
    /// edge takes 500 to 3000; the B-1 level's proof goes from 77 seconds to 43.
    ///
    /// **On by default, as a trade.** At thirty seconds a model over the cached MIPLIB
    /// set it closes `n9-3` and `ex9`, which the largest violation does not, and is
    /// 12% slower by geometric mean on the 32 both close; `n2seq36f` is the worst of
    /// them, 11.7 seconds to 29.7. Instances closed count for more than time on those
    /// already closed, which is what decides it.
    pub warm_steepest_edge: bool,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            primal_feasibility: 1e-7,
            dual_feasibility: 1e-7,
            pivot: 1e-9,
            refactor_interval: 200,
            primal_devex: false,
            entering_density_stats: false,
            warm_steepest_edge: true,
        }
    }
}

/// Largest devex weight tolerated before the reference framework is restarted.
///
/// Weights only ever rise under the update, so past some point they stop approximating
/// anything and the rule degenerates. Restarting sets every weight back to one, which
/// makes the current basis the reference.
const DEVEX_RESET: f64 = 1e7;

/// How a simplex solve ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LpStatus {
    Optimal,
    /// No point satisfies the constraints and bounds.
    Infeasible,
    /// The objective is unbounded below on the feasible region.
    Unbounded,
    /// The iteration limit was reached before the search concluded.
    IterationLimit,
    /// The dual objective rose past the caller's cutoff, so the true optimum is
    /// worse than a solution the caller already holds. Only the dual simplex can
    /// report this, since only there does the objective climb monotonically.
    CutOff,
}

/// A basis, saved so a related LP can start from it instead of from scratch.
///
/// Branch-and-bound lives on this: a child differs from its parent only in one
/// column's bounds, which leaves the parent's basis dual feasible. Re-solving with
/// the dual simplex from here typically takes a handful of pivots, where a cold
/// start would repeat the whole phase-1.
#[derive(Clone, Debug)]
pub struct BasisState {
    basic: Vec<usize>,
    status: Vec<Status>,
}

impl BasisState {
    /// Extend a basis saved before `k` rows were appended so it applies after.
    ///
    /// Each new row's logical starts basic in that row. A cut is violated by the
    /// point the basis describes (that is what makes it a cut) so the new basic
    /// variable starts outside its bounds and the basis is primal infeasible but
    /// still dual feasible, which is exactly the dual simplex's entry condition.
    pub fn extend_for_rows(&mut self, n_structural: usize, old_rows: usize, k: usize) {
        for i in 0..k {
            let logical = n_structural + old_rows + i;
            debug_assert_eq!(self.status.len(), logical);
            self.basic.push(logical);
            self.status.push(Status::Basic { row: old_rows + i });
        }
    }

    /// This basis for the same LP with the rows `removed` taken out, the rest keeping
    /// their order. `None` unless each removed row's logical is basic.
    ///
    /// A row whose logical is basic can go with its logical and leave a basis behind:
    /// one row and one basic variable fewer, every other column of the basis matrix
    /// losing only its entry in that row, which is the logical's own pivot row. That is
    /// the case for a cut gone slack, the only kind anyone removes, and the basis left
    /// is still optimal: the removed row bound nothing. Without this, dropping a cut
    /// cost a cold solve of the whole relaxation.
    pub fn without_rows(&self, n_structural: usize, removed: &[usize]) -> Option<BasisState> {
        let m = self.status.len() - n_structural;
        let mut gone = vec![false; m];
        for &i in removed {
            gone[i] = true;
        }
        // Where each surviving row lands, and which basis positions empty.
        let mut row_to = vec![usize::MAX; m];
        let mut next = 0;
        for i in 0..m {
            if !gone[i] {
                row_to[i] = next;
                next += 1;
            }
        }
        let mut position_gone = vec![false; self.basic.len()];
        for &i in removed {
            let Status::Basic { row } = self.status[n_structural + i] else {
                return None;
            };
            position_gone[row] = true;
        }
        let rename = |j: usize| {
            if j < n_structural {
                j
            } else {
                n_structural + row_to[j - n_structural]
            }
        };
        let basic: Vec<usize> = self
            .basic
            .iter()
            .zip(&position_gone)
            .filter(|&(_, &gone)| !gone)
            .map(|(&j, _)| rename(j))
            .collect();
        let mut status = vec![Status::NonBasic(At::Lower); n_structural + next];
        for (j, st) in self.status.iter().enumerate() {
            if j >= n_structural && gone[j - n_structural] {
                continue;
            }
            if let Status::NonBasic(at) = st {
                status[rename(j)] = Status::NonBasic(*at);
            }
        }
        for (row, &j) in basic.iter().enumerate() {
            status[j] = Status::Basic { row };
        }
        Some(BasisState { basic, status })
    }
}

/// A row in range form: coefficients over the structural columns, then `lb` and `ub`.
pub type RangeRow = (Vec<(usize, f64)>, f64, f64);

/// The result of solving an LP relaxation.
#[derive(Clone, Debug)]
pub struct LpSolution {
    pub status: LpStatus,
    /// Objective of the internal minimization form, valid when `status` is
    /// [`LpStatus::Optimal`].
    pub objective: f64,
    /// Values of the structural variables only.
    pub x: Vec<f64>,
    pub iterations: usize,
    /// Iterations that ran under Bland's rule, and iterations whose step was zero.
    ///
    /// Reported because the two together are what a degenerate stall looks like from
    /// outside, and finding that out cost a profiler and a day: `cod105`'s relaxation
    /// was 90% Bland and 94% zero-step, and neither is visible in an iteration count.
    pub bland_iterations: usize,
    pub degenerate_iterations: usize,
    /// Entering-column solves, their total nonzeros, and how many came out at or under
    /// [`HYPERSPARSE_SHARE`] of the row count. Zero unless
    /// [`Tolerances::entering_density_stats`] asked for them.
    pub entering_solves: usize,
    pub entering_nonzeros: u64,
    pub entering_sparse_solves: usize,
    /// A candidate Farkas certificate, when the status is [`LpStatus::Infeasible`] and
    /// the dual method is what proved it.
    ///
    /// One multiplier per row: `B^-T e_r` for the row the dual could not repair. Aggregate
    /// the model's rows by it and the result is a linear form no point of the current box
    /// can satisfy, which is what makes it a *proof* rather than an assertion. **It is a
    /// candidate and not a guarantee** -- the sign convention and the arithmetic both have
    /// to hold -- so a caller must verify it before resting anything on it; see
    /// [`crate::propagate::farkas_conflict`].
    pub farkas: Option<Vec<f64>>,
    /// The final basis, for warm-starting a related solve.
    pub basis: BasisState,
}

/// Where a nonbasic variable is parked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum At {
    Lower,
    Upper,
    /// A free variable, held at zero.
    Zero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Basic { row: usize },
    NonBasic(At),
}

/// What a warm-started solve should do first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// Repair primal infeasibility with the dual simplex, keeping dual feasibility.
    Dual,
    /// Repair primal infeasibility with phase 1, then optimize.
    Primal,
}

/// An LP in computational form, ready to solve.
///
/// Cloneable so that each thread of a parallel search can hold its own, since
/// solving a node mutates the column bounds.
pub struct Lp {
    n_structural: usize,
    m: usize,
    cost: Vec<f64>,
    lower: Vec<f64>,
    upper: Vec<f64>,
    /// The original `A`, column-major, `m x n_structural`.
    matrix: SparseMatrix,
    /// Which structural columns must take integer values. Logicals are continuous.
    integer: Vec<bool>,
    /// When to give up, if the caller set a wall-clock budget.
    ///
    /// Checked inside the pivot loops, not only between solves. A single LP on a
    /// large model can outlive the whole budget: on MIPLIB's atlanta-ip (48738
    /// columns) the solver ran four times past a sixty-second limit because nothing
    /// below the node loop had any notion of time.
    deadline: Option<Instant>,
    /// Set by someone else to stop this solve as though its deadline had passed. The
    /// conflict-driven search beside the tree sets it when it proves the incumbent
    /// optimal, and a relaxation that was never going to finish -- `neos-5129192-manaia`'s
    /// -- has nothing left to contribute.
    interrupt: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    tol: Tolerances,
    /// Recently used factorizations, most recent first, keyed by basis columns.
    ///
    /// Branch and bound re-solves the same basis constantly: a child differs from
    /// its parent only in a column's *bounds*, which do not enter the basis matrix
    /// at all, so the parent's factors are valid for the child verbatim. Without
    /// this, every warm start refactorized from scratch, measured at 9.1ms of a
    /// 10.9ms node on a 1000-row model, against 1.8ms of actual pivoting.
    ///
    /// Several entries rather than one, because best-bound node selection does not
    /// visit the tree in an order that keeps a single entry warm: consecutive nodes
    /// are usually unrelated, and a one-entry cache measured an 8-20% hit rate. What
    /// does recur is siblings, which share a parent's basis and tend to be reached
    /// near each other.
    factors: Vec<(Vec<usize>, Basis)>,
}

impl Clone for Lp {
    /// The cache is deliberately not cloned: it is a scratch optimization, and a
    /// fresh clone has no basis history worth carrying.
    fn clone(&self) -> Self {
        Self {
            n_structural: self.n_structural,
            m: self.m,
            cost: self.cost.clone(),
            lower: self.lower.clone(),
            upper: self.upper.clone(),
            matrix: self.matrix.clone(),
            integer: self.integer.clone(),
            deadline: self.deadline,
            interrupt: self.interrupt.clone(),
            tol: self.tol,
            factors: Vec::new(),
        }
    }
}

impl Lp {
    /// Build the LP relaxation of a binary program.
    ///
    /// Column bounds are taken from the problem, so a problem whose bounds have
    /// already been narrowed by presolve or branching relaxes accordingly.
    pub fn relaxation(problem: &Problem) -> Lp {
        let integer: Vec<bool> = (0..problem.n_cols())
            .map(|j| problem.is_integer(j))
            .collect();
        let n = problem.n_cols();
        let m = problem.n_rows();
        let mut cost = problem.obj.clone();
        cost.resize(n + m, 0.0);

        let mut lower = problem.col_lb.clone();
        let mut upper = problem.col_ub.clone();
        lower.extend_from_slice(&problem.row_lb);
        upper.extend_from_slice(&problem.row_ub);

        Lp {
            n_structural: n,
            m,
            cost,
            lower,
            upper,
            matrix: problem.matrix.clone(),
            integer,
            deadline: None,
            interrupt: None,
            tol: Tolerances::default(),
            factors: Vec::new(),
        }
    }

    /// Append rows to the model in place, one logical variable each.
    ///
    /// Rows are given as `(coefficients, lb, ub)` over the structural columns, in the
    /// same range form the model already uses. The new logicals take the highest
    /// indices, which is what lets every existing column index, and so every saved
    /// basis, keep its meaning across the growth.
    ///
    /// This is how a cut reaches a node's LP without rebuilding it. The alternative,
    /// constructing a fresh `Lp` from an augmented `Problem`, costs a cold
    /// factorization at every node that separates.
    pub fn add_rows(&mut self, rows: &[RangeRow]) {
        if rows.is_empty() {
            return;
        }
        let mut by_column: Vec<Vec<(usize, f64)>> = vec![Vec::new(); self.n_structural];
        for (offset, (coefficients, _, _)) in rows.iter().enumerate() {
            for &(j, a) in coefficients {
                by_column[j].push((offset, a));
            }
        }
        self.matrix = self.matrix.with_rows_appended(rows.len(), &by_column);

        // Logicals for the new rows land at the end of the variable order, after the
        // existing `n + m` variables, so nothing already indexed moves.
        for (_, lb, ub) in rows {
            self.cost.push(0.0);
            self.lower.push(*lb);
            self.upper.push(*ub);
        }
        self.m += rows.len();
        // Every cached factorization was built for the old row count.
        self.invalidate_factors();
    }

    /// Extend a basis saved before rows were appended to this LP so it applies now.
    ///
    /// Rows are only ever appended, so a basis from fewer rows describes a prefix of
    /// this LP and needs only the new rows' logicals; see
    /// [`BasisState::extend_for_rows`].
    pub fn fit(&self, basis: &mut BasisState) {
        let rows = basis.status.len() - self.n_structural;
        debug_assert!(rows <= self.m, "a basis from more rows than the LP has");
        if rows < self.m {
            basis.extend_for_rows(self.n_structural, rows, self.m - rows);
        }
    }

    pub fn with_tolerances(mut self, tol: Tolerances) -> Self {
        self.tol = tol;
        self
    }

    /// Narrow a structural column's bounds, as branching does.
    pub fn set_column_bounds(&mut self, j: usize, lo: f64, hi: f64) {
        debug_assert!(j < self.n_structural);
        self.lower[j] = lo;
        self.upper[j] = hi;
    }

    /// Give the solver a wall-clock budget, after which a solve gives up and
    /// reports [`LpStatus::IterationLimit`], the node is unresolved either way,
    /// and the search treats both the same.
    pub fn set_deadline(&mut self, deadline: Option<Instant>) {
        self.deadline = deadline;
    }

    /// Stop any solve once `flag` is set, as if the deadline had passed.
    pub fn set_interrupt(&mut self, flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>) {
        self.interrupt = flag;
    }

    /// The tolerances this LP solves with, for an LP built to replace it.
    pub fn tolerances(&self) -> Tolerances {
        self.tol
    }

    /// The flag [`Lp::set_interrupt`] installed, for an LP built to replace this one.
    pub fn interrupt(&self) -> Option<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        self.interrupt.clone()
    }

    /// Has the budget run out, or has someone asked this to stop?
    pub fn out_of_time(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
            || self
                .interrupt
                .as_ref()
                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Discard any cached factorization.
    ///
    /// Only needed if the constraint matrix changes; bounds and costs do not affect
    /// the basis matrix, which is the whole reason the cache is worth having.
    pub fn invalidate_factors(&mut self) {
        self.factors.clear();
    }

    /// Replace the structural objective coefficients.
    ///
    /// The feasibility pump repeatedly re-optimizes the same constraint set under a
    /// distance objective, so it needs to swap the costs without rebuilding the LP.
    /// Logical columns keep their zero cost.
    pub fn set_costs(&mut self, costs: &[f64]) {
        debug_assert_eq!(costs.len(), self.n_structural);
        self.cost[..self.n_structural].copy_from_slice(costs);
    }

    /// The structural objective coefficients, for saving and restoring.
    pub fn costs(&self) -> &[f64] {
        &self.cost[..self.n_structural]
    }

    /// The current bounds of a structural column.
    pub fn column_bounds(&self, j: usize) -> (f64, f64) {
        (self.lower[j], self.upper[j])
    }

    pub fn n_columns(&self) -> usize {
        self.n_structural
    }

    /// Rows currently in the model, including any added since construction.
    pub fn n_rows(&self) -> usize {
        self.m
    }

    fn n_total(&self) -> usize {
        self.n_structural + self.m
    }

    /// Column `j` of `[A | -I]`, appended into caller-owned buffers.
    ///
    /// Refactorization needs all `m` basis columns at once, and allocating a pair of
    /// vectors for each of them every time put a large share of the solve in the
    /// allocator.
    fn column_sparse_into(&self, j: usize, rows: &mut Vec<usize>, values: &mut Vec<f64>) {
        rows.clear();
        values.clear();
        if j < self.n_structural {
            let (r, v) = self.matrix.column(j);
            rows.extend_from_slice(r);
            values.extend_from_slice(v);
        } else {
            rows.push(j - self.n_structural);
            values.push(-1.0);
        }
    }

    /// Scatter column `j` of `[A | -I]` into a dense buffer of length `m`.
    fn column_into(&self, j: usize, out: &mut [f64]) {
        out.fill(0.0);
        if j < self.n_structural {
            let (rows, vals) = self.matrix.column(j);
            for (&i, &v) in rows.iter().zip(vals) {
                out[i] = v;
            }
        } else {
            out[j - self.n_structural] = -1.0;
        }
    }

    /// The bound a nonbasic variable sits on.
    fn value_at(&self, j: usize, at: At) -> f64 {
        match at {
            At::Lower => self.lower[j],
            At::Upper => self.upper[j],
            At::Zero => 0.0,
        }
    }

    /// Solve from a cold start, returning the optimum of the internal minimization
    /// form.
    pub fn solve(&mut self) -> LpSolution {
        self.solve_with_limit(200_000)
    }

    pub fn solve_with_limit(&mut self, max_iterations: usize) -> LpSolution {
        self.run_solver(None, None, max_iterations)
    }

    /// Solve from a cold start, entering through the dual method rather than the primal.
    ///
    /// The primal method reaches feasibility through phase 1, and phase 1 is where this
    /// solver gets stuck: the cost vector there scores a basic variable by whether it
    /// currently violates a bound, so it is blind to the kink at one sitting exactly on
    /// one, and no ratio test can repair a column choice made that way. The dual method
    /// has no phase 1 to get stuck in.
    ///
    /// This is a fallback, not the default, and the distinction is the whole of why it
    /// is safe. The two methods end on *different* optimal vertices, and everything
    /// downstream reads the root's vertex rather than its objective: Gomory cuts come
    /// off that tableau and branching reads its fractional values. Made the default,
    /// this reaches the worse vertex on most of this set and widens most gaps. Reached
    /// only where the primal method returned no vertex at all, there is nothing for it
    /// to be worse than.
    ///
    /// Reports `IterationLimit` without trying when the model cannot be parked dual
    /// feasibly, which is a column whose cost points at a bound it does not have.
    pub fn solve_cold_dual(&mut self, max_iterations: usize) -> LpSolution {
        let (solution, basic, basis) = {
            let lp = &*self;
            let solver = Solver::cold(lp, true);
            if solver.entry_hint != Entry::Dual {
                // Not parkable dual feasibly. Running anyway would be the primal solve
                // the caller has already tried and which already failed.
                return LpSolution {
                    status: LpStatus::IterationLimit,
                    objective: f64::NAN,
                    x: Vec::new(),
                    iterations: 0,
                    bland_iterations: 0,
                    degenerate_iterations: 0,
                    farkas: None,
                    entering_solves: 0,
                    entering_nonzeros: 0,
                    entering_sparse_solves: 0,
                    basis: BasisState {
                        basic: Vec::new(),
                        status: Vec::new(),
                    },
                };
            }
            solver.run(max_iterations, None)
        };
        self.factors.insert(0, (basic, basis));
        self.factors.truncate(FACTOR_CACHE);
        solution
    }

    /// The reduced cost of every structural column at `basis`, with the bound each
    /// nonbasic one is parked on. Basic columns report `None`.
    ///
    /// This is what makes a bound do more than prune. A nonbasic column's reduced cost
    /// is the rate at which the objective rises as it moves off the bound it sits on,
    /// so with a proven bound `z` and an incumbent `u`, a column that would push the
    /// objective past `u` before reaching its other bound cannot take that other value
    /// in any solution better than the one already in hand.
    ///
    /// Returned rather than acted on here because the decision needs an incumbent,
    /// which is the search's business and not the LP's.
    /// Reuses the factorization `basis` was left with when the cache is still holding
    /// it, which it is immediately after the solve that produced it. Called there this
    /// costs one BTRAN and a pass over the columns; called anywhere else it falls back
    /// to a fresh factorization, which is what the old path cost and is never wrong.
    pub fn reduced_costs(&self, basis: &BasisState) -> Option<Vec<Option<(f64, bool)>>> {
        let factors = self
            .factors
            .iter()
            .find(|(columns, _)| *columns == basis.basic)
            .map(|(_, held)| held.clone());
        let factorized = factors.is_some();
        let mut solver = Solver::warm(self, basis, factors);
        if !factorized && solver.refactorize().is_err() {
            return None;
        }
        solver.load_basic_costs(false);
        let mut y = std::mem::take(&mut solver.y);
        solver.basis.btran(&solver.cost_b, &mut y);
        solver.y = y;
        Some(
            (0..self.n_structural)
                .map(|j| match solver.status[j] {
                    Status::Basic { .. } => None,
                    Status::NonBasic(At::Lower) => Some((solver.reduced_cost(j, false), false)),
                    Status::NonBasic(At::Upper) => Some((solver.reduced_cost(j, false), true)),
                    // A free column parked at zero is on no bound, so there is no
                    // "other value" for a reduced cost to rule out.
                    Status::NonBasic(At::Zero) => None,
                })
                .collect(),
        )
    }

    /// Generate Gomory mixed-integer cuts from the tableau at `basis`.
    ///
    /// Returned in terms of the *structural* columns, as `(coefficients, lower
    /// bound)` pairs meaning `sum a_j x_j >= lb`.
    ///
    /// # Derivation
    ///
    /// Each tableau row reads `z_B(i) + sum_j a_j w_j = beta`, where `w_j >= 0` is
    /// the nonbasic's distance from the bound it sits on, `z_j - l_j` at a lower
    /// bound, `u_j - z_j` at an upper one. That shift is what lets a
    /// bounded-variable simplex use the textbook formula, which assumes nonbasics at
    /// zero, and it is also where the sign of `a_j` flips for an at-upper column.
    ///
    /// With `f0` the fractional part of `beta`, the cut is `sum_j c_j w_j >= 1` for
    /// coefficients `c_j` that differ between integer and continuous `w_j`. Every
    /// structural here is binary and so integer; the logicals are continuous.
    ///
    /// Substituting `w_j` back gives a row over `z = [x; s]`, and the logical part
    /// is eliminated with `s = A x`, leaving a cut over the structural columns only.
    pub fn gomory_cuts(
        &self,
        basis: &BasisState,
        max_cuts: usize,
    ) -> Vec<(Vec<(usize, f64)>, f64)> {
        let mut solver = Solver::warm(self, basis, None);
        if solver.refactorize().is_err() {
            return Vec::new();
        }
        solver.gomory_cuts(max_cuts)
    }

    /// Solve this model grown by `rows`, reusing the factorization `basis` was left
    /// with instead of building a new one.
    ///
    /// The grown basis is block triangular against the one already factorized, so the
    /// reuse is exact rather than approximate, see [`Basis::extend`]. Without it,
    /// every call pays a cold factorization at the grown dimension, which is what put
    /// node-local cutting out of reach at any useful frequency.
    ///
    /// The grown model is temporary and is not cached: the caller wants the bound it
    /// proves, not a model to keep.
    pub fn solve_with_rows(
        &self,
        basis: &BasisState,
        rows: &[RangeRow],
        cutoff: Option<f64>,
        max_iterations: usize,
    ) -> LpSolution {
        let coefficients: Vec<Vec<(usize, f64)>> = rows.iter().map(|(c, _, _)| c.clone()).collect();
        // Falls back to a fresh factorization if this basis is not the one the cache
        // is holding, which costs what the old path cost and is never wrong.
        let factors = self
            .factors
            .iter()
            .find(|(columns, _)| *columns == basis.basic)
            .map(|(_, held)| {
                let mut extended = held.clone();
                extended.extend(&coefficients, &basis.basic, self.n_structural);
                extended
            });

        let mut grown = self.clone();
        grown.add_rows(rows);
        let mut state = basis.clone();
        state.extend_for_rows(self.n_structural, self.m, rows.len());

        Solver::warm(&grown, &state, factors)
            .run(max_iterations, cutoff)
            .0
    }

    /// Re-solve starting from a saved basis.
    ///
    /// `cutoff` lets the solve abandon a node early: the dual simplex's objective
    /// only ever climbs, so once it passes the cutoff the node's true bound must be
    /// worse still and the result is [`LpStatus::CutOff`].
    pub fn solve_warm(
        &mut self,
        start: &BasisState,
        cutoff: Option<f64>,
        max_iterations: usize,
    ) -> LpSolution {
        self.run_solver(Some(start), cutoff, max_iterations)
    }

    /// Shared body of the solve entry points, wrapping the factorization cache.
    ///
    /// The cache hits when the requested basis is the one the last solve *ended*
    /// on, which in a tree search is the common case: both children of a node ask
    /// for their parent's final basis, and a dive or a probe asks for the basis it
    /// just produced.
    fn run_solver(
        &mut self,
        start: Option<&BasisState>,
        cutoff: Option<f64>,
        max_iterations: usize,
    ) -> LpSolution {
        let factors = start.and_then(|start| {
            let found = self
                .factors
                .iter()
                .position(|(columns, _)| *columns == start.basic)?;
            Some(self.factors.remove(found).1)
        });

        let (solution, basic, basis) = {
            let lp = &*self;
            match start {
                Some(start) => Solver::warm(lp, start, factors).run(max_iterations, cutoff),
                None => Solver::new(lp).run(max_iterations, None),
            }
        };
        // Most recent first, oldest evicted. The key comparison is a slice equality
        // over the basis columns, which is negligible against the cost of the
        // factorization it avoids.
        self.factors.insert(0, (basic, basis));
        self.factors.truncate(FACTOR_CACHE);
        solution
    }
}

/// Mutable state for one solve.
struct Solver<'a> {
    lp: &'a Lp,
    basis: Basis,
    /// Which variable is basic in each row.
    basic: Vec<usize>,
    status: Vec<Status>,
    /// Current value of every variable.
    z: Vec<f64>,
    /// The basis columns, refilled in place for each refactorization so the inner
    /// vectors keep their allocations.
    basis_columns: Vec<(Vec<usize>, Vec<f64>)>,
    /// Scratch buffers, reused across iterations to keep the loop allocation-free.
    col: Vec<f64>,
    alpha: Vec<f64>,
    y: Vec<f64>,
    cost_b: Vec<f64>,
    rhs: Vec<f64>,
    /// Which method to try first; see [`Solver::run`].
    entry_hint: Entry,
    /// True when `basis` already factorizes `basic`, so entry can skip the rebuild.
    factorized: bool,
    /// Values of the basic variables, recomputed after a refactorization.
    basic_values: Vec<f64>,
    /// Price the dual method's leaving row by steepest edge rather than by the largest
    /// violation, carrying the weights below across each pivot.
    ///
    /// Only a cold dual entry sets this. A warm start begins next to its parent's
    /// optimum and repairs one bound in a few pivots, so the row choice barely matters
    /// there and the extra FTRAN per iteration is pure cost; a cold one crosses the
    /// whole model and the choice is most of the solve.
    dual_steepest: bool,
    /// Price the dual method's leaving row by steepest edge rather than by the largest
    /// violation. Set by a cold dual entry with the rest of `dual_steepest`, and by a
    /// warm start alone; see [`Solver::warm`].
    steepest_rows: bool,
    /// Dual steepest edge weights, one per basis row; see [`Solver::leaving_row`].
    row_weights: Vec<f64>,
    /// Scratch for the steepest edge update, `B^-1` applied to the pivot row.
    tau: Vec<f64>,
    /// Devex reference weights, one per column; see [`Solver::price`].
    col_weights: Vec<f64>,
    /// Scratch for the pivot row, `B^-T e_r`.
    rho: Vec<f64>,
    /// Iterations that ran under Bland's rule, and iterations whose step was zero.
    /// Reported out so a stalling solve can be read without a profiler; see
    /// `examples/pivotmix.rs`.
    bland_iterations: usize,
    degenerate_iterations: usize,
    /// Entering-column solves counted, their total nonzeros, and how many of them came
    /// out at or under [`HYPERSPARSE_SHARE`] of the row count. Only filled when
    /// [`Tolerances::entering_density_stats`] is set.
    entering_solves: usize,
    entering_nonzeros: u64,
    entering_sparse_solves: usize,
    /// Set when the dual method proves infeasibility; see [`LpSolution::farkas`].
    farkas: Option<Vec<f64>>,
    /// Whether devex is pricing right now, which is not the same as whether the option
    /// asked for it; see [`Solver::consider_devex`].
    devex_active: bool,
    /// Whether this solve may switch pricing at all.
    ///
    /// Only a cold entry may. A cold solve crosses the whole model and is where a
    /// relaxation that will not converge announces itself; a warm one starts beside its
    /// parent's optimum and repairs a bound in a few pivots, so a long warm solve is a
    /// different animal and switching on it is what cost `neos-787933` its solve.
    devex_switchable: bool,
}

/// What the ratio test decided.
enum Step {
    /// Pivot: `leaving_row` leaves, entering advances by `step`.
    Pivot {
        leaving_row: usize,
        step: f64,
        to: At,
    },
    /// The entering variable reached its opposite bound first; no basis change.
    BoundFlip { step: f64 },
    /// The objective improves without limit along this ray.
    Unbounded,
}

impl<'a> Solver<'a> {
    fn new(lp: &'a Lp) -> Solver<'a> {
        Solver::cold(lp, false)
    }

    /// A solver with every logical basic and every structural parked on a bound.
    ///
    /// Where each structural is parked decides which method can run from here, so with
    /// `prefer_dual` it is chosen deliberately. The logicals being basic makes the
    /// basis the identity, and their costs are zero, so the duals are zero and every
    /// structural's reduced cost is exactly its own objective coefficient. Dual
    /// feasibility is then a sign condition per column, decidable without any
    /// factorization: a column that costs something to increase belongs at its lower
    /// bound, one that pays to increase at its upper, and a free column is fine
    /// wherever it sits as long as it costs nothing.
    ///
    /// A column whose cost points at a bound it does not have cannot be parked
    /// feasibly, and one such column rules the dual method out for the model. The
    /// ordinary rule, nearest bound to zero, is then restored so the primal path starts
    /// exactly where it always did.
    fn cold(lp: &'a Lp, prefer_dual: bool) -> Solver<'a> {
        let n_total = lp.n_total();
        let m = lp.m;
        let tol = lp.tol.dual_feasibility;

        let mut status = vec![Status::NonBasic(At::Lower); n_total];
        let mut z = vec![0.0; n_total];
        let mut dual_ready = prefer_dual;
        if prefer_dual {
            for j in 0..lp.n_structural {
                let (lo, hi) = (lp.lower[j], lp.upper[j]);
                let at = if lo == hi {
                    // Fixed: it cannot move, so its reduced cost constrains nothing.
                    if lo.is_finite() { At::Lower } else { At::Zero }
                } else if lp.cost[j] > tol {
                    if lo.is_finite() {
                        At::Lower
                    } else {
                        dual_ready = false;
                        break;
                    }
                } else if lp.cost[j] < -tol {
                    if hi.is_finite() {
                        At::Upper
                    } else {
                        dual_ready = false;
                        break;
                    }
                } else if lo.is_finite() {
                    At::Lower
                } else if hi.is_finite() {
                    At::Upper
                } else {
                    At::Zero
                };
                status[j] = Status::NonBasic(at);
                z[j] = lp.value_at(j, at);
            }
        }
        if !dual_ready {
            // Every structural nonbasic on the bound nearer zero, which for a binary
            // relaxation is the lower bound.
            for j in 0..lp.n_structural {
                let at = if lp.lower[j].is_finite() {
                    At::Lower
                } else if lp.upper[j].is_finite() {
                    At::Upper
                } else {
                    At::Zero
                };
                status[j] = Status::NonBasic(at);
                z[j] = lp.value_at(j, at);
            }
        }
        let mut basic = Vec::with_capacity(m);
        for i in 0..m {
            let j = lp.n_structural + i;
            status[j] = Status::Basic { row: i };
            basic.push(j);
        }

        let mut solver = Solver {
            lp,
            basis: Basis::all_logical(m),
            basic,
            status,
            z,
            basis_columns: Vec::new(),
            col: vec![0.0; m],
            alpha: Vec::new(),
            y: Vec::new(),
            cost_b: vec![0.0; m],
            rhs: vec![0.0; m],
            entry_hint: if dual_ready { Entry::Dual } else { Entry::Primal },
            factorized: false,
            basic_values: Vec::new(),
            dual_steepest: dual_ready,
            steepest_rows: dual_ready,
            row_weights: vec![1.0; m],
            tau: Vec::new(),
            col_weights: vec![1.0; lp.n_total()],
            rho: Vec::new(),
            bland_iterations: 0,
            degenerate_iterations: 0,
            entering_solves: 0,
            entering_nonzeros: 0,
            entering_sparse_solves: 0,
            farkas: None,
            devex_active: lp.tol.primal_devex,
            devex_switchable: true,
        };
        solver.recompute_basic_values();
        solver
    }

    /// Rebuild a solver around a saved basis.
    ///
    /// Nonbasic variables are re-parked on their (possibly just-changed) bounds. A
    /// variable that has become fixed is pulled onto the fixed value regardless of
    /// which side it was parked on, so the recomputed basic values reflect the new
    /// bounds rather than the parent's.
    fn warm(lp: &'a Lp, start: &BasisState, factors: Option<Basis>) -> Solver<'a> {
        let m = lp.m;
        let mut z = vec![0.0; lp.n_total()];
        let mut status = start.status.clone();

        for (j, st) in status.iter_mut().enumerate() {
            if let Status::NonBasic(at) = st {
                // Re-park anything sitting on a bound that is now infinite, which can
                // happen only if the caller widened bounds rather than narrowing them.
                let mut here = *at;
                if !lp.value_at(j, here).is_finite() {
                    here = match here {
                        At::Lower if lp.upper[j].is_finite() => At::Upper,
                        At::Upper if lp.lower[j].is_finite() => At::Lower,
                        _ => At::Zero,
                    };
                    *at = here;
                }
                z[j] = lp.value_at(j, here);
            }
        }

        // Reusing the caller's factors is the point of the cache; without them the
        // solver falls back to factorizing on entry, as it always did.
        let factorized = factors.is_some();
        let mut solver = Solver {
            lp,
            basis: factors.unwrap_or_else(|| Basis::all_logical(m)),
            factorized,
            basic_values: Vec::new(),
            dual_steepest: false,
            steepest_rows: lp.tol.warm_steepest_edge,
            row_weights: vec![1.0; m],
            tau: Vec::new(),
            col_weights: vec![1.0; lp.n_total()],
            rho: Vec::new(),
            bland_iterations: 0,
            degenerate_iterations: 0,
            entering_solves: 0,
            entering_nonzeros: 0,
            entering_sparse_solves: 0,
            farkas: None,
            devex_active: lp.tol.primal_devex,
            devex_switchable: false,
            basic: start.basic.clone(),
            status,
            z,
            basis_columns: Vec::new(),
            col: vec![0.0; m],
            alpha: Vec::new(),
            y: Vec::new(),
            cost_b: vec![0.0; m],
            rhs: vec![0.0; m],
            entry_hint: Entry::Dual,
        };
        solver.recompute_basic_values();
        solver
    }

    /// Recompute the basic variables from the nonbasic ones: `z_B = B^-1 (-N z_N)`.
    ///
    /// Called at the start and after every refactorization, so drift from the
    /// incremental per-pivot updates cannot accumulate indefinitely.
    fn recompute_basic_values(&mut self) {
        let lp = self.lp;
        self.rhs.fill(0.0);
        for j in 0..lp.n_total() {
            if let Status::NonBasic(_) = self.status[j] {
                let v = self.z[j];
                if v == 0.0 {
                    continue;
                }
                if j < lp.n_structural {
                    let (rows, vals) = lp.matrix.column(j);
                    for (&i, &a) in rows.iter().zip(vals) {
                        self.rhs[i] -= a * v;
                    }
                } else {
                    self.rhs[j - lp.n_structural] += v;
                }
            }
        }
        // Its own buffer rather than borrowing `alpha`. `alpha` means one thing, the
        // entering column transformed by the basis, and it is read by the ratio test
        // and the basis update after this returns. Lending it out as scratch is how a
        // change to one of those quietly corrupts the other, which has happened here.
        let mut out = std::mem::take(&mut self.basic_values);
        self.basis.ftran(&self.rhs, &mut out);
        for (i, &v) in out.iter().enumerate() {
            self.z[self.basic[i]] = v;
        }
        self.basic_values = out;
    }

    /// The largest amount by which any basic variable sits outside its bounds.
    fn worst_violation(&self) -> f64 {
        let mut worst: f64 = 0.0;
        for &j in &self.basic {
            let over = (self.lp.lower[j] - self.z[j]).max(self.z[j] - self.lp.upper[j]);
            if over > worst {
                worst = over;
            }
        }
        worst
    }

    /// The phase-1 gradient of the basic costs, or the true costs in phase 2.
    fn load_basic_costs(&mut self, phase_one: bool) {
        let tol = self.lp.tol.primal_feasibility;
        for i in 0..self.lp.m {
            let j = self.basic[i];
            self.cost_b[i] = if phase_one {
                let v = self.z[j];
                if v < self.lp.lower[j] - tol {
                    -1.0
                } else if v > self.lp.upper[j] + tol {
                    1.0
                } else {
                    0.0
                }
            } else {
                self.lp.cost[j]
            };
        }
    }

    /// Reduced cost of nonbasic `j` given the current duals.
    fn reduced_cost(&mut self, j: usize, phase_one: bool) -> f64 {
        let cj = if phase_one { 0.0 } else { self.lp.cost[j] };
        let dot = if j < self.lp.n_structural {
            let (rows, vals) = self.lp.matrix.column(j);
            rows.iter()
                .zip(vals)
                .map(|(&i, &a)| self.y[i] * a)
                .sum::<f64>()
        } else {
            -self.y[j - self.lp.n_structural]
        };
        cj - dot
    }

    /// Choose an entering variable, returning it with the direction to move.
    ///
    /// Devex normally, switching to Bland's rule -- lowest index that improves -- once
    /// the solve has stalled. Devex can cycle on degenerate vertices as Dantzig's rule
    /// can; Bland's cannot, so falling back to it guarantees termination.
    ///
    /// # Why not Dantzig, which this was
    ///
    /// Dantzig's rule takes the largest reduced cost, which is the improvement per unit
    /// of *that variable* and says nothing about how far the solve actually moves along
    /// the edge it picks. On a degenerate model it takes an enormous number of very
    /// short steps. Measured against HiGHS on the same relaxations, with every column
    /// relaxed to continuous:
    ///
    /// ```text
    ///                   HiGHS iters    here, under Dantzig
    ///     cod105               4090    >100,000, does not converge
    ///     cvs16r70-62          7636    >100,000, does not converge
    ///     cvs16r106-72        12650    >100,000, does not converge
    /// ```
    ///
    /// Per iteration this solver was the faster of the two on `cod105` -- 823 a second
    /// against 547 -- so the whole of that gap was pivots that should not have been
    /// taken.
    ///
    /// Devex divides the squared reduced cost by an approximate squared norm of the step
    /// direction, so the score is improvement per unit of distance rather than per unit
    /// of variable. The norms are approximated from a reference framework, updated
    /// across each pivot from the pivot row and reset when they drift too far, which is
    /// what makes it affordable where exact steepest edge is not.
    fn price(&mut self, phase_one: bool, bland: bool) -> Option<(usize, f64)> {
        // Every column, every iteration. Partial pricing was implemented here and
        // reverted: sweeping a rotating window took `leo1`'s relaxation from 0.24s to
        // 0.12s, and on MIPLIB's neos-619167 it made the solver report a feasible model
        // infeasible. The mechanism is that a window's best candidate can have a tiny
        // reduced cost where the global best does not, and entering on one is entering
        // a column nearly dependent on the basis, which drives it singular. Dantzig's
        // rule is doing double duty as a numerical safeguard, and a partial rule needs
        // its own before it can replace that. Speed bought this way is not worth a
        // wrong answer.
        self.scan(0, self.lp.n_total(), phase_one, bland)
    }

    /// Price `count` columns starting at `start`, wrapping at the end.
    ///
    /// Returns the best candidate found, or under `bland` the first one that improves.
    fn scan(
        &mut self,
        start: usize,
        count: usize,
        phase_one: bool,
        bland: bool,
    ) -> Option<(usize, f64)> {
        let tol = self.lp.tol.dual_feasibility;
        let n = self.lp.n_total();
        let mut best: Option<(usize, f64)> = None;
        let mut best_score = 0.0;

        for step in 0..count {
            let j = if start == 0 { step } else { (start + step) % n };
            let Status::NonBasic(at) = self.status[j] else {
                continue;
            };
            // A fixed variable has nowhere to go.
            if self.lp.lower[j] == self.lp.upper[j] {
                continue;
            }
            let d = self.reduced_cost(j, phase_one);
            let sigma = match at {
                At::Lower if d < -tol => 1.0,
                At::Upper if d > tol => -1.0,
                At::Zero if d < -tol => 1.0,
                At::Zero if d > tol => -1.0,
                _ => continue,
            };
            if bland {
                return Some((j, sigma));
            }
            // Improvement per unit of distance travelled, not per unit of variable.
            // Squared, because the weight is a squared norm and taking a square root per
            // column to compare would cost more than the comparison is worth.
            let score = if self.devex() {
                d * d / self.col_weights[j]
            } else {
                d.abs()
            };
            if score > best_score {
                best_score = score;
                best = Some((j, sigma));
            }
        }
        best
    }

    /// Ratio test: how far can the entering variable move, and what stops it.
    ///
    /// In phase 2 every basic variable must stay inside its bounds. In phase 1 the
    /// step also stops when an *infeasible* basic reaches the bound it is violating,
    /// because the objective's slope changes there.
    ///
    /// Ties are broken on the largest pivot, which is the numerically safest choice.
    /// Under `bland` they are broken on the lowest variable index instead: Bland's
    /// rule only guarantees termination if it governs *both* the entering and the
    /// leaving choice, so pairing a Bland entering rule with a largest-pivot ratio
    /// test does not prevent cycling.
    fn ratio_test(&self, entering: usize, sigma: f64, phase_one: bool, bland: bool) -> Step {
        let lp = self.lp;
        let tol = lp.tol.primal_feasibility;

        // A pivot has to be significant against the *largest* entry of the transformed
        // column, not merely nonzero. An entry that clears the absolute tolerance while
        // being a billionth of its column leaves the next basis all but singular, and
        // the damage is not local: on MIPLIB's neos-850681 one such pivot, 3.7e-5 in a
        // column reaching 1.8e4, took the inverse from entries of 1e4 to 1e15 within two
        // iterations. Everything read through it afterwards is noise, and the solve
        // ended by reporting a feasible relaxation infeasible.
        //
        // The row holding the column maximum always clears this, so the test can never
        // empty the candidate set and turn a pivot into a bound flip.
        const RELATIVE_PIVOT: f64 = 1e-7;
        let largest = self.alpha.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let pivot_tol = lp.tol.pivot.max(RELATIVE_PIVOT * largest);

        // The entering variable's own range, if both bounds are finite.
        let mut best = if lp.lower[entering].is_finite() && lp.upper[entering].is_finite() {
            lp.upper[entering] - lp.lower[entering]
        } else {
            f64::INFINITY
        };
        let mut chosen: Option<(usize, At)> = None;
        // Among ties, prefer the largest pivot: it is the numerically safest choice
        // and is the standard cheap defence against degenerate stalling.
        let mut best_pivot = 0.0;

        for i in 0..lp.m {
            let beta = self.alpha[i] * sigma;
            if beta.abs() <= pivot_tol {
                continue;
            }
            let j = self.basic[i];
            let v = self.z[j];
            let (lo, hi) = (lp.lower[j], lp.upper[j]);

            // The bound this basic variable is heading towards, and hence the
            // breakpoint that limits the step. `beta > 0` means the variable falls.
            //
            // The bound it lands on is carried along with the target rather than
            // inferred from the direction of travel: in phase 1 an infeasible
            // variable travels *towards* the bound it violates, so a rising variable
            // can land on its lower bound, the opposite of the phase-2 case.
            let target = if phase_one {
                if v < lo - tol {
                    // Infeasible below: rising reaches `lo`; falling only worsens it,
                    // which phase 1 permits, so it imposes no limit.
                    if beta < 0.0 {
                        Some((lo, At::Lower))
                    } else {
                        None
                    }
                } else if v > hi + tol {
                    if beta > 0.0 {
                        Some((hi, At::Upper))
                    } else {
                        None
                    }
                } else if beta > 0.0 {
                    Some((lo, At::Lower))
                } else {
                    Some((hi, At::Upper))
                }
            } else if beta > 0.0 {
                Some((lo, At::Lower))
            } else {
                Some((hi, At::Upper))
            };

            let Some((target, to)) = target else { continue };
            if !target.is_finite() {
                continue;
            }

            // Clamp at zero: a basic variable already a hair past its bound would
            // otherwise produce a small negative ratio and a backwards step.
            let ratio = ((v - target) / beta).max(0.0);
            let pivot = self.alpha[i].abs();
            let ties = ratio <= best + 1e-12;
            let wins_tie = if bland {
                chosen.is_none_or(|(prev, _)| j < self.basic[prev])
            } else {
                pivot > best_pivot
            };
            if ratio < best - 1e-12 || (ties && wins_tie) {
                if ratio < best {
                    best = ratio;
                }
                chosen = Some((i, to));
                best_pivot = pivot;
            }
        }

        match chosen {
            Some((row, to)) => Step::Pivot {
                leaving_row: row,
                step: best,
                to,
            },
            None if best.is_finite() => Step::BoundFlip { step: best },
            None => Step::Unbounded,
        }
    }

    /// Apply a step of `sigma * step` on the entering variable.
    fn take_step(&mut self, entering: usize, sigma: f64, step: f64) {
        if step == 0.0 {
            return;
        }
        let delta = sigma * step;
        self.z[entering] += delta;
        for i in 0..self.lp.m {
            let j = self.basic[i];
            self.z[j] -= self.alpha[i] * delta;
        }
    }

    /// Rebuild the basis inverse, repairing singular positions with logicals.
    /// Rebuild the factors, repairing a singular basis by swapping in logicals.
    ///
    /// The error carries the status to report, and it is never `Infeasible`: failing to
    /// factorize a basis says nothing about whether the model has a feasible point.
    /// Callers used to collapse any failure to `Infeasible`, which turned a repair that
    /// merely ran out of time into a false proof: hypothyroid-k1, which HiGHS solves in
    /// 16s, came back "Infeasible" in 65s.
    ///
    /// That invariant was stated here and broken at the bottom of this same function,
    /// which is why the callers now assert it rather than trust it.
    fn refactorize(&mut self) -> Result<(), LpStatus> {
        let lp = self.lp;
        for attempt in 0..lp.m + 1 {
            // Each attempt is a whole factorization, and the caller's clock is only
            // read between simplex iterations, so a long run of repairs is invisible to
            // it. On MIPLIB's hypothyroid-k1, 5195 rows, that let a 60 second limit run
            // past 300 seconds inside a single call.
            if attempt > 0 && attempt.is_multiple_of(REPAIR_CLOCK_INTERVAL) && lp.out_of_time() {
                return Err(LpStatus::IterationLimit);
            }
            self.basis_columns.resize_with(lp.m, Default::default);
            for (slot, &j) in self.basis_columns.iter_mut().zip(&self.basic) {
                lp.column_sparse_into(j, &mut slot.0, &mut slot.1);
            }
            match self
                .basis
                .refactorize(&self.basis_columns, lp.tol.pivot, lp.deadline)
            {
                Ok(()) => {
                    self.factorized = true;
                    self.recompute_basic_values();
                    return Ok(());
                }
                // Out of time is not a repairable basis; it is the caller's budget.
                Err(BasisError::OutOfTime) => return Err(LpStatus::IterationLimit),
                Err(BasisError::Singular { row }) => {
                    // `row` is a *basis position*, not a model row, so the logical that
                    // shares its index is only the first candidate. If it is already
                    // sitting here the basis is still singular with it in place, and
                    // any logical not currently basic will do instead.
                    let preferred = lp.n_structural + row;
                    let replacement = if self.basic[row] == preferred {
                        match (0..lp.m)
                            .map(|r| lp.n_structural + r)
                            .find(|&l| !matches!(self.status[l], Status::Basic { .. }))
                        {
                            Some(l) => l,
                            // Every logical is basic, so the basis is `-I` and cannot
                            // be singular. Reaching this is a numerical failure, and
                            // reporting it as infeasibility would be a proof the solver
                            // has not got: on MIPLIB's neos-619167 that turned a
                            // feasible model into a confident "Infeasible".
                            None => return Err(LpStatus::IterationLimit),
                        }
                    } else {
                        preferred
                    };
                    let displaced = self.basic[row];
                    // A free column has neither bound to park against and belongs at
                    // zero. Sending it to `At::Upper` gives it the value infinity,
                    // which then propagates through every solve that follows: on
                    // MIPLIB's neos-619167, whose 1560 free columns make a singular
                    // basis likely, that surfaced as a confident proof that a feasible
                    // model was infeasible.
                    let at = if lp.lower[displaced].is_finite() {
                        At::Lower
                    } else if lp.upper[displaced].is_finite() {
                        At::Upper
                    } else {
                        At::Zero
                    };
                    self.status[displaced] = Status::NonBasic(at);
                    self.z[displaced] = lp.value_at(displaced, at);
                    self.basic[row] = replacement;
                    self.status[replacement] = Status::Basic { row };
                }
            }
        }
        // Every repair attempt used up. Each one swaps a logical into a singular
        // position, so `m + 1` of them should always be enough, and running out means
        // the arithmetic is defeating the repair rather than that the model has no
        // feasible point. Saying `Infeasible` here is the exact mistake this function's
        // own comment describes and the other two bail-outs above avoid: on
        // `supportcase4` it exhausted 9493 attempts and returned **infeasible for a
        // relaxation whose optimum is 0**, which the primal method reaches in 146485
        // iterations when it is not handed this basis.
        Err(LpStatus::IterationLimit)
    }

    /// Current value of the internal minimization objective.
    fn objective(&self) -> f64 {
        (0..self.lp.n_total())
            .map(|j| self.lp.cost[j] * self.z[j])
            .sum()
    }

    /// Is every nonbasic reduced cost on the correct side of zero?
    ///
    /// A basis inherited from a parent node is dual feasible, because changing a
    /// column's *bounds* leaves reduced costs untouched. This checks that rather
    /// than assuming it, so a caller passing an unrelated basis falls back to the
    /// primal method instead of getting a wrong answer from the dual one.
    fn is_dual_feasible(&mut self) -> bool {
        let tol = self.lp.tol.dual_feasibility;
        self.load_basic_costs(false);
        let mut y = std::mem::take(&mut self.y);
        self.basis.btran(&self.cost_b, &mut y);
        self.y = y;

        for j in 0..self.lp.n_total() {
            let Status::NonBasic(at) = self.status[j] else {
                continue;
            };
            // A fixed variable cannot move, so its reduced cost is unconstrained.
            if self.lp.lower[j] == self.lp.upper[j] {
                continue;
            }
            let d = self.reduced_cost(j, false);
            let ok = match at {
                At::Lower => d >= -tol,
                At::Upper => d <= tol,
                At::Zero => d.abs() <= tol,
            };
            if !ok {
                return false;
            }
        }
        true
    }

    /// The most primal-infeasible basic variable, with the bound it violates.
    ///
    /// Under `bland` it returns the *first* infeasible row by basic variable index
    /// instead of the worst, which is the leaving half of Bland's rule.
    fn most_infeasible_row(&self, bland: bool) -> Option<(usize, At)> {
        let tol = self.lp.tol.primal_feasibility;
        let mut worst = tol;
        let mut found: Option<(usize, At)> = None;
        for i in 0..self.lp.m {
            let j = self.basic[i];
            let v = self.z[j];
            let (below, above) = (self.lp.lower[j] - v, v - self.lp.upper[j]);
            let violation = below.max(above);
            if violation <= tol {
                continue;
            }
            let side = if below > above { At::Lower } else { At::Upper };
            if bland {
                // Lowest basic variable index wins, regardless of how bad it is.
                if found.is_none_or(|(prev, _)| j < self.basic[prev]) {
                    found = Some((i, side));
                }
            } else if !self.steepest_rows {
                if violation > worst {
                    worst = violation;
                    found = Some((i, side));
                }
            } else {
                // Steepest edge, not the largest violation. The violation says how far
                // this row is from feasible in whatever units the row happens to be
                // written in; dividing by the row's norm in the basis inverse says how
                // far the solution actually moves to repair it, which is what decides
                // whether the pivot is worth taking. The two differ by however badly the
                // rows are scaled against one another, which on a real model is a lot.
                //
                // Whichever row scores highest, but *some* row every time. A violating
                // row must never be passed over, because returning nothing from here is
                // read as primal feasibility and ends the solve; scores can legitimately
                // underflow to zero when a weight is enormous, and a rule accepting only
                // a strict improvement over zero would then skip every violating row.
                let score = violation * violation / self.row_weights[i];
                if found.is_none() || score > worst {
                    worst = score;
                    found = Some((i, side));
                }
            }
        }
        found
    }

    /// Keep a steepest edge weight inside the range where it still means something.
    ///
    /// A weight is a squared norm, so it is positive by construction, but the update is
    /// only exact in exact arithmetic: it can drift negative, and `ratio^2 * reference`
    /// can overflow outright when a pivot is small. Either ruins the score it feeds, and
    /// an infinite weight is the worse of the two, because it drives the score to zero
    /// rather than to nonsense, which reads as a row not worth choosing.
    fn clamp_weight(weight: f64) -> f64 {
        if weight.is_finite() {
            weight.clamp(1e-4, 1e12)
        } else {
            1.0
        }
    }

    /// Whether devex is pricing this solve.
    ///
    /// Not a constant: see [`Solver::consider_devex`]. Reading a field rather than the
    /// option is what lets the rule change mid-solve.
    fn devex(&self) -> bool {
        self.devex_active
    }

    /// Switch pricing to devex once the solve has gone on long enough to prove it needs it.
    ///
    /// Devex costs a BTRAN per iteration and is not worth it on a model that converges
    /// anyway; measured over the whole binary set, running it always **lost two
    /// instances** -- see [`Tolerances::primal_devex`]. Running it never loses a different
    /// one: `supportcase4`'s relaxation takes 7891 iterations under devex and **146485**
    /// under Dantzig, 24 times the wall clock, and the search reaches a single node
    /// because of it. A reference solver needs 953.
    ///
    /// So it is neither, and the model decides. A solve that has not finished in
    /// [`DEVEX_AFTER`] iterations is one Dantzig is not finishing, and the per-iteration
    /// cost of devex is cheap against not converging. The weights start from a fresh
    /// reference framework at the switch, which is the same state a devex solve begins in.
    fn consider_devex(&mut self, iterations: usize) {
        if !self.devex_switchable
            || self.devex_active
            || self.lp.tol.primal_devex
            || iterations < DEVEX_AFTER
        {
            return;
        }
        self.devex_active = true;
        self.col_weights.fill(1.0);
    }

    /// Carry the devex reference weights across a pivot.
    ///
    /// The weight of column `j` approximates the squared norm of the step direction
    /// entering `j` would take, measured against the reference framework the weights
    /// were last reset to. With the entering column `q`, the leaving row `r`, the pivot
    /// row `alpha_r = e_r' B^-1 N` and the pivot `alpha_rq`, the update is
    ///
    /// ```text
    ///     w_j <- max(w_j, (alpha_rj / alpha_rq)^2 w_q)      for every nonbasic j
    ///     w_leaving <- max(w_q / alpha_rq^2, 1)
    /// ```
    ///
    /// It costs one BTRAN and a pass over the columns to form the pivot row, which is
    /// the same order as pricing itself, so devex roughly doubles the per-iteration work
    /// outside the solves. That is the price of the rule.
    ///
    /// Weights only ever rise, so they drift away from the norms they approximate as the
    /// basis leaves the reference framework behind. When the largest passes
    /// `DEVEX_RESET` the framework is restarted at the current basis and every weight
    /// goes back to one, which is what keeps the approximation honest without ever
    /// computing an exact norm.
    ///
    /// Must be called before [`Basis::update`], because `rho` is a row of the *old*
    /// `B^-1`.
    fn update_column_weights(&mut self, entering: usize, r: usize) {
        let pivot = self.alpha[r];
        if pivot.abs() <= self.lp.tol.pivot {
            return;
        }
        let mut rho = std::mem::take(&mut self.rho);
        self.basis.btran_unit(r, &mut rho);
        let reference = Self::clamp_weight(self.col_weights[entering]).max(1.0);
        let leaving = self.basic[r];

        let mut largest = 0.0f64;
        for j in 0..self.lp.n_total() {
            if j == entering || !matches!(self.status[j], Status::NonBasic(_)) {
                continue;
            }
            let arj = if j < self.lp.n_structural {
                let (rows, vals) = self.lp.matrix.column(j);
                rows.iter()
                    .zip(vals)
                    .map(|(&i, &a)| rho[i] * a)
                    .sum::<f64>()
            } else {
                -rho[j - self.lp.n_structural]
            };
            if arj != 0.0 {
                let ratio = arj / pivot;
                let candidate = ratio * ratio * reference;
                if candidate > self.col_weights[j] {
                    self.col_weights[j] = candidate;
                }
            }
            largest = largest.max(self.col_weights[j]);
        }

        // The leaving variable is about to become nonbasic, and its direction is the
        // entering one's scaled by the pivot.
        self.col_weights[leaving] = Self::clamp_weight(reference / (pivot * pivot)).max(1.0);
        self.rho = rho;

        if largest > DEVEX_RESET {
            self.col_weights.fill(1.0);
        }
    }

    /// Carry the steepest edge weights across a pivot.
    ///
    /// The weight of row `i` approximates the squared norm of row `i` of `B^-1`, which
    /// is what turns a bound violation into the distance the solution must travel to
    /// remove it. With the entering column `alpha = B^-1 A_q`, the pivot row
    /// `rho = B^-T e_r` and `tau = B^-1 rho`, the exact update is
    ///
    /// ```text
    ///     w_r <- w_r / alpha_r^2
    ///     w_i <- w_i - 2 (alpha_i / alpha_r) tau_i + (alpha_i / alpha_r)^2 w_r
    /// ```
    ///
    /// which costs one extra FTRAN per iteration. That is the price of the rule, and
    /// the reason it is paid only here, where the alternative is choosing a row by a
    /// number with no fixed meaning across rows.
    fn update_row_weights(&mut self, r: usize, rho: &[f64]) {
        let pivot = self.alpha[r];
        if pivot.abs() <= self.lp.tol.pivot {
            return;
        }
        let mut tau = std::mem::take(&mut self.tau);
        self.basis.ftran(rho, &mut tau);

        // The chosen row's weight is available exactly rather than carried forward:
        // `rho` is `B^-T e_r`, so the squared norm this weight holds is `rho . rho`.
        // Taking it from there costs nothing, since `rho` is already in hand, and stops
        // the update compounding its own error, which is what a recurrence like this
        // does if left to itself: carried forward, `drayage-100-23` took 487230
        // iterations where the exact value takes 2374.
        let reference = Self::clamp_weight(rho.iter().map(|v| v * v).sum());
        for (i, &tau_i) in tau.iter().enumerate().take(self.lp.m) {
            if i == r {
                continue;
            }
            let ratio = self.alpha[i] / pivot;
            if ratio == 0.0 {
                continue;
            }
            let updated = self.row_weights[i] - 2.0 * ratio * tau_i + ratio * ratio * reference;
            self.row_weights[i] = Self::clamp_weight(updated);
        }
        self.row_weights[r] = Self::clamp_weight(reference / (pivot * pivot));
        self.tau = tau;
    }

    /// The dual simplex: repair primal infeasibility while holding dual feasibility.
    ///
    /// Each iteration takes the most primal-infeasible basic variable out of the
    /// basis at the bound it violates, and chooses the entering column by the dual
    /// ratio test, the smallest `|d_j / alpha_rj|` among columns that can move in
    /// the required direction. No such column means no assignment can repair the
    /// violation, which is exactly primal infeasibility.
    ///
    /// The objective climbs monotonically here, which is what makes `cutoff` sound:
    /// once it passes, the node's true bound can only be worse.
    fn run_dual(
        &mut self,
        max_iterations: usize,
        cutoff: Option<f64>,
        iterations: &mut usize,
    ) -> Option<LpStatus> {
        let tol = self.lp.tol.dual_feasibility;
        let pivot_tol = self.lp.tol.pivot;
        let mut rho: Vec<f64> = Vec::new();
        // Degenerate steps in a row before switching to Bland's rule. The primal
        // method has had this since it first cycled; the dual method had nothing,
        // and since every warm start enters through the dual method, that is nearly
        // every node in the search. MIPLIB's pk1, 86 columns, burned 100,000
        // iterations across four nodes before the caller gave up.
        const STALL_LIMIT: usize = 100;

        // Bland's rule is a way out of a cycle, not a way to run a solve. Staying in it
        // costs enormously on a merely degenerate model: on MIPLIB's neos-555001, 98%
        // of 116953 pivots ran under Bland where the reference solver needs 1396
        // pivots in total.
        //
        // So it is left after a short spell and re-entered if the stall resumes, each
        // spell twice the length of the last. A model that is degenerate but not
        // cycling pays only the short spells; one that really is cycling escalates
        // until the spell outlasts the cycle, which is what preserves termination. A
        // fixed short spell does not: at twenty pivots, signed_c48_r48_s3 cycled until
        // the iteration limit.
        const BLAND_RUN: usize = 20;
        let mut stalled = 0usize;
        let mut degenerate = 0usize;
        let mut bland_run = BLAND_RUN;

        while *iterations < max_iterations {
            // Polled rather than checked every pivot: reading the clock is not free,
            // and this granularity bounds the overrun to a few hundred pivots.
            if iterations.is_multiple_of(CLOCK_INTERVAL) && self.lp.out_of_time() {
                return Some(LpStatus::IterationLimit);
            }
            let bland = stalled > STALL_LIMIT;
            if stalled > STALL_LIMIT + bland_run {
                stalled = 0;
                bland_run *= 2;
            }
            if cutoff.is_some_and(|limit| self.objective() > limit) {
                return Some(LpStatus::CutOff);
            }

            // Refresh the duals every iteration. Each pivot changes the basis and so
            // changes every reduced cost; pricing the ratio test against duals left
            // over from a previous iteration silently picks the wrong entering column
            // and the solve converges to a point that is not the optimum.
            self.load_basic_costs(false);
            let mut y = std::mem::take(&mut self.y);
            self.basis.btran(&self.cost_b, &mut y);
            self.y = y;

            let Some((r, violated)) = self.most_infeasible_row(bland) else {
                // Primal feasible. For a warm start that is the optimum, because the
                // basis came in dual feasible and the ratio test holds it there.
                //
                // A cold entry crosses the whole model to get here, and the invariant
                // the ratio test preserves in exact arithmetic drifts out in this one.
                // Checking rather than assuming is what turned two wrong answers into
                // right ones on the branch this came from, `gasprod1-2` and `s55`,
                // where a non-optimal basis was reported as the optimum. Primal
                // feasible but not dual feasible is exactly the state the primal method
                // is for, so hand it the basis rather than ruling here.
                if !self.dual_steepest || self.is_dual_feasible() {
                    return Some(LpStatus::Optimal);
                }
                return None;
            };

            self.basis.btran_unit(r, &mut rho);
            // The leaving variable must rise to its lower bound, or fall to its upper.
            let rising = violated == At::Lower;

            let mut best: Option<(usize, f64)> = None;
            let mut best_ratio = f64::INFINITY;
            let mut best_pivot = 0.0;
            let mut rejected_small = false;
            for j in 0..self.lp.n_total() {
                let Status::NonBasic(at) = self.status[j] else {
                    continue;
                };
                if self.lp.lower[j] == self.lp.upper[j] {
                    continue;
                }
                let arj = if j < self.lp.n_structural {
                    let (rows, vals) = self.lp.matrix.column(j);
                    rows.iter()
                        .zip(vals)
                        .map(|(&i, &a)| rho[i] * a)
                        .sum::<f64>()
                } else {
                    -rho[j - self.lp.n_structural]
                };
                // Moving z_j by t moves the leaving variable by -arj * t. A column at
                // its lower bound can only increase, one at its upper only decrease;
                // keep those that push the leaving variable the way it needs to go.
                let usable = match at {
                    At::Lower => (arj < 0.0) == rising,
                    At::Upper => (arj > 0.0) == rising,
                    At::Zero => true,
                };
                if !usable {
                    continue;
                }
                if arj.abs() <= pivot_tol {
                    // It would serve; it is too small to pivot on safely. That is a
                    // statement about this factorization, not about the model, and the
                    // difference matters below: an empty candidate list is read as the
                    // dual being unbounded.
                    rejected_small = true;
                    continue;
                }
                let d = self.reduced_cost(j, false);
                let ratio = (d.abs() / arj.abs()).max(0.0);
                let pivot = arj.abs();
                // Ties go to the largest pivot, which is numerically safest. Under
                // Bland's they go to the lowest index instead: the rule only
                // guarantees termination when it governs both choices, which is the
                // same mistake that made strong branching cycle earlier.
                let wins_tie = if bland {
                    best.is_none_or(|(prev, _)| j < prev)
                } else {
                    pivot > best_pivot
                };
                if ratio < best_ratio - tol || (ratio <= best_ratio + tol && wins_tie) {
                    if ratio < best_ratio {
                        best_ratio = ratio;
                    }
                    best = Some((j, arj));
                    best_pivot = pivot;
                }
            }

            let Some((entering, _)) = best else {
                // The dual is unbounded, so the primal has no feasible point. That
                // reasoning rests on two things, and an infeasibility claim is the most
                // expensive thing in here to get wrong, so both are checked.
                //
                // The basis has to be dual feasible, which a cold entry verifies.
                if self.dual_steepest && !self.is_dual_feasible() {
                    return None;
                }
                // And the candidate list has to be empty because the model has no
                // column to offer, not because this factorization cannot pivot on the
                // ones it has. `supportcase4` reaches a row where every usable column
                // prices under the pivot tolerance, and calling that dual unboundedness
                // returned **infeasible for a relaxation whose optimum is 0**. Handing
                // the basis to the primal loop instead is what the dual method already
                // does wherever its own invariants have lapsed.
                if rejected_small {
                    return None;
                }
                // The row the dual could not repair is the certificate: aggregating the
                // model by `B^-T e_r` yields a form no point of this box satisfies. Kept
                // so a caller can *check* that and, if it holds, say which bounds are to
                // blame. It is handed over unverified on purpose; see `LpSolution::farkas`.
                self.farkas = Some(rho.clone());
                return Some(LpStatus::Infeasible);
            };

            // Pivot the chosen column into row r.
            let mut col = std::mem::take(&mut self.col);
            self.lp.column_into(entering, &mut col);
            let mut alpha = std::mem::take(&mut self.alpha);
            self.basis.ftran(&col, &mut alpha);
            self.col = col;
            self.alpha = alpha;
            self.count_entering_density();

            if self.alpha[r].abs() <= pivot_tol {
                // The pivot row and column disagree about this element's magnitude,
                // which means the inverse has drifted. Rebuild and try again.
                if let Err(status) = self.refactorize() {
                    debug_assert_ne!(status, LpStatus::Infeasible, "refactorize claimed infeasibility");
                    return Some(status);
                }
                *iterations += 1;
                continue;
            }

            let leaving = self.basic[r];
            let target = self.lp.value_at(leaving, violated);
            // Move the entering variable exactly far enough to place the leaving one
            // on the bound it was violating.
            let step = (target - self.z[leaving]) / -self.alpha[r];
            // Counted by primal step length, which is the wrong signal in principle:
            // degeneracy here is a zero *ratio*, meaning the entering column was
            // already priced at zero and the dual objective cannot move whatever the
            // step is. Counting ratios instead was tried and reverted. It is the right
            // signal and the wrong trade: zero ratios are ordinary in the dual method,
            // so it hands nearly every node to Bland's rule, and Bland is slow. On
            // MIPLIB's drayage-25-23 the search went from 98 nodes in a minute to 4.
            //
            // The pathology the ratio signal catches, a dual objective frozen while the
            // step length keeps the counter at zero, was only ever reached through the
            // cold-start dual entry on the `dual-cold-start` branch, where it is kept.
            // Warm starts, which is all this method does here, have not shown it.
            if self.dual_steepest {
                // A cold entry counts the ratio and escalates to Bland's rule. Handing
                // over, as a warm start does below, hands this basis to the primal loop
                // it was reached in place of, which is stuck: `tanglegram6` finishes in
                // 207 pivots when the dual method is allowed its own way out of a
                // degenerate patch and does not finish at all when it gives up here.
                // The cost of Bland's rule is affordable because this runs once.
                stalled = if best_ratio <= tol { stalled + 1 } else { 0 };
            } else {
                stalled = if step.abs() <= 1e-12 { stalled + 1 } else { 0 };
                // The counter above reads primal step length. Degeneracy here is a zero
                // *ratio*: the entering column was already priced at zero, so the dual
                // objective cannot move whatever the step turns out to be, and no count
                // of steps can see it.
                //
                // Counting ratios and escalating to Bland's rule was tried and reverted
                // for warm starts, because zero ratios are ordinary rather than
                // exceptional there and Bland is slow: drayage-25-23 fell from 98 nodes
                // in a minute to 4. Handing the node over is the cheaper answer to the
                // same signal. The primal loop continues from this basis and brings its
                // own way out of a cycle, so nothing is given up by leaving here.
                degenerate = if best_ratio <= tol { degenerate + 1 } else { 0 };
                if degenerate > STALL_LIMIT {
                    return None;
                }
            }
            self.z[entering] += step;
            for i in 0..self.lp.m {
                let bj = self.basic[i];
                self.z[bj] -= self.alpha[i] * step;
            }
            self.z[leaving] = target;

            // Before the basis moves: the update reads `alpha` and the pivot row
            // against the factorization they were computed from.
            if self.steepest_rows {
                self.update_row_weights(r, &rho);
            }

            self.basis.update(&self.alpha, r);
            self.basic[r] = entering;
            self.status[entering] = Status::Basic { row: r };
            self.status[leaving] = Status::NonBasic(violated);

            *iterations += 1;
            if (self.basis.updates() >= self.lp.tol.refactor_interval
                || self.basis.eta_file_is_expensive(ETA_GROWTH_LIMIT))
                && let Err(status) = self.refactorize()
            {
                return Some(status);
            }
        }
        None
    }

    /// See [`Lp::gomory_cuts`].
    fn gomory_cuts(&mut self, max_cuts: usize) -> Vec<(Vec<(usize, f64)>, f64)> {
        // A fractionality this close to an integer makes the cut's coefficients blow
        // up: they carry 1/f0 and 1/(1 - f0) factors.
        const MIN_FRACTIONALITY: f64 = 0.01;
        // Cuts whose coefficients span more than this are numerically worthless and
        // actively harmful to the LPs that must then carry them.
        const MAX_DYNAMISM: f64 = 1e6;
        // How dense a cut may be, relative to the rows of the model it came from.
        //
        // GMI cuts come out denser than the rows they derive from, and a row much
        // denser than the model destroys the sparsity the LU depends on: on MIPLIB's
        // pk1, whose rows average 23% of the columns, cuts reaching 81% took the
        // solve from 171k nodes to 24.
        //
        // Relative, not absolute. An absolute cap was tried and silently rejected
        // *every* GMI cut on the models this solver targets, which run about 99.5%
        // dense, a dense model's cuts are supposed to be dense, and the bound they
        // were worth went with them. The comparison that matters is against the
        // model's own rows.
        //
        // Relative and *generous*. At three times the average row this rejected almost
        // every cut worth having, and did it silently: on `neos-1516309`, rows of 62
        // columns in a model of 4500 allowed 186 terms and threw away 77 of the 80 GMI
        // cuts the root produced, where the three survivors were worth 1300 of bound
        // between them and the rest another 640. On `n2seq36f` it rejected every cut
        // the root produced, the largest missing by eight terms out of 647, and the
        // search then spent a minute against a bound that four thousand later cuts
        // could not move either.
        //
        // Bounded above as well, because the two ends catch different models and
        // neither bound alone survives both. Relative alone, at any factor loose enough
        // to help, lets the allowance pass the column count on a model of 39 rows and
        // 20315 columns, and `irp` stops closing at all. A share of the columns alone
        // allows a cut seven hundred times denser than anything in a model whose rows
        // average five columns of fourteen thousand, and `decomp2` goes from closing in
        // 25 seconds to closing once in three attempts. Twenty-four times the average
        // row, capped at a third of the columns, closes `neos-1516309` and
        // `neos-1599274` and keeps both of those.
        const DENSITY_FACTOR: usize = 24;
        const DENSITY_SHARE: usize = 3;
        const MIN_SUPPORT: usize = 30;
        const TINY: f64 = 1e-11;

        let lp = self.lp;
        let mut rho: Vec<f64> = Vec::new();
        let mut cuts = Vec::new();
        // Row-major view of A, built once. Substituting the logicals out needs the
        // rows of A, and the matrix is stored by column.
        let rows_of_a = lp.matrix.to_csr();

        for i in 0..lp.m {
            if cuts.len() >= max_cuts {
                break;
            }
            // Generating cuts on a large model is itself a long operation.
            if i.is_multiple_of(CLOCK_INTERVAL) && lp.out_of_time() {
                break;
            }
            // Only an integer column carries a requirement to exploit.
            let basic = self.basic[i];
            if basic >= lp.n_structural || !lp.integer[basic] {
                continue;
            }
            let beta = self.z[basic];
            let f0 = beta - beta.floor();
            if !(MIN_FRACTIONALITY..=1.0 - MIN_FRACTIONALITY).contains(&f0) {
                continue;
            }

            self.basis.btran_unit(i, &mut rho);

            // Accumulate the cut over z = [x; s], then fold the logicals into x.
            let mut z_coeff = vec![0.0f64; lp.n_total()];
            let mut rhs = 1.0f64;
            let mut usable = true;

            for j in 0..lp.n_total() {
                let Status::NonBasic(at) = self.status[j] else {
                    continue;
                };
                if lp.lower[j] == lp.upper[j] {
                    // Fixed: contributes a constant, and w_j is identically zero.
                    continue;
                }
                let alpha = if j < lp.n_structural {
                    let (rows, vals) = lp.matrix.column(j);
                    rows.iter()
                        .zip(vals)
                        .map(|(&r, &v)| rho[r] * v)
                        .sum::<f64>()
                } else {
                    -rho[j - lp.n_structural]
                };
                // At an upper bound the shift w_j = u_j - z_j reverses the sign.
                let a = match at {
                    At::Upper => -alpha,
                    _ => alpha,
                };
                if a.abs() <= TINY {
                    continue;
                }

                // The two coefficient formulas differ, and picking by position
                // rather than by declared integrality would silently produce an
                // invalid cut on any model with continuous columns.
                let c = if j < lp.n_structural && lp.integer[j] {
                    // Integer column: the piecewise-linear integer coefficient.
                    let f = a - a.floor();
                    if f <= f0 {
                        f / f0
                    } else {
                        (1.0 - f) / (1.0 - f0)
                    }
                } else {
                    // Continuous column.
                    if a >= 0.0 { a / f0 } else { -a / (1.0 - f0) }
                };
                if c.abs() <= TINY {
                    continue;
                }

                // Undo the shift: w_j = z_j - l_j, or u_j - z_j at an upper bound.
                match at {
                    At::Upper => {
                        let u = lp.upper[j];
                        if !u.is_finite() {
                            usable = false;
                            break;
                        }
                        z_coeff[j] -= c;
                        rhs -= c * u;
                    }
                    _ => {
                        let l = lp.lower[j];
                        if !l.is_finite() {
                            usable = false;
                            break;
                        }
                        z_coeff[j] += c;
                        rhs += c * l;
                    }
                }
            }
            if !usable {
                continue;
            }

            // Eliminate the logicals with s_k = sum_j A[k][j] x_j, walking row k of A
            // directly. Searching every column for row k instead turns this into
            // O(columns * nonzeros-per-column) per logical, which on a dense 256x256
            // model was most of the solve time.
            let mut x_coeff = z_coeff[..lp.n_structural].to_vec();
            for k in 0..lp.m {
                let h = z_coeff[lp.n_structural + k];
                if h == 0.0 {
                    continue;
                }
                let (columns, values) = rows_of_a.column(k);
                for (&j, &v) in columns.iter().zip(values) {
                    x_coeff[j] += h * v;
                }
            }

            let coefficients: Vec<(usize, f64)> = x_coeff
                .iter()
                .enumerate()
                .filter(|&(_, &v)| v.abs() > 1e-9)
                .map(|(j, &v)| (j, v))
                .collect();
            if coefficients.is_empty() {
                continue;
            }
            let largest = coefficients
                .iter()
                .map(|&(_, v)| v.abs())
                .fold(0.0f64, f64::max);
            let smallest = coefficients
                .iter()
                .map(|&(_, v)| v.abs())
                .fold(f64::INFINITY, f64::min);
            let average_row = lp.matrix.nnz() / lp.m.max(1);
            let allowed =
                MIN_SUPPORT.max((DENSITY_FACTOR * average_row).min(lp.n_structural / DENSITY_SHARE));
            if largest / smallest > MAX_DYNAMISM || !rhs.is_finite() || coefficients.len() > allowed
            {
                continue;
            }

            cuts.push((coefficients, rhs));
        }
        cuts
    }

    /// Record how sparse the entering column's solve came out; see
    /// [`Tolerances::entering_density_stats`], which gates it.
    fn count_entering_density(&mut self) {
        if !self.lp.tol.entering_density_stats {
            return;
        }
        let nonzeros = self.alpha.iter().filter(|v| **v != 0.0).count();
        self.entering_solves += 1;
        self.entering_nonzeros += nonzeros as u64;
        if nonzeros as f64 <= HYPERSPARSE_SHARE * self.alpha.len() as f64 {
            self.entering_sparse_solves += 1;
        }
    }

    /// Finish, handing the final basis and its factorization back for caching.
    fn done(self, status: LpStatus, iterations: usize) -> (LpSolution, Vec<usize>, Basis) {
        let solution = self.finish(status, iterations);
        (solution, self.basic, self.basis)
    }

    fn finish(&self, status: LpStatus, iterations: usize) -> LpSolution {
        let x = self.z[..self.lp.n_structural].to_vec();
        let objective = if status == LpStatus::Optimal {
            self.objective()
        } else {
            f64::NAN
        };
        LpSolution {
            status,
            objective,
            x,
            iterations,
            bland_iterations: self.bland_iterations,
            degenerate_iterations: self.degenerate_iterations,
            farkas: self.farkas.clone(),
            entering_solves: self.entering_solves,
            entering_nonzeros: self.entering_nonzeros,
            entering_sparse_solves: self.entering_sparse_solves,
            basis: BasisState {
                basic: self.basic.clone(),
                status: self.status.clone(),
            },
        }
    }

    fn run(
        mut self,
        max_iterations: usize,
        cutoff: Option<f64>,
    ) -> (LpSolution, Vec<usize>, Basis) {
        // Degenerate (zero-length) steps in a row before switching to Bland's rule.
        /// Degenerate steps in a row before the *primal* method switches to Bland's rule.
        ///
        /// Twenty times the dual method's, and the difference is the point. A run of degenerate
        /// steps is not a cycle: `cvs16r70-62`'s relaxation is 94% zero-step pivots *while
        /// converging normally*, and 100 in a row is an ordinary event on a model like that.
        /// Treating it as a cycle sent the solve into Bland's rule and left it there, which is a
        /// way out of a cycle and not a way to run a solve.
        ///
        /// What that cost, measured over 200,000 iterations of the relaxation:
        ///
        /// ```text
        ///                   at 100                          at 2000
        ///     cod105        95% Bland, no convergence       6337 iterations, optimal, 7.4s
        ///     cvs16r70-62   93% Bland, no convergence       8942 iterations, optimal, 3.2s
        ///     cvs16r106-72  93% Bland, no convergence      22835 iterations, optimal, 11.5s
        /// ```
        ///
        /// Nothing that converged at 100 stops converging at 2000, and the termination guarantee
        /// is untouched: a genuine cycle produces an unbounded run of degenerate steps, so it
        /// still reaches Bland's rule, just after a longer and bounded delay.
        ///
        /// The dual method keeps 100 and was not measured here. It is entered warm at nearly
        /// every node in the search, where it repairs one bound in a few pivots and a long
        /// degenerate run means something different.
        const PRIMAL_STALL_LIMIT: usize = 2_000;
        // Bland's rule is a way out of a cycle, not a way to run a solve. Staying in it
        // costs enormously on a merely degenerate model: on MIPLIB's neos-555001, 98%
        // of 116953 pivots ran under Bland where the reference solver needs 1396
        // pivots in total.
        //
        // So it is left after a short spell and re-entered if the stall resumes, each
        // spell twice the length of the last. A model that is degenerate but not
        // cycling pays only the short spells; one that really is cycling escalates
        // until the spell outlasts the cycle, which is what preserves termination. A
        // fixed short spell does not: at twenty pivots, signed_c48_r48_s3 cycled until
        // the iteration limit.
        const BLAND_RUN: usize = 20;
        // A bound violation at or below this is read as the width of the arithmetic
        // rather than as evidence that the model has no feasible point.
        const INFEASIBILITY_PROOF: f64 = 1e-5;

        let mut iterations = 0usize;
        // How far a basic variable may sit outside its bounds before this solve calls
        // the basis infeasible. It starts at the model's tolerance and only ever rises,
        // never past `INFEASIBILITY_PROOF`, so the loop below cannot run forever.
        let mut slack = self.lp.tol.primal_feasibility;
        let mut stalled = 0usize;
        let mut bland_run = BLAND_RUN;

        if !self.factorized
            && let Err(status) = self.refactorize()
        {
            return self.done(status, 0);
        }

        // A warm start inherits dual feasibility from its parent, so the dual method
        // repairs the one bound the branch just changed in a few pivots. When that
        // does not hold (a cold start, or a basis from an unrelated problem) fall
        // through to the primal method, which needs no assumptions.
        let entry = if self.entry_hint == Entry::Dual && self.is_dual_feasible() {
            Entry::Dual
        } else {
            Entry::Primal
        };
        if entry == Entry::Dual {
            // The dual method is the right one for a warm start and usually repairs the
            // single bound a branch changed in a few pivots, which is why it is tried
            // first. On the models where it is not, it was spending the node's whole
            // budget and the node was abandoned: MIPLIB's drayage-25-23 opened four
            // nodes in a minute, each of them a dual solve that ran to the iteration
            // cap, where the primal method opens thousands and resolves them.
            //
            // So it gets a share of the budget rather than all of it, and what it has
            // not finished is handed to the primal loop below, which continues from the
            // basis it reached. Answering the node slowly beats not answering it, and
            // the general case is untouched because the dual method almost always
            // finishes well inside its share.
            //
            // A cold entry is the exception: there is no parent's optimum to be near,
            // so the dual method is not repairing a bound, it is the solve. It gets the
            // whole budget.
            const DUAL_SHARE: usize = 2;
            let budget = if self.dual_steepest {
                max_iterations
            } else {
                max_iterations / DUAL_SHARE
            };
            if let Some(status) = self.run_dual(budget, cutoff, &mut iterations) {
                return self.done(status, iterations);
            }
        }

        while iterations < max_iterations {
            if iterations.is_multiple_of(CLOCK_INTERVAL) && self.lp.out_of_time() {
                return self.done(LpStatus::IterationLimit, iterations);
            }
            let phase_one = self.worst_violation() > slack;

            self.load_basic_costs(phase_one);
            let mut y = std::mem::take(&mut self.y);
            self.basis.btran(&self.cost_b, &mut y);
            self.y = y;

            // A solve this long is one Dantzig is not going to finish; see
            // `Solver::consider_devex`.
            self.consider_devex(iterations);

            let bland = stalled > PRIMAL_STALL_LIMIT;
            if stalled > PRIMAL_STALL_LIMIT + bland_run {
                stalled = 0;
                bland_run *= 2;
            }
            if bland {
                self.bland_iterations += 1;
            }
            let Some((entering, sigma)) = self.price(phase_one, bland) else {
                // No improving column. In phase 2 that is optimality; in phase 1 it
                // means the sum of bound violations sits at its minimum, and since
                // that minimum is global for an LP, a nonzero one proves the model has
                // no feasible point.
                //
                // Only if it really is nonzero. Maintaining feasibility and proving
                // its absence want different tolerances, and reading the proof at the
                // tolerance that decides phase entry leaves no room for the
                // arithmetic. On MIPLIB's gasprod1-2 that is what happened: phase 1
                // ran out of moves at a violation of 1.2e-7, one part in ten million
                // on a model whose objective is 5e4, and a feasible relaxation was
                // declared infeasible after 266067 iterations against a reference
                // solver's 4901.
                //
                // A violation that small is the width of the arithmetic rather than a
                // property of the model, so it is pulled out and the solve carries on
                // into phase 2. Repairs are bounded so that a model which really is
                // infeasible still terminates.
                if phase_one {
                    let worst = self.worst_violation();
                    if worst <= INFEASIBILITY_PROOF {
                        // Widening the tolerance rather than pulling the values back.
                        // Moving them would leave `z` disagreeing with the basis it was
                        // computed from, and the next refactorization would recover the
                        // same violation and send the solve back through phase 1 over
                        // it, which is the loop being escaped. Recording how noisy this
                        // model's arithmetic actually is settles the question once and
                        // survives refactorization.
                        slack = worst * 1.01;
                        continue;
                    }
                }
                let status = if phase_one {
                    LpStatus::Infeasible
                } else {
                    LpStatus::Optimal
                };
                if status == LpStatus::Infeasible {
                    // Phase one has finished with a violation it cannot remove, and its
                    // dual vector is the certificate of that: aggregating the model by it
                    // yields a form the box cannot satisfy. Handed over unverified, like
                    // the dual method's; see `LpSolution::farkas`.
                    self.farkas = Some(self.y.clone());
                }
                return self.done(status, iterations);
            };

            let mut col = std::mem::take(&mut self.col);
            self.lp.column_into(entering, &mut col);
            let mut alpha = std::mem::take(&mut self.alpha);
            self.basis.ftran(&col, &mut alpha);
            self.col = col;
            self.alpha = alpha;
            self.count_entering_density();

            match self.ratio_test(entering, sigma, phase_one, bland) {
                Step::Unbounded => {
                    // Phase 1's objective is bounded below by zero, so an unbounded ray
                    // there means the ratio test lost a breakpoint to rounding rather
                    // than that the problem is genuinely unbounded.
                    let status = if phase_one {
                        LpStatus::Infeasible
                    } else {
                        LpStatus::Unbounded
                    };
                    if status == LpStatus::Infeasible {
                        self.farkas = Some(self.y.clone());
                    }
                    return self.done(status, iterations);
                }
                Step::BoundFlip { step } => {
                    self.take_step(entering, sigma, step);
                    let at = if sigma > 0.0 { At::Upper } else { At::Lower };
                    self.status[entering] = Status::NonBasic(at);
                    self.z[entering] = self.lp.value_at(entering, at);
                    let degenerate = step.abs() <= 1e-12;
                    self.degenerate_iterations += usize::from(degenerate);
                    stalled = if degenerate { stalled + 1 } else { 0 };
                }
                Step::Pivot {
                    leaving_row,
                    step,
                    to,
                } => {
                    // Before the basis moves: the pivot row is a row of the old inverse.
                    if self.devex() {
                        self.update_column_weights(entering, leaving_row);
                    }
                    self.take_step(entering, sigma, step);
                    let leaving = self.basic[leaving_row];

                    self.basis.update(&self.alpha, leaving_row);
                    self.basic[leaving_row] = entering;
                    self.status[entering] = Status::Basic { row: leaving_row };
                    self.status[leaving] = Status::NonBasic(to);
                    self.z[leaving] = self.lp.value_at(leaving, to);

                    let degenerate = step.abs() <= 1e-12;
                    self.degenerate_iterations += usize::from(degenerate);
                    stalled = if degenerate { stalled + 1 } else { 0 };
                }
            }

            iterations += 1;
            if (self.basis.updates() >= self.lp.tol.refactor_interval
                || self.basis.eta_file_is_expensive(ETA_GROWTH_LIMIT))
                && let Err(status) = self.refactorize()
            {
                debug_assert_ne!(status, LpStatus::Infeasible, "refactorize claimed infeasibility");
                return self.done(status, iterations);
            }
        }

        self.done(LpStatus::IterationLimit, iterations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RowSense, Sense};

    use crate::sparse::SparseMatrix;

    /// Growing an LP in place must land exactly where building it with the same rows
    /// from the start does. This is the property the whole node-local cut path rests
    /// on, so it is checked against a from-scratch solve rather than against itself.
    #[test]
    fn rows_added_in_place_match_a_model_built_with_them() {
        use crate::cuts::Cut;

        let base = problem(
            &[-1.0, -2.0],
            &[
                (&[1.0, 1.0], RowSense::Le, 3.5),
                (&[2.0, 1.0], RowSense::Le, 5.0),
            ],
        );
        let mut lp = Lp::relaxation(&base);
        let first = lp.solve();
        assert_eq!(first.status, LpStatus::Optimal);

        // A row the relaxation optimum violates, so the growth has real work to do.
        let cut = Cut {
            coefficients: vec![(0, 1.0), (1, 1.0)],
            lb: f64::NEG_INFINITY,
            ub: 1.5,
        };
        assert!(cut.violation(&first.x) > 0.0);

        let mut grown = lp.clone();
        let mut basis = first.basis.clone();
        grown.add_rows(&[(cut.coefficients.clone(), cut.lb, cut.ub)]);
        basis.extend_for_rows(base.n_cols(), base.n_rows(), 1);
        let warm = grown.solve_warm(&basis, None, 10_000);

        let mut rebuilt_problem = base.clone();
        rebuilt_problem.add_cuts(std::slice::from_ref(&cut));
        let cold = Lp::relaxation(&rebuilt_problem).solve();

        assert_eq!(warm.status, LpStatus::Optimal);
        assert_eq!(cold.status, LpStatus::Optimal);
        assert!(
            (warm.objective - cold.objective).abs() < 1e-9,
            "grown {} vs rebuilt {}",
            warm.objective,
            cold.objective
        );
        // And the cut actually bit: the bound is worse than before it was added.
        assert!(warm.objective > first.objective + 1e-9);
        assert!(cut.violation(&warm.x) <= 1e-9);
    }

    /// `fit` is `extend_for_rows` with the count read off the basis, so a basis saved
    /// before two rounds of growth fits the LP after both.
    #[test]
    fn a_basis_from_before_rows_were_added_fits_the_grown_lp() {
        let base = problem(
            &[-1.0, -2.0],
            &[
                (&[1.0, 1.0], RowSense::Le, 3.5),
                (&[2.0, 1.0], RowSense::Le, 5.0),
            ],
        );
        let mut lp = Lp::relaxation(&base);
        let first = lp.solve();
        lp.add_rows(&[(vec![(0, 1.0), (1, 1.0)], f64::NEG_INFINITY, 2.5)]);
        lp.add_rows(&[(vec![(1, 1.0)], f64::NEG_INFINITY, 1.5)]);

        let mut basis = first.basis.clone();
        lp.fit(&mut basis);
        let warm = lp.solve_warm(&basis, None, 10_000);

        let mut rebuilt = base.clone();
        rebuilt.add_cuts(&[
            crate::cuts::Cut {
                coefficients: vec![(0, 1.0), (1, 1.0)],
                lb: f64::NEG_INFINITY,
                ub: 2.5,
            },
            crate::cuts::Cut {
                coefficients: vec![(1, 1.0)],
                lb: f64::NEG_INFINITY,
                ub: 1.5,
            },
        ]);
        let cold = Lp::relaxation(&rebuilt).solve();
        assert_eq!(warm.status, LpStatus::Optimal);
        assert!((warm.objective - cold.objective).abs() < 1e-9);
    }

    /// Taking out a slack row with its basic logical leaves the optimal basis of the
    /// smaller LP, which a warm start then confirms without a pivot.
    #[test]
    fn removing_slack_rows_leaves_an_optimal_basis() {
        use crate::cuts::Cut;

        let base = problem(
            &[-3.0, -2.0, -4.0],
            &[
                (&[1.0, 1.0, 1.0], RowSense::Le, 2.5),
                (&[2.0, 1.0, 0.0], RowSense::Le, 2.0),
            ],
        );
        // A binding cut between two slack ones, so the survivors are renumbered on
        // both sides of a gap.
        let cuts = [
            Cut {
                coefficients: vec![(0, 1.0)],
                lb: f64::NEG_INFINITY,
                ub: 10.0,
            },
            Cut {
                coefficients: vec![(2, 1.0)],
                lb: f64::NEG_INFINITY,
                ub: 1.0,
            },
            Cut {
                coefficients: vec![(1, 1.0), (2, 1.0)],
                lb: f64::NEG_INFINITY,
                ub: 50.0,
            },
        ];
        let mut with = base.clone();
        with.add_cuts(&cuts);
        let solved = Lp::relaxation(&with).solve();
        assert_eq!(solved.status, LpStatus::Optimal);
        assert!(!cuts[0].is_tight(&solved.x, 1e-9) && !cuts[2].is_tight(&solved.x, 1e-9));
        assert!(cuts[1].is_tight(&solved.x, 1e-9));

        let n = base.n_cols();
        let rows = base.n_rows();
        let basis = solved
            .basis
            .without_rows(n, &[rows, rows + 2])
            .expect("slack rows have basic logicals");

        let mut trimmed = base.clone();
        trimmed.add_cuts(std::slice::from_ref(&cuts[1]));
        let mut lp = Lp::relaxation(&trimmed);
        let warm = lp.solve_warm(&basis, None, 10_000);
        assert_eq!(warm.status, LpStatus::Optimal);
        assert_eq!(warm.iterations, 0);
        assert!((warm.objective - solved.objective).abs() < 1e-9);

        // A binding row's logical is nonbasic, and then there is no basis to give back.
        assert!(solved.basis.without_rows(n, &[rows + 1]).is_none());
    }

    /// Reusing the parent factorization must land exactly where refactorizing does.
    /// This is the property that makes `solve_with_rows` an optimization rather than a
    /// different algorithm, so it is checked against the path it replaces.
    #[test]
    fn reusing_the_factorization_matches_refactorizing_from_scratch() {
        use crate::cuts::Cut;

        let base = problem(
            &[-3.0, -2.0, -4.0],
            &[
                (&[1.0, 1.0, 1.0], RowSense::Le, 2.5),
                (&[2.0, 1.0, 0.0], RowSense::Le, 2.0),
                (&[0.0, 1.0, 3.0], RowSense::Le, 3.5),
            ],
        );
        let mut lp = Lp::relaxation(&base);
        let first = lp.solve();
        assert_eq!(first.status, LpStatus::Optimal);

        let cuts = [
            Cut {
                coefficients: vec![(0, 1.0), (1, 1.0), (2, 1.0)],
                lb: f64::NEG_INFINITY,
                ub: 1.5,
            },
            Cut {
                coefficients: vec![(1, 1.0), (2, 2.0)],
                lb: f64::NEG_INFINITY,
                ub: 1.25,
            },
        ];
        let rows: Vec<RangeRow> = cuts
            .iter()
            .map(|c| (c.coefficients.clone(), c.lb, c.ub))
            .collect();

        // The cache now holds the factorization the last solve ended on, which is what
        // the reuse path looks for.
        let reused = lp.solve_with_rows(&first.basis, &rows, None, 10_000);

        let mut rebuilt = base.clone();
        rebuilt.add_cuts(&cuts);
        let cold = Lp::relaxation(&rebuilt).solve();

        assert_eq!(reused.status, LpStatus::Optimal);
        assert_eq!(cold.status, LpStatus::Optimal);
        assert!(
            (reused.objective - cold.objective).abs() < 1e-9,
            "reused {} vs cold {}",
            reused.objective,
            cold.objective
        );
        assert!(
            reused.objective > first.objective + 1e-9,
            "the cuts must bite"
        );
    }

    #[test]
    fn adding_a_row_the_optimum_already_satisfies_leaves_the_bound_alone() {
        let base = problem(&[-1.0, -2.0], &[(&[1.0, 1.0], RowSense::Le, 3.0)]);
        let mut lp = Lp::relaxation(&base);
        let first = lp.solve();

        let mut basis = first.basis.clone();
        lp.add_rows(&[(vec![(0, 1.0), (1, 1.0)], f64::NEG_INFINITY, 10.0)]);
        basis.extend_for_rows(base.n_cols(), base.n_rows(), 1);
        let after = lp.solve_warm(&basis, None, 10_000);

        assert_eq!(after.status, LpStatus::Optimal);
        assert!((after.objective - first.objective).abs() < 1e-9);
    }

    /// Build a problem from dense rows, for readable test cases.
    pub(super) fn problem(obj: &[f64], rows: &[(&[f64], RowSense, f64)]) -> Problem {
        let n = obj.len();
        let m = rows.len();
        let mut triplets = Vec::new();
        let (mut row_lb, mut row_ub) = (Vec::new(), Vec::new());
        for (i, (coeffs, sense, rhs)) in rows.iter().enumerate() {
            for (j, &v) in coeffs.iter().enumerate() {
                if v != 0.0 {
                    triplets.push((i, j, v));
                }
            }
            let (lo, hi) = sense.bounds(*rhs);
            row_lb.push(lo);
            row_ub.push(hi);
        }
        Problem {
            name: "test".into(),
            sense: Sense::Minimize,
            obj: obj.to_vec(),
            obj_offset: 0.0,
            matrix: SparseMatrix::from_triplets(m, n, triplets),
            row_lb,
            row_ub,
            col_lb: vec![0.0; n],
            col_ub: vec![1.0; n],
            col_type: vec![crate::model::VarType::Integer; n],
            col_names: (0..n).map(|j| format!("x{j}")).collect(),
            row_names: (0..m).map(|i| format!("c{i}")).collect(),
        }
    }

    /// The ratio test must not pivot on an entry that is negligible against the rest
    /// of its column, even when that entry clears the absolute pivot tolerance and
    /// offers the shortest step. Doing so leaves the next basis all but singular, and
    /// on MIPLIB's neos-850681 it was the difference between converging on the true
    /// relaxation bound and declaring a feasible model infeasible.
    #[test]
    fn the_ratio_test_refuses_a_pivot_that_is_negligible_in_its_column() {
        let p = problem(
            &[-1.0, -1.0, -1.0],
            &[
                (&[1.0, 0.0, 0.0], RowSense::Le, 1.0),
                (&[0.0, 1.0, 0.0], RowSense::Le, 1.0),
                (&[0.0, 0.0, 1.0], RowSense::Le, 1.0),
            ],
        );
        let lp = Lp::relaxation(&p);
        let mut solver = Solver::new(&lp);
        solver.refactorize().expect("identity basis factorizes");
        // Every logical rises towards a shared upper bound of one.
        let rows: Vec<usize> = solver.basic.clone();
        assert!(rows.iter().all(|&j| lp.upper[j] == 1.0));

        // Row 0 offers the shortest step by far, but on an entry a hundred-millionth
        // the size of the column's largest, well above the absolute floor of 1e-9.
        // Rows 1 and 2 are sound, and row 1 is the shorter of them.
        solver.alpha = vec![-1e-8, -2.0, -1.0];
        for (&j, v) in rows.iter().zip([1.0 - 5e-9, -0.5, -2.0]) {
            solver.z[j] = v;
        }

        let Step::Pivot { leaving_row, .. } = solver.ratio_test(0, 1.0, false, false) else {
            panic!("a bounded column must produce a pivot");
        };
        assert_ne!(leaving_row, 0, "pivoted on an entry negligible in its column");
        assert_eq!(leaving_row, 1);

        // The same entry scaled up is no longer negligible and is chosen: the test is
        // relative to the column, not an absolute floor on the entry.
        solver.alpha[0] = -4.0;
        let Step::Pivot { leaving_row, .. } = solver.ratio_test(0, 1.0, false, false) else {
            panic!("a bounded column must produce a pivot");
        };
        assert_eq!(leaving_row, 0);
    }

    fn solve(obj: &[f64], rows: &[(&[f64], RowSense, f64)]) -> LpSolution {
        Lp::relaxation(&problem(obj, rows)).solve()
    }

    /// Every row's activity lies within its bounds, and every variable within its.
    fn assert_feasible(p: &Problem, x: &[f64]) {
        let csr = p.matrix.to_csr();
        for i in 0..p.n_rows() {
            let (cols, vals) = csr.column(i);
            let activity: f64 = cols.iter().zip(vals).map(|(&j, &a)| a * x[j]).sum();
            assert!(
                activity >= p.row_lb[i] - 1e-7 && activity <= p.row_ub[i] + 1e-7,
                "row {i}: activity {activity} outside [{}, {}]",
                p.row_lb[i],
                p.row_ub[i]
            );
        }
        for (j, &v) in x.iter().enumerate() {
            assert!(
                v >= p.col_lb[j] - 1e-9 && v <= p.col_ub[j] + 1e-9,
                "column {j}: {v} outside [{}, {}]",
                p.col_lb[j],
                p.col_ub[j]
            );
        }
    }

    #[test]
    fn ge_row_forces_a_fractional_value() {
        // Regression: phase 1 parked a variable rising towards its *lower* bound at
        // its upper bound instead, so `>=` rows were silently left violated and the
        // solver returned the infeasible origin as optimal.
        let s = solve(&[1.0], &[(&[1.0], RowSense::Ge, 0.5)]);
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective - 0.5).abs() < 1e-9, "{}", s.objective);
        assert!((s.x[0] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn picks_the_cheaper_column_to_cover_a_row() {
        let s = solve(&[2.0, 3.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective - 2.0).abs() < 1e-9, "{}", s.objective);
    }

    #[test]
    fn le_row_with_a_negative_cost_pushes_to_the_upper_bound() {
        let s = solve(&[-1.0], &[(&[1.0], RowSense::Le, 1.0)]);
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective + 1.0).abs() < 1e-9, "{}", s.objective);
        assert!((s.x[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn equality_row_is_satisfied_exactly() {
        let s = solve(&[1.0, 1.0], &[(&[1.0, 1.0], RowSense::Eq, 1.0)]);
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective - 1.0).abs() < 1e-9, "{}", s.objective);
    }

    #[test]
    fn relaxation_is_fractional_where_the_integer_answer_is_not() {
        // 2a + 3b >= 4 with a, b in [0,1]: the LP takes b = 1 and a = 0.5 for 6,
        // while the integer optimum is a = b = 1 for 11.
        let p = problem(&[10.0, 1.0], &[(&[2.0, 3.0], RowSense::Ge, 4.0)]);
        let s = Lp::relaxation(&p).solve();
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective - 6.0).abs() < 1e-9, "{}", s.objective);
        assert_feasible(&p, &s.x);
    }

    /// A model whose rows contradict each other by less than the arithmetic can
    /// resolve is feasible for every practical purpose, and calling it infeasible is a
    /// claim the solver has not got. A model that contradicts itself by a visible
    /// margin still has to be caught, so both sides of that line are pinned here.
    ///
    /// On MIPLIB's gasprod1-2 the unrescued version reported a feasible relaxation
    /// infeasible over a violation of 1.2e-7.
    #[test]
    fn a_contradiction_too_small_to_resolve_is_not_a_proof_of_infeasibility() {
        // `x >= 0.5 + gap/2` against `x <= 0.5 - gap/2`, so the rows miss each other
        // by exactly `gap`.
        let contradiction = |gap: f64| {
            solve(
                &[1.0],
                &[
                    (&[1.0], RowSense::Ge, 0.5 + gap / 2.0),
                    (&[1.0], RowSense::Le, 0.5 - gap / 2.0),
                ],
            )
            .status
        };

        // Wider than the tolerance that decides feasibility, narrower than the one that
        // decides a proof: rescued rather than reported.
        assert_eq!(contradiction(1e-6), LpStatus::Optimal);
        // A contradiction the model really does contain.
        assert_eq!(contradiction(1e-3), LpStatus::Infeasible);
        assert_eq!(contradiction(0.5), LpStatus::Infeasible);
    }

    #[test]
    fn detects_an_infeasible_pair_of_rows() {
        let s = solve(
            &[1.0],
            &[(&[1.0], RowSense::Ge, 1.0), (&[1.0], RowSense::Le, 0.0)],
        );
        assert_eq!(s.status, LpStatus::Infeasible);
    }

    #[test]
    fn respects_bounds_narrowed_by_branching() {
        // Fixing x0 to 1 is how branching will present a subproblem.
        let mut p = problem(&[1.0, 5.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        p.col_lb[0] = 1.0;
        let s = Lp::relaxation(&p).solve();
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective - 1.0).abs() < 1e-9, "{}", s.objective);

        // Fixing it to 0 instead forces the expensive column in.
        let mut p = problem(&[1.0, 5.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        p.col_ub[0] = 0.0;
        let s = Lp::relaxation(&p).solve();
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective - 5.0).abs() < 1e-9, "{}", s.objective);
    }

    #[test]
    fn terminates_on_a_degenerate_problem() {
        // Regression: Bland's rule was applied to the entering variable but not the
        // leaving one, so a degenerate vertex could cycle until the iteration limit.
        // Many identical rows make degeneracy near-certain.
        let rows: Vec<(&[f64], RowSense, f64)> = (0..12)
            .map(|_| (&[1.0, 1.0, 1.0][..], RowSense::Ge, 1.0))
            .collect();
        let p = problem(&[1.0, 1.0, 1.0], &rows);
        let s = Lp::relaxation(&p).solve_with_limit(5_000);
        assert_eq!(
            s.status,
            LpStatus::Optimal,
            "took {} iterations",
            s.iterations
        );
        assert!((s.objective - 1.0).abs() < 1e-9, "{}", s.objective);
        assert_feasible(&p, &s.x);
    }

    #[test]
    fn an_empty_row_set_leaves_the_objective_at_its_best_bound() {
        // No constraints: each variable goes to whichever bound its cost prefers.
        let s = solve(&[1.0, -2.0], &[]);
        assert_eq!(s.status, LpStatus::Optimal);
        assert!((s.objective + 2.0).abs() < 1e-9, "{}", s.objective);
        assert!((s.x[0] - 0.0).abs() < 1e-9 && (s.x[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn warm_start_agrees_with_a_cold_solve_on_every_branch() {
        // The differential test the dual simplex actually needs: for each column,
        // fix it to 0 and to 1 in turn, then check that re-solving from the parent
        // basis reaches the same objective as solving that subproblem from scratch.
        // This is exactly the operation branch-and-bound performs at every node.
        let p = problem(
            &[3.0, 5.0, 2.0, 7.0],
            &[
                (&[2.0, 3.0, 1.0, 4.0], RowSense::Ge, 5.0),
                (&[1.0, 1.0, 1.0, 1.0], RowSense::Le, 3.0),
                (&[4.0, 1.0, 2.0, 1.0], RowSense::Ge, 3.0),
            ],
        );
        let mut lp = Lp::relaxation(&p);
        let root = lp.solve();
        assert_eq!(root.status, LpStatus::Optimal);

        for j in 0..p.n_cols() {
            for fix in [0.0, 1.0] {
                let saved = lp.column_bounds(j);
                lp.set_column_bounds(j, fix, fix);

                let warm = lp.solve_warm(&root.basis, None, 10_000);
                let cold = lp.solve();
                assert_eq!(warm.status, cold.status, "column {j} fixed to {fix}");
                if cold.status == LpStatus::Optimal {
                    assert!(
                        (warm.objective - cold.objective).abs() < 1e-7,
                        "column {j} fixed to {fix}: warm {} vs cold {}",
                        warm.objective,
                        cold.objective
                    );
                    assert_feasible(&p, &warm.x);
                }

                lp.set_column_bounds(j, saved.0, saved.1);
            }
        }
    }

    #[test]
    fn warm_start_detects_an_infeasible_branch() {
        // Fixing both columns to 0 cannot satisfy the row, and the dual simplex must
        // report that rather than returning a bogus optimum.
        let p = problem(&[1.0, 1.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        let mut lp = Lp::relaxation(&p);
        let root = lp.solve();
        lp.set_column_bounds(0, 0.0, 0.0);
        lp.set_column_bounds(1, 0.0, 0.0);
        let warm = lp.solve_warm(&root.basis, None, 10_000);
        assert_eq!(warm.status, LpStatus::Infeasible);
    }

    #[test]
    fn cutoff_abandons_a_node_whose_bound_is_already_too_weak() {
        // Forcing the expensive column in pushes the bound to 5; a cutoff below that
        // must stop the solve rather than complete it.
        let p = problem(&[1.0, 5.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        let mut lp = Lp::relaxation(&p);
        let root = lp.solve();
        lp.set_column_bounds(0, 0.0, 0.0);

        let cut = lp.solve_warm(&root.basis, Some(2.0), 10_000);
        assert_eq!(cut.status, LpStatus::CutOff);

        // With a cutoff above the true bound the same solve completes normally.
        let full = lp.solve_warm(&root.basis, Some(10.0), 10_000);
        assert_eq!(full.status, LpStatus::Optimal);
        assert!((full.objective - 5.0).abs() < 1e-9, "{}", full.objective);
    }

    #[test]
    fn warm_start_costs_fewer_iterations_than_a_cold_solve() {
        // The whole point of warm starting. Not a tight bound, just that inheriting
        // the parent basis avoids repeating the work.
        let rows: Vec<(&[f64], RowSense, f64)> = vec![
            (&[2.0, 3.0, 1.0, 4.0, 1.0, 2.0][..], RowSense::Ge, 6.0),
            (&[1.0, 2.0, 3.0, 1.0, 2.0, 1.0][..], RowSense::Ge, 5.0),
            (&[3.0, 1.0, 2.0, 2.0, 1.0, 3.0][..], RowSense::Ge, 7.0),
            (&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0][..], RowSense::Le, 4.0),
        ];
        let p = problem(&[4.0, 6.0, 3.0, 8.0, 2.0, 5.0], &rows);
        let mut lp = Lp::relaxation(&p);
        let root = lp.solve();

        lp.set_column_bounds(3, 1.0, 1.0);
        let warm = lp.solve_warm(&root.basis, None, 10_000);
        let cold = lp.solve();
        assert_eq!(warm.status, LpStatus::Optimal);
        assert!(
            warm.iterations < cold.iterations,
            "warm {} iterations, cold {}",
            warm.iterations,
            cold.iterations
        );
    }

    #[test]
    fn range_row_is_honoured_on_both_sides() {
        let mut p = problem(&[-1.0], &[(&[1.0], RowSense::Ge, 0.25)]);
        p.row_ub[0] = 0.75;
        let s = Lp::relaxation(&p).solve();
        assert_eq!(s.status, LpStatus::Optimal);
        // Cost is negative, so x rises until the row's upper bound stops it.
        assert!((s.x[0] - 0.75).abs() < 1e-9, "{:?}", s.x);
        assert_feasible(&p, &s.x);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::tests::problem;
    use super::*;
    use crate::model::RowSense;

    #[test]
    fn a_cached_factorization_does_not_change_the_answer() {
        // Re-solving the same bases repeatedly must give the same results whether
        // the factors came from the cache or were rebuilt. Alternating between two
        // unrelated bases and back exercises eviction and reuse together.
        let p = problem(
            &[3.0, 5.0, 2.0, 7.0, 1.0],
            &[
                (&[2.0, 3.0, 1.0, 4.0, 1.0], RowSense::Ge, 5.0),
                (&[1.0, 1.0, 1.0, 1.0, 1.0], RowSense::Le, 3.0),
                (&[4.0, 1.0, 2.0, 1.0, 3.0], RowSense::Ge, 3.0),
            ],
        );
        let mut lp = Lp::relaxation(&p);
        let root = lp.solve();

        // Two different subproblems, each solved twice, interleaved.
        let mut results = Vec::new();
        for round in 0..3 {
            for column in [0usize, 3] {
                let saved = lp.column_bounds(column);
                lp.set_column_bounds(column, 1.0, 1.0);
                let solved = lp.solve_warm(&root.basis, None, 10_000);
                lp.set_column_bounds(column, saved.0, saved.1);
                if round == 0 {
                    results.push((solved.status, solved.objective));
                } else {
                    let (status, objective) = results[usize::from(column == 3)];
                    assert_eq!(solved.status, status, "column {column}, round {round}");
                    assert!(
                        (solved.objective - objective).abs() < 1e-9,
                        "column {column}, round {round}: {} vs {objective}",
                        solved.objective
                    );
                }
            }
        }
    }

    #[test]
    fn a_cloned_lp_starts_with_no_cache() {
        // Each worker in a parallel search clones the LP; carrying a cache across
        // would share nothing useful and only cost memory.
        let p = problem(&[1.0, 1.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        let mut lp = Lp::relaxation(&p);
        let _ = lp.solve();
        assert!(!lp.factors.is_empty(), "solving should populate the cache");
        assert!(
            lp.clone().factors.is_empty(),
            "a clone carried the cache over"
        );
    }

    #[test]
    fn invalidation_empties_the_cache() {
        let p = problem(&[1.0, 1.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        let mut lp = Lp::relaxation(&p);
        let _ = lp.solve();
        lp.invalidate_factors();
        assert!(lp.factors.is_empty());
        // And it still solves correctly afterwards.
        assert_eq!(lp.solve().status, LpStatus::Optimal);
    }
}
