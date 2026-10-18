//! Conflict-driven search on propagation alone, with no LP anywhere in it.
//!
//! A reference solver's `rapidlearning` is this: a sub-search with the relaxation switched
//! off, branching and propagating and learning a clause from every dead end, under a node
//! limit. It was read here once as "a MIP solve with LPs" and what was built in its place
//! was a single dive whose clauses ran back to every decision taken; that dive found no
//! unit clause on `chromaticindex32-8` and no point on anything.
//!
//! What makes the difference is the learning scheme, and it is the SAT solver's:
//!
//! - **Trail and levels.** Every assignment is recorded in order with the decision level
//!   it was made at and the reason that forced it, so undoing is a walk back along the
//!   trail rather than a copy of the bounds per level.
//! - **First unique implication point.** A conflict is explained backwards only until one
//!   literal of the current level remains, not all the way to the decisions. The clause
//!   that results asserts that literal's negation as soon as the search backs up, and it
//!   is short: this is where unit clauses -- columns decided for good -- come from.
//! - **Backjumping.** The search backs up to the second-highest level in the clause, not
//!   one level, so a conflict caused by an early decision does not wait for everything
//!   after it to be tried both ways.
//! - **Activity.** Columns in recent conflicts are branched on first, and a column keeps
//!   the last value it held when the search comes back to it.
//!
//! Reasons for a row's implications are kept to the side of the row that forced them and,
//! within that, to the largest contributions that suffice. A reason that names every
//! fixed column in the row is sound but makes every clause through it long, and a long
//! clause forbids almost nothing.
//!
//! Only binaries are branched on. Any other column contributes its bound range to each
//! row's activity and is never assigned, which keeps every inference sound on a mixed
//! model; a complete assignment is reported as a solution only when there is nothing
//! else to assign.
//!
//! What comes back is whichever of three things the budget allowed: a feasible point, a
//! proof that none exists (under the cutoff row, if one was given), or neither -- and in
//! every case the short clauses and unit fixings learned on the way.

use crate::cuts::{Conflicts, literal};
use crate::model::{Problem, VarType};
use crate::propagate::Clause;
use std::time::Instant;

/// How much the search may spend.
#[derive(Clone, Copy, Debug)]
pub struct Limits<'a> {
    /// Conflicts before giving up. This is the node limit's counterpart: every conflict
    /// is a dead end, and dead ends are what the budget buys.
    pub max_conflicts: usize,
    pub deadline: Option<Instant>,
    /// Longest learned clause handed back to the caller. The search keeps longer ones for
    /// itself, since they justify its own implications.
    pub export_length: usize,
    /// Most clauses handed back.
    pub export_count: usize,
    /// For [`optimize`]: conflicts it may spend without finding a better point or a proof
    /// before it stops. A search that has gone this long without either is on a model it
    /// cannot crack, and the budget is worth more to the relaxation.
    pub stall_conflicts: usize,
    /// For [`optimize`], once it holds a point: seconds it may go without a better one or
    /// a proof. Conflicts are a poor clock on a model where each one is expensive --
    /// `neos-787933` spends ten seconds on 1178 of them after finding its point in the
    /// first twentieth of a second -- and the run it is taking them from is the relaxation's.
    pub stall_seconds: f64,
    /// Set by the caller to stop the search at the next check, as the deadline would.
    pub cancel: Option<&'a std::sync::atomic::AtomicBool>,
}

impl Limits<'_> {
    fn expired(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
            || self
                .cancel
                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    }
}

/// What the search concluded.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// A point satisfying every row, and the cutoff row if there was one.
    Solution(Vec<f64>),
    /// No point satisfies the model with the cutoff row added.
    Infeasible,
    /// The budget ran out first.
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub verdict: Verdict,
    /// Learned clauses no longer than `Limits::export_length`, units excluded.
    pub clauses: Vec<Clause>,
    /// Columns decided for good: clauses of one literal.
    pub fixings: Vec<(u32, bool)>,
    pub conflicts: usize,
    pub decisions: usize,
}

/// Why a column holds the value it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// Fixed by the model's own bounds, or learned at level zero. Never explained further.
    Given,
    Decision,
    /// The row's upper bound left no room for the other value.
    Upper(u32),
    /// The row's lower bound left no room for the other value.
    Lower(u32),
    /// The named column holds a value the conflict graph excludes this one's other value
    /// alongside.
    Excluded(u32),
    /// Every other literal of the named learned clause had failed.
    Clause(u32),
}

/// A literal the search can hold true or false: `(column, value)`, read "column takes
/// value", exactly as a [`Clause`] entry is.
type Lit = (u32, bool);

fn lit_index((j, value): Lit) -> usize {
    2 * j as usize + usize::from(value)
}

/// Relative slack on every row comparison, in the direction that infers less. A conflict
/// found by rounding would be learned as a clause and obeyed by the whole search.
const TOL: f64 = 1e-6;

/// Conflicts between restarts, scaled by the Luby sequence.
const RESTART_UNIT: usize = 128;

/// How activity bumps grow, which is how old conflicts fade.
const ACTIVITY_DECAY: f64 = 0.95;

struct Row {
    /// Binary columns and their coefficients.
    entries: Vec<(u32, f64)>,
    /// Largest `|a|` among `entries`, which rules out a scan when the slack exceeds it.
    largest: f64,
    lb: f64,
    ub: f64,
    /// Whether each side can be reasoned with: it is finite, and the non-binary columns'
    /// contribution to the activity it is compared against is finite too.
    upper_usable: bool,
    lower_usable: bool,
    /// Whether the non-binary columns leave the least activity finite, which is what a
    /// tightened upper bound needs to become usable.
    rest_min_finite: bool,
}

/// Indexed max-heap of free columns by activity.
struct Heap {
    items: Vec<u32>,
    position: Vec<usize>,
}

const ABSENT: usize = usize::MAX;

impl Heap {
    fn new(n: usize) -> Self {
        Heap {
            items: Vec::new(),
            position: vec![ABSENT; n],
        }
    }

    fn contains(&self, j: u32) -> bool {
        self.position[j as usize] != ABSENT
    }

    fn push(&mut self, j: u32, activity: &[f64]) {
        if self.contains(j) {
            return;
        }
        self.position[j as usize] = self.items.len();
        self.items.push(j);
        self.up(self.items.len() - 1, activity);
    }

    fn pop(&mut self, activity: &[f64]) -> Option<u32> {
        let top = *self.items.first()?;
        let last = self.items.pop().unwrap();
        self.position[top as usize] = ABSENT;
        if !self.items.is_empty() {
            self.items[0] = last;
            self.position[last as usize] = 0;
            self.down(0, activity);
        }
        Some(top)
    }

    /// Restore order after `j`'s activity rose.
    fn bumped(&mut self, j: u32, activity: &[f64]) {
        let at = self.position[j as usize];
        if at != ABSENT {
            self.up(at, activity);
        }
    }

    fn up(&mut self, mut at: usize, activity: &[f64]) {
        let item = self.items[at];
        while at > 0 {
            let parent = (at - 1) / 2;
            if activity[self.items[parent] as usize] >= activity[item as usize] {
                break;
            }
            self.items[at] = self.items[parent];
            self.position[self.items[at] as usize] = at;
            at = parent;
        }
        self.items[at] = item;
        self.position[item as usize] = at;
    }

    fn down(&mut self, mut at: usize, activity: &[f64]) {
        let item = self.items[at];
        loop {
            let left = 2 * at + 1;
            if left >= self.items.len() {
                break;
            }
            let right = left + 1;
            let child = if right < self.items.len()
                && activity[self.items[right] as usize] > activity[self.items[left] as usize]
            {
                right
            } else {
                left
            };
            if activity[self.items[child] as usize] <= activity[item as usize] {
                break;
            }
            self.items[at] = self.items[child];
            self.position[self.items[at] as usize] = at;
            at = child;
        }
        self.items[at] = item;
        self.position[item as usize] = at;
    }
}

struct Search<'a> {
    conflicts: &'a Conflicts,
    /// The objective's row, when the search is aimed at points better than something.
    cutoff_row: Option<u32>,
    rows: Vec<Row>,
    /// For each column, the rows it appears in with its coefficient there.
    columns: Vec<Vec<(u32, f64)>>,
    /// Least and greatest activity of each row under the current assignment.
    min_activity: Vec<f64>,
    max_activity: Vec<f64>,
    /// Which columns are binaries this search may assign.
    binary: Vec<bool>,
    value: Vec<i8>,
    level: Vec<u32>,
    position: Vec<u32>,
    why: Vec<Why>,
    trail: Vec<u32>,
    /// Where each decision level begins on the trail.
    level_start: Vec<usize>,
    head: usize,
    learned: Vec<Clause>,
    /// Learned clauses by the literal whose failure should wake them.
    watching: Vec<Vec<u32>>,
    activity: Vec<f64>,
    bump: f64,
    heap: Heap,
    phase: Vec<bool>,
    seen: Vec<bool>,
    /// Scratch for reasons, so explaining allocates nothing per step.
    scratch: Vec<(u32, f64)>,
}

/// A contradiction, as the set of assignments that cannot all hold.
type Conflict = Vec<Lit>;

impl<'a> Search<'a> {
    fn new(problem: &Problem, extra: Option<&(Vec<(usize, f64)>, f64)>, conflicts: &'a Conflicts) -> Self {
        let n = problem.n_cols();
        let binary: Vec<bool> = (0..n)
            .map(|j| {
                let (lb, ub) = (problem.col_lb[j], problem.col_ub[j]);
                // A continuous column fixed at 0 or 1 is a fixed binary in all but name.
                // Presolve leaves one behind for a placeholder objective column that is in
                // no row, as the sudoku models have, and without this it makes the model
                // impure, so a complete assignment is never taken for a solution.
                let fixed_binary = lb == ub && (lb == 0.0 || lb == 1.0);
                (problem.col_type[j] == VarType::Integer || fixed_binary)
                    && lb >= -TOL
                    && ub <= 1.0 + TOL
            })
            .collect();

        let csr = problem.matrix.to_csr();
        let mut rows = Vec::with_capacity(problem.n_rows() + 1);
        let mut add_row = |entries: &mut dyn Iterator<Item = (usize, f64)>, lb: f64, ub: f64| {
            let (mut binaries, mut rest_min, mut rest_max) = (Vec::new(), 0.0f64, 0.0f64);
            for (k, a) in entries {
                if a == 0.0 {
                    continue;
                }
                if binary[k] {
                    binaries.push((k as u32, a));
                } else {
                    let (low, high) = if a > 0.0 {
                        (a * problem.col_lb[k], a * problem.col_ub[k])
                    } else {
                        (a * problem.col_ub[k], a * problem.col_lb[k])
                    };
                    rest_min += low;
                    rest_max += high;
                }
            }
            let largest = binaries.iter().map(|&(_, a): &(u32, f64)| a.abs()).fold(0.0, f64::max);
            // The binaries' own contribution is added by the caller once their bounds are
            // known; what is stored here is the side and what the rest of the row takes.
            rows.push((
                Row {
                    entries: binaries,
                    largest,
                    lb,
                    ub,
                    upper_usable: ub.is_finite() && rest_min.is_finite(),
                    lower_usable: lb.is_finite() && rest_max.is_finite(),
                    rest_min_finite: rest_min.is_finite(),
                },
                rest_min,
                rest_max,
            ));
        };
        for i in 0..problem.n_rows() {
            let (cols, vals) = csr.column(i);
            add_row(
                &mut cols.iter().copied().zip(vals.iter().copied()),
                problem.row_lb[i],
                problem.row_ub[i],
            );
        }
        let objective_row = problem.n_rows();
        if let Some((entries, ub)) = extra {
            add_row(&mut entries.iter().copied(), f64::NEG_INFINITY, *ub);
        }

        let mut columns: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
        let (mut min_activity, mut max_activity) = (Vec::new(), Vec::new());
        let mut kept = Vec::with_capacity(rows.len());
        let mut cutoff_row = None;
        for (index, (row, rest_min, rest_max)) in rows.into_iter().enumerate() {
            // The objective row is kept whatever its bound, since tightening it later is
            // what it is for.
            let is_objective = extra.is_some() && index == objective_row;
            if is_objective && !row.entries.is_empty() {
                cutoff_row = Some(kept.len() as u32);
            } else if !row.upper_usable && !row.lower_usable || row.entries.is_empty() {
                // Nothing a binary can be told by it. A row with no binaries at all is
                // the caller's business: the model's bounds decide it or they do not.
                continue;
            }
            let i = kept.len() as u32;
            let (mut low, mut high) = (rest_min, rest_max);
            for &(k, a) in &row.entries {
                columns[k as usize].push((i, a));
                low += a.min(0.0);
                high += a.max(0.0);
            }
            min_activity.push(low);
            max_activity.push(high);
            kept.push(row);
        }

        Search {
            conflicts,
            cutoff_row,
            rows: kept,
            columns,
            min_activity,
            max_activity,
            binary,
            value: vec![-1; n],
            level: vec![0; n],
            position: vec![0; n],
            why: vec![Why::Given; n],
            trail: Vec::new(),
            level_start: Vec::new(),
            head: 0,
            learned: Vec::new(),
            watching: vec![Vec::new(); 2 * n],
            activity: vec![0.0; n],
            bump: 1.0,
            heap: Heap::new(n),
            phase: vec![false; n],
            seen: vec![false; n],
            scratch: Vec::new(),
        }
    }

    fn current_level(&self) -> u32 {
        self.level_start.len() as u32
    }

    fn holds(&self, (j, value): Lit) -> Option<bool> {
        match self.value[j as usize] {
            -1 => None,
            v => Some((v == 1) == value),
        }
    }

    /// Record `j = value`. Its consequences are drawn when propagation reaches it.
    fn assign(&mut self, j: u32, value: bool, why: Why) {
        let k = j as usize;
        debug_assert_eq!(self.value[k], -1);
        self.value[k] = i8::from(value);
        self.level[k] = self.current_level();
        self.position[k] = self.trail.len() as u32;
        self.why[k] = why;
        self.trail.push(j);
        let v = if value { 1.0 } else { 0.0 };
        for &(i, a) in &self.columns[k] {
            self.min_activity[i as usize] += a * v - a.min(0.0);
            self.max_activity[i as usize] += a * v - a.max(0.0);
        }
    }

    fn unassign(&mut self, j: u32) {
        let k = j as usize;
        let v = if self.value[k] == 1 { 1.0 } else { 0.0 };
        for &(i, a) in &self.columns[k] {
            self.min_activity[i as usize] -= a * v - a.min(0.0);
            self.max_activity[i as usize] -= a * v - a.max(0.0);
        }
        self.phase[k] = self.value[k] == 1;
        self.value[k] = -1;
        self.heap.push(j, &self.activity);
    }

    fn backjump(&mut self, to: u32) {
        if self.current_level() <= to {
            return;
        }
        let start = self.level_start[to as usize];
        while self.trail.len() > start {
            let j = self.trail.pop().unwrap();
            self.unassign(j);
        }
        self.level_start.truncate(to as usize);
        self.head = self.trail.len();
    }

    /// Force `lit`, or report that it contradicts what already holds.
    fn force(&mut self, lit: Lit, why: Why) -> Result<(), ()> {
        match self.holds(lit) {
            Some(true) => Ok(()),
            Some(false) => Err(()),
            None => {
                self.assign(lit.0, lit.1, why);
                Ok(())
            }
        }
    }

    /// Draw every consequence of the trail from `head` on.
    fn propagate(&mut self) -> Result<(), Conflict> {
        while self.head < self.trail.len() {
            let j = self.trail[self.head];
            self.head += 1;
            let value = self.value[j as usize] == 1;

            // The conflict graph: this literal excludes others outright.
            let node = literal(j as usize, value);
            let mut clash: Option<Lit> = None;
            let mut excluded: Vec<Lit> = Vec::new();
            self.conflicts.for_each_adjacent(node, |other| {
                let k = other / 2;
                let other_value = other.is_multiple_of(2);
                if !self.binary[k as usize] {
                    return true;
                }
                match self.value[k as usize] {
                    -1 => excluded.push((k, !other_value)),
                    v if (v == 1) == other_value => {
                        clash = Some((k, other_value));
                        return false;
                    }
                    _ => {}
                }
                true
            });
            if let Some(other) = clash {
                return Err(vec![(j, value), other]);
            }
            for lit in excluded {
                if self.force(lit, Why::Excluded(j)).is_err() {
                    return Err(vec![(j, value), (lit.0, !lit.1)]);
                }
            }

            // Learned clauses watching the literal this assignment just failed. Each
            // clause watches its first two literals; a watcher that fails is replaced by
            // any other literal not yet false, and only when there is none does the clause
            // force its other watcher or become the conflict. So an assignment touches the
            // clauses that might have become unit, not every clause that mentions it.
            let failed = lit_index((j, value));
            let mut watchers = std::mem::take(&mut self.watching[failed]);
            let mut keep = 0;
            let mut outcome: Result<(), Conflict> = Ok(());
            let mut index = 0;
            while index < watchers.len() {
                let id = watchers[index];
                index += 1;
                let clause = &mut self.learned[id as usize];
                // The failed watcher goes second.
                if clause[0] == (j, !value) {
                    clause.swap(0, 1);
                }
                let first = clause[0];
                let first_holds = match self.value[first.0 as usize] {
                    -1 => None,
                    v => Some((v == 1) == first.1),
                };
                if first_holds == Some(true) {
                    watchers[keep] = id;
                    keep += 1;
                    continue;
                }
                let replacement = (2..clause.len()).find(|&t| {
                    let (k, w) = clause[t];
                    let v = self.value[k as usize];
                    v == -1 || (v == 1) == w
                });
                if let Some(t) = replacement {
                    clause.swap(1, t);
                    let (k, w) = clause[1];
                    self.watching[lit_index((k, !w))].push(id);
                    continue;
                }
                watchers[keep] = id;
                keep += 1;
                match first_holds {
                    None => self.assign(first.0, first.1, Why::Clause(id)),
                    _ => {
                        outcome = Err(clause.iter().map(|&(k, w)| (k, !w)).collect());
                        // The rest keep their watch unexamined.
                        while index < watchers.len() {
                            watchers[keep] = watchers[index];
                            keep += 1;
                            index += 1;
                        }
                    }
                }
            }
            watchers.truncate(keep);
            // Anything pushed onto this list while it was taken belongs to it too.
            let added = std::mem::take(&mut self.watching[failed]);
            watchers.extend(added);
            self.watching[failed] = watchers;
            outcome?;

            // Rows.
            for r in 0..self.columns[j as usize].len() {
                let (i, _) = self.columns[j as usize][r];
                self.propagate_row(i)?;
            }
        }
        Ok(())
    }

    fn propagate_row(&mut self, i: u32) -> Result<(), Conflict> {
        let row = &self.rows[i as usize];
        let low = self.min_activity[i as usize];
        let scale = 1.0 + row.lb.abs().min(row.ub.abs());
        if row.upper_usable {
            if low > row.ub + TOL * scale {
                return Err(self.row_conflict(i, true));
            }
            let slack = row.ub - low;
            if row.largest > slack + TOL * scale {
                for e in 0..self.rows[i as usize].entries.len() {
                    let (k, a) = self.rows[i as usize].entries[e];
                    if self.value[k as usize] == -1 && a.abs() > slack + TOL * scale {
                        // The value that adds nothing to the least activity.
                        self.assign(k, a < 0.0, Why::Upper(i));
                    }
                }
            }
        }
        let row = &self.rows[i as usize];
        if row.lower_usable {
            let high = self.max_activity[i as usize];
            if high < row.lb - TOL * scale {
                return Err(self.row_conflict(i, false));
            }
            let slack = high - row.lb;
            if row.largest > slack + TOL * scale {
                for e in 0..self.rows[i as usize].entries.len() {
                    let (k, a) = self.rows[i as usize].entries[e];
                    if self.value[k as usize] == -1 && a.abs() > slack + TOL * scale {
                        // The value that keeps everything in the greatest activity.
                        self.assign(k, a > 0.0, Why::Lower(i));
                    }
                }
            }
        }
        Ok(())
    }

    /// The assignments in row `i` that pushed it towards the side that broke, largest
    /// first and only as many as it takes, among those made before trail position
    /// `before`.
    ///
    /// On the upper side a column contributes by sitting at the value that is *not* its
    /// least: `a > 0` at one, `a < 0` at zero. What has to be explained is that the least
    /// activity passed `ub - allowance`, where the allowance is what the implied literal's
    /// other value would have added (zero for a conflict).
    fn row_reason(&mut self, i: u32, upper: bool, before: u32, allowance: f64) -> Vec<Lit> {
        // Upper side: least activity `L = C + base + sum |a|` over the columns sitting at
        // the value that raises it, `base` being every binary at its least. Lower side,
        // mirrored: `U = C + top - sum |a|` over those lowering it. `C` is what the
        // non-binary columns contribute, recovered from the maintained activity.
        let row = &self.rows[i as usize];
        self.scratch.clear();
        let (mut extreme, mut all_pushes) = (0.0f64, 0.0f64);
        for &(k, a) in &row.entries {
            extreme += if upper { a.min(0.0) } else { a.max(0.0) };
            let v = self.value[k as usize];
            if v == -1 {
                continue;
            }
            let pushing = if upper { (v == 1) == (a > 0.0) } else { (v == 1) == (a < 0.0) };
            if pushing {
                all_pushes += a.abs();
                if self.position[k as usize] < before {
                    self.scratch.push((k, a.abs()));
                }
            }
        }
        // What the pushes chosen have to exceed.
        let target = if upper {
            let constant = self.min_activity[i as usize] - extreme - all_pushes;
            row.ub - allowance - constant - extreme
        } else {
            let constant = self.max_activity[i as usize] - extreme + all_pushes;
            constant + extreme - row.lb - allowance
        };
        self.scratch
            .sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut reason = Vec::new();
        let mut sum = 0.0;
        for &(k, push) in &self.scratch {
            reason.push((k, self.value[k as usize] == 1));
            sum += push;
            if sum > target {
                break;
            }
        }
        reason
    }

    fn row_conflict(&mut self, i: u32, upper: bool) -> Conflict {
        self.row_reason(i, upper, u32::MAX, 0.0)
    }

    /// The assignments that forced `j`.
    fn reason(&mut self, j: u32) -> Vec<Lit> {
        match self.why[j as usize] {
            Why::Given | Why::Decision => Vec::new(),
            Why::Excluded(from) => vec![(from, self.value[from as usize] == 1)],
            Why::Clause(id) => self.learned[id as usize]
                .iter()
                .filter(|&&(k, _)| k != j)
                .map(|&(k, w)| (k, !w))
                .collect(),
            Why::Upper(i) | Why::Lower(i) => {
                let upper = matches!(self.why[j as usize], Why::Upper(_));
                let a = self.rows[i as usize]
                    .entries
                    .iter()
                    .find(|&&(k, _)| k == j)
                    .map_or(0.0, |&(_, a)| a);
                self.row_reason(i, upper, self.position[j as usize], a.abs())
            }
        }
    }

    /// First-UIP analysis. Returns the learned clause, asserting literal first, and the
    /// level to back up to.
    fn analyze(&mut self, conflict: Conflict) -> (Clause, u32) {
        let current = self.current_level();
        let mut learned: Clause = vec![(0, false)];
        let mut pending = 0usize;
        let mut marked: Vec<u32> = Vec::new();
        let mut index = self.trail.len();
        let mut incoming = conflict;
        loop {
            for (k, v) in incoming.drain(..) {
                let kk = k as usize;
                if self.seen[kk] || self.level[kk] == 0 {
                    continue;
                }
                self.seen[kk] = true;
                marked.push(k);
                self.bump_activity(k);
                if self.level[kk] == current {
                    pending += 1;
                } else {
                    learned.push((k, !v));
                }
            }
            // The latest assignment of this level still waiting to be explained.
            let j = loop {
                index -= 1;
                let j = self.trail[index];
                if self.seen[j as usize] && self.level[j as usize] == current {
                    break j;
                }
            };
            self.seen[j as usize] = false;
            pending -= 1;
            if pending == 0 {
                learned[0] = (j, self.value[j as usize] != 1);
                break;
            }
            incoming = self.reason(j);
        }
        for k in marked {
            self.seen[k as usize] = false;
        }
        let back = learned[1..]
            .iter()
            .map(|&(k, _)| self.level[k as usize])
            .max()
            .unwrap_or(0);
        // The literal at the backjump level goes second, as a watcher would want; nothing
        // here relies on it, but it keeps the clause's order meaningful to read.
        if learned.len() > 2 {
            let at = 1 + learned[1..]
                .iter()
                .position(|&(k, _)| self.level[k as usize] == back)
                .unwrap();
            learned.swap(1, at);
        }
        (learned, back)
    }

    fn bump_activity(&mut self, j: u32) {
        self.activity[j as usize] += self.bump;
        if self.activity[j as usize] > 1e100 {
            for a in &mut self.activity {
                *a *= 1e-100;
            }
            self.bump *= 1e-100;
        }
        self.heap.bumped(j, &self.activity);
    }

    fn add_learned(&mut self, clause: Clause) -> u32 {
        let id = self.learned.len() as u32;
        // Watched on its first two literals, woken when either fails. `analyze` puts the
        // asserting literal first and one from the backjump level second, which are the
        // last two to become false, as a pair of watchers must be. A unit clause needs no
        // watch: it is asserted at level zero and never undone.
        for &lit in clause.iter().take(2) {
            if clause.len() > 1 {
                self.watching[lit_index((lit.0, !lit.1))].push(id);
            }
        }
        self.learned.push(clause);
        id
    }

    fn next_decision(&mut self) -> Option<u32> {
        while let Some(j) = self.heap.pop(&self.activity) {
            if self.value[j as usize] == -1 {
                return Some(j);
            }
        }
        None
    }
}

/// The Luby sequence, 1 1 2 1 1 2 4 1 1 2 ..., which spaces restarts so that no single
/// guess at the right interval can be wrong by more than a logarithmic factor.
fn luby(mut i: usize) -> usize {
    let mut size = 1usize;
    let mut sequence = 0u32;
    while size < i + 1 {
        sequence += 1;
        size = 2 * size + 1;
    }
    while size - 1 != i {
        size = (size - 1) / 2;
        sequence -= 1;
        i %= size;
    }
    1usize << sequence
}

impl Search<'_> {
    /// Set the starting activities and phases, and draw level zero. False if level zero
    /// is already contradictory.
    fn start(&mut self, problem: &Problem) -> bool {
        let n = problem.n_cols();
        // A column that excludes many others is where contradictions are found quickly,
        // as the dive this replaces measured. Scaled far below one bump so the first
        // conflicts take over at once.
        let degree = |j: usize| {
            self.conflicts.degree(literal(j, true)) + self.conflicts.degree(literal(j, false))
        };
        let scale = (0..n)
            .filter(|&j| self.binary[j])
            .map(degree)
            .max()
            .unwrap_or(0)
            .max(1) as f64;
        for j in 0..n {
            if self.binary[j] {
                self.activity[j] = 1e-3 * degree(j) as f64 / scale;
                // Each column first takes the value the objective prefers, so the first
                // point found is already aimed somewhere. On `acc-tight5` the other way
                // round finds a point worth 1 and needs a second search to find the 0.
                self.phase[j] = problem.obj[j] < 0.0;
            }
        }
        for j in 0..n {
            if self.binary[j] && problem.col_lb[j] == problem.col_ub[j] {
                self.assign(j as u32, problem.col_lb[j] > 0.5, Why::Given);
            }
        }
        for i in 0..self.rows.len() as u32 {
            if self.propagate_row(i).is_err() {
                return false;
            }
        }
        if self.propagate().is_err() {
            return false;
        }
        for j in 0..n {
            if self.binary[j] && self.value[j] == -1 {
                self.heap.push(j as u32, &self.activity);
            }
        }
        true
    }

    /// Lower the objective row's bound. Every clause learned so far stays valid, since it
    /// held for a superset of the points that satisfy the tighter row. False if level zero
    /// now contradicts it.
    fn tighten(&mut self, ub: f64) -> bool {
        let Some(i) = self.cutoff_row else {
            return true;
        };
        self.backjump(0);
        let row = &mut self.rows[i as usize];
        row.ub = row.ub.min(ub);
        row.upper_usable = row.ub.is_finite() && row.rest_min_finite;
        if self.propagate_row(i).is_err() {
            return false;
        }
        self.propagate().is_ok()
    }

    /// Search until a point, a proof, or the budget. Resumable: the trail, the clauses and
    /// the activities carry over to the next call.
    fn run(&mut self, problem: &Problem, limits: &Limits<'_>, out: &mut Outcome) -> Verdict {
        let n = problem.n_cols();
        let pure = (0..n).all(|j| self.binary[j]);
        let mut restart_index = 0usize;
        let mut until_restart = RESTART_UNIT * luby(restart_index);
        loop {
            match self.propagate() {
                Err(conflict) => {
                    out.conflicts += 1;
                    // A conflict need not involve the current level at all: a row's
                    // reason is its largest contributions, which may all be older. It is
                    // then a conflict of the highest level it does involve, and is
                    // analysed there.
                    let highest = conflict
                        .iter()
                        .map(|&(k, _)| self.level[k as usize])
                        .max()
                        .unwrap_or(0);
                    if highest == 0 {
                        return Verdict::Infeasible;
                    }
                    self.backjump(highest);
                    let (clause, back) = self.analyze(conflict);
                    self.backjump(back);
                    let asserting = clause[0];
                    if clause.len() == 1 {
                        out.fixings.push(asserting);
                    } else if clause.len() <= limits.export_length
                        && out.clauses.len() < limits.export_count
                    {
                        out.clauses.push(clause.clone());
                    }
                    let id = self.add_learned(clause);
                    let why = if back == 0 { Why::Given } else { Why::Clause(id) };
                    self.assign(asserting.0, asserting.1, why);
                    self.bump /= ACTIVITY_DECAY;

                    until_restart = until_restart.saturating_sub(1);
                    if out.conflicts >= limits.max_conflicts {
                        return Verdict::Unknown;
                    }
                    if out.conflicts.is_multiple_of(64) && limits.expired() {
                        return Verdict::Unknown;
                    }
                }
                Ok(()) => {
                    if until_restart == 0 {
                        restart_index += 1;
                        until_restart = RESTART_UNIT * luby(restart_index);
                        self.backjump(0);
                        continue;
                    }
                    let Some(j) = self.next_decision() else {
                        // Everything assigned and nothing contradicted. Checked rather
                        // than believed, since the row tests carry a tolerance.
                        if !pure {
                            return Verdict::Unknown;
                        }
                        let x: Vec<f64> = self.value.iter().map(|&v| f64::from(v)).collect();
                        let within = self.cutoff_row.is_none_or(|i| {
                            let row = &self.rows[i as usize];
                            let v: f64 = row.entries.iter().map(|&(k, a)| a * x[k as usize]).sum();
                            v <= row.ub + 1e-9 * (1.0 + row.ub.abs())
                        });
                        return if within && crate::heuristic::is_feasible(problem, &x, 1e-9) {
                            Verdict::Solution(x)
                        } else {
                            Verdict::Unknown
                        };
                    };
                    out.decisions += 1;
                    if out.decisions.is_multiple_of(1024) && limits.expired() {
                        return Verdict::Unknown;
                    }
                    self.level_start.push(self.trail.len());
                    let value = self.phase[j as usize];
                    self.assign(j, value, Why::Decision);
                }
            }
        }
    }
}

fn empty_outcome() -> Outcome {
    Outcome {
        verdict: Verdict::Unknown,
        clauses: Vec::new(),
        fixings: Vec::new(),
        conflicts: 0,
        decisions: 0,
    }
}

/// Search `problem` once, with `cutoff` added as a row `sum a_j x_j <= ub` if given.
///
/// The cutoff is how the search is aimed at *better* points: with the incumbent's value as
/// the bound, every clause it learns holds for every point the real search still wants,
/// which is the footing reduced cost fixing already stands on, and exhausting it proves the
/// incumbent optimal.
pub fn search(
    problem: &Problem,
    conflicts: &Conflicts,
    cutoff: Option<&(Vec<(usize, f64)>, f64)>,
    limits: Limits<'_>,
) -> Outcome {
    let mut s = Search::new(problem, cutoff, conflicts);
    let mut out = empty_outcome();
    out.verdict = if s.start(problem) {
        s.run(problem, &limits, &mut out)
    } else {
        Verdict::Infeasible
    };
    out
}

/// What [`optimize`] found.
#[derive(Clone, Debug)]
pub struct Optimized {
    /// The best point found and its objective, `sum obj_j x_j` with no offset.
    pub best: Option<(f64, Vec<f64>)>,
    /// Whether nothing better than `best` exists -- or, with no `best`, whether the model
    /// has no point at all below the starting cutoff.
    pub proved: bool,
    /// Learned on the way. Valid for every point better than the starting cutoff, not for
    /// every point of the model.
    pub clauses: Vec<Clause>,
    pub fixings: Vec<(u32, bool)>,
    pub conflicts: usize,
    /// Points found, each better than the last.
    pub improvements: usize,
    /// When each point, and the proof if there was one, arrived: conflicts so far and
    /// seconds since the search began. What a stopping rule should be measured against.
    pub trace: Vec<(usize, f64)>,
}

/// Find a point, then keep asking for a better one on the same search until there is none.
///
/// `next(v)` is the bound a point has to meet to count as better than one worth `v`: `v`
/// less one objective step when the objective moves in whole steps, less the gap otherwise.
/// Tightening the objective row in place keeps every clause learned under the looser one,
/// so each search starts where the last left off rather than from nothing, and the final
/// one ending in a contradiction is a proof of optimality that needed no relaxation.
pub fn optimize(
    problem: &Problem,
    conflicts: &Conflicts,
    start_cutoff: Option<f64>,
    next: impl Fn(f64) -> f64,
    limits: Limits<'_>,
    mut on_point: impl FnMut(f64, &[f64]),
) -> Optimized {
    let objective: Vec<(usize, f64)> = problem
        .obj
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, c)| c != 0.0)
        .collect();
    let row = (objective, start_cutoff.unwrap_or(f64::INFINITY));
    let mut s = Search::new(problem, Some(&row), conflicts);
    let mut out = empty_outcome();
    let mut result = Optimized {
        best: None,
        proved: false,
        clauses: Vec::new(),
        fixings: Vec::new(),
        conflicts: 0,
        improvements: 0,
        trace: Vec::new(),
    };
    let began = Instant::now();
    let mut proved = !s.start(problem) || (start_cutoff.is_some() && !s.tighten(row.1));
    while !proved {
        // An infinite `stall_seconds` means no patience limit. `from_secs_f64` panics on
        // it, so the conversion is the checked one and an unrepresentable span is none.
        let patience = result.trace.last().and_then(|&(_, at)| {
            std::time::Duration::try_from_secs_f64(at + limits.stall_seconds)
                .ok()
                .and_then(|span| began.checked_add(span))
        });
        let this_run = Limits {
            max_conflicts: limits
                .max_conflicts
                .min(out.conflicts.saturating_add(limits.stall_conflicts)),
            deadline: match (limits.deadline, patience) {
                (Some(hard), Some(own)) => Some(hard.min(own)),
                (hard, own) => hard.or(own),
            },
            ..limits
        };
        match s.run(problem, &this_run, &mut out) {
            Verdict::Solution(x) => {
                let value: f64 = problem.obj.iter().zip(&x).map(|(c, v)| c * v).sum();
                on_point(value, &x);
                result.best = Some((value, x));
                result.improvements += 1;
                result.trace.push((out.conflicts, began.elapsed().as_secs_f64()));
                // Past the row test's own tolerance, or the point just found satisfies
                // the tightened row too and is found again. So what a proof establishes
                // is that nothing beats `value` by more than the larger of the two.
                let bound = next(value).min(value - 2.0 * TOL * (1.0 + value.abs()));
                // A model whose objective is constant has nothing better to look for.
                if s.cutoff_row.is_none() || !s.tighten(bound) {
                    proved = true;
                }
            }
            Verdict::Infeasible => proved = true,
            Verdict::Unknown => break,
        }
    }
    if proved {
        result.trace.push((out.conflicts, began.elapsed().as_secs_f64()));
    }
    result.proved = proved;
    result.clauses = out.clauses;
    result.fixings = out.fixings;
    result.conflicts = out.conflicts;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Builder, RowSense, Sense};

    fn limits() -> Limits<'static> {
        Limits {
            max_conflicts: 100_000,
            deadline: None,
            export_length: 32,
            export_count: 100_000,
            stall_conflicts: usize::MAX,
            stall_seconds: f64::INFINITY,
            cancel: None,
        }
    }

    /// Every assignment of a small pure binary model, by brute force.
    fn feasible_points(problem: &Problem) -> Vec<Vec<f64>> {
        let n = problem.n_cols();
        (0..1u32 << n)
            .map(|mask| (0..n).map(|j| f64::from((mask >> j) & 1)).collect::<Vec<f64>>())
            .filter(|x| {
                (0..n).all(|j| x[j] >= problem.col_lb[j] && x[j] <= problem.col_ub[j])
                    && crate::heuristic::is_feasible(problem, x, 1e-9)
            })
            .collect()
    }

    /// Pigeonhole: `n + 1` pigeons, `n` holes. Infeasible, and famously so for
    /// propagation alone, which makes it a test that the search really searches.
    fn pigeonhole(holes: usize) -> Problem {
        let pigeons = holes + 1;
        let mut b = Builder::new(Sense::Minimize);
        let x: Vec<Vec<usize>> = (0..pigeons)
            .map(|p| (0..holes).map(|h| b.binary(&format!("x{p}_{h}"))).collect())
            .collect();
        for row in &x {
            b.row(&row.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>(), RowSense::Ge, 1.0);
        }
        for h in 0..holes {
            b.row(&(0..pigeons).map(|p| (x[p][h], 1.0)).collect::<Vec<_>>(), RowSense::Le, 1.0);
        }
        b.build()
    }

    #[test]
    fn proves_pigeonhole_infeasible() {
        for holes in 2..6 {
            let p = pigeonhole(holes);
            let conflicts = Conflicts::of(&p);
            let out = search(&p, &conflicts, None, limits());
            assert_eq!(out.verdict, Verdict::Infeasible, "{holes} holes");
        }
    }

    /// On random small models the verdict agrees with enumeration, every point returned
    /// is feasible, and every clause and fixing learned holds at every feasible point --
    /// with and without a cutoff row, since under a cutoff the clauses must hold for the
    /// points that satisfy it.
    #[test]
    fn agrees_with_enumeration() {
        use crate::generate::{Kind, Spec};
        use lp_parser_rs::problem::LpProblem;
        for kind in [Kind::Knapsack, Kind::Covering, Kind::Signed] {
            for seed in 0..40u64 {
                let spec = Spec {
                    kind,
                    n_cols: 12,
                    n_rows: 7,
                    seed,
                };
                let p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
                let conflicts = Conflicts::of(&p);
                let all = feasible_points(&p);
                let best = all
                    .iter()
                    .map(|x| p.obj.iter().zip(x).map(|(c, v)| c * v).sum::<f64>())
                    .fold(f64::INFINITY, f64::min);
                let objective: Vec<(usize, f64)> =
                    p.obj.iter().copied().enumerate().filter(|&(_, c)| c != 0.0).collect();
                let cutoffs = [None, best.is_finite().then(|| (objective.clone(), best - 1e-3))];
                for cutoff in cutoffs.iter() {
                    let points: Vec<&Vec<f64>> = all
                        .iter()
                        .filter(|x| match cutoff {
                            None => true,
                            Some((row, ub)) => {
                                row.iter().map(|&(j, c)| c * x[j]).sum::<f64>() <= *ub + 1e-9
                            }
                        })
                        .collect();
                    let out = search(&p, &conflicts, cutoff.as_ref(), limits());
                    let label = format!("{} cutoff {}", spec.name(), cutoff.is_some());
                    match &out.verdict {
                        Verdict::Infeasible => assert!(points.is_empty(), "{label}: false infeasible"),
                        Verdict::Solution(x) => {
                            assert!(crate::heuristic::is_feasible(&p, x, 1e-9), "{label}: bad point");
                            if let Some((row, ub)) = cutoff {
                                let v: f64 = row.iter().map(|&(j, c)| c * x[j]).sum();
                                assert!(v <= ub + 1e-9, "{label}: point misses the cutoff");
                            }
                        }
                        Verdict::Unknown => panic!("{label}: a tiny model ran out of budget"),
                    }
                    for x in &points {
                        for clause in &out.clauses {
                            assert!(
                                clause.iter().any(|&(j, w)| (x[j as usize] > 0.5) == w),
                                "{label}: clause {clause:?} cuts off a feasible point"
                            );
                        }
                        for &(j, w) in &out.fixings {
                            assert_eq!(x[j as usize] > 0.5, w, "{label}: fixing cuts off a point");
                        }
                    }
                }
            }
        }
    }

    /// `optimize` has to end at the true optimum and say it proved it, on every model
    /// small enough to enumerate -- which is the claim the root will act on.
    #[test]
    fn optimize_proves_the_enumerated_optimum() {
        use crate::generate::{Kind, Spec};
        use lp_parser_rs::problem::LpProblem;
        for kind in [Kind::Knapsack, Kind::Covering, Kind::Signed] {
            for seed in 0..40u64 {
                let spec = Spec {
                    kind,
                    n_cols: 12,
                    n_rows: 7,
                    seed,
                };
                let p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
                let conflicts = Conflicts::of(&p);
                let best = feasible_points(&p)
                    .iter()
                    .map(|x| p.obj.iter().zip(x).map(|(c, v)| c * v).sum::<f64>())
                    .fold(f64::INFINITY, f64::min);
                let out = optimize(&p, &conflicts, None, |v| v - 1e-6, limits(), |_, _| {});
                assert!(out.proved, "{}: not proved", spec.name());
                match out.best {
                    None => assert!(best.is_infinite(), "{}: missed every point", spec.name()),
                    Some((value, x)) => {
                        assert!((value - best).abs() < 1e-9, "{}: {value} against {best}", spec.name());
                        assert!(crate::heuristic::is_feasible(&p, &x, 1e-9));
                    }
                }
            }
        }
    }

    #[test]
    fn luby_sequence() {
        let first: Vec<usize> = (0..15).map(luby).collect();
        assert_eq!(first, vec![1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8]);
    }
}
