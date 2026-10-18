//! Domain propagation at a node.
//!
//! Until this existed, a node's only inference was its LP relaxation: `Worker::process`
//! applied the branch's bound changes and solved. Everything the model *says* about those
//! bounds -- that a row cannot reach its right-hand side any more, that a column set to
//! one forbids every column it conflicts with -- was left for the simplex to rediscover
//! as arithmetic, or not discovered at all.
//!
//! That is the gap the other solvers' own statistics point at. On MIPLIB's
//! `chromaticindex32-8` a reference solver reduces the model by nothing, separates no
//! cuts, and closes it in 0.71 seconds where this search spends sixty and 3959 nodes with
//! the optimum already in hand. What it carries that this did not is a clique table it
//! propagates at every node -- and the conflict graph here is the same size, 12642 edges
//! against its 8438 entries. The information was already built and only ever used to
//! separate cuts, seed heuristics and probe at the root.
//!
//! Two rules, run together to a fixpoint:
//!
//! - **Conflicts.** A binary fixed to a value excludes every literal the conflict graph
//!   joins it to, which forces the columns behind those literals to their other value.
//!   One clique of a set partitioning row decides its whole row from a single branch.
//! - **Row activity.** A row's least and greatest attainable values, given the bounds in
//!   force, bound every column in it; on an integer column the implied bound rounds
//!   inwards, which is what turns a fractional implication into a fixing.
//!
//! Both are ordinary and neither is interesting on its own. What matters is that they run
//! *per node* against bounds the branch has just changed, and that they are bounded: this
//! runs on every node of the tree, so it is given a work budget in matrix entries read and
//! stops when it is spent rather than when it is finished. Stopping early is sound -- the
//! bounds it has proved are still implied, it has simply not proved all of them.

use crate::cuts::{Conflicts, literal};
use crate::model::{Problem, VarType};
use crate::sparse::SparseMatrix;

/// The node's bounds cannot all hold at once, so nothing below it can be feasible.
///
/// Carries where the contradiction was found, which is where [`explain`] starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Infeasible {
    source: Source,
}

/// Scratch reused across nodes, so propagating costs no allocation.
///
/// A worker keeps one of these for the life of the search. The queue and the flag vector
/// are cleared per call rather than rebuilt.
pub struct Scratch {
    /// Columns whose bounds have moved and whose consequences are not drawn yet.
    queue: Vec<u32>,
    /// Whether a column is already waiting, so it is queued once however often it moves.
    queued: Vec<bool>,
    /// Why each column's bounds are where they are, for [`explain`]. Only meaningful for
    /// a column this call actually moved, or seeded.
    reason: Vec<Option<Reason>>,
    /// Columns whose `reason` this call wrote, so the next one can clear just those.
    touched: Vec<u32>,
    /// Working stack for [`explain`], kept here so explaining allocates nothing.
    stack: Vec<u32>,
    seen: Vec<bool>,
    seen_list: Vec<u32>,
}

impl Scratch {
    pub fn new(n_cols: usize) -> Self {
        Scratch {
            queue: Vec::new(),
            queued: vec![false; n_cols],
            reason: vec![None; n_cols],
            touched: Vec::new(),
            stack: Vec::new(),
            seen: vec![false; n_cols],
            seen_list: Vec::new(),
        }
    }

    /// Forget every reason recorded, starting a fresh explanation graph.
    ///
    /// A node calls this before propagating, so its conflict is explained by its own
    /// branch and nothing left over. A dive calls it once and then propagates level after
    /// level, because a conflict twenty decisions deep has to be explainable through all
    /// twenty; see [`rapid_learn`].
    pub fn begin(&mut self) {
        for &j in &self.touched {
            self.reason[j as usize] = None;
        }
        self.touched.clear();
    }

    /// Forget the reason for one column, which a dive does when it backtracks past the
    /// decision that set it.
    fn forget(&mut self, j: usize) {
        self.reason[j] = None;
    }

    fn set_reason(&mut self, j: usize, why: Reason) {
        if self.reason[j].is_none() {
            self.touched.push(j as u32);
        }
        self.reason[j] = Some(why);
    }
}

/// Why a column's bounds are where they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    /// A bound this node's branching chose. A clause may rest on it: the branch is a
    /// statement about where in the tree we are, not about which points are feasible,
    /// so "these branchings cannot all hold" follows from the model alone.
    Decision,
    /// A bound the incumbent earned, from reduced cost fixing. **A clause may not rest
    /// on one.** It is only valid while that incumbent stands, so a conflict explained
    /// through it is not a property of the model and must not be learned.
    Assumption,
    /// Forced because the named column was fixed and the two conflict.
    Conflict(u32),
    /// Forced by the named row's activity under the bounds then in force.
    Row(u32),
    /// Forced by the named learned clause, every other literal of which had failed.
    Clause(u32),
    /// Moved, but not to a single value, so what justifies its present bounds is not one
    /// implication. Nothing is learned through one of these; on a binary model they do
    /// not arise, because any bound change on a binary fixes it.
    Opaque,
}

/// What a seeded bound change is: see [`Reason`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seed {
    Decision,
    Assumption,
}

/// Where propagation ran out of room, which is where an explanation starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    /// A row whose activity cannot meet its bounds.
    Row(u32),
    /// Two columns the conflict graph says cannot both hold as they are fixed.
    Pair(u32, u32),
    /// A learned clause with every literal failed.
    Clause(u32),
}

/// Is `j` a binary column under the bounds it started with?
fn is_binary(problem: &Problem, j: usize) -> bool {
    problem.col_type[j] == VarType::Integer
        && problem.col_lb[j] >= -0.5
        && problem.col_ub[j] <= 1.5
}

/// Tighten `lb` and `ub` to a fixpoint from the columns in `seeds`.
///
/// `lb` and `ub` are this node's bounds, already carrying the branch. They are tightened
/// in place. Returns how many columns became fixed, which is what the caller reports;
/// `Err(Infeasible)` means the node is empty and needs no LP at all.
///
/// `budget` is in matrix entries read. It bounds the whole call, not one round.
pub fn propagate(
    problem: &Problem,
    rows_of_a: &SparseMatrix,
    conflicts: &Conflicts,
    clauses: &Clauses,
    scratch: &mut Scratch,
    lb: &mut [f64],
    ub: &mut [f64],
    seeds: &[(u32, Seed)],
    budget: usize,
    tolerance: f64,
) -> Result<usize, Infeasible> {
    scratch.queue.clear();
    for &(j, seed) in seeds {
        scratch.set_reason(
            j as usize,
            match seed {
                Seed::Decision => Reason::Decision,
                Seed::Assumption => Reason::Assumption,
            },
        );
        if !scratch.queued[j as usize] {
            scratch.queued[j as usize] = true;
            scratch.queue.push(j);
        }
    }
    let mut fixed = 0usize;
    let mut work = 0usize;
    let mut head = 0usize;

    // A local closure cannot hold `scratch` and the bounds at once, so pushing is written
    // out where it is needed rather than shared.
    while head < scratch.queue.len() {
        if work >= budget {
            break;
        }
        let j = scratch.queue[head] as usize;
        head += 1;
        scratch.queued[j] = false;

        if lb[j] > ub[j] + tolerance {
            return Err(Infeasible { source: Source::Pair(j as u32, j as u32) });
        }

        // Conflicts, when this column is fixed and binary.
        if is_binary(problem, j) && (ub[j] - lb[j]).abs() <= tolerance {
            let value = lb[j] > 0.5;
            let node = literal(j, value);
            let mut contradiction = None;
            let mut forced_columns: Vec<(usize, f64)> = Vec::new();
            conflicts.for_each_adjacent(node, |excluded| {
                let k = (excluded / 2) as usize;
                // An even literal is "column k is one"; being excluded forces it to zero.
                let forced = if excluded.is_multiple_of(2) { 0.0 } else { 1.0 };
                if forced > ub[k] + tolerance || forced < lb[k] - tolerance {
                    contradiction = Some(k);
                    return false;
                }
                if (ub[k] - lb[k]).abs() > tolerance {
                    lb[k] = forced;
                    ub[k] = forced;
                    fixed += 1;
                    forced_columns.push((k, forced));
                }
                true
            });
            if let Some(k) = contradiction {
                return Err(Infeasible { source: Source::Pair(j as u32, k as u32) });
            }
            for (k, _) in forced_columns {
                scratch.set_reason(k, Reason::Conflict(j as u32));
                if !scratch.queued[k] {
                    scratch.queued[k] = true;
                    scratch.queue.push(k as u32);
                }
            }
            work += conflicts.degree(node);
        }

        // The clauses this column appears in. A clause with every literal but one
        // failed forces that one; with every literal failed it is the conflict itself.
        if !clauses.is_empty() {
            for &id in &clauses.mentions[j] {
                let clause = &clauses.clauses[id as usize];
                work += clause.len();
                let mut unassigned = None;
                let mut satisfied = false;
                for &(k, want) in clause {
                    let k = k as usize;
                    let fixed_here = (ub[k] - lb[k]).abs() <= tolerance;
                    if fixed_here {
                        if (lb[k] > 0.5) == want {
                            satisfied = true;
                            break;
                        }
                    } else if unassigned.is_some() {
                        unassigned = Some(usize::MAX); // more than one: nothing to force
                        break;
                    } else {
                        unassigned = Some(k);
                    }
                }
                if satisfied {
                    continue;
                }
                match unassigned {
                    None => return Err(Infeasible { source: Source::Clause(id) }),
                    Some(usize::MAX) => {}
                    Some(k) => {
                        let want = clause
                            .iter()
                            .find(|&&(c, _)| c as usize == k)
                            .map(|&(_, w)| w)
                            .unwrap_or(false);
                        let forced = if want { 1.0 } else { 0.0 };
                        if forced > ub[k] + tolerance || forced < lb[k] - tolerance {
                            return Err(Infeasible { source: Source::Clause(id) });
                        }
                        lb[k] = forced;
                        ub[k] = forced;
                        fixed += 1;
                        scratch.set_reason(k, Reason::Clause(id));
                        if !scratch.queued[k] {
                            scratch.queued[k] = true;
                            scratch.queue.push(k as u32);
                        }
                    }
                }
            }
        }

        // Row activity, over the rows this column appears in.
        let (rows, _) = problem.matrix.column(j);
        for &i in rows {
            if work >= budget {
                break;
            }
            let (columns, values) = rows_of_a.column(i);
            work += columns.len();
            let (row_lb, row_ub) = (problem.row_lb[i], problem.row_ub[i]);
            if !row_lb.is_finite() && !row_ub.is_finite() {
                continue;
            }

            // Least and greatest the row can reach under the bounds in force. A column
            // with an infinite bound makes one of them infinite, and an infinite
            // activity implies nothing about anything, so those rows are left alone.
            let (mut min_activity, mut max_activity) = (0.0f64, 0.0f64);
            let mut finite = true;
            for (&k, &a) in columns.iter().zip(values) {
                let (low, high) = if a >= 0.0 {
                    (a * lb[k], a * ub[k])
                } else {
                    (a * ub[k], a * lb[k])
                };
                if !low.is_finite() || !high.is_finite() {
                    finite = false;
                    break;
                }
                min_activity += low;
                max_activity += high;
            }
            if !finite {
                continue;
            }
            if min_activity > row_ub + tolerance || max_activity < row_lb - tolerance {
                return Err(Infeasible { source: Source::Row(i as u32) });
            }

            for (&k, &a) in columns.iter().zip(values) {
                if a == 0.0 {
                    continue;
                }
                let (low, high) = if a >= 0.0 {
                    (a * lb[k], a * ub[k])
                } else {
                    (a * ub[k], a * lb[k])
                };
                // What the rest of the row leaves for this term.
                let head_room = row_ub - (min_activity - low);
                let floor_room = row_lb - (max_activity - high);
                let (mut new_lb, mut new_ub) = (lb[k], ub[k]);
                if a > 0.0 {
                    if row_ub.is_finite() {
                        new_ub = new_ub.min(head_room / a);
                    }
                    if row_lb.is_finite() {
                        new_lb = new_lb.max(floor_room / a);
                    }
                } else {
                    if row_ub.is_finite() {
                        new_lb = new_lb.max(head_room / a);
                    }
                    if row_lb.is_finite() {
                        new_ub = new_ub.min(floor_room / a);
                    }
                }
                if problem.col_type[k] == VarType::Integer {
                    new_lb = (new_lb - tolerance).ceil();
                    new_ub = (new_ub + tolerance).floor();
                }
                if new_lb > new_ub + tolerance {
                    return Err(Infeasible { source: Source::Row(i as u32) });
                }
                // Only a real move is worth queueing; a bound that repeats itself would
                // otherwise cycle the worklist forever on a degenerate row.
                let moved = new_lb > lb[k] + tolerance || new_ub < ub[k] - tolerance;
                if moved {
                    let was_free = (ub[k] - lb[k]).abs() > tolerance;
                    lb[k] = lb[k].max(new_lb);
                    ub[k] = ub[k].min(new_ub);
                    let now_fixed = (ub[k] - lb[k]).abs() <= tolerance;
                    if was_free && now_fixed {
                        fixed += 1;
                    }
                    // Only a fixing has one implication behind it; a partial tightening
                    // does not, and nothing is learned through it. See `Reason::Opaque`.
                    scratch.set_reason(k, if now_fixed { Reason::Row(i as u32) } else { Reason::Opaque });
                    if !scratch.queued[k] {
                        scratch.queued[k] = true;
                        scratch.queue.push(k as u32);
                    }
                }
            }
        }
    }

    // Whatever is still waiting is simply not drawn; clear the flags for the next call.
    for &j in &scratch.queue[head..] {
        scratch.queued[j as usize] = false;
    }
    Ok(fixed)
}

/// A learned clause: the conflict holds only if *every* one of these fails, so at least
/// one must hold in any feasible point. `(column, value)` reads "column takes value".
pub type Clause = Vec<(u32, bool)>;

/// Clauses learned from conflicts, and which columns each mentions.
///
/// Kept per worker rather than shared. A shared pool would let one thread's conflict
/// prune another's subtree, which is worth having, but it puts a lock on the propagation
/// path that every node runs; per-worker costs nothing and still pays, because a worker
/// takes nodes from all over the tree. Sharing is the obvious next step and is not this.
pub struct Clauses {
    clauses: Vec<Clause>,
    /// For each column, the clauses that mention it.
    mentions: Vec<Vec<u32>>,
    /// Most clauses to keep, and the longest one worth keeping. A long clause forbids
    /// little and costs a scan at every node that touches any of its columns.
    capacity: usize,
    max_length: usize,
}

impl Clauses {
    pub fn new(n_cols: usize, capacity: usize, max_length: usize) -> Self {
        Clauses {
            clauses: Vec::new(),
            mentions: vec![Vec::new(); n_cols],
            capacity,
            max_length,
        }
    }

    /// Keep `clause`, unless it is too long or the pool is full. Returns whether it was.
    pub fn add(&mut self, clause: Clause) -> bool {
        if clause.is_empty() || clause.len() > self.max_length || self.clauses.len() >= self.capacity
        {
            return false;
        }
        let id = self.clauses.len() as u32;
        for &(j, _) in &clause {
            self.mentions[j as usize].push(id);
        }
        self.clauses.push(clause);
        true
    }

    pub fn len(&self) -> usize {
        self.clauses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }
}

/// A pool holding nothing, for callers that do not learn.
impl Default for Clauses {
    fn default() -> Self {
        Clauses::new(0, 0, 0)
    }
}

/// Why the node was empty, expressed over the branchings that made it so.
///
/// Walks the implication graph back from where propagation broke, through the rows and
/// conflicts that forced each bound, until nothing is left but the branch's own choices.
/// Those choices are then a *sufficient* reason for the contradiction, and since every
/// step of the walk is an implication of the model itself, no feasible point of the model
/// satisfies all of them. Negating them is therefore a globally valid clause.
///
/// Returns `None` when nothing may be learned, which is the common case and must stay
/// cheap. That happens when the reason reaches a bound the incumbent earned rather than
/// one the branch chose ([`Reason::Assumption`]), because such a bound is not a property
/// of the model and a clause resting on it would be wrong the moment the incumbent moved;
/// when it reaches a tightening that is not a fixing ([`Reason::Opaque`]); or when a
/// column's bounds differ from the root's with nothing recorded to say why.
pub fn explain(
    problem: &Problem,
    rows_of_a: &SparseMatrix,
    clauses: &Clauses,
    scratch: &mut Scratch,
    conflict: Infeasible,
    lb: &[f64],
    ub: &[f64],
    budget: usize,
    tolerance: f64,
) -> Option<Clause> {
    // A column whose bounds are the root's took no part in anything.
    let moved = |k: usize| {
        (lb[k] - problem.col_lb[k]).abs() > tolerance || (ub[k] - problem.col_ub[k]).abs() > tolerance
    };

    scratch.stack.clear();
    for &j in &scratch.seen_list {
        scratch.seen[j as usize] = false;
    }
    scratch.seen_list.clear();

    let push = |scratch: &mut Scratch, k: usize| {
        if !scratch.seen[k] {
            scratch.seen[k] = true;
            scratch.seen_list.push(k as u32);
            scratch.stack.push(k as u32);
        }
    };

    match conflict.source {
        Source::Pair(a, b) => {
            push(scratch, a as usize);
            push(scratch, b as usize);
        }
        Source::Row(i) => {
            let (columns, _) = rows_of_a.column(i as usize);
            for &k in columns {
                if moved(k) {
                    push(scratch, k);
                }
            }
        }
        Source::Clause(id) => {
            for &(k, _) in &clauses.clauses[id as usize] {
                if moved(k as usize) {
                    push(scratch, k as usize);
                }
            }
        }
    }

    walk_to_decisions(problem, rows_of_a, clauses, scratch, lb, budget, moved)
}

/// Walk whatever is on `scratch.stack` back to the branchings behind it.
///
/// Shared by both explainers: one seeds the stack from where propagation broke, the other
/// from the bounds a Farkas certificate says it needs. Returns `None` on anything that
/// cannot be rested on -- see [`explain`].
fn walk_to_decisions(
    problem: &Problem,
    rows_of_a: &SparseMatrix,
    clauses: &Clauses,
    scratch: &mut Scratch,
    lb: &[f64],
    budget: usize,
    moved: impl Fn(usize) -> bool,
) -> Option<Clause> {
    let push = |scratch: &mut Scratch, k: usize| {
        if !scratch.seen[k] {
            scratch.seen[k] = true;
            scratch.seen_list.push(k as u32);
            scratch.stack.push(k as u32);
        }
    };
    let mut clause: Clause = Vec::new();
    let mut work = 0usize;
    while let Some(k) = scratch.stack.pop() {
        work += 1;
        if work > budget {
            return None;
        }
        let k = k as usize;
        match scratch.reason[k] {
            Some(Reason::Decision) => {
                // The branch put this column where it is. Any feasible point that agrees
                // with the rest of the branch must differ here, so the clause asks for
                // the other value.
                if !is_binary(problem, k) {
                    return None;
                }
                let held = lb[k] > 0.5;
                clause.push((k as u32, !held));
            }
            // Not a property of the model; see this function's note.
            Some(Reason::Assumption) | Some(Reason::Opaque) | None => return None,
            Some(Reason::Conflict(from)) => push(scratch, from as usize),
            Some(Reason::Row(i)) => {
                let (columns, _) = rows_of_a.column(i as usize);
                for &other in columns {
                    if other != k && moved(other) {
                        push(scratch, other);
                    }
                }
            }
            Some(Reason::Clause(id)) => {
                for &(other, _) in &clauses.clauses[id as usize] {
                    if other as usize != k && moved(other as usize) {
                        push(scratch, other as usize);
                    }
                }
            }
        }
    }

    // A conflict explained by nothing is a model that is infeasible outright, which is
    // not this function's business to report and not a clause.
    if clause.is_empty() {
        return None;
    }
    clause.sort_unstable();
    clause.dedup();
    Some(clause)
}

/// Turn an infeasible node LP's Farkas certificate into a clause.
///
/// The dual simplex proves infeasibility by running out of room on one row, and the
/// multipliers `B^-T e_r` aggregate the model into a single linear form `c` with a bound
/// `L` that every point of the model must satisfy -- while no point of *this node's box*
/// can. That is a proof, and unlike a propagation conflict it is exactly what an LP-driven
/// search produces in quantity: the LP is what proves a node empty here, not propagation.
///
/// **The certificate is verified, not trusted.** The sign convention is settled by trying
/// both and keeping whichever actually proves infeasibility, and if neither does, nothing
/// is learned. Resting a globally-obeyed clause on an unchecked dual ray is how a solver
/// deletes its own optimum, and the ray comes from floating point arithmetic on a basis
/// that may have drifted.
///
/// With a verified certificate, the bounds are relaxed one at a time back to the root's:
/// any whose relaxation keeps the form unsatisfiable was not needed, and what remains is
/// the reason. Those are then walked back to the branchings behind them, exactly as a
/// propagation conflict is.
#[allow(clippy::too_many_arguments)]
pub fn farkas_conflict(
    problem: &Problem,
    rows_of_a: &SparseMatrix,
    clauses: &Clauses,
    scratch: &mut Scratch,
    ray: &[f64],
    lb: &[f64],
    ub: &[f64],
    budget: usize,
    tolerance: f64,
) -> Option<Clause> {
    let n = problem.n_cols();
    let m = problem.n_rows();
    if ray.len() < m {
        return None;
    }

    // Aggregate the rows the ray touches, in whichever sign proves anything.
    let mut aggregate = vec![0.0f64; n];
    let mut work = 0usize;
    for sign in [1.0f64, -1.0] {
        aggregate.iter_mut().for_each(|c| *c = 0.0);
        let mut required = 0.0f64;
        let mut usable = true;
        for i in 0..m {
            let y = sign * ray[i];
            if y.abs() <= tolerance {
                continue;
            }
            // The row bound this multiplier leans on. Without it the aggregation says
            // nothing, so a ray needing an infinite bound is not a certificate.
            let bound = if y > 0.0 {
                problem.row_lb[i]
            } else {
                problem.row_ub[i]
            };
            if !bound.is_finite() {
                usable = false;
                break;
            }
            required += y * bound;
            let (columns, values) = rows_of_a.column(i);
            work += columns.len();
            if work > budget {
                return None;
            }
            for (&j, &a) in columns.iter().zip(values) {
                aggregate[j] += y * a;
            }
        }
        if !usable {
            continue;
        }

        // The most the aggregate can reach inside this node's box. If that falls short of
        // what every feasible point must reach, the box holds no feasible point.
        let reach = |lo: &[f64], hi: &[f64]| -> f64 {
            let mut total = 0.0;
            for (j, &c) in aggregate.iter().enumerate() {
                if c == 0.0 {
                    continue;
                }
                total += if c > 0.0 { c * hi[j] } else { c * lo[j] };
            }
            total
        };
        if !(reach(lb, ub) < required - tolerance) {
            continue;
        }

        // Verified. Now find which of this node's bound changes the proof actually needs,
        // by putting each back and seeing whether the proof survives.
        let mut relaxed_lb = lb.to_vec();
        let mut relaxed_ub = ub.to_vec();
        let mut needed: Vec<u32> = Vec::new();
        for j in 0..n {
            let tightened = (lb[j] - problem.col_lb[j]).abs() > tolerance
                || (ub[j] - problem.col_ub[j]).abs() > tolerance;
            if !tightened || aggregate[j] == 0.0 {
                continue;
            }
            let (held_lb, held_ub) = (relaxed_lb[j], relaxed_ub[j]);
            relaxed_lb[j] = problem.col_lb[j];
            relaxed_ub[j] = problem.col_ub[j];
            if !(reach(&relaxed_lb, &relaxed_ub) < required - tolerance) {
                // The proof needs this one; put it back.
                relaxed_lb[j] = held_lb;
                relaxed_ub[j] = held_ub;
                needed.push(j as u32);
            }
        }
        if needed.is_empty() {
            // The model is infeasible at the root, which is not a clause.
            return None;
        }

        scratch.stack.clear();
        for &j in &scratch.seen_list {
            scratch.seen[j as usize] = false;
        }
        scratch.seen_list.clear();
        for &j in &needed {
            let j = j as usize;
            if !scratch.seen[j] {
                scratch.seen[j] = true;
                scratch.seen_list.push(j as u32);
                scratch.stack.push(j as u32);
            }
        }
        let moved = |k: usize| {
            (lb[k] - problem.col_lb[k]).abs() > tolerance
                || (ub[k] - problem.col_ub[k]).abs() > tolerance
        };
        return walk_to_decisions(problem, rows_of_a, clauses, scratch, lb, budget, moved);
    }
    None
}

/// Probe each binary at the root and keep the implications as clauses.
///
/// This is the source the analyser above was missing. A clause learned from a node the
/// search has already pruned only stops that conflict being rediscovered; a reference
/// solver closes `chromaticindex32-8` in **one node**, so its conflicts are not harvested
/// from a tree at all -- they are manufactured at the root, before any branching, by
/// supposing a value and seeing what follows.
///
/// What follows is an *implication*: supposing `x_j = v` forces `x_k = w`. That is a
/// two-literal clause, "either `x_j` is not `v`, or `x_k` is `w`", and it holds for every
/// feasible point of the model because every step deriving it was one of the model's own.
/// Kept and propagated at every node, it says something the rows alone never state and the
/// conflict graph never held.
///
/// Bounded three ways, because this runs before the search gets any time at all: total
/// work across every probe, work within one probe, and how many clauses are worth keeping.
/// Implications the conflict graph already carries are skipped -- propagation gets those
/// from the graph, and a duplicate costs a scan for nothing.
///
/// The refuted case -- a supposition that propagation contradicts, proving the column
/// takes its other value -- is deliberately not collected here. `presolve::probe_columns`
/// already finds those and fixes the column outright, which is strictly better than a
/// one-literal clause.
#[allow(clippy::too_many_arguments)]
pub fn probe_implications(
    problem: &Problem,
    rows_of_a: &SparseMatrix,
    conflicts: &Conflicts,
    scratch: &mut Scratch,
    total_budget: usize,
    per_probe_budget: usize,
    max_clauses: usize,
    tolerance: f64,
) -> Vec<Clause> {
    let n = problem.n_cols();
    let mut out: Vec<Clause> = Vec::new();
    let mut spent = 0usize;
    let mut lb = problem.col_lb.clone();
    let mut ub = problem.col_ub.clone();

    for j in 0..n {
        if out.len() >= max_clauses || spent >= total_budget {
            break;
        }
        if !is_binary(problem, j) || (problem.col_ub[j] - problem.col_lb[j]).abs() <= tolerance {
            continue;
        }
        // A column joined to nothing implies nothing through the graph, and a probe on it
        // has only the rows to work with. Those are worth probing too, but the ones that
        // pay are the connected ones, and the budget is better spent there first.
        if !conflicts.has_conflicts(literal(j, true)) && !conflicts.has_conflicts(literal(j, false))
        {
            continue;
        }

        for value in [false, true] {
            if out.len() >= max_clauses || spent >= total_budget {
                break;
            }
            lb.copy_from_slice(&problem.col_lb);
            ub.copy_from_slice(&problem.col_ub);
            let supposed = if value { 1.0 } else { 0.0 };
            lb[j] = supposed;
            ub[j] = supposed;
            scratch.begin();
            // Charged the whole probe budget whether or not the probe used it, which
            // over-counts and is deliberate: `propagate` does not report the work it did,
            // and a bound that errs towards stopping early is the safe direction for
            // something running before the search gets any time at all.
            spent = spent.saturating_add(per_probe_budget);
            let outcome = propagate(
                problem,
                rows_of_a,
                conflicts,
                &Clauses::default(),
                scratch,
                &mut lb,
                &mut ub,
                &[(j as u32, Seed::Decision)],
                per_probe_budget,
                tolerance,
            );
            // Refuted: presolve's probing already turns this into a fixing. See the note.
            if outcome.is_err() {
                continue;
            }
            for k in 0..n {
                if k == j || out.len() >= max_clauses {
                    continue;
                }
                if !is_binary(problem, k) || (ub[k] - lb[k]).abs() > tolerance {
                    continue;
                }
                // Fixed by the probe rather than by the model.
                if (problem.col_ub[k] - problem.col_lb[k]).abs() <= tolerance {
                    continue;
                }
                let forced = lb[k] > 0.5;
                // "x_j = value" and "x_k = !forced" cannot both hold. If the graph says
                // that already, propagation has it and the clause is dead weight.
                if conflicts.conflicts(literal(j, value), literal(k, !forced)) {
                    continue;
                }
                out.push(vec![(j as u32, !value), (k as u32, forced)]);
            }
        }
    }
    out
}

/// What a dive brought back.
#[derive(Clone, Debug, Default)]
pub struct Learned {
    /// Clauses valid for the whole model.
    pub clauses: Vec<Clause>,
    /// Columns the dive proved take one value, from a clause of a single literal.
    pub fixings: Vec<(u32, bool)>,
    /// Decisions taken, which is what the budget is spent on.
    pub decisions: usize,
}

/// Dive on the model's bounds and learn from every contradiction.
///
/// This is the piece every other part of the conflict machinery was waiting for. A clause
/// can only be learned from a conflict, and a conflict only happens when enough columns
/// are fixed at once for the model to contradict itself. The tree search never does that:
/// it branches on an LP-fractional column and moves on, and measured over eleven instances
/// its propagation contradicts on nothing at sixteen threads.
///
/// So this makes conflicts on purpose, and cheaply -- **no LP is solved here at all**. It
/// is a depth-first walk over bound assignments: fix the most constrained free binary,
/// propagate, and on a contradiction explain it, keep the clause, and back up. The clauses
/// it keeps are properties of the model, so the real search obeys them from its first node.
///
/// It is what a reference solver's `rapidlearning` separator does, and its numbers are the
/// reason this exists: on MIPLIB's `chromaticindex32-8` that separator returns 200 conflict
/// constraints and 620 bound changes in 0.21 seconds, and the root bound moves from 3 to 4
/// on ten further simplex iterations -- the whole instance, in one node. Ablated, it is
/// worth 8.7x there.
///
/// Bounded by decisions and by propagation work, because it runs before the search has had
/// a node. Stopping early costs clauses and never costs a solve.
#[allow(clippy::too_many_arguments)]
pub fn rapid_learn(
    problem: &Problem,
    rows_of_a: &SparseMatrix,
    conflicts: &Conflicts,
    scratch: &mut Scratch,
    max_decisions: usize,
    max_depth: usize,
    per_step_budget: usize,
    max_clauses: usize,
    deadline: Option<std::time::Instant>,
    tolerance: f64,
) -> Learned {
    let n = problem.n_cols();
    let mut out = Learned::default();
    let mut pool = Clauses::new(n, max_clauses, LEARNED_MAX_LENGTH);
    let mut lb = problem.col_lb.clone();
    let mut ub = problem.col_ub.clone();

    // Most constrained first: a column excluding many others propagates furthest, which is
    // what makes a contradiction arrive within the budget rather than beyond it.
    let mut order: Vec<u32> = (0..n)
        .filter(|&j| is_binary(problem, j) && (problem.col_ub[j] - problem.col_lb[j]) > tolerance)
        .map(|j| j as u32)
        .collect();
    order.sort_by_key(|&j| {
        std::cmp::Reverse(
            conflicts.degree(literal(j as usize, true)) + conflicts.degree(literal(j as usize, false)),
        )
    });
    if order.is_empty() {
        return out;
    }

    let mut levels: Vec<Level0> = Vec::new();
    scratch.begin();

    // A column just assigned and not yet propagated. A flipped decision arrives here the
    // same way a fresh one does: it is an assignment like any other and its consequences
    // have to be drawn, which is what the first version of this got wrong.
    let mut pending: Option<u32> = None;

    while out.decisions < max_decisions && out.clauses.len() < max_clauses {
        // Bounded by the clock as well as by decisions. Everything else that runs before
        // the search gets a node is held to a share of the run, and this is the third
        // place in this file where forgetting that cost the whole budget: without it a
        // dive on `n2seq36f` runs for minutes and the model never reaches a node.
        if out.decisions.is_multiple_of(64)
            && deadline.is_some_and(|d| std::time::Instant::now() >= d)
        {
            break;
        }
        let column = match pending.take() {
            Some(column) => column,
            None => {
                // The next free column, in the order fixed above.
                let next = order
                    .iter()
                    .copied()
                    .find(|&j| (ub[j as usize] - lb[j as usize]).abs() > tolerance);
                let Some(column) = next else {
                    // Everything is fixed and nothing contradicted: a feasible assignment.
                    // Backing up from it is what keeps the dive exploring; the point
                    // itself is not this function's business.
                    match backtrack(&mut levels, &mut lb, &mut ub, scratch, tolerance) {
                        Some(flipped) => {
                            pending = Some(flipped);
                            continue;
                        }
                        None => break,
                    }
                };
                // Deep enough. Every level holds the bounds to restore, so depth costs
                // memory as well as time; backing up here costs the clauses below this
                // point and nothing else.
                if levels.len() >= max_depth {
                    match backtrack(&mut levels, &mut lb, &mut ub, scratch, tolerance) {
                        Some(flipped) => {
                            pending = Some(flipped);
                            continue;
                        }
                        None => break,
                    }
                }
                // Take the value that propagates further first; for a set packing row
                // that is one.
                levels.push(Level0 {
                    column,
                    value: true,
                    flipped: false,
                    lb: lb.clone(),
                    ub: ub.clone(),
                });
                out.decisions += 1;
                let j = column as usize;
                lb[j] = 1.0;
                ub[j] = 1.0;
                column
            }
        };
        // Whether fresh or flipped, this bound is a decision and the explanation has to
        // be able to say so.
        scratch.set_reason(column as usize, Reason::Decision);

        let outcome = propagate(
            problem,
            rows_of_a,
            conflicts,
            &pool,
            scratch,
            &mut lb,
            &mut ub,
            &[(column, Seed::Decision)],
            per_step_budget,
            tolerance,
        );
        let Err(conflict) = outcome else {
            continue;
        };
        // Explain it in terms of the decisions that caused it, and keep that.
        if let Some(clause) = explain(
            problem,
            rows_of_a,
            &pool,
            scratch,
            conflict,
            &lb,
            &ub,
            per_step_budget,
            tolerance,
        ) {
            if clause.len() == 1 {
                out.fixings.push(clause[0]);
            }
            if pool.add(clause.clone()) {
                out.clauses.push(clause);
            }
        }
        match backtrack(&mut levels, &mut lb, &mut ub, scratch, tolerance) {
            Some(flipped) => pending = Some(flipped),
            None => break,
        }
    }
    out
}

/// One decision of a dive, and the bounds to put back when it is undone.
struct Level0 {
    column: u32,
    value: bool,
    flipped: bool,
    lb: Vec<f64>,
    ub: Vec<f64>,
}

/// The longest clause a dive keeps. A long one forbids one assignment of many columns and
/// is checked wherever any of them moves.
const LEARNED_MAX_LENGTH: usize = 32;

/// Undo decisions until one can be taken the other way, and take it.
///
/// Returns the column it flipped, or `None` when every decision has been tried both ways
/// and the dive has exhausted what it was going to reach.
fn backtrack(
    levels: &mut Vec<Level0>,
    lb: &mut [f64],
    ub: &mut [f64],
    scratch: &mut Scratch,
    tolerance: f64,
) -> Option<u32> {
    while let Some(level) = levels.last_mut() {
        // Anything assigned since this decision loses its reason along with its bounds.
        for j in 0..lb.len() {
            if (lb[j] - level.lb[j]).abs() > tolerance || (ub[j] - level.ub[j]).abs() > tolerance {
                scratch.forget(j);
            }
        }
        lb.copy_from_slice(&level.lb);
        ub.copy_from_slice(&level.ub);
        if level.flipped {
            levels.pop();
            continue;
        }
        level.flipped = true;
        level.value = !level.value;
        let j = level.column as usize;
        let value = if level.value { 1.0 } else { 0.0 };
        lb[j] = value;
        ub[j] = value;
        return Some(level.column);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Builder, RowSense, Sense};

    /// Propagate from a set of branch fixings, returning the bounds it proves.
    fn run(problem: &Problem, fixings: &[(usize, f64)]) -> Result<(Vec<f64>, Vec<f64>), Infeasible> {
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let mut lb = problem.col_lb.clone();
        let mut ub = problem.col_ub.clone();
        let mut seeds = Vec::new();
        for &(j, v) in fixings {
            lb[j] = v;
            ub[j] = v;
            seeds.push((j as u32, Seed::Decision));
        }
        scratch.begin();
        propagate(
            problem,
            &csr,
            &conflicts,
            &Clauses::default(),
            &mut scratch,
            &mut lb,
            &mut ub,
            &seeds,
            usize::MAX,
            1e-9,
        )?;
        Ok((lb, ub))
    }

    #[test]
    fn a_set_partitioning_row_is_decided_by_one_branch() {
        // x + y + z = 1: setting x to one leaves nothing for y and z, which is the
        // reduction a clique carries and the one a colouring model is made of.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        let z = b.binary("z");
        b.objective(&[(x, 1.0), (y, 1.0), (z, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0), (z, 1.0)], RowSense::Eq, 1.0);
        let problem = b.build();
        let (lb, ub) = run(&problem, &[(x, 1.0)]).unwrap();
        assert_eq!((lb[y], ub[y]), (0.0, 0.0), "y must go to zero");
        assert_eq!((lb[z], ub[z]), (0.0, 0.0), "z must go to zero");
    }

    #[test]
    fn a_chain_of_implications_runs_to_a_fixpoint() {
        // x=1 forces y=0 through the first row, and y=0 forces z=1 through the second.
        // One round would stop at y; the fixpoint is the point of the worklist.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        let z = b.binary("z");
        b.objective(&[(x, 1.0), (y, 1.0), (z, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Le, 1.0);
        b.row(&[(y, 1.0), (z, 1.0)], RowSense::Ge, 1.0);
        let problem = b.build();
        let (lb, ub) = run(&problem, &[(x, 1.0)]).unwrap();
        assert_eq!((lb[y], ub[y]), (0.0, 0.0), "y follows from x");
        assert_eq!((lb[z], ub[z]), (1.0, 1.0), "z follows from y, which needs a second round");
    }

    #[test]
    fn a_row_that_cannot_be_satisfied_is_reported_infeasible() {
        // x + y >= 2 with x driven to zero leaves y alone to carry 2, which it cannot.
        let mut b = Builder::new(Sense::Minimize);
        let x = b.binary("x");
        let y = b.binary("y");
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Ge, 2.0);
        let problem = b.build();
        assert!(run(&problem, &[(x, 0.0)]).is_err());
    }

    #[test]
    fn an_integer_bound_rounds_inwards() {
        // 2x <= 3 over an integer x in [0,5] implies x <= 1.5, and therefore x <= 1.
        // Without the rounding this proves nothing a branch would act on.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.column("x", 0.0, 5.0, VarType::Integer);
        let y = b.column("y", 0.0, 5.0, VarType::Integer);
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 2.0), (y, 1.0)], RowSense::Le, 3.0);
        let problem = b.build();
        let (_, ub) = run(&problem, &[(y, 0.0)]).unwrap();
        assert_eq!(ub[x], 1.0, "2x <= 3 makes x at most 1, not 1.5");
    }

    #[test]
    fn the_budget_stops_it_without_making_it_wrong() {
        // A budget of zero draws no consequence at all, which has to be sound: the node
        // is simply not tightened. Anything it *had* proved would still hold.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Le, 1.0);
        let problem = b.build();
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(&problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let (mut lb, mut ub) = (problem.col_lb.clone(), problem.col_ub.clone());
        lb[x] = 1.0;
        ub[x] = 1.0;
        scratch.begin();
        let fixed = propagate(
            &problem,
            &csr,
            &conflicts,
            &Clauses::default(),
            &mut scratch,
            &mut lb,
            &mut ub,
            &[(x as u32, Seed::Decision)],
            0,
            1e-9,
        )
        .unwrap();
        assert_eq!(fixed, 0);
        assert_eq!((lb[y], ub[y]), (0.0, 1.0), "y is untouched, not wrongly fixed");
    }

    /// Propagate from a branch and, if it conflicts, explain it.
    fn learn(problem: &Problem, fixings: &[(usize, f64)]) -> Option<Clause> {
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let mut lb = problem.col_lb.clone();
        let mut ub = problem.col_ub.clone();
        let mut seeds = Vec::new();
        for &(j, v) in fixings {
            lb[j] = v;
            ub[j] = v;
            seeds.push((j as u32, Seed::Decision));
        }
        scratch.begin();
        let conflict = propagate(
            problem,
            &csr,
            &conflicts,
            &Clauses::default(),
            &mut scratch,
            &mut lb,
            &mut ub,
            &seeds,
            usize::MAX,
            1e-9,
        )
        .err()?;
        explain(
            problem,
            &csr,
            &Clauses::default(),
            &mut scratch,
            conflict,
            &lb,
            &ub,
            usize::MAX,
            1e-9,
        )
    }

    #[test]
    fn a_clause_names_the_branchings_that_caused_the_conflict() {
        // x + y >= 1 with both branched to zero is a conflict, and the clause has to say
        // so: at least one of them must be one.
        let mut b = Builder::new(Sense::Minimize);
        let x = b.binary("x");
        let y = b.binary("y");
        let z = b.binary("z");
        b.objective(&[(x, 1.0), (y, 1.0), (z, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Ge, 1.0);
        let problem = b.build();
        let clause = learn(&problem, &[(x, 0.0), (y, 0.0)]).expect("this conflicts");
        assert_eq!(clause, vec![(x as u32, true), (y as u32, true)]);
        // And z, which had nothing to do with it, is not in the clause.
        assert!(clause.iter().all(|&(j, _)| j != z as u32));
    }

    #[test]
    fn nothing_is_learned_through_a_bound_the_incumbent_earned() {
        // The same conflict, but y is fixed by reduced cost rather than by branching.
        // That bound holds only while the incumbent does, so a clause rested on it would
        // be wrong the moment a better incumbent arrived. It must refuse to learn.
        let mut b = Builder::new(Sense::Minimize);
        let x = b.binary("x");
        let y = b.binary("y");
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Ge, 1.0);
        let problem = b.build();
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(&problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let (mut lb, mut ub) = (problem.col_lb.clone(), problem.col_ub.clone());
        lb[x] = 0.0;
        ub[x] = 0.0;
        lb[y] = 0.0;
        ub[y] = 0.0;
        scratch.begin();
        let conflict = propagate(
            &problem,
            &csr,
            &conflicts,
            &Clauses::default(),
            &mut scratch,
            &mut lb,
            &mut ub,
            &[(x as u32, Seed::Decision), (y as u32, Seed::Assumption)],
            usize::MAX,
            1e-9,
        )
        .expect_err("this conflicts");
        assert_eq!(
            explain(
                &problem,
                &csr,
                &Clauses::default(),
                &mut scratch,
                conflict,
                &lb,
                &ub,
                usize::MAX,
                1e-9
            ),
            None,
            "a clause resting on an incumbent-earned bound must not be learned"
        );
    }

    #[test]
    fn probing_learns_an_implication_no_single_row_states() {
        // x + y <= 1 makes x=1 force y=0, and y + z >= 1 makes y=0 force z=1. So x=1
        // forces z=1 -- through two rows, which is why no row states it and the conflict
        // graph, built from rows, does not hold it. That is exactly what probing is for.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        let z = b.binary("z");
        b.objective(&[(x, 1.0), (y, 1.0), (z, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Le, 1.0);
        b.row(&[(y, 1.0), (z, 1.0)], RowSense::Ge, 1.0);
        let problem = b.build();
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(&problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let learned = probe_implications(
            &problem, &csr, &conflicts, &mut scratch, 1_000_000, 10_000, 1000, 1e-9,
        );
        assert!(
            learned.contains(&vec![(x as u32, false), (z as u32, true)]),
            "x=1 implies z=1 through two rows; learned {learned:?}"
        );
        for clause in &learned {
            assert_eq!(clause.len(), 2, "these are pair clauses: {clause:?}");
        }
    }

    #[test]
    fn probing_does_not_relearn_what_the_conflict_graph_holds() {
        // A partitioning row is a clique, and propagation already reads it from the
        // graph. Learning it again would cost a scan at every node for nothing.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        let z = b.binary("z");
        b.objective(&[(x, 1.0), (y, 1.0), (z, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0), (z, 1.0)], RowSense::Eq, 1.0);
        let problem = b.build();
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(&problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let learned = probe_implications(
            &problem, &csr, &conflicts, &mut scratch, 1_000_000, 10_000, 1000, 1e-9,
        );
        assert!(
            learned.is_empty(),
            "every implication here is already a graph edge; learned {learned:?}"
        );
    }

    /// **The property the dive rests on.** Its clauses are imported into the real solve
    /// and obeyed from the first node, so one that excludes a feasible point deletes the
    /// optimum. Enumerate every binary point and check every clause the dive brought back.
    #[test]
    fn a_dived_clause_never_excludes_a_feasible_point() {
        let mut learned_any = 0;
        for seed in 0..400u64 {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let n = 5 + (next() % 5) as usize;
            let mut b = Builder::new(Sense::Maximize);
            let columns: Vec<usize> = (0..n).map(|j| b.binary(&format!("x{j}"))).collect();
            b.objective(&columns.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>());
            for _ in 0..2 + (next() % 5) {
                let mut terms = Vec::new();
                for &j in &columns {
                    if next() % 2 == 0 {
                        terms.push((j, (next() % 5) as f64 - 2.0));
                    }
                }
                if terms.is_empty() {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let rhs = (next() % (2 * span as u64 + 1)) as f64 - span;
                let sense = match next() % 3 {
                    0 => RowSense::Le,
                    1 => RowSense::Ge,
                    _ => RowSense::Eq,
                };
                b.row(&terms, sense, rhs);
            }
            let problem = b.build();
            let feasible: Vec<Vec<f64>> = (0..1u32 << n)
                .map(|mask| (0..n).map(|j| f64::from((mask >> j) & 1)).collect::<Vec<f64>>())
                .filter(|x| crate::heuristic::is_feasible(&problem, x, 1e-9))
                .collect();
            if feasible.is_empty() {
                continue;
            }
            let csr = problem.matrix.to_csr();
            let conflicts = Conflicts::of(&problem);
            let mut scratch = Scratch::new(n);
            let learned = rapid_learn(
                &problem, &csr, &conflicts, &mut scratch, 500, 32, 100_000, 1000, None, 1e-9,
            );
            learned_any += learned.clauses.len();
            for clause in &learned.clauses {
                for point in &feasible {
                    let satisfied = clause
                        .iter()
                        .any(|&(j, want)| (point[j as usize] > 0.5) == want);
                    assert!(
                        satisfied,
                        "seed {seed}: dived clause {clause:?} excludes the feasible point \
                         {point:?}"
                    );
                }
            }
            // A fixing is a one-literal clause and carries the same obligation.
            for &(j, want) in &learned.fixings {
                for point in &feasible {
                    assert!(
                        (point[j as usize] > 0.5) == want,
                        "seed {seed}: dive fixed x{j} to {want}, but a feasible point has \
                         {}",
                        point[j as usize]
                    );
                }
            }
        }
        assert!(
            learned_any > 100,
            "only {learned_any} dived clauses, so little was checked"
        );
    }

    #[test]
    fn the_dive_finds_what_two_rows_together_forbid() {
        // x + y >= 1 and x + y <= 1 and a third column free: the dive should discover the
        // contradiction of setting x and y both one, and keep it.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        let z = b.binary("z");
        b.objective(&[(x, 1.0), (y, 1.0), (z, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Le, 1.0);
        b.row(&[(x, 1.0), (y, 1.0), (z, 1.0)], RowSense::Ge, 2.0);
        let problem = b.build();
        let csr = problem.matrix.to_csr();
        let conflicts = Conflicts::of(&problem);
        let mut scratch = Scratch::new(problem.n_cols());
        let learned = rapid_learn(
            &problem, &csr, &conflicts, &mut scratch, 200, 32, 100_000, 100, None, 1e-9,
        );
        assert!(learned.decisions > 0, "the dive has to take decisions to learn");
        // Every feasible point needs z, since x and y cannot both be one.
        for clause in &learned.clauses {
            assert!(!clause.is_empty());
        }
        let _ = z;
    }

    /// **The property probe-learned clauses rest on.** They are kept for the whole search
    /// and propagated at every node, so one that excludes a feasible point deletes the
    /// optimum. Enumerate every binary point of a random model and check every clause
    /// probing produced against every one of them.
    #[test]
    fn a_probed_clause_never_excludes_a_feasible_point() {
        let mut learned_any = 0;
        for seed in 0..400u64 {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let n = 4 + (next() % 5) as usize;
            let mut b = Builder::new(Sense::Maximize);
            let columns: Vec<usize> = (0..n).map(|j| b.binary(&format!("x{j}"))).collect();
            b.objective(&columns.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>());
            for _ in 0..2 + (next() % 4) {
                let mut terms = Vec::new();
                for &j in &columns {
                    if next() % 2 == 0 {
                        terms.push((j, (next() % 5) as f64 - 2.0));
                    }
                }
                if terms.is_empty() {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let rhs = (next() % (2 * span as u64 + 1)) as f64 - span;
                let sense = match next() % 3 {
                    0 => RowSense::Le,
                    1 => RowSense::Ge,
                    _ => RowSense::Eq,
                };
                b.row(&terms, sense, rhs);
            }
            let problem = b.build();
            let feasible: Vec<Vec<f64>> = (0..1u32 << n)
                .map(|mask| (0..n).map(|j| f64::from((mask >> j) & 1)).collect::<Vec<f64>>())
                .filter(|x| crate::heuristic::is_feasible(&problem, x, 1e-9))
                .collect();
            if feasible.is_empty() {
                continue;
            }
            let csr = problem.matrix.to_csr();
            let conflicts = Conflicts::of(&problem);
            let mut scratch = Scratch::new(n);
            let learned = probe_implications(
                &problem, &csr, &conflicts, &mut scratch, 10_000_000, 100_000, 10_000, 1e-9,
            );
            learned_any += learned.len();
            for clause in &learned {
                for point in &feasible {
                    let satisfied = clause
                        .iter()
                        .any(|&(j, want)| (point[j as usize] > 0.5) == want);
                    assert!(
                        satisfied,
                        "seed {seed}: probed clause {clause:?} excludes the feasible \
                         point {point:?}"
                    );
                }
            }
        }
        assert!(
            learned_any > 20,
            "only {learned_any} probed clauses, so little was checked. Most implications \
             on a random model are already conflict graph edges and are deliberately not \
             relearned, so this counts only the derived ones"
        );
    }

    /// **The same property, for the certificate-driven clauses.** These are the ones that
    /// will actually fire, and they rest on a floating point dual ray rather than on a
    /// chain of exact implications, so they get the same exhaustive treatment: enumerate
    /// every binary point, learn from every branch whose LP is infeasible, and check each
    /// clause against every feasible point.
    #[test]
    fn a_farkas_clause_never_excludes_a_feasible_point() {
        use crate::lp::Lp;
        let mut learned_any = 0;
        let (mut infeasible_lps, mut rays) = (0, 0);
        for seed in 0..300u64 {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let n = 4 + (next() % 4) as usize;
            let mut b = Builder::new(Sense::Maximize);
            let columns: Vec<usize> = (0..n).map(|j| b.binary(&format!("x{j}"))).collect();
            b.objective(&columns.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>());
            for _ in 0..2 + (next() % 4) {
                let mut terms = Vec::new();
                for &j in &columns {
                    if next() % 2 == 0 {
                        terms.push((j, (next() % 5) as f64 - 2.0));
                    }
                }
                if terms.is_empty() {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let rhs = (next() % (2 * span as u64 + 1)) as f64 - span;
                let sense = match next() % 3 {
                    0 => RowSense::Le,
                    1 => RowSense::Ge,
                    _ => RowSense::Eq,
                };
                b.row(&terms, sense, rhs);
            }
            let problem = b.build();
            let feasible: Vec<Vec<f64>> = (0..1u32 << n)
                .map(|mask| (0..n).map(|j| f64::from((mask >> j) & 1)).collect::<Vec<f64>>())
                .filter(|x| crate::heuristic::is_feasible(&problem, x, 1e-9))
                .collect();
            if feasible.is_empty() {
                continue;
            }
            let csr = problem.matrix.to_csr();
            let conflicts = Conflicts::of(&problem);

            for a in 0..n {
                for va in [0.0, 1.0] {
                    for c in (a + 1)..n {
                        for vc in [0.0, 1.0] {
                            let mut scratch = Scratch::new(n);
                            let mut lb = problem.col_lb.clone();
                            let mut ub = problem.col_ub.clone();
                            lb[a] = va;
                            ub[a] = va;
                            lb[c] = vc;
                            ub[c] = vc;
                            let seeds =
                                [(a as u32, Seed::Decision), (c as u32, Seed::Decision)];
                            // Seed the reasons the walk reads back, without drawing any
                            // consequence: a zero budget stops propagation before its
                            // first step. That is deliberate -- on models this small
                            // propagation refutes every infeasible branch itself, and the
                            // LP never gets to prove anything, which is exactly the path
                            // under test here.
                            scratch.begin();
                            if propagate(
                                &problem,
                                &csr,
                                &conflicts,
                                &Clauses::default(),
                                &mut scratch,
                                &mut lb,
                                &mut ub,
                                &seeds,
                                0,
                                1e-9,
                            )
                            .is_err()
                            {
                                continue;
                            }
                            let mut lp = Lp::relaxation(&problem);
                            for j in 0..n {
                                lp.set_column_bounds(j, lb[j], ub[j]);
                            }
                            let solved = lp.solve_with_limit(100_000);
                            if solved.status != crate::lp::LpStatus::Infeasible {
                                continue;
                            }
                            infeasible_lps += 1;
                            let Some(ray) = solved.farkas.as_ref() else {
                                continue;
                            };
                            rays += 1;
                            let Some(clause) = farkas_conflict(
                                &problem,
                                &csr,
                                &Clauses::default(),
                                &mut scratch,
                                ray,
                                &lb,
                                &ub,
                                usize::MAX,
                                1e-9,
                            ) else {
                                continue;
                            };
                            learned_any += 1;
                            for point in &feasible {
                                let satisfied = clause
                                    .iter()
                                    .any(|&(j, want)| (point[j as usize] > 0.5) == want);
                                assert!(
                                    satisfied,
                                    "seed {seed}: Farkas clause {clause:?} from x{a}={va}, \
                                     x{c}={vc} excludes the feasible point {point:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(
            learned_any > 20,
            "only {learned_any} Farkas clauses learned from {infeasible_lps} infeasible \
             LPs and {rays} rays, so little was checked"
        );
    }

    /// **The property the whole mechanism rests on.** A learned clause is added to the
    /// model and every node below obeys it, so a clause that excludes even one feasible
    /// point silently deletes the optimum. Enumerate every binary point of a random
    /// model, learn from every branch that conflicts, and check each clause against every
    /// feasible point.
    #[test]
    fn a_learned_clause_never_excludes_a_feasible_point() {
        let mut learned_any = 0;
        for seed in 0..500u64 {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let n = 4 + (next() % 5) as usize;
            let mut b = Builder::new(Sense::Maximize);
            let columns: Vec<usize> = (0..n).map(|j| b.binary(&format!("x{j}"))).collect();
            b.objective(&columns.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>());
            for _ in 0..2 + (next() % 4) {
                let mut terms = Vec::new();
                for &j in &columns {
                    if next() % 2 == 0 {
                        terms.push((j, (next() % 5) as f64 - 2.0));
                    }
                }
                if terms.is_empty() {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let rhs = (next() % (2 * span as u64 + 1)) as f64 - span;
                let sense = match next() % 3 {
                    0 => RowSense::Le,
                    1 => RowSense::Ge,
                    _ => RowSense::Eq,
                };
                b.row(&terms, sense, rhs);
            }
            let problem = b.build();
            let feasible: Vec<Vec<f64>> = (0..1u32 << n)
                .map(|mask| (0..n).map(|j| f64::from((mask >> j) & 1)).collect::<Vec<f64>>())
                .filter(|x| crate::heuristic::is_feasible(&problem, x, 1e-9))
                .collect();

            // Branch on every pair, which is where conflicts actually arise.
            for a in 0..n {
                for va in [0.0, 1.0] {
                    for c in (a + 1)..n {
                        for vc in [0.0, 1.0] {
                            let Some(clause) = learn(&problem, &[(a, va), (c, vc)]) else {
                                continue;
                            };
                            learned_any += 1;
                            for point in &feasible {
                                let satisfied = clause
                                    .iter()
                                    .any(|&(j, want)| (point[j as usize] > 0.5) == want);
                                assert!(
                                    satisfied,
                                    "seed {seed}: clause {clause:?} learned from \
                                     x{a}={va}, x{c}={vc} excludes the feasible point \
                                     {point:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(
            learned_any > 200,
            "only {learned_any} clauses learned, so little was checked"
        );
    }

    /// Every point the original admits must survive propagation from a partial assignment
    /// that is consistent with it. This is the property that matters: propagation may
    /// prove bounds, and may prove infeasibility, but it may never remove a feasible
    /// completion of the branch it was given.
    #[test]
    fn propagation_never_removes_a_feasible_point() {
        for seed in 0..400u64 {
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let n = 4 + (next() % 5) as usize;
            let mut b = Builder::new(Sense::Maximize);
            let columns: Vec<usize> = (0..n).map(|j| b.binary(&format!("x{j}"))).collect();
            b.objective(&columns.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>());
            for _ in 0..2 + (next() % 4) {
                let mut terms = Vec::new();
                for &j in &columns {
                    if next() % 2 == 0 {
                        terms.push((j, (next() % 5) as f64 - 2.0));
                    }
                }
                if terms.is_empty() {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let rhs = (next() % (2 * span as u64 + 1)) as f64 - span;
                let sense = match next() % 3 {
                    0 => RowSense::Le,
                    1 => RowSense::Ge,
                    _ => RowSense::Eq,
                };
                b.row(&terms, sense, rhs);
            }
            let problem = b.build();

            // Enumerate every binary point, keep the feasible ones.
            let feasible: Vec<Vec<f64>> = (0..1u32 << n)
                .map(|mask| {
                    (0..n)
                        .map(|j| f64::from((mask >> j) & 1))
                        .collect::<Vec<f64>>()
                })
                .filter(|x| crate::heuristic::is_feasible(&problem, x, 1e-9))
                .collect();

            // Branch on one column at each value, and check propagation keeps every
            // feasible point that agrees with the branch.
            for j in 0..n {
                for value in [0.0, 1.0] {
                    let agreeing: Vec<&Vec<f64>> =
                        feasible.iter().filter(|x| x[j] == value).collect();
                    match run(&problem, &[(j, value)]) {
                        Err(_) => assert!(
                            agreeing.is_empty(),
                            "seed {seed}: propagation called x{j}={value} infeasible, but \
                             {} feasible points agree with it",
                            agreeing.len()
                        ),
                        Ok((lb, ub)) => {
                            for point in agreeing {
                                for k in 0..n {
                                    assert!(
                                        point[k] >= lb[k] - 1e-9 && point[k] <= ub[k] + 1e-9,
                                        "seed {seed}: branch x{j}={value} propagated x{k} to \
                                         [{}, {}], which excludes a feasible point at {}",
                                        lb[k], ub[k], point[k]
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
