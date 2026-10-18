//! Presolve: tighten the model before the search ever sees it.
//!
//! Every reduction here is applied **in place**. A fixed column becomes
//! `lb == ub` rather than being deleted, and a redundant row has its bounds freed
//! to `(-inf, +inf)` rather than being dropped. Nothing is renumbered, so the
//! solution vector still maps one-to-one onto the original columns and there is no
//! postsolve step at all.
//!
//! That trade is deliberate. Physically removing rows and columns would make the
//! LPs smaller, but it requires an index map and a postsolve pass to rebuild the
//! original solution, historically the most bug-prone part of a presolver. The
//! simplex already skips fixed columns during pricing and a freed row's logical is
//! basic and never binds, so most of the benefit arrives anyway. Physical removal
//! is a later optimization, worth doing when instance sizes make the wasted rows
//! matter.
//!
//! The reductions, applied to a fixpoint:
//!
//! - **Activity bounds.** A row whose minimum activity already exceeds its upper
//!   bound (or maximum falls short of its lower) proves the model infeasible; one
//!   whose whole activity range sits inside its bounds is redundant.
//! - **Forcing rows.** When a row can only be satisfied at the extreme of its
//!   activity range, every column in it is pinned to the value that achieves it.
//! - **Bound propagation.** The residual activity of the other columns implies a
//!   bound on each column, which for a binary rounds to a fixing.
//! - **Coefficient tightening.** A coefficient larger than the row can actually
//!   use is reduced. This never changes the binary feasible set, but it tightens
//!   the LP relaxation, which is the whole point.

use crate::model::Problem;
use crate::sparse::SparseMatrix;

/// What presolve did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub rounds: usize,
    pub fixed_columns: usize,
    pub redundant_rows: usize,
    pub tightened_coefficients: usize,
    /// Columns decided because both of a probe's suppositions forced the same value,
    /// rather than because one of them was refuted. Reported so a test can require the
    /// rule to still be reached rather than only to still be sound.
    pub agreed_columns: usize,
}

/// The result of presolving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The model was reduced (possibly not at all) and remains possibly feasible.
    Reduced(Stats),
    /// Presolve proved the model has no feasible binary assignment.
    Infeasible,
}

const TOL: f64 = 1e-9;

/// A row's coefficients, as `(column, value)` pairs.
type Row = Vec<(usize, f64)>;

/// Reduce `problem` in place.
pub fn presolve(problem: &mut Problem, max_rounds: usize) -> Outcome {
    presolve_until(problem, max_rounds, None)
}

/// Presolve, abandoning the expensive half once `deadline` passes.
pub fn presolve_until(
    problem: &mut Problem,
    max_rounds: usize,
    deadline: Option<std::time::Instant>,
) -> Outcome {
    presolve_with(problem, max_rounds, deadline, true)
}

/// Presolve, with or without the reductions that read the objective.
///
/// Two reductions fix a column by its cost rather than by the rows: a column in no row,
/// and a column in a single row. Each keeps *an* optimum rather than every feasible
/// point, which is sound only when the rows in the model are all the rows there are.
/// A search with a [`crate::search::Separator`] has rows the model does not show, and
/// the column one of these fixes can be the one those rows need, so it presolves with
/// `dual` off.
pub fn presolve_with(
    problem: &mut Problem,
    max_rounds: usize,
    deadline: Option<std::time::Instant>,
    dual: bool,
) -> Outcome {
    let n = problem.n_cols();
    let m = problem.n_rows();
    let mut stats = Stats::default();

    // Work row-wise: every reduction here reasons about one row at a time.
    let csr = problem.matrix.to_csr();
    let mut rows: Vec<Row> = (0..m)
        .map(|i| {
            let (cols, vals) = csr.column(i);
            cols.iter().copied().zip(vals.iter().copied()).collect()
        })
        .collect();

    // The cheap reductions run to a fixpoint, then the conflict graph is read off what
    // they left and used to tighten again; anything that finds is fed back through the
    // cheap reductions, since a smaller coefficient can make a row redundant or forcing.
    for pass in 0..=CONFLICT_PASSES {
        if !cheap_reductions(problem, &mut rows, max_rounds, dual, &mut stats) {
            return Outcome::Infeasible;
        }
        rebuild_matrix(problem, &rows);
        if pass == CONFLICT_PASSES
            || deadline.is_some_and(|d| std::time::Instant::now() >= d)
            || !tighten_with_conflicts(problem, &mut rows, &mut stats)
        {
            break;
        }
    }

    // Probing last, on the model the cheap reductions have already shrunk.
    if !probe_columns(problem, &mut stats, deadline) {
        return Outcome::Infeasible;
    }

    debug_assert_eq!(problem.n_cols(), n);
    Outcome::Reduced(stats)
}

/// Rebuild the column-major matrix from the (possibly retightened) rows.
///
/// A coefficient tightened all the way to zero is dropped here rather than stored: the
/// rest of the solver reads an explicit zero as a column the row mentions.
fn rebuild_matrix(problem: &mut Problem, rows: &[Row]) {
    let triplets = rows.iter().enumerate().flat_map(|(i, row)| {
        row.iter()
            .filter(|&&(_, v)| v != 0.0)
            .map(move |&(j, v)| (i, j, v))
    });
    problem.matrix = SparseMatrix::from_triplets(problem.n_rows(), problem.n_cols(), triplets);
}

/// The row-at-a-time reductions, to a fixpoint or `max_rounds`. False on infeasibility.
fn cheap_reductions(
    problem: &mut Problem,
    rows: &mut [Row],
    max_rounds: usize,
    dual: bool,
    stats: &mut Stats,
) -> bool {
    for _ in 0..max_rounds {
        stats.rounds += 1;
        let mut changed = false;

        for (i, row) in rows.iter().enumerate() {
            if is_free(problem.row_lb[i], problem.row_ub[i]) {
                continue;
            }
            let (min_activity, max_activity) = activity(row, &problem.col_lb, &problem.col_ub);

            if min_activity > problem.row_ub[i] + TOL || max_activity < problem.row_lb[i] - TOL {
                return false;
            }

            if min_activity >= problem.row_lb[i] - TOL && max_activity <= problem.row_ub[i] + TOL {
                free_row(problem, i);
                stats.redundant_rows += 1;
                changed = true;
                continue;
            }

            // Forcing rows: the row is satisfiable only at one end of its activity
            // range, which pins every column appearing in it.
            if (max_activity - problem.row_lb[i]).abs() <= TOL {
                if !pin_row(problem, row, Extreme::Max, stats) {
                    return false;
                }
                free_row(problem, i);
                stats.redundant_rows += 1;
                changed = true;
                continue;
            }
            if (problem.row_ub[i] - min_activity).abs() <= TOL {
                if !pin_row(problem, row, Extreme::Min, stats) {
                    return false;
                }
                free_row(problem, i);
                stats.redundant_rows += 1;
                changed = true;
                continue;
            }

            match propagate(problem, row, i, min_activity, max_activity, stats) {
                Propagation::Infeasible => return false,
                Propagation::Changed => changed = true,
                Propagation::Unchanged => {}
            }
        }

        for (i, row) in rows.iter_mut().enumerate() {
            if tighten_coefficients(problem, row, i, stats) {
                changed = true;
            }
        }

        if dual && fix_columns_absent_from_every_row(problem, rows, stats) {
            changed = true;
        }

        if dual && fix_singleton_columns(problem, rows, stats) {
            changed = true;
        }

        if !changed {
            break;
        }
    }

    true
}

fn is_free(lb: f64, ub: f64) -> bool {
    lb == f64::NEG_INFINITY && ub == f64::INFINITY
}

fn free_row(problem: &mut Problem, i: usize) {
    problem.row_lb[i] = f64::NEG_INFINITY;
    problem.row_ub[i] = f64::INFINITY;
}

/// The minimum and maximum a row's activity can reach within the column bounds.
fn activity(row: &Row, col_lb: &[f64], col_ub: &[f64]) -> (f64, f64) {
    let mut min = 0.0;
    let mut max = 0.0;
    for &(j, a) in row {
        let (lo, hi) = (a * col_lb[j], a * col_ub[j]);
        min += lo.min(hi);
        max += lo.max(hi);
    }
    (min, max)
}

/// Which end of its activity range a row is being pinned to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Extreme {
    Min,
    Max,
}

/// Fix every column of a forcing row to the value that achieves the extreme.
///
/// Returns false if that demands two different values for one column.
fn pin_row(problem: &mut Problem, row: &Row, extreme: Extreme, stats: &mut Stats) -> bool {
    for &(j, a) in row {
        let (lo, hi) = (problem.col_lb[j], problem.col_ub[j]);
        // A positive coefficient reaches its minimum contribution at the column's
        // lower bound; a negative one at its upper.
        let target = match (extreme, a > 0.0) {
            (Extreme::Min, true) | (Extreme::Max, false) => lo,
            (Extreme::Min, false) | (Extreme::Max, true) => hi,
        };
        if !fix_column(problem, j, target, stats) {
            return false;
        }
    }
    true
}

/// Narrow a column to a single value, reporting false if that is impossible.
fn fix_column(problem: &mut Problem, j: usize, value: f64, stats: &mut Stats) -> bool {
    if problem.col_lb[j] > value + TOL || problem.col_ub[j] < value - TOL {
        return false;
    }
    let already_fixed = problem.col_lb[j] == problem.col_ub[j];
    problem.col_lb[j] = value;
    problem.col_ub[j] = value;
    if !already_fixed {
        stats.fixed_columns += 1;
    }
    true
}

enum Propagation {
    Unchanged,
    Changed,
    Infeasible,
}

/// Derive per-column bounds from the residual activity of the rest of the row.
///
/// With `row_lb <= rest + a*x_j <= row_ub`, the term `a*x_j` is confined to
/// `[row_lb - rest_max, row_ub - rest_min]`. Dividing by `a` turns that into a
/// bound on the column itself, which for a binary rounds inwards to an integer and
/// so often fixes it outright.
fn propagate(
    problem: &mut Problem,
    row: &Row,
    i: usize,
    min_activity: f64,
    max_activity: f64,
    stats: &mut Stats,
) -> Propagation {
    let (row_lb, row_ub) = (problem.row_lb[i], problem.row_ub[i]);
    let mut changed = false;

    for &(j, a) in row {
        if a == 0.0 {
            continue;
        }
        let (lo, hi) = (a * problem.col_lb[j], a * problem.col_ub[j]);
        let (own_min, own_max) = (lo.min(hi), lo.max(hi));
        // What the *other* columns of this row can contribute.
        let rest_min = min_activity - own_min;
        let rest_max = max_activity - own_max;

        // An infinite row bound imposes nothing on that side; `inf - finite` is
        // still infinite, so the arithmetic carries that through correctly.
        let contribution_lo = row_lb - rest_max;
        let contribution_hi = row_ub - rest_min;

        let (implied_lo, implied_hi) = if a > 0.0 {
            (contribution_lo / a, contribution_hi / a)
        } else {
            (contribution_hi / a, contribution_lo / a)
        };

        // An integer column's implied bound rounds inwards, which is what turns a
        // weak implication into a fixing. A continuous column must not be rounded:
        // doing so cuts off values it is entitled to take, and the search then
        // proves a worse solution optimal.
        let (implied_lo, implied_hi) = if problem.is_integer(j) {
            (
                ceil_with_tolerance(implied_lo),
                floor_with_tolerance(implied_hi),
            )
        } else {
            (implied_lo, implied_hi)
        };
        let new_lb = problem.col_lb[j].max(implied_lo);
        let new_ub = problem.col_ub[j].min(implied_hi);
        if new_lb > new_ub + TOL {
            return Propagation::Infeasible;
        }
        if new_lb > problem.col_lb[j] + TOL || new_ub < problem.col_ub[j] - TOL {
            let was_fixed = problem.col_lb[j] == problem.col_ub[j];
            problem.col_lb[j] = new_lb;
            problem.col_ub[j] = new_ub;
            if !was_fixed && new_lb == new_ub {
                stats.fixed_columns += 1;
            }
            changed = true;
        }
    }

    if changed {
        Propagation::Changed
    } else {
        Propagation::Unchanged
    }
}

fn ceil_with_tolerance(v: f64) -> f64 {
    if v.is_finite() { (v - TOL).ceil() } else { v }
}

fn floor_with_tolerance(v: f64) -> f64 {
    if v.is_finite() { (v + TOL).floor() } else { v }
}

/// A column appearing in no still-active row is decided entirely by its cost.
/// Fix a column that appears in exactly one row, when the direction the objective
/// wants can never violate that row.
///
/// A singleton column has only one reason to move: its single row. If moving it the
/// way the objective prefers pushes the row's activity towards a side that has no
/// bound, then no feasible point is lost by pinning it there, and none of the objective
/// either. This is the standard dual fixing for column singletons, and it stays within
/// what this presolve can do because it only narrows bounds, so nothing is renumbered
/// and no postsolve is needed.
///
/// It matters more than its simplicity suggests. MIPLIB's neos-3048764-nadi is 8731
/// singleton columns out of 12360, and HiGHS, which does this, solves it in 0.1s
/// against a model a quarter the size.
fn fix_singleton_columns(problem: &mut Problem, rows: &[Row], stats: &mut Stats) -> bool {
    let n = problem.n_cols();
    // Where each column appears, and how often. Only the columns seen exactly once
    // matter, so the entry is overwritten freely until the count settles.
    let mut appearances = vec![0usize; n];
    let mut only = vec![(0usize, 0.0f64); n];
    for (i, row) in rows.iter().enumerate() {
        if is_free(problem.row_lb[i], problem.row_ub[i]) {
            continue;
        }
        for &(j, a) in row {
            if a != 0.0 {
                appearances[j] += 1;
                only[j] = (i, a);
            }
        }
    }

    let mut changed = false;
    for j in 0..n {
        if appearances[j] != 1 || problem.col_lb[j] == problem.col_ub[j] {
            continue;
        }
        let (i, a) = only[j];
        // Lowering the column moves the row's activity one way and raising it moves
        // the other; each is safe exactly when the side it moves towards is unbounded.
        let down_safe = (a > 0.0 && problem.row_lb[i] == f64::NEG_INFINITY)
            || (a < 0.0 && problem.row_ub[i] == f64::INFINITY);
        let up_safe = (a > 0.0 && problem.row_ub[i] == f64::INFINITY)
            || (a < 0.0 && problem.row_lb[i] == f64::NEG_INFINITY);

        // Minimization form, so a positive cost wants the column down. A zero cost is
        // indifferent and takes whichever side is safe.
        let cost = problem.obj[j];
        let target = if cost > 0.0 && down_safe {
            problem.col_lb[j]
        } else if cost < 0.0 && up_safe {
            problem.col_ub[j]
        } else if cost == 0.0 && down_safe {
            problem.col_lb[j]
        } else if cost == 0.0 && up_safe {
            problem.col_ub[j]
        } else {
            continue;
        };
        // An infinite target means the objective is unbounded along this column, which
        // is the LP's finding to make and not presolve's.
        if !target.is_finite() {
            continue;
        }
        if fix_column(problem, j, target, stats) {
            changed = true;
        }
    }
    changed
}

fn fix_columns_absent_from_every_row(
    problem: &mut Problem,
    rows: &[Row],
    stats: &mut Stats,
) -> bool {
    let n = problem.n_cols();
    let mut appears = vec![false; n];
    for (i, row) in rows.iter().enumerate() {
        if is_free(problem.row_lb[i], problem.row_ub[i]) {
            continue;
        }
        for &(j, a) in row {
            if a != 0.0 {
                appears[j] = true;
            }
        }
    }

    let mut changed = false;
    for (j, &seen) in appears.iter().enumerate() {
        if seen || problem.col_lb[j] == problem.col_ub[j] {
            continue;
        }
        // Minimization: a positive cost prefers the lower bound, a negative one the
        // upper, and a zero cost is indifferent and takes whichever bound exists.
        let (favoured, other) = if problem.obj[j] < 0.0 {
            (problem.col_ub[j], problem.col_lb[j])
        } else {
            (problem.col_lb[j], problem.col_ub[j])
        };
        // The bound has to exist. Fixing a column at an infinite one puts a value in
        // the model that is not a number the rest of the solver can use: the column
        // reads as `[inf, inf]`, every later feasibility check compares against it and
        // fails, and a zero cost times an infinite value is a NaN objective. On
        // MIPLIB's pigeon-08 that silently threw away every solution the heuristics
        // found, twelve of them in fifteen seconds, and the model solves to an
        // incumbent with presolve switched off. The singleton reduction next door
        // already guards this; this one did not.
        let target = if favoured.is_finite() {
            favoured
        } else if problem.obj[j] == 0.0 && other.is_finite() {
            other
        } else if problem.obj[j] == 0.0 {
            // Free and costless: any value does, and zero is inside no bound to break.
            0.0
        } else {
            // The objective improves without limit along this column, which is the
            // LP's finding to report rather than presolve's.
            continue;
        };
        if fix_column(problem, j, target, stats) {
            changed = true;
        }
    }
    changed
}

/// Reduce coefficients that are larger than their row can use.
fn tighten_coefficients(problem: &mut Problem, row: &mut Row, i: usize, stats: &mut Stats) -> bool {
    // Only single-sided rows. Tightening one side of a range row invalidates the
    // reasoning for the other, and an equality row has no slack to exploit.
    let lb = problem.row_lb[i];
    let ub = problem.row_ub[i];
    let ge = lb.is_finite() && ub == f64::INFINITY;
    let le = ub.is_finite() && lb == f64::NEG_INFINITY;
    if !ge && !le {
        return false;
    }

    let mut changed = false;
    for k in 0..row.len() {
        let (j, a) = row[k];
        // Only free binaries. The reduction reasons about a column being switched
        // fully on or fully off, which is meaningless for a continuous column and
        // wrong for a general integer that can land in between.
        if a == 0.0 || !problem.is_binary(j) || problem.col_lb[j] != 0.0 || problem.col_ub[j] != 1.0
        {
            continue;
        }
        let (min_activity, max_activity) = activity(row, &problem.col_lb, &problem.col_ub);

        if ge {
            let slack = problem.row_lb[i] - min_activity;
            if slack <= TOL {
                continue;
            }
            if a > 0.0 && a > slack + TOL {
                // `x_j = 1` already satisfies the row on its own; the excess above
                // `slack` can never be needed, so spending it is pure LP looseness.
                row[k].1 = slack;
                changed = true;
                stats.tightened_coefficients += 1;
            } else if a < 0.0 && -a > slack + TOL {
                // Mirror image: `x_j = 0` is the non-binding case.
                row[k].1 = -slack;
                problem.row_lb[i] = min_activity - a;
                changed = true;
                stats.tightened_coefficients += 1;
            }
        } else {
            let slack = max_activity - problem.row_ub[i];
            if slack <= TOL {
                continue;
            }
            if a > 0.0 && a > slack + TOL {
                row[k].1 = slack;
                problem.row_ub[i] = max_activity - a;
                changed = true;
                stats.tightened_coefficients += 1;
            } else if a < 0.0 && -a > slack + TOL {
                row[k].1 = -slack;
                changed = true;
                stats.tightened_coefficients += 1;
            }
        }
    }
    changed
}

/// Times the conflict graph is rebuilt and read for coefficients. Each pass can only
/// find something the previous one's tightened rows made derivable, and on the models
/// measured the second pass finds nothing.
const CONFLICT_PASSES: usize = 2;

/// Pairs of a row a conflict pass may test, as a multiple of the matrix's entries.
///
/// A row of length `k` costs `k * k` membership tests, which is nothing on a three-column
/// row and ruinous on a set partitioning row of thousands; the graph's own reading of
/// those rows as cliques is already the stronger statement of them.
const CONFLICT_WORK_PER_ENTRY: usize = 64;

/// Longest row a conflict pass reads. See `CONFLICT_WORK_PER_ENTRY`.
const CONFLICT_MAX_ROW: usize = 64;

/// Coefficient tightening again, with the columns a binary excludes held at their values.
///
/// `tighten_coefficients` shrinks a coefficient to what the row can use when the column
/// is on (or off), reading every other column's range from its bounds. But setting a
/// binary also settles every column it conflicts with, and a column so settled cannot
/// contribute what its bounds say it could. The reasoning is otherwise identical: on a
/// `<=` row with `a_q < 0`, only the case `x_q = 1` depends on `a_q`, and there the rest
/// of the row reaches at most `M`, so `a_q` may rise to `min(0, b - M)`; with `a_q > 0`,
/// the case `x_q = 0` leaves slack `b - M0` that `a_q` and `b` can both give up.
///
/// **What it is for.** MIPLIB's `neos-633273` is 10164 rows `x - y - w <= 0` where `x`
/// and `w` sit in one set partitioning row once aggregation has merged `w` with the
/// member it is declared equal to. `w = 1` forces `x = 0`, so `w` never helps and the row
/// is `x <= y`. Bounds alone cannot see that, and the relaxation is 4.68e9 against an
/// optimum of 6.997e9; with it, the first relaxation *is* the optimum.
///
/// Sound for the same reason the bound version is. The new row implies the old, since
/// the coefficient only moves towards making it harder; and the old model implies the new
/// row, since the conflicts are implications of the old model. So the integer feasible
/// set is exactly unchanged, and a conflict derived from the very row being tightened is
/// no exception: it held before the change, so the change cannot have invalidated it.
fn tighten_with_conflicts(problem: &mut Problem, rows: &mut [Row], stats: &mut Stats) -> bool {
    let conflicts = crate::cuts::Conflicts::of(problem);
    if conflicts.is_empty() {
        return false;
    }
    let free_binary =
        |p: &Problem, j: usize| p.is_binary(j) && p.col_lb[j] == 0.0 && p.col_ub[j] == 1.0;
    let mut budget = CONFLICT_WORK_PER_ENTRY.saturating_mul(problem.matrix.nnz());
    let mut changed = false;

    for (i, row) in rows.iter_mut().enumerate() {
        let (lb, ub) = (problem.row_lb[i], problem.row_ub[i]);
        // Single-sided rows only, as in `tighten_coefficients` and for the same reason.
        let sign = if ub.is_finite() && lb == f64::NEG_INFINITY {
            1.0
        } else if lb.is_finite() && ub == f64::INFINITY {
            -1.0
        } else {
            continue;
        };
        if row.len() < 2 || row.len() > CONFLICT_MAX_ROW {
            continue;
        }
        let cost = row.len() * row.len();
        if cost > budget {
            break;
        }
        budget -= cost;
        // Written as `sum c_j x_j <= b` whichever side the row is bounded on.
        let mut b = if sign > 0.0 { ub } else { -lb };
        let mut row_changed = false;

        for k in 0..row.len() {
            let (q, a) = row[k];
            let c = sign * a;
            if c == 0.0 || !free_binary(problem, q) {
                continue;
            }
            // The case the coefficient matters in: on for a negative one, off for a
            // positive one.
            let node = crate::cuts::literal(q, c < 0.0);
            let mut most = 0.0;
            for (t, &(j, aj)) in row.iter().enumerate() {
                if t == k {
                    continue;
                }
                let cj = sign * aj;
                let (lo, hi) = (problem.col_lb[j], problem.col_ub[j]);
                let settled = free_binary(problem, j)
                    && conflicts.conflicts(node, crate::cuts::literal(j, cj > 0.0));
                most += if settled {
                    // The value that helps the row most is excluded, so the column takes
                    // the other one.
                    if cj > 0.0 { cj * lo } else { cj * hi }
                } else if cj > 0.0 {
                    cj * hi
                } else {
                    cj * lo
                };
            }
            if !most.is_finite() {
                continue;
            }
            if c < 0.0 {
                let raised = (b - most).min(0.0);
                if raised > c + CONFLICT_TIGHTEN_MIN {
                    row[k].1 = sign * raised;
                    row_changed = true;
                    stats.tightened_coefficients += 1;
                }
            } else {
                let slack = (b - most).min(c);
                if slack > CONFLICT_TIGHTEN_MIN {
                    row[k].1 = sign * (c - slack);
                    b -= slack;
                    row_changed = true;
                    stats.tightened_coefficients += 1;
                }
            }
        }
        if row_changed {
            if sign > 0.0 {
                problem.row_ub[i] = b;
            } else {
                problem.row_lb[i] = -b;
            }
            row.retain(|&(_, v)| v != 0.0);
            changed = true;
        }
    }
    changed
}

/// Smallest coefficient change a conflict pass bothers to make. Below this it is
/// rounding, and taking it would let two passes trade the same row back and forth.
const CONFLICT_TIGHTEN_MIN: f64 = 1e-7;

/// Matrix entries one probe may read before it is abandoned.
///
/// Abandoning a probe that outruns this is safe in the only direction that matters: an
/// unfinished sweep refutes nothing, so the column keeps both of its values.
const PROBE_REACH: usize = 4_000_000;

/// Matrix entries probing may read without proving anything, before it gives up.
///
/// Raised from thirty million once it was measured rather than guessed. `ex9`'s proofs
/// are front-loaded -- four in its first eighteen probes -- and then it goes 57.8
/// million entries between two of them before finding another 2600. Thirty million cut
/// it off in that gap, at 63 probes of 8560 and 1848 columns of the 10076 that are
/// there, so what looked like a model probing could not reduce was a model probing was
/// not allowed to finish reducing. This is the longest dry stretch on any model in the
/// set, with a little room over it.
const PROBE_PATIENCE: usize = 120_000_000;

/// Probes in a row that prove nothing and do not finish, before probing stops.
///
/// The patience above prices a probe by what it reads, which is right for a model
/// probing can reason about and wrong for one it cannot. `irp`, `nw04`, `air03` and
/// `eil33-2` abandon **every** sweep they start: each one runs into `PROBE_REACH` and
/// gives up, and across fifteen hundred probes they prove nothing whatever. Under the
/// patience alone they spend nine to twenty-three seconds finding that out. A sweep
/// that did not finish has not said the column has nothing to give, only that it was
/// not allowed to look, and a run of those says the model is out of reach.
///
/// It separates the set exactly. Those four abandon everything; `ex9` abandons nothing
/// at all, `ex10` 45 sweeps of 15170, and every other model in the benchmark zero.
const ABANDON_RUN: usize = 16;

/// Matrix entries probing may read in all, whether or not it is proving anything.
///
/// A count of work rather than a share of the caller's time limit, which would price
/// this against how long someone happens to be willing to wait, and rather than a
/// number of passes over the matrix, which lets a model with eight million nonzeros
/// spend a hundred times what a model with eighty thousand does for the same reduction.
///
/// It was a hundred million, and the note here argued that ten times that "buys another
/// few thousand fixings on models that do not close either way, and costs `irp` more
/// than two seconds". The first half was measured on a budget that was stopping probing
/// for the wrong reason. `ex9` needs four thousand million and, given them, goes from
/// 1848 columns fixed to 10076 and from timing out to optimal in 25 seconds. The second
/// half is now `ABANDON_RUN`'s job, which is what `irp` actually needed: it costs `irp`
/// nothing, because `irp` never finishes a sweep and is stopped for that.
///
/// So this is a backstop against a model that keeps earning its budget for longer than
/// any answer is worth waiting for -- `ex10` would spend sixteen thousand million and
/// eighty-six seconds -- and the models that pay are stopped well short of it.
const PROBE_TOTAL: usize = 5_000_000_000;

/// Fix the columns for which only one of the two values survives its own consequences.
///
/// Every other reduction here reasons forwards from the bounds it has. This one reasons
/// backwards: suppose a column takes a value, work out what follows, and if that ends in
/// a contradiction the supposition was wrong and the column is fixed to the other value.
/// Nothing is guessed; a value refuted this way is in no feasible solution.
///
/// It reasons with the search's own propagator, conflict graph and row activities
/// together, which is what makes it reach anything: the conflicts of these models live
/// in long set partitioning rows that pairwise reasoning never sees.
///
/// **Parked, not finished.** It reduces the models it is aimed at, `mitre` by 3677
/// columns of 10724 against a reference solver's 4693, `air04` by 787, and it is the
/// only thing yet tried here that closes `mitre`. It also costs `air03` between three
/// and four times its solve, because the reach cap gates the column a probe starts from
/// and not the ones its propagation walks into. Six budgets were tried and every one
/// trades the reduction against the cost; the coupling is not incidental, since probing
/// pays where conflicts are dense and dense conflicts are what make it expensive.
///
/// What it wants next is not another budget: it is propagation that does not rebuild its
/// state per probe, or a restriction to columns whose whole reachable set is small.
fn probe_columns(
    problem: &mut Problem,
    stats: &mut Stats,
    deadline: Option<std::time::Instant>,
) -> bool {
    use crate::heuristic::Sweep;
    // Matrix entries between two readings of the clock. Counting probes instead prices
    // a model whose probes cost a thousand entries the same as one whose probes cost
    // half a million, and on the second a stride of 64 probes is half a minute of
    // overshoot past a deadline the caller was promised. Counting work bounds the
    // overshoot by the work rather than by the model.
    const CLOCK_STRIDE: usize = 1_000_000;
    let conflicts = crate::cuts::Conflicts::of(problem);
    if conflicts.is_empty() {
        return true;
    }
    let csr = problem.matrix.to_csr();
    let n = problem.n_cols();
    let mut state = crate::heuristic::Propagator::new(problem, &conflicts, &csr, TOL);

    // Whatever the model already pins implies more, and that is settled once here
    // rather than re-derived inside every probe.
    state.seed_fixed();
    if state.propagate(PROBE_TOTAL) == Sweep::Refuted {
        return false;
    }
    state.commit();

    let mut candidates: Vec<(usize, usize)> = (0..n)
        .filter(|&j| crate::heuristic::is_binary_column(problem, j) && !state.fixed(j))
        .map(|j| {
            let reach = conflicts.degree(2 * j as u32) + conflicts.degree(2 * j as u32 + 1);
            (j, reach)
        })
        .collect();
    // Widest first. Reach is where the refutations are; it used to also be where the
    // cost was, which made this ordering the worst available, and it is not any more.
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // Probing spends what the model has already earned it. `PROBE_PATIENCE` resets on
    // every column proved, so a model that keeps proving things keeps its budget and
    // one that has stopped proving them loses it quickly; `PROBE_TOTAL` catches what
    // that cannot, a model with nine thousand proofs to find, each of them individually
    // earned and affordable, that would otherwise go on finding them for two minutes.
    let mut idle_since = state.work();
    let mut settled: Vec<usize> = Vec::new();
    // What supposing zero forced, kept across the rewind that makes room for the other
    // supposition. Indexed by column so the second sweep can ask about a column in
    // constant time; `NAN` means the first sweep said nothing about it, and no
    // comparison against `NAN` succeeds, so untouched columns need no separate guard.
    let mut under_zero: Vec<f64> = vec![f64::NAN; n];
    let mut touched: Vec<usize> = Vec::new();
    let mut agreed: Vec<(usize, f64)> = Vec::new();
    // Consecutive probes that proved nothing and did not finish. A sweep cut off by
    // `PROBE_REACH` has not said the column has nothing to give, only that it was not
    // allowed to look, so a run of them is a model this reasoning cannot reach at all.
    let mut abandoned_run = 0usize;
    let mut last_clock = 0usize;
    for &(j, reach) in &candidates {
        if state.work() - idle_since > PROBE_PATIENCE || state.work() > PROBE_TOTAL {
            break;
        }
        if state.work() - last_clock >= CLOCK_STRIDE {
            last_clock = state.work();
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                break;
            }
        }
        if reach == 0 || state.fixed(j) {
            continue;
        }
        if abandoned_run >= ABANDON_RUN {
            break;
        }
        let mark = state.mark();
        let zero = state.probe(j, 0.0, PROBE_REACH);
        if zero != Sweep::Refuted {
            state.fixed_since(mark, &mut settled);
            for &k in &settled {
                if state.fixed(k) {
                    if !under_zero[k].is_finite() {
                        touched.push(k);
                    }
                    under_zero[k] = state.value(k);
                }
            }
        }
        state.rewind(mark);
        let one = state.probe(j, 1.0, PROBE_REACH);
        // A value refuted means the other one holds, and so does everything its own
        // sweep forced: those are consequences of a value now proven rather than of a
        // guess, so the whole trail is kept instead of being thrown away and re-derived
        // one column at a time. A sweep that ran out of work refutes nothing, but the
        // fixings it did make are still sound, so the surviving side is kept whether it
        // finished or not.
        match (zero, one) {
            // Neither value survives its own consequences, so neither does the model.
            (Sweep::Refuted, Sweep::Refuted) => return false,
            // Zero is refuted, and the sweep that proved one is the live state already.
            (Sweep::Refuted, _) => abandoned_run = 0,
            // One is refuted, but the sweep for zero was rewound to make room for it.
            // Only a proof pays for this, so it is re-run rather than held onto.
            (_, Sweep::Refuted) => {
                abandoned_run = 0;
                state.rewind(mark);
                if state.probe(j, 0.0, PROBE_REACH) == Sweep::Refuted {
                    return false;
                }
            }
            // Neither value is refuted, so the column itself is not decided. What the
            // two suppositions *agree* on still is: the model takes one of them, so a
            // column both force to the same value is forced outright, whether or not
            // either sweep finished. That is where the reduction on these models comes
            // from -- refutation decides the probed column alone, and agreement decides
            // the columns around it -- and both sweeps have been paid for already.
            _ => {
                agreed.clear();
                state.fixed_since(mark, &mut settled);
                for &k in &settled {
                    let held = under_zero[k];
                    if state.fixed(k) && (held - state.value(k)).abs() <= TOL {
                        agreed.push((k, held));
                    }
                }
                state.rewind(mark);
                if agreed.is_empty() {
                    if zero == Sweep::Abandoned || one == Sweep::Abandoned {
                        abandoned_run += 1;
                    }
                    clear(&mut under_zero, &mut touched);
                    continue;
                }
                abandoned_run = 0;
                stats.agreed_columns += agreed.len();
                // Proved, not supposed, so the whole batch is asserted before the one
                // sweep that works out what follows from all of it.
                for &(k, value) in &agreed {
                    if !state.assert_value(k, value) {
                        return false;
                    }
                }
                if state.propagate(PROBE_REACH) == Sweep::Refuted {
                    return false;
                }
            }
        }
        clear(&mut under_zero, &mut touched);
        state.fixed_since(mark, &mut settled);
        for &k in &settled {
            if !fix_column(problem, k, state.value(k), stats) {
                return false;
            }
        }
        state.commit();
        idle_since = state.work();
    }
    true
}

/// Reset the columns `touched` names, leaving the rest of `slots` alone.
///
/// The table is one entry per column and is read once per probe, so clearing it whole
/// would cost the model's width on every probe and swamp what a probe itself costs.
fn clear(slots: &mut [f64], touched: &mut Vec<usize>) {
    for &k in touched.iter() {
        slots[k] = f64::NAN;
    }
    touched.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{Kind, Spec};
    use crate::model::{RowSense, Sense};
    use lp_parser_rs::problem::LpProblem;

    /// A column no row constrains still has to end up somewhere the rest of the
    /// solver can use. Fixing it at an infinite bound leaves the model reading
    /// `[inf, inf]`, which every later feasibility check compares against and fails,
    /// and which multiplies a zero cost into a NaN objective. On MIPLIB's pigeon-08
    /// that discarded every solution the heuristics found.
    #[test]
    fn an_unconstrained_column_is_never_fixed_at_an_infinite_bound() {
        for (cost, lower, upper) in [
            (0.0, 0.0, f64::INFINITY),
            (0.0, f64::NEG_INFINITY, 0.0),
            (0.0, f64::NEG_INFINITY, f64::INFINITY),
            (-1.0, 0.0, f64::INFINITY),
            (1.0, f64::NEG_INFINITY, 0.0),
        ] {
            // Column 1 appears in no row; column 0 keeps the model non-trivial.
            let mut p = problem(&[1.0, cost], &[(&[1.0, 0.0], RowSense::Ge, 1.0)]);
            p.col_lb[1] = lower;
            p.col_ub[1] = upper;
            presolve(&mut p, 20);
            assert!(
                p.col_lb[1] <= p.col_ub[1],
                "cost {cost} bounds [{lower}, {upper}] left an inverted range [{}, {}]",
                p.col_lb[1],
                p.col_ub[1]
            );
            if p.col_lb[1] == p.col_ub[1] {
                assert!(
                    p.col_lb[1].is_finite(),
                    "cost {cost} bounds [{lower}, {upper}] fixed the column at {}",
                    p.col_lb[1]
                );
                assert!(
                    p.col_lb[1] >= lower && p.col_lb[1] <= upper,
                    "cost {cost} fixed the column outside its own bounds"
                );
            }
        }
    }

    /// The agreement rule fires on generated models, and never costs a feasible point.
    ///
    /// A column both of a probe's suppositions force is forced, since the model takes
    /// one supposition or the other. What makes that worth taking rather than redundant
    /// is which columns it reaches: probing starts only from a binary the conflict graph
    /// has an edge for and decides the column it started from, so a column with no edge
    /// of its own is never the subject of a probe and is only ever decided as a
    /// by-product like this. On `ex9` it is the difference between 8244 columns fixed
    /// and 10076, which is the difference between timing out and closing.
    ///
    /// Soundness is the real hazard -- a rule that fixes a column to a value some
    /// feasible point disagrees with loses that point silently -- so this enumerates the
    /// feasible set before and after and requires it unchanged. The firing count is
    /// asserted too, so that a change which stops reaching the rule fails here rather
    /// than passing vacuously.
    #[test]
    fn agreement_fires_on_generated_models_and_keeps_every_feasible_point() {
        let mut agreed = 0;
        for n_rows in [4usize, 6] {
            for seed in 0..60u64 {
                let spec = Spec {
                    kind: Kind::Signed,
                    n_cols: 10,
                    n_rows,
                    seed,
                };
                let parsed = LpProblem::parse(&spec.to_lp()).unwrap();
                let p = Problem::from_lp(&parsed).unwrap();
                agreed += assert_presolve_is_sound(&p, &spec.name()).agreed_columns;
            }
        }
        assert!(
            agreed > 0,
            "no generated model reached the agreement rule, so nothing here tested it"
        );
    }

    fn problem(obj: &[f64], rows: &[(&[f64], RowSense, f64)]) -> Problem {
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

    /// Every binary assignment the model admits, by exhaustive enumeration.
    ///
    /// Only usable for small `n`, but it is ground truth: presolve is allowed to
    /// change coefficients and bounds however it likes, provided this set, and the
    /// objective value of each member, comes out identical.
    fn feasible_set(p: &Problem) -> Vec<(u32, f64)> {
        let n = p.n_cols();
        assert!(n <= 20, "exhaustive enumeration needs a small model");
        let csr = p.matrix.to_csr();
        let mut out = Vec::new();

        for mask in 0u32..(1u32 << n) {
            let x: Vec<f64> = (0..n).map(|j| f64::from((mask >> j) & 1)).collect();
            let within_bounds =
                (0..n).all(|j| x[j] >= p.col_lb[j] - 1e-9 && x[j] <= p.col_ub[j] + 1e-9);
            if !within_bounds {
                continue;
            }
            let satisfied = (0..p.n_rows()).all(|i| {
                let (cols, vals) = csr.column(i);
                let activity: f64 = cols.iter().zip(vals).map(|(&j, &a)| a * x[j]).sum();
                activity >= p.row_lb[i] - 1e-9 && activity <= p.row_ub[i] + 1e-9
            });
            if satisfied {
                let obj: f64 = p.obj.iter().zip(&x).map(|(c, v)| c * v).sum();
                out.push((mask, obj));
            }
        }
        out
    }

    /// Presolve's actual contract, checked exhaustively.
    ///
    /// It is *not* that the feasible set is preserved, fixing a column that appears
    /// in no constraint to its cheaper value legitimately discards feasible points.
    /// The contract is two-sided:
    ///
    /// 1. **Sound**: every point the reduced model admits was admitted by the
    ///    original, at the same cost. Presolve may never invent solutions.
    /// 2. **Optimality-preserving**: the best achievable objective is unchanged, so
    ///    no optimal solution was reduced away.
    fn assert_presolve_is_sound(before: &Problem, label: &str) -> Stats {
        let expected = feasible_set(before);
        let mut after = before.clone();

        let stats = match presolve(&mut after, 20) {
            Outcome::Infeasible => {
                assert!(
                    expected.is_empty(),
                    "{label}: presolve rejected a model with {} feasible points",
                    expected.len()
                );
                return Stats::default();
            }
            Outcome::Reduced(stats) => stats,
        };

        after
            .validate()
            .unwrap_or_else(|e| panic!("{label}: invalid after presolve: {e}"));
        let got = feasible_set(&after);

        for (mask, obj) in &got {
            let original = expected.iter().find(|(m, _)| m == mask);
            match original {
                None => panic!("{label}: presolve admits {mask:0b}, which the original rejects"),
                Some((_, want)) => assert!(
                    (want - obj).abs() < 1e-9,
                    "{label}: point {mask:0b} costs {obj} after presolve, {want} before"
                ),
            }
        }

        let best = |set: &[(u32, f64)]| set.iter().map(|(_, o)| *o).fold(f64::INFINITY, f64::min);
        let (before_best, after_best) = (best(&expected), best(&got));
        assert!(
            (before_best - after_best).abs() < 1e-9
                || (before_best.is_infinite() && after_best.is_infinite()),
            "{label}: optimum moved from {before_best} to {after_best}"
        );
        stats
    }

    /// Coefficient tightening on its own must preserve the feasible set *exactly*.
    ///
    /// Unlike the pipeline as a whole, this reduction is only ever allowed to change
    /// the relaxation, never the integer solutions, so it gets the strict check the
    /// full presolve cannot be held to.
    fn assert_tightening_preserves_feasible_set(before: &Problem, label: &str) {
        let expected = feasible_set(before);
        let mut after = before.clone();

        let csr = after.matrix.to_csr();
        let mut rows: Vec<Row> = (0..after.n_rows())
            .map(|i| {
                let (cols, vals) = csr.column(i);
                cols.iter().copied().zip(vals.iter().copied()).collect()
            })
            .collect();
        let mut stats = Stats::default();
        for (i, row) in rows.iter_mut().enumerate() {
            tighten_coefficients(&mut after, row, i, &mut stats);
        }
        let triplets = rows
            .iter()
            .enumerate()
            .flat_map(|(i, row)| row.iter().map(move |&(j, v)| (i, j, v)));
        after.matrix = SparseMatrix::from_triplets(after.n_rows(), after.n_cols(), triplets);

        assert_eq!(
            expected,
            feasible_set(&after),
            "{label}: coefficient tightening changed the feasible set"
        );
    }

    #[test]
    fn is_sound_on_generated_instances() {
        // The broad net: presolve is allowed to do anything to these models except
        // change which binary points satisfy them.
        for kind in [Kind::Knapsack, Kind::Covering, Kind::Signed] {
            for seed in 0..12u64 {
                let spec = Spec {
                    kind,
                    n_cols: 12,
                    n_rows: 6,
                    seed,
                };
                let parsed = LpProblem::parse(&spec.to_lp()).unwrap();
                let p = Problem::from_lp(&parsed).unwrap();
                assert_presolve_is_sound(&p, &spec.name());
            }
        }
    }

    /// The coefficient of column `j` in row `i`, read back out of the matrix.
    fn coefficient(p: &Problem, i: usize, j: usize) -> f64 {
        let csr = p.matrix.to_csr();
        let (cols, vals) = csr.column(i);
        cols.iter()
            .zip(vals)
            .find(|(c, _)| **c == j)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }

    #[test]
    fn pins_a_singleton_column_the_objective_and_row_agree_on() {
        // `x0` appears once, in a row with no lower bound, and costs something. Pushing
        // it down can only make that row easier, so zero is both optimal and safe.
        //
        // The first row is deliberately not redundant (its maximum activity of 5
        // exceeds the bound of 4) and does not pin `x0` by propagation either, since
        // that only gives `x0 <= 1`. Without this reduction nothing fixes it, which is
        // what makes the test able to fail.
        let p = problem(
            &[1.0, 1.0, 1.0],
            &[
                (&[3.0, 2.0, 0.0], RowSense::Le, 4.0),
                (&[0.0, 1.0, 1.0], RowSense::Ge, 1.0),
            ],
        );
        let stats = assert_presolve_is_sound(&p, "singleton fixing");
        assert!(stats.fixed_columns >= 1);

        let mut after = p.clone();
        presolve(&mut after, 20);
        assert_eq!(
            (after.col_lb[0], after.col_ub[0]),
            (0.0, 0.0),
            "the singleton column should be pinned at its lower bound"
        );
        // `x2` is a singleton too, but in a row whose lower bound is exactly what it
        // may be needed for, so it must survive.
        assert!(
            after.col_lb[2] < after.col_ub[2],
            "a singleton its row may still need must not be pinned"
        );
    }

    #[test]
    fn leaves_a_singleton_column_its_row_still_needs() {
        // Same shape, but the row now has a lower bound that only `x0` can satisfy, so
        // pushing it down is not safe and the reduction must decline.
        let p = problem(
            &[1.0, 1.0],
            &[
                (&[1.0, 0.0], RowSense::Ge, 1.0),
                (&[0.0, 1.0], RowSense::Le, 5.0),
            ],
        );
        assert_presolve_is_sound(&p, "singleton kept");

        let mut after = p.clone();
        presolve(&mut after, 20);
        assert_eq!(
            (after.col_lb[0], after.col_ub[0]),
            (1.0, 1.0),
            "propagation should pin it at one, not at zero"
        );
    }

    #[test]
    fn tightens_an_oversized_coefficient_on_a_ge_row() {
        // `5x0 + 2x1 + 2x2 + 2x3 >= 3`: x0 alone already satisfies the row, so 5 is
        // more than the row can use and reduces to 3. The binary solutions are
        // unchanged; the relaxation is not, since x0 = 0.6 was feasible and is not now.
        //
        // The spare columns are load-bearing: with fewer of them, bound propagation
        // fixes x0 = 1 outright before tightening gets a chance, a stronger
        // reduction, but not the one under test.
        let p = problem(
            &[1.0, 1.0, 1.0, 1.0],
            &[(&[5.0, 2.0, 2.0, 2.0], RowSense::Ge, 3.0)],
        );
        let stats = assert_presolve_is_sound(&p, "ge tightening");
        assert_eq!(stats.tightened_coefficients, 1);
        assert_eq!(
            stats.fixed_columns, 0,
            "propagation should not have decided anything"
        );

        let mut after = p.clone();
        presolve(&mut after, 20);
        assert!(
            (coefficient(&after, 0, 0) - 3.0).abs() < 1e-9,
            "{}",
            coefficient(&after, 0, 0)
        );
        assert!(
            (after.row_lb[0] - 3.0).abs() < 1e-9,
            "row bound moved: {}",
            after.row_lb[0]
        );
        assert_tightening_preserves_feasible_set(&p, "ge tightening");
    }

    #[test]
    fn tightens_an_oversized_coefficient_on_a_le_row() {
        // `5x0 + 2x1 + 2x2 + 2x3 <= 8`: the row can only ever be violated by 3 above
        // its bound, so x0's coefficient reduces to 3 and the bound follows to 6.
        // The second row is what keeps the columns out of reach of singleton fixing.
        // Without it every column appears once, the objective prefers zero, and the
        // model is solved outright before tightening can be observed.
        let p = problem(
            &[1.0, 1.0, 1.0, 1.0],
            &[
                (&[5.0, 2.0, 2.0, 2.0], RowSense::Le, 8.0),
                (&[1.0, 1.0, 1.0, 1.0], RowSense::Ge, 1.0),
            ],
        );
        let stats = assert_presolve_is_sound(&p, "le tightening");
        assert_eq!(stats.tightened_coefficients, 1);

        let mut after = p.clone();
        presolve(&mut after, 20);
        assert!(
            (coefficient(&after, 0, 0) - 3.0).abs() < 1e-9,
            "{}",
            coefficient(&after, 0, 0)
        );
        assert!(
            (after.row_ub[0] - 6.0).abs() < 1e-9,
            "row bound is {}",
            after.row_ub[0]
        );
        assert_tightening_preserves_feasible_set(&p, "le tightening");
    }

    #[test]
    fn tightening_strengthens_the_relaxation() {
        // The point of the reduction: the same integer solutions, a higher LP bound.
        use crate::lp::{Lp, LpStatus};
        let p = problem(
            &[1.0, 1.0, 1.0, 1.0],
            &[(&[5.0, 2.0, 2.0, 2.0], RowSense::Ge, 3.0)],
        );
        let before = Lp::relaxation(&p).solve();
        let mut after = p.clone();
        presolve(&mut after, 20);
        let reduced = Lp::relaxation(&after).solve();

        assert_eq!(before.status, LpStatus::Optimal);
        assert_eq!(reduced.status, LpStatus::Optimal);
        assert!(
            reduced.objective > before.objective + 1e-9,
            "relaxation did not improve: {} -> {}",
            before.objective,
            reduced.objective
        );
    }

    #[test]
    fn tightening_handles_negative_coefficients() {
        for sense in [RowSense::Ge, RowSense::Le] {
            for rhs in [-4.0, -1.0, 0.0, 1.0, 3.0] {
                let p = problem(&[1.0, 2.0, 1.0], &[(&[-6.0, 3.0, -2.0], sense, rhs)]);
                assert_presolve_is_sound(&p, &format!("{sense:?} rhs {rhs}"));
            }
        }
    }

    /// `neos-633273`'s shape: `x <= y + w` where `x` and `w` share a partitioning row, so
    /// `w` can never be what satisfies it and the row is really `x <= y`. Bounds alone
    /// see nothing to tighten; the conflict graph does.
    #[test]
    fn conflicts_tighten_a_coefficient_bounds_cannot() {
        // x0 = x, x1 = y, x2 = w, x3 = the partition's third member.
        let p = problem(
            &[-3.0, 2.0, 0.0, 1.0],
            &[
                (&[1.0, -1.0, -1.0, 0.0], RowSense::Le, 0.0),
                (&[1.0, 0.0, 1.0, 1.0], RowSense::Eq, 1.0),
            ],
        );
        assert_presolve_is_sound(&p, "conflict tightening");

        let mut after = p.clone();
        let Outcome::Reduced(stats) = presolve(&mut after, 20) else {
            panic!("presolve rejected a feasible model");
        };
        assert!(stats.tightened_coefficients >= 1);
        assert_eq!(coefficient(&after, 0, 2), 0.0, "w should leave the row");
        assert_eq!(coefficient(&after, 0, 1), -1.0, "y should keep its coefficient");

        use crate::lp::Lp;
        let (before, reduced) = (Lp::relaxation(&p).solve(), Lp::relaxation(&after).solve());
        assert!(
            reduced.objective > before.objective + 1e-9,
            "relaxation did not improve: {} -> {}",
            before.objective,
            reduced.objective
        );
    }

    /// The same reasoning on the positive side, and with every row sense, checked
    /// against enumeration: the conflict pass may never change the integer points.
    #[test]
    fn conflict_tightening_is_sound_on_every_sense() {
        for sense in [RowSense::Ge, RowSense::Le] {
            for rhs in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                for signs in [[1.0, -1.0, -1.0], [2.0, 1.0, -3.0], [-2.0, 3.0, 1.0]] {
                    let p = problem(
                        &[1.0, -2.0, 1.0, -1.0],
                        &[
                            (&[signs[0], signs[1], signs[2], 0.0], sense, rhs),
                            (&[1.0, 0.0, 1.0, 1.0], RowSense::Le, 1.0),
                            (&[0.0, 1.0, 0.0, 1.0], RowSense::Le, 1.0),
                        ],
                    );
                    assert_presolve_is_sound(&p, &format!("{sense:?} {rhs} {signs:?}"));
                }
            }
        }
    }

    #[test]
    fn detects_an_infeasible_row() {
        // Maximum activity is 3, so the row can never reach 5.
        let p = problem(&[1.0, 1.0], &[(&[2.0, 1.0], RowSense::Ge, 5.0)]);
        let mut after = p.clone();
        assert_eq!(presolve(&mut after, 20), Outcome::Infeasible);
    }

    #[test]
    fn removes_a_redundant_row() {
        // Maximum activity is 3, so `<= 10` can never bind.
        let p = problem(&[1.0, 1.0], &[(&[2.0, 1.0], RowSense::Le, 10.0)]);
        let stats = assert_presolve_is_sound(&p, "redundant");
        assert_eq!(stats.redundant_rows, 1);
    }

    #[test]
    fn a_forcing_row_pins_every_column() {
        // Only x0 = x1 = 1 reaches 3, so both are forced.
        let p = problem(&[1.0, 1.0], &[(&[2.0, 1.0], RowSense::Ge, 3.0)]);
        let mut after = p.clone();
        assert!(matches!(presolve(&mut after, 20), Outcome::Reduced(_)));
        assert_eq!((after.col_lb[0], after.col_ub[0]), (1.0, 1.0));
        assert_eq!((after.col_lb[1], after.col_ub[1]), (1.0, 1.0));
        assert_presolve_is_sound(&p, "forcing");
    }

    #[test]
    fn a_singleton_row_fixes_its_column() {
        let p = problem(&[1.0, 1.0], &[(&[3.0, 0.0], RowSense::Ge, 2.0)]);
        let mut after = p.clone();
        assert!(matches!(presolve(&mut after, 20), Outcome::Reduced(_)));
        assert_eq!((after.col_lb[0], after.col_ub[0]), (1.0, 1.0));
        assert_presolve_is_sound(&p, "singleton");
    }

    #[test]
    fn a_column_in_no_row_follows_its_cost() {
        // x1 appears nowhere, so cost decides: positive goes to 0, negative to 1.
        let p = problem(&[1.0, 4.0], &[(&[1.0, 0.0], RowSense::Ge, 1.0)]);
        let mut after = p.clone();
        presolve(&mut after, 20);
        assert_eq!((after.col_lb[1], after.col_ub[1]), (0.0, 0.0));

        let p = problem(&[1.0, -4.0], &[(&[1.0, 0.0], RowSense::Ge, 1.0)]);
        let mut after = p.clone();
        presolve(&mut after, 20);
        assert_eq!((after.col_lb[1], after.col_ub[1]), (1.0, 1.0));
    }

    #[test]
    fn presolve_is_idempotent() {
        // A second pass over an already-reduced model must find nothing new.
        let spec = Spec {
            kind: Kind::Knapsack,
            n_cols: 14,
            n_rows: 7,
            seed: 4,
        };
        let mut p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
        presolve(&mut p, 20);
        let once = p.clone();
        let stats = presolve(&mut p, 20);
        if let Outcome::Reduced(stats) = stats {
            assert_eq!(stats.fixed_columns, 0);
            assert_eq!(stats.tightened_coefficients, 0);
            assert_eq!(stats.redundant_rows, 0);
        }
        assert_eq!(once.col_lb, p.col_lb);
        assert_eq!(once.col_ub, p.col_ub);
        assert_eq!(once.matrix, p.matrix);
    }
}
