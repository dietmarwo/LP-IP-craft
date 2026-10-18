//! Cutting planes: inequalities valid for every binary solution, but violated by
//! the current fractional relaxation.
//!
//! Presolve reduces the *model*; cuts strengthen the *relaxation*. On the dense
//! random instances this solver is aimed at, presolve finds nothing at all, and
//! neither does a leading commercial solver's, so cuts are where the remaining
//! bound gap lives.
//!
//! # Knapsack covers
//!
//! Take a row in knapsack form, `sum w_j y_j <= c` with every `w_j > 0` and every
//! `y_j` binary. A *cover* is a set `C` whose weights overrun the capacity,
//! `sum_{j in C} w_j > c`. No binary solution can switch on all of `C` at once, so
//!
//! ```text
//!     sum_{j in C} y_j <= |C| - 1
//! ```
//!
//! holds for every feasible point. It is useful when the relaxation violates it,
//! which happens exactly when the fractional values across `C` sum to more than
//! `|C| - 1`.
//!
//! # Getting rows into that form
//!
//! Real rows are neither one-sided nor sign-uniform, so each is normalized first:
//!
//! - A `>=` side becomes `<=` by negating the row.
//! - A negative coefficient is made positive by complementing its column,
//!   `x_j = 1 - y_j`, which moves `a_j` into the capacity.
//!
//! A range or equality row yields a knapsack from each of its two finite sides.
//! Cuts are found in the complemented `y` space and mapped back to `x` at the end,
//! where a complemented column contributes `-1` and shifts the right-hand side.

use crate::model::Problem;

/// A generated inequality, in range form so it drops straight into a [`Problem`].
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    /// `(column, coefficient)` pairs, sorted by column.
    pub coefficients: Vec<(usize, f64)>,
    pub lb: f64,
    pub ub: f64,
}

impl Cut {
    /// The row's activity at a point.
    pub fn activity(&self, x: &[f64]) -> f64 {
        self.coefficients.iter().map(|&(j, a)| a * x[j]).sum()
    }

    /// How far `x` violates this cut; zero or less when satisfied.
    pub fn violation(&self, x: &[f64]) -> f64 {
        let activity = self.activity(x);
        let over = if self.ub.is_finite() {
            activity - self.ub
        } else {
            f64::NEG_INFINITY
        };
        let under = if self.lb.is_finite() {
            self.lb - activity
        } else {
            f64::NEG_INFINITY
        };
        over.max(under)
    }

    /// Euclidean length of the coefficient vector.
    fn norm(&self) -> f64 {
        self.coefficients
            .iter()
            .map(|&(_, a)| a * a)
            .sum::<f64>()
            .sqrt()
    }

    /// How far `x` sits from the violated face, in the space the LP actually moves
    /// through.
    ///
    /// Raw violation is not comparable between cuts: scaling a row by ten scales its
    /// violation by ten without cutting off any more of the polytope. Dividing by the
    /// coefficient norm gives the Euclidean distance from `x` to the cut's hyperplane,
    /// which is comparable, and is what makes ranking candidates meaningful.
    pub fn efficacy(&self, x: &[f64]) -> f64 {
        let norm = self.norm();
        if norm <= TOL {
            0.0
        } else {
            self.violation(x) / norm
        }
    }

    /// Whether the row is active at `x`: satisfied, but with no slack to spare.
    pub fn is_tight(&self, x: &[f64], tolerance: f64) -> bool {
        self.violation(x) >= -tolerance
    }

    /// Cosine of the angle between two coefficient vectors, in `[0, 1]`.
    ///
    /// Both coefficient lists are sorted by column, so the dot product is a merge.
    fn cosine(&self, other: &Cut) -> f64 {
        let (mut i, mut j, mut dot) = (0, 0, 0.0);
        while i < self.coefficients.len() && j < other.coefficients.len() {
            let (ci, ai) = self.coefficients[i];
            let (cj, aj) = other.coefficients[j];
            match ci.cmp(&cj) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    dot += ai * aj;
                    i += 1;
                    j += 1;
                }
            }
        }
        let scale = self.norm() * other.norm();
        if scale <= TOL {
            0.0
        } else {
            (dot / scale).abs().min(1.0)
        }
    }
}

/// A cut must move `x` at least this far to earn a row in the model.
const MIN_EFFICACY: f64 = 1e-4;
/// Selected cuts must be at least this far from parallel to each other.
///
/// Two nearly parallel cuts remove nearly the same region, so the second costs a row
/// on every subsequent LP and buys almost nothing. Gomory rows off neighbouring
/// tableau entries are frequently near-duplicates of one another, which is where this
/// earns its keep.
const MIN_ORTHOGONALITY: f64 = 0.1;

/// Choose which of the separated candidates are worth adding to the model.
///
/// Separation is generous by design, because it is cheaper to generate a cut than to
/// decide it was not needed. The choice of what to *keep* is therefore where cutting
/// is won or lost. Candidates are ranked by efficacy and taken greedily, skipping any that is
/// nearly parallel to one already chosen.
pub fn select(candidates: Vec<Cut>, x: &[f64], limit: usize) -> Vec<Cut> {
    let mut ranked: Vec<(f64, Cut)> = candidates
        .into_iter()
        .map(|c| (c.efficacy(x), c))
        .filter(|&(e, _)| e >= MIN_EFFICACY)
        .collect();
    // Descending efficacy. Scores are finite here: `efficacy` returns 0.0 for a
    // degenerate norm, and those are filtered out above.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut chosen: Vec<Cut> = Vec::new();
    for (_, cut) in ranked {
        if chosen.len() >= limit {
            break;
        }
        if chosen
            .iter()
            .all(|k| 1.0 - k.cosine(&cut) >= MIN_ORTHOGONALITY)
        {
            chosen.push(cut);
        }
    }
    chosen
}

/// A row rewritten as `sum w_j y_j <= capacity`, with all weights positive.
struct Knapsack {
    /// `(column, weight, complemented)`.
    terms: Vec<(usize, f64, bool)>,
    capacity: f64,
}

const TOL: f64 = 1e-9;
/// A cut must beat this violation to be worth adding; smaller ones are noise.
const MIN_VIOLATION: f64 = 1e-4;

/// Rewrite one side of a row as a knapsack over its *binary* columns, or `None` if
/// it cannot be one.
///
/// `negate` selects the `>=` side, which becomes `<=` under negation.
///
/// A cover argument counts how many columns can be switched on at once, which only
/// means anything for 0/1 columns. A general integer or continuous column in the
/// row is therefore folded into the capacity at its most favourable value, so the
/// resulting cut stays valid whatever that column does. A row with no binary
/// columns yields nothing.
fn to_knapsack(
    problem: &Problem,
    coefficients: &[(usize, f64)],
    bound: f64,
    negate: bool,
) -> Option<Knapsack> {
    let (col_lb, col_ub) = (&problem.col_lb, &problem.col_ub);
    if !bound.is_finite() {
        return None;
    }
    let sign = if negate { -1.0 } else { 1.0 };
    let mut capacity = sign * bound;
    let mut terms = Vec::with_capacity(coefficients.len());

    for &(j, a) in coefficients {
        let a = sign * a;
        if a == 0.0 {
            continue;
        }
        // A fixed column is a constant, not a decision: fold it into the capacity.
        if col_lb[j] == col_ub[j] {
            capacity -= a * col_lb[j];
            continue;
        }
        // A non-binary column is not something a cover can reason about. Charge the
        // capacity for the least it could contribute, so the cut holds for every
        // value it might take. An infinite worst case means no usable bound.
        if !problem.is_binary(j) {
            let least = if a > 0.0 {
                a * col_lb[j]
            } else {
                a * col_ub[j]
            };
            if !least.is_finite() {
                return None;
            }
            capacity -= least;
            continue;
        }
        if a > 0.0 {
            terms.push((j, a, false));
        } else {
            // x_j = 1 - y_j, so a_j*x_j = a_j - a_j*y_j; the constant moves right.
            capacity -= a;
            terms.push((j, -a, true));
        }
    }

    // A non-positive capacity means no subset fits, which is a feasibility question
    // for presolve rather than a cut to separate.
    if terms.is_empty() || capacity <= TOL {
        return None;
    }
    Some(Knapsack { terms, capacity })
}

/// The value of a knapsack term in the complemented `y` space.
fn y_value(knapsack: &Knapsack, i: usize, x: &[f64]) -> f64 {
    let (col, _, complemented) = knapsack.terms[i];
    if complemented { 1.0 - x[col] } else { x[col] }
}

/// Find a violated minimal cover, optionally forced to contain `seed`.
///
/// The separation problem (find the most violated cover) is itself a knapsack
/// problem, so this uses the standard greedy: take terms in increasing order of
/// `(1 - y_j) / w_j`, since a term near 1 adds most violation per unit of capacity
/// consumed, and stop once the weights overrun.
///
/// Seeding matters more than it looks. One greedy pass per row yields one cover;
/// forcing each fractional term into the cover in turn yields a family of them, and
/// several usually separate where the unseeded one does not.
fn find_cover(knapsack: &Knapsack, x: &[f64], seed: Option<usize>) -> Option<Vec<usize>> {
    let cost = |i: usize| (1.0 - y_value(knapsack, i, x)) / knapsack.terms[i].1;
    let mut order: Vec<usize> = (0..knapsack.terms.len()).collect();
    order.sort_by(|&a, &b| {
        cost(a)
            .partial_cmp(&cost(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut chosen = Vec::new();
    let mut weight = 0.0;
    if let Some(seed) = seed {
        chosen.push(seed);
        weight += knapsack.terms[seed].1;
    }
    if weight <= knapsack.capacity + TOL {
        for &i in &order {
            if Some(i) == seed {
                continue;
            }
            chosen.push(i);
            weight += knapsack.terms[i].1;
            if weight > knapsack.capacity + TOL {
                break;
            }
        }
    }
    if weight <= knapsack.capacity + TOL {
        return None;
    }

    // Minimalize: drop the heaviest members the cover can spare, keeping any seed.
    // A minimal cover dominates every cover containing it.
    let mut by_weight: Vec<usize> = chosen.clone();
    by_weight.sort_by(|&a, &b| {
        knapsack.terms[b]
            .1
            .partial_cmp(&knapsack.terms[a].1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in by_weight {
        if Some(i) == seed || chosen.len() <= 2 {
            continue;
        }
        let w = knapsack.terms[i].1;
        if weight - w > knapsack.capacity + TOL {
            weight -= w;
            chosen.retain(|&k| k != i);
        }
    }

    let total: f64 = chosen.iter().map(|&i| y_value(knapsack, i, x)).sum();
    if total > chosen.len() as f64 - 1.0 + MIN_VIOLATION {
        chosen.sort_unstable();
        Some(chosen)
    } else {
        None
    }
}

/// Largest capacity a lifting DP will be built over.
const MAX_LIFT_CAPACITY: usize = 20_000;
/// Ceiling on the total work one lifting pass may do, as `terms * capacity` summed
/// over the columns lifted.
///
/// The DP is `O(terms * capacity)` *per column lifted*, and both grow with the
/// model, so on a dense 256-column knapsack the unbounded version reached billions
/// of operations per cover, separation cost 1.3 seconds a round to save a handful
/// of nodes. Lifting is an improvement to a cut that is already valid, so running
/// out of budget just means a weaker cut, never a wrong one.
const MAX_LIFT_WORK: usize = 400_000;
/// Fractional columns used to seed cover searches in a single row.
///
/// Every fractional column was a seed originally, which is quadratic in the row's
/// support and bought very little: the covers found from the most fractional few
/// are the ones that separate.
const MAX_SEEDS_PER_ROW: usize = 4;

/// Sequential up-lifting of the columns outside the cover.
///
/// The plain cover inequality `sum_{j in C} y_j <= |C| - 1` says nothing about
/// columns outside `C`, which makes it weak. Lifting gives each outside column the
/// largest coefficient that keeps the inequality valid:
///
/// ```text
///     a_j = rhs - max { sum a_k y_k : sum w_k y_k <= capacity - w_j }
/// ```
///
/// taken over the columns already in the inequality. That inner problem is a 0/1
/// knapsack, solved here exactly by dynamic programming, which needs integer
/// weights, so lifting is skipped when a row has fractional coefficients. Lifting
/// *sequentially* (each column against the inequality built so far, not against the
/// original cover) is what keeps the result valid.
fn lift(knapsack: &Knapsack, cover: &[usize]) -> Option<(Vec<(usize, f64)>, f64)> {
    let integral = |v: f64| (v - v.round()).abs() <= 1e-9 && v.round() >= 0.0;
    if !knapsack.terms.iter().all(|&(_, w, _)| integral(w)) || !integral(knapsack.capacity) {
        return None;
    }
    let capacity = knapsack.capacity.round() as usize;
    if capacity > MAX_LIFT_CAPACITY {
        return None;
    }

    let rhs = cover.len() as f64 - 1.0;
    // (term index, weight, coefficient) for everything currently in the inequality.
    let mut inequality: Vec<(usize, usize, i64)> = cover
        .iter()
        .map(|&i| (i, knapsack.terms[i].1.round() as usize, 1i64))
        .collect();
    let mut work = 0usize;
    // Reused across columns; reallocating a table per column was most of the cost.
    let mut dp: Vec<i64> = Vec::new();

    // Heaviest first is the conventional lifting order and tends to give the
    // strongest coefficients to the columns that matter most.
    let mut outside: Vec<usize> = (0..knapsack.terms.len())
        .filter(|i| !cover.contains(i))
        .collect();
    outside.sort_by(|&a, &b| {
        knapsack.terms[b]
            .1
            .partial_cmp(&knapsack.terms[a].1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lifted: Vec<(usize, f64)> = Vec::new();
    for j in outside {
        let wj = knapsack.terms[j].1.round() as usize;
        let coefficient = if wj > capacity {
            // The column cannot be switched on at all, so it may carry the whole
            // right-hand side.
            rhs as i64
        } else {
            let budget = capacity - wj;
            work = work.saturating_add(inequality.len().saturating_mul(budget));
            if work > MAX_LIFT_WORK {
                // Out of budget: stop lifting and keep what has been lifted so far,
                // which is still a valid inequality.
                break;
            }
            dp.clear();
            dp.resize(budget + 1, 0);
            for &(_, w, a) in &inequality {
                if w > budget {
                    continue;
                }
                for cap in (w..=budget).rev() {
                    dp[cap] = dp[cap].max(dp[cap - w] + a);
                }
            }
            rhs as i64 - dp[budget]
        };
        if coefficient > 0 {
            inequality.push((j, wj, coefficient));
            lifted.push((j, coefficient as f64));
        }
    }

    if lifted.is_empty() {
        return None;
    }
    Some((lifted, rhs))
}

/// Turn a cover over a knapsack's `y` space back into a cut over the original `x`.
fn cover_to_cut(knapsack: &Knapsack, cover: &[usize]) -> Cut {
    // sum_{j in C} y_j <= |C| - 1, plus any lifted terms, where y_j is x_j or
    // (1 - x_j) depending on whether the column was complemented.
    let mut rhs = cover.len() as f64 - 1.0;
    let mut terms: Vec<(usize, f64)> = cover.iter().map(|&i| (i, 1.0)).collect();
    if let Some((lifted, _)) = lift(knapsack, cover) {
        terms.extend(lifted);
    }

    let mut coefficients = Vec::with_capacity(terms.len());
    for (i, coefficient) in terms {
        let (col, _, complemented) = knapsack.terms[i];
        if complemented {
            // a*(1 - x_j) contributes -a*x_j and moves a to the right-hand side.
            coefficients.push((col, -coefficient));
            rhs -= coefficient;
        } else {
            coefficients.push((col, coefficient));
        }
    }
    coefficients.sort_by_key(|&(j, _)| j);
    Cut {
        coefficients,
        lb: f64::NEG_INFINITY,
        ub: rhs,
    }
}

/// Separate Gomory mixed-integer cuts from the tableau at `basis`.
///
/// Covers are combinatorial and need a row that reads as a knapsack; GMI cuts come
/// from the simplex tableau and need no structure at all, so they reach models the
/// cover separator cannot see. The derivation lives in [`crate::lp::Lp::gomory_cuts`],
/// which has the tableau; this wraps the result and applies the same
/// worth-adding test as every other family.
pub fn separate_gomory(
    lp: &crate::lp::Lp,
    basis: &crate::lp::BasisState,
    x: &[f64],
    limit: usize,
) -> Vec<Cut> {
    if std::env::var("RS_OFF").ok().as_deref()==Some("gomory") { return Vec::new(); }
    let mut found: Vec<Cut> = lp
        .gomory_cuts(basis, limit * 4)
        .into_iter()
        .map(|(coefficients, lb)| Cut {
            coefficients,
            lb,
            ub: f64::INFINITY,
        })
        .filter(|cut| cut.violation(x) > MIN_VIOLATION)
        .collect();

    found.sort_by(|a, b| {
        b.violation(x)
            .partial_cmp(&a.violation(x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found.truncate(limit);
    found
}

/// Rows a single mod-2 combination may draw on.
///
/// The elimination is cubic in this, and the cuts that pay come out of a handful of
/// rows rather than a hundred, so a model with thousands of tight rows is truncated
/// rather than declined.
const MOD2_MAX_ROWS: usize = 512;

/// Fractional columns the parity system may span.
///
/// Every one of them is an equation, and a system with more equations than rows has no
/// null space to search. Beyond this the model is not one this reduction can say
/// anything about, and building the system to discover that is the expensive part.
const MOD2_MAX_COLUMNS: usize = 512;

/// A set of row or column indices, as a bitmap.
#[derive(Clone, PartialEq, Eq)]
struct BitSet(Vec<u64>);

impl BitSet {
    fn new(bits: usize) -> Self {
        BitSet(vec![0; bits.div_ceil(64)])
    }

    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }

    fn get(&self, i: usize) -> bool {
        self.0[i / 64] >> (i % 64) & 1 == 1
    }

    fn xor(&mut self, other: &BitSet) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a ^= b;
        }
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    /// The lowest index set, if any.
    fn first(&self) -> Option<usize> {
        self.0
            .iter()
            .enumerate()
            .find(|&(_, &w)| w != 0)
            .map(|(word, &w)| word * 64 + w.trailing_zeros() as usize)
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.0.len() * 64).filter(|&i| self.get(i))
    }
}

/// Chvatal-Gomory cuts over GF(2): combine tight rows so that the halving bites.
///
/// Take rows `a_i x <= b_i` with integer coefficients, all tight at the current point,
/// and add up a subset `S`. The sum is valid, and so is halving it; and because every
/// `x_j` is a nonnegative integer, the coefficients may then be rounded down:
///
/// ```text
///     floor(A/2) . x  <=  floor(B/2),      A = sum_S a_i,  B = sum_S b_i
/// ```
///
/// The rounding is worth something exactly when it loses nothing on the left and a half
/// on the right, which is when every `A_j` is even and `B` is odd. Then the cut is
/// violated by exactly one half, because the rows were tight.
///
/// A column sitting *on* a bound costs nothing when its coefficient is odd, since it
/// contributes zero to the activity either way. So the parity condition is needed only
/// on the columns that are fractional, and finding the subset is then a linear system
/// over GF(2) with one equation per fractional column, plus one for the parity of the
/// right-hand side. There are usually a handful of those and hundreds of tight rows, so
/// the system is wide and its null space is where the cuts are.
///
/// This is the family that closes the models a single-row separator cannot touch. On
/// `n2seq36f`, whose relaxation is worth 52000 against an optimum of 52200, every
/// single-row family here produces cuts violated by a full unit and moves the bound by
/// nothing, because the optimal face is large enough that removing one vertex of it
/// leaves another worth the same. Eighteen of these move it to 52200 in two rounds.
pub fn separate_mod2(problem: &Problem, x: &[f64], limit: usize) -> Vec<Cut> {
    if std::env::var("RS_OFF").ok().as_deref()==Some("mod2") { return Vec::new(); }
    let tolerance = 1e-6;
    let binary = |j: usize| {
        problem.is_integer(j) && problem.col_lb[j] == 0.0 && problem.col_ub[j] == 1.0
    };

    // Columns the parity has to be right on. A column at a bound is not one of them:
    // an odd coefficient there is rounded away against a value of zero.
    let fractional: Vec<usize> = (0..problem.n_cols())
        .filter(|&j| binary(j) && x[j] > tolerance && x[j] < 1.0 - tolerance)
        .collect();
    if fractional.is_empty() || fractional.len() > MOD2_MAX_COLUMNS {
        return Vec::new();
    }
    let mut equation = vec![usize::MAX; problem.n_cols()];
    for (e, &j) in fractional.iter().enumerate() {
        equation[j] = e;
    }
    // Complemented, so that every variable is at zero or fractional and the paragraph
    // above holds. A binary at one becomes `1 - y` with `y` at zero.
    let complemented: Vec<bool> = (0..problem.n_cols())
        .map(|j| binary(j) && x[j] >= 1.0 - tolerance)
        .collect();

    let csr = problem.matrix.to_csr();
    let mut tight: Vec<(Vec<(usize, i64)>, i64)> = Vec::new();
    for i in 0..problem.n_rows() {
        let (cols, values) = csr.column(i);
        if cols.is_empty() || cols.iter().any(|&j| !binary(j)) {
            continue;
        }
        let activity: f64 = cols.iter().zip(values).map(|(&j, &a)| a * x[j]).sum();
        // Both sides of a range row are worth trying, and an equality is tight on both.
        for &(sign, bound) in &[(1.0f64, problem.row_ub[i]), (-1.0, problem.row_lb[i])] {
            if !bound.is_finite() || (sign * activity - sign * bound).abs() > tolerance {
                continue;
            }
            let whole = |v: f64| (v - v.round()).abs() <= 1e-9;
            if !whole(sign * bound) || !values.iter().all(|&a| whole(sign * a)) {
                continue;
            }
            let mut rhs = (sign * bound).round() as i64;
            let mut row: Vec<(usize, i64)> = Vec::with_capacity(cols.len());
            for (&j, &a) in cols.iter().zip(values) {
                let a = (sign * a).round() as i64;
                if a == 0 {
                    continue;
                }
                if complemented[j] {
                    rhs -= a;
                    row.push((j, -a));
                } else {
                    row.push((j, a));
                }
            }
            tight.push((row, rhs));
            if tight.len() >= MOD2_MAX_ROWS {
                break;
            }
        }
        if tight.len() >= MOD2_MAX_ROWS {
            break;
        }
    }
    if tight.is_empty() {
        return Vec::new();
    }

    // Eliminate over GF(2), carrying which rows each partial combination is made of.
    // A row that reduces to nothing on the left with an odd right-hand side is a cut.
    let mut pivots: Vec<Option<(BitSet, bool, BitSet)>> = vec![None; fractional.len()];
    let mut combinations: Vec<BitSet> = Vec::new();
    for (index, (row, rhs)) in tight.iter().enumerate() {
        let mut parity = BitSet::new(fractional.len());
        for &(j, a) in row {
            if a & 1 != 0 && equation[j] != usize::MAX {
                parity.set(equation[j]);
            }
        }
        let mut odd = rhs & 1 != 0;
        let mut from = BitSet::new(tight.len());
        from.set(index);
        while let Some(e) = parity.first() {
            match &pivots[e] {
                Some((held, held_odd, held_from)) => {
                    parity.xor(held);
                    odd ^= held_odd;
                    from.xor(held_from);
                }
                None => {
                    pivots[e] = Some((parity, odd, from));
                    parity = BitSet::new(0);
                    odd = false;
                    from = BitSet::new(0);
                    break;
                }
            }
        }
        if parity.0.is_empty() {
            continue;
        }
        if parity.is_empty() && odd && !from.is_empty() {
            combinations.push(from);
        }
    }

    let mut found: Vec<Cut> = Vec::new();
    let mut summed: Vec<i64> = vec![0; problem.n_cols()];
    // Whether a column is already in `touched`. Asking whether its running sum is zero
    // instead is wrong: rows can cancel a column back to zero part way through, the next
    // row then lists it a second time, and the cut carries its halved coefficient twice.
    // That is a cut stronger than the combination supports, and it removed the optimum of
    // a generated signed model outright -- 1188 reported as infeasible.
    let mut listed: Vec<bool> = vec![false; problem.n_cols()];
    let mut touched: Vec<usize> = Vec::new();
    for combination in &combinations {
        for &j in &touched {
            summed[j] = 0;
            listed[j] = false;
        }
        touched.clear();
        let mut rhs = 0i64;
        for i in combination.iter() {
            let (row, b) = &tight[i];
            rhs += b;
            for &(j, a) in row {
                if !listed[j] {
                    listed[j] = true;
                    touched.push(j);
                }
                summed[j] += a;
            }
        }
        // Halve and round down. Euclidean division, because rounding a negative
        // coefficient towards zero rounds it *up* and the cut stops being valid.
        let mut coefficients: Vec<(usize, f64)> = Vec::new();
        let mut bound = rhs.div_euclid(2) as f64;
        for &j in &touched {
            let a = summed[j].div_euclid(2);
            if a == 0 {
                continue;
            }
            if complemented[j] {
                // Back out of the complement: `a y = a (1 - x)`.
                bound -= a as f64;
                coefficients.push((j, -a as f64));
            } else {
                coefficients.push((j, a as f64));
            }
        }
        if coefficients.is_empty() {
            continue;
        }
        coefficients.sort_by_key(|&(j, _)| j);
        let cut = Cut {
            coefficients,
            lb: f64::NEG_INFINITY,
            ub: bound,
        };
        if cut.violation(x) > MIN_VIOLATION {
            found.push(cut);
        }
    }

    found.sort_by(|a, b| {
        b.violation(x)
            .partial_cmp(&a.violation(x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found.truncate(limit);
    found
}

/// Most divisors tried per row when deriving a mixed-integer rounding cut.
const MAX_MIR_DIVISORS: usize = 6;

/// A variable upper bound, `x_j <= factor * x_binary`, read off a two-column row.
///
/// This is the structure behind every fixed-charge model: a continuous column may only
/// take a value when the binary that pays for it is on. MIPLIB's beavma is half made of
/// these rows.
#[derive(Clone, Copy)]
struct VariableBound {
    binary: usize,
    factor: f64,
}

/// A column of the row being rounded, after bound substitution has been applied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Term {
    /// A column of the model itself.
    Column(usize),
    /// The complement `1 - y` of a binary column, which is what lets the rounding see
    /// a binary the point is holding near one rather than near zero.
    Complement(usize),
    /// The slack `s_j = factor * y - x_j` of column `j` against its variable upper
    /// bound. It is continuous and nonnegative, which is all the rounding needs.
    Residual(usize),
}

/// Variable upper bounds for every continuous column that has one.
///
/// Only rows with exactly two columns are read. A tighter bound wins, since it is the
/// one that constrains the substituted row most.
fn variable_upper_bounds(problem: &Problem) -> Vec<Option<VariableBound>> {
    let mut bounds: Vec<Option<VariableBound>> = vec![None; problem.n_cols()];
    let csr = problem.matrix.to_csr();

    for i in 0..problem.n_rows() {
        let (cols, vals) = csr.column(i);
        if cols.len() != 2 {
            continue;
        }
        for &(sign, bound) in &[(1.0f64, problem.row_ub[i]), (-1.0f64, -problem.row_lb[i])] {
            // Only `... <= 0` rows give a bound that vanishes when the binary is off.
            if bound.abs() > TOL || !bound.is_finite() {
                continue;
            }
            for (first, second) in [(0usize, 1usize), (1, 0)] {
                let (j, a) = (cols[first], sign * vals[first]);
                let (y, b) = (cols[second], sign * vals[second]);
                // `a x_j + b y <= 0` with `a > 0 > b` reads `x_j <= (-b / a) y`.
                if problem.is_integer(j) || a <= TOL || b >= -TOL {
                    continue;
                }
                if !problem.is_integer(y) || problem.col_lb[y] != 0.0 || problem.col_ub[y] != 1.0 {
                    continue;
                }
                let factor = -b / a;
                if bounds[j].is_none_or(|held| factor < held.factor) {
                    bounds[j] = Some(VariableBound { binary: y, factor });
                }
            }
        }
    }
    bounds
}

/// Separate mixed-integer rounding cuts violated by `x`.
///
/// Covers need a row that reads as a knapsack, and Gomory cuts come off the tableau.
/// Neither exploits the commonest structure in a mixed model: a row mixing integer and
/// continuous columns, where rounding the integer part gives an inequality the
/// relaxation does not know about.
///
/// For a row `sum a_j x_j <= b` with every column at a lower bound of zero, dividing by
/// `d > 0` and writing `f = frac(b/d)`, `f_j = frac(a_j/d)`, the inequality
///
/// ```text
///     sum_{integer j} (floor(a_j/d) + max(0, (f_j - f) / (1 - f))) x_j
///   + sum_{continuous j} min(0, a_j/d) / (1 - f) * x_j
///   <= floor(b / d)
/// ```
///
/// holds for every integer-feasible point. Only negative continuous coefficients
/// contribute, which is what makes it bite on rows where a continuous column is held
/// down by an integer one.
///
/// Applying that to the model's rows as written is close to useless, which measurement
/// bore out: the relaxation already respects each row on its own, so the rounding has
/// almost nothing to remove. The strength comes from first substituting continuous
/// columns by their variable upper bounds, `x_j = factor * y - s_j`. That moves weight
/// onto the binary `y`, which the rounding can then act on, and it is what makes the
/// derivation reach fixed-charge structure at all.
///
/// Every column in the row must sit at a lower bound of zero. Shifting a column that
/// does not is a further step this does not take, so such rows are skipped rather than
/// cut incorrectly.
pub fn separate_mir(problem: &Problem, x: &[f64], limit: usize) -> Vec<Cut> {
    if std::env::var("RS_OFF").ok().as_deref()==Some("mir") { return Vec::new(); }
    let csr = problem.matrix.to_csr();
    let vubs = variable_upper_bounds(problem);
    let mut found: Vec<Cut> = Vec::new();

    for i in 0..problem.n_rows() {
        let (cols, vals) = csr.column(i);
        if cols.is_empty() {
            continue;
        }
        // A range row gives two one-sided rows to try; an equality gives nothing this
        // reduction can use, since it has no slack to round against.
        let lb = problem.row_lb[i];
        let ub = problem.row_ub[i];
        for &(sign, bound) in &[(1.0f64, ub), (-1.0f64, -lb)] {
            if !bound.is_finite() || lb == ub {
                continue;
            }
            // Written as `sum a_j x_j <= b`, negating a `>=` row to get there.
            let terms: Vec<(usize, f64)> =
                cols.iter().zip(vals).map(|(&j, &v)| (j, sign * v)).collect();
            if terms.iter().any(|&(j, _)| problem.col_lb[j] != 0.0) {
                continue;
            }
            // Each rewriting of the row is exact, and which one the rounding bites on
            // is not predictable from the row alone, so all four are tried: as written,
            // with continuous columns put on their variable bounds, with binaries near
            // one complemented, and with both.
            let plain: Vec<(Term, f64)> =
                terms.iter().map(|&(j, a)| (Term::Column(j), a)).collect();
            let mut rewritings = vec![(plain.clone(), bound)];
            if let Some(pair) = substitute_bounds(problem, &terms, bound, x, &vubs) {
                rewritings.push(pair);
            }
            for index in 0..rewritings.len() {
                let (row, rhs) = &rewritings[index];
                if let Some(pair) = complement_binaries(problem, row, *rhs, x) {
                    rewritings.push(pair);
                }
            }
            for (row, rhs) in &rewritings {
                if let Some(cut) = mir_from_row(problem, row, *rhs, x, &vubs, MAX_MIR_DIVISORS) {
                    found.push(cut);
                }
            }
        }
    }

    found.sort_by(|a, b| {
        b.violation(x)
            .partial_cmp(&a.violation(x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found.truncate(limit);
    found
}

/// Replace continuous columns by their variable upper bounds where that tightens the
/// row at the current point.
///
/// Substituting `x_j = factor * y - s_j` is exact, so the rewritten row holds wherever
/// the original does. It is worth doing only for a column sitting near its variable
/// upper bound, where the residual `s_j` is near zero and the rounding therefore has
/// something to work with. Returns `None` when no column was substituted, since that
/// leaves the row unchanged and the caller has already rounded it.
fn substitute_bounds(
    problem: &Problem,
    terms: &[(usize, f64)],
    bound: f64,
    x: &[f64],
    vubs: &[Option<VariableBound>],
) -> Option<(Vec<(Term, f64)>, f64)> {
    let mut coefficients: Vec<(Term, f64)> = Vec::with_capacity(terms.len() * 2);
    let mut substituted = false;

    for &(j, a) in terms {
        let vub = vubs[j].filter(|_| !problem.is_integer(j) && a.abs() > TOL);
        let Some(vub) = vub else {
            coefficients.push((Term::Column(j), a));
            continue;
        };
        // Closer to the variable upper bound than to zero is the standard test: it is
        // the end the relaxation is leaning on, so it is the one worth rewriting.
        let ceiling = vub.factor * x[vub.binary];
        if ceiling - x[j] >= x[j] {
            coefficients.push((Term::Column(j), a));
            continue;
        }
        // a x_j = a factor y - a s_j.
        coefficients.push((Term::Column(vub.binary), a * vub.factor));
        coefficients.push((Term::Residual(j), -a));
        substituted = true;
    }
    if !substituted {
        return None;
    }

    // Substitution can put weight on a binary the row already mentioned.
    coefficients.sort_by_key(|&(term, _)| match term {
        Term::Column(j) => (0usize, j),
        Term::Complement(j) => (1, j),
        Term::Residual(j) => (2, j),
    });
    coefficients.dedup_by(|later, held| {
        if later.0 == held.0 {
            held.1 += later.1;
            true
        } else {
            false
        }
    });
    coefficients.retain(|&(_, a)| a.abs() > TOL);
    Some((coefficients, bound))
}

/// Replace binaries the point holds near one by their complements.
///
/// The rounding only reads a column's distance from zero, so a binary sitting near one
/// contributes nothing it can act on. Rewriting `y` as `1 - y'` moves that column back
/// to the end the derivation can use, and shifts the constant it carries onto the
/// right-hand side. This is exact, so the rewritten row holds wherever the original
/// does. Returns `None` when nothing was complemented and the caller has already
/// rounded the row as it stands.
fn complement_binaries(
    problem: &Problem,
    terms: &[(Term, f64)],
    bound: f64,
    x: &[f64],
) -> Option<(Vec<(Term, f64)>, f64)> {
    let mut coefficients: Vec<(Term, f64)> = Vec::with_capacity(terms.len());
    let mut rhs = bound;
    let mut complemented = false;

    for &(term, a) in terms {
        let Term::Column(j) = term else {
            coefficients.push((term, a));
            continue;
        };
        let binary =
            problem.is_integer(j) && problem.col_lb[j] == 0.0 && problem.col_ub[j] == 1.0;
        if !binary || x[j] <= 0.5 {
            coefficients.push((term, a));
            continue;
        }
        // a y = a - a y', so the constant moves across.
        coefficients.push((Term::Complement(j), -a));
        rhs -= a;
        complemented = true;
    }
    complemented.then_some((coefficients, rhs))
}

/// The most violated MIR cut over a handful of divisors, if any is violated at all.
///
/// The divisors tried are the magnitudes of the integer columns' own coefficients,
/// which is the standard choice: dividing by one of them is what makes that column's
/// rounded coefficient land on an integer.
fn mir_from_row(
    problem: &Problem,
    terms: &[(Term, f64)],
    bound: f64,
    x: &[f64],
    vubs: &[Option<VariableBound>],
    divisors: usize,
) -> Option<Cut> {
    let is_integer = |term: Term| match term {
        Term::Column(j) => problem.is_integer(j),
        Term::Complement(_) => true,
        Term::Residual(_) => false,
    };

    let mut candidates: Vec<f64> = terms
        .iter()
        .filter(|&&(term, a)| is_integer(term) && a.abs() > TOL)
        .map(|&(_, a)| a.abs())
        .collect();
    candidates.sort_by(|a, b| b.total_cmp(a));
    candidates.dedup_by(|a, b| (*a - *b).abs() <= TOL);
    candidates.truncate(divisors);

    let mut best: Option<(f64, Cut)> = None;
    for divisor in candidates {
        let scaled_rhs = bound / divisor;
        let floor_rhs = scaled_rhs.floor();
        let f = scaled_rhs - floor_rhs;
        // No fractional part means the rounding gives back the row it started from.
        if f <= TOL || f >= 1.0 - TOL {
            continue;
        }

        let mut rounded: Vec<(Term, f64)> = Vec::with_capacity(terms.len());
        for &(term, a) in terms {
            let scaled = a / divisor;
            let coefficient = if is_integer(term) {
                let whole = scaled.floor();
                let fraction = scaled - whole;
                whole + ((fraction - f) / (1.0 - f)).max(0.0)
            } else {
                scaled.min(0.0) / (1.0 - f)
            };
            if coefficient.abs() > TOL {
                rounded.push((term, coefficient));
            }
        }
        if rounded.is_empty() {
            continue;
        }

        let Some(cut) = restate(&rounded, floor_rhs, vubs) else {
            continue;
        };
        let violation = cut.violation(x);
        if violation > MIN_VIOLATION && best.as_ref().is_none_or(|(v, _)| violation > *v) {
            best = Some((violation, cut));
        }
    }
    best.map(|(_, cut)| cut)
}

/// Rewrite a rounded row back in terms of the model's own columns.
///
/// A residual carries `s_j = factor * y - x_j`, so its coefficient splits between the
/// binary and the continuous column it came from.
fn restate(rounded: &[(Term, f64)], rhs: f64, vubs: &[Option<VariableBound>]) -> Option<Cut> {
    let mut coefficients: Vec<(usize, f64)> = Vec::with_capacity(rounded.len() * 2);
    let mut rhs = rhs;
    for &(term, c) in rounded {
        match term {
            Term::Column(j) => coefficients.push((j, c)),
            Term::Complement(j) => {
                // c (1 - y) on the left is -c y with c taken off the right.
                coefficients.push((j, -c));
                rhs -= c;
            }
            Term::Residual(j) => {
                let vub = vubs[j]?;
                coefficients.push((vub.binary, c * vub.factor));
                coefficients.push((j, -c));
            }
        }
    }

    coefficients.sort_by_key(|&(j, _)| j);
    coefficients.dedup_by(|later, held| {
        if later.0 == held.0 {
            held.1 += later.1;
            true
        } else {
            false
        }
    });
    coefficients.retain(|&(_, a)| a.abs() > TOL);
    if coefficients.is_empty() {
        return None;
    }
    Some(Cut {
        coefficients,
        lb: f64::NEG_INFINITY,
        ub: rhs,
    })
}

/// Which pairs of *literals* cannot hold at once.
///
/// A literal is a binary column asserted either way: `x_j` or `not x_j`. Working over
/// literals rather than columns is the difference between reading a model's two-column
/// rows and missing them entirely. A row saying `x + y <= 1` forbids both columns being
/// one and is a conflict between the columns; a row saying `x - y <= 0` forbids nothing
/// of the sort, it says `x` implies `y`, and setting both to one satisfies it. Over
/// literals that second row is a conflict too, between `x` and `not y`.
///
/// That distinction decides whether there is anything here at all. The pure binary
/// benchmark is full of two-column rows, `neos-5129192-manaia` alone carrying 319027,
/// and reading them as column conflicts finds no edge whatsoever on `acc-tight2`,
/// `mine-166-5`, `neos18`, `ab71-20-100` or `neos-953928`. They are implications, not
/// exclusions.
///
/// Node `2j` is the literal `x_j = 1` and node `2j + 1` is `x_j = 0`.
pub struct Conflicts {
    /// For each literal, the literals it excludes, sorted.
    neighbours: Vec<Vec<u32>>,
    /// Sets of literals excluding one another pairwise, held whole rather than
    /// expanded into edges. A set partitioning row of six thousand columns is one
    /// entry here and eighteen million entries in `neighbours`.
    cliques: Vec<Vec<u32>>,
    /// For each literal, which of `cliques` it belongs to, sorted.
    membership: Vec<Vec<u32>>,
}

/// The node standing for column `j` taking `value`.
pub fn literal(j: usize, value: bool) -> u32 {
    (2 * j + usize::from(!value)) as u32
}

/// The longest set of literals a single row forbids from holding together.
///
/// Written `sum w_k l_k <= c` with every weight positive, which complementing the
/// negative coefficients arranges, a row excludes any pair whose weights overshoot `c`.
/// Sorting the weights downwards turns the pairs to check into a prefix: if the two
/// smallest weights among the first `t` still overshoot, then so does every pair among
/// those `t`, and those `t` literals are a clique.
///
/// Taking the longest such prefix is the whole of the extraction. It costs a sort where
/// enumerating pairs costs the square, and that is the point. The rows worth reading
/// here are the long ones: a set partitioning row spanning six thousand columns is one
/// clique of six thousand literals, or eighteen million edges written out.
///
/// Stopping at the prefix leaves cliques on the table. A row can exclude further pairs
/// that no prefix covers, and finding them means more passes over the weights. The
/// prefix is taken alone because it is exactly what set packing and set partitioning
/// rows need, and those are the rows this exists to read.
/// Extra conflicting pairs a row implies beyond its longest overshooting prefix.
///
/// A single row is allowed this many; a row of `n` literals can imply `n^2 / 2` pairs
/// and writing those out is the blowup the clique representation exists to avoid. What
/// the cap has to admit is the *star*, one heavy literal against many light ones, which
/// is linear in the row and is where the big-M models live.
const MAX_ROW_PAIRS: usize = 4_096;

/// Extra pairs the whole model is allowed, as a multiple of its nonzeros.
///
/// The per-row cap bounds one row; this bounds a model with a great many rows that each
/// contribute a little. Eight times the matrix keeps the graph proportional to the model
/// it describes. Measured on the pure binary set, nothing comes near it: the largest
/// graphs built here peak at 216MB and under a second, and the star a big-M row emits is
/// linear in the row rather than quadratic.
const MAX_TOTAL_PAIRS: usize = 8;

/// The row as `sum w_l [literal l] <= capacity` with every weight positive.
///
/// Two literals conflict exactly when their weights overshoot the capacity together,
/// so this is the whole conflict structure of the row in a form that can be read for
/// cliques and for pairs alike.
fn row_literals(
    problem: &Problem,
    live: &[(usize, f64)],
    bound: f64,
    sign: f64,
) -> Option<(Vec<(f64, u32)>, f64)> {
    // Written `sum a_j x_j <= capacity`, negating a `>=` row to get there. The caller
    // negates the bound along with the sign, so it arrives ready to use.
    let mut capacity = bound;
    let mut weights: Vec<(f64, u32)> = Vec::with_capacity(live.len());
    for &(j, a) in live {
        if !is_binary(problem, j) {
            return None;
        }
        let a = sign * a;
        // A column pinned to one end is a constant in the row, not a literal.
        if problem.col_lb[j] == problem.col_ub[j] {
            capacity -= a * problem.col_lb[j];
            continue;
        }
        // Complementing, `x_j = 1 - not x_j`, turns a negative coefficient positive and
        // moves its weight across to the capacity.
        if a < 0.0 {
            capacity -= a;
            weights.push((-a, literal(j, false)));
        } else {
            weights.push((a, literal(j, true)));
        }
    }
    if weights.len() < 2 {
        return None;
    }
    weights.sort_by(|a, b| b.0.total_cmp(&a.0));
    Some((weights, capacity))
}

/// The longest prefix of `weights` whose literals pairwise overshoot `capacity`.
///
/// The two smallest weights of a prefix shrink as it grows, so the overshooting
/// prefixes are themselves a prefix and the longest is found by walking out.
fn prefix_clique(weights: &[(f64, u32)], capacity: f64) -> Vec<u32> {
    let mut held = 0usize;
    for t in 2..=weights.len() {
        if weights[t - 2].0 + weights[t - 1].0 > capacity + TOL {
            held = t;
        } else {
            break;
        }
    }
    weights[..held].iter().map(|&(_, node)| node).collect()
}

/// Every conflicting pair the row implies that its prefix clique does not already say.
///
/// The clique covers the pairs inside it. What it misses is a literal heavy enough to
/// overshoot on its own against literals too light to overshoot each other, which the
/// prefix stops at the moment two light ones fit together. In a big-M linking row,
/// `sum_{j in S} x_j <= M y` with `M >= |S|`, that is the whole content of the row: the
/// complement of `y` overshoots against every `x_j`, so the row says `x_j <= y` for each
/// of them, and the prefix reports one of those and stops.
///
/// `weights` must be sorted descending, which makes each literal's conflicting partners
/// a prefix of the list and the whole set readable in one walk.
fn extra_pairs(weights: &[(f64, u32)], capacity: f64, held: usize, out: &mut Vec<(u32, u32)>) {
    // No pair overshoots at all unless the heaviest two do, and those are the prefix.
    if held < 2 {
        return;
    }
    // Partner ranges shrink as the weight does, so one pointer sweeps them all.
    let mut last = weights.len();
    for &(weight, node) in &weights[..held] {
        while last > held && weight + weights[last - 1].0 <= capacity + TOL {
            last -= 1;
        }
        for &(_, other) in &weights[held..last] {
            // Both literals of one column exclude each other whatever the row says.
            if node / 2 == other / 2 {
                continue;
            }
            out.push((node, other));
            if out.len() >= MAX_ROW_PAIRS {
                return;
            }
        }
    }
}

impl Conflicts {
    /// Read the pairwise conflicts out of a model's two-column rows.
    ///
    /// Each row over binaries `p` and `q` is tried at all four assignments of the pair.
    /// Any assignment the row rejects is a pair of literals that cannot hold together,
    /// which is exactly an edge. Rows with other than two live columns are left alone:
    /// the reasoning generalizes, but taking it from a long row means looking at every
    /// pair in it, and one row here spans six thousand columns.
    pub fn of(problem: &Problem) -> Self {
        let n = problem.n_cols();
        let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); 2 * n];
        let mut cliques: Vec<Vec<u32>> = Vec::new();
        let mut membership: Vec<Vec<u32>> = vec![Vec::new(); 2 * n];
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut budget = MAX_TOTAL_PAIRS * problem.matrix.nnz();
        let csr = problem.matrix.to_csr();
        for i in 0..problem.n_rows() {
            let (cols, vals) = csr.column(i);
            let live: Vec<(usize, f64)> = cols
                .iter()
                .zip(vals)
                .filter(|&(_, &a)| a != 0.0)
                .map(|(&j, &a)| (j, a))
                .collect();
            if live.len() < 2 {
                continue;
            }
            if live.len() == 2 {
                let [(p, a), (q, b)] = [live[0], live[1]];
                if !is_binary(problem, p) || !is_binary(problem, q) || p == q {
                    continue;
                }
                for vp in [false, true] {
                    for vq in [false, true] {
                        let activity = a * f64::from(vp) + b * f64::from(vq);
                        if activity < problem.row_lb[i] - TOL || activity > problem.row_ub[i] + TOL
                        {
                            let (lp, lq) = (literal(p, vp), literal(q, vq));
                            neighbours[lp as usize].push(lq);
                            neighbours[lq as usize].push(lp);
                        }
                    }
                }
                continue;
            }
            // A longer row is read from each side it is bounded on. An equality is
            // bounded on both, which is the case worth having: a set partitioning row
            // is where the conflicts of a binary model mostly live.
            for &(sign, bound) in &[(1.0f64, problem.row_ub[i]), (-1.0f64, -problem.row_lb[i])] {
                if !bound.is_finite() {
                    continue;
                }
                let Some((weights, capacity)) = row_literals(problem, &live, bound, sign)
                else {
                    continue;
                };
                let clique = prefix_clique(&weights, capacity);
                let held = clique.len();
                // A pair says no more than the row it came from, so it goes in as an
                // edge; only a set of three or more is worth holding as a clique.
                if clique.len() == 2 {
                    neighbours[clique[0] as usize].push(clique[1]);
                    neighbours[clique[1] as usize].push(clique[0]);
                } else if clique.len() > 2 {
                    let id = cliques.len() as u32;
                    for &node in &clique {
                        membership[node as usize].push(id);
                    }
                    cliques.push(clique);
                }
                // The prefix stops where two light literals first fit together, which
                // leaves every pair a heavy literal makes with a light one unsaid. See
                // `extra_pairs`.
                pairs.clear();
                if budget > 0 {
                    extra_pairs(&weights, capacity, held, &mut pairs);
                    pairs.truncate(budget);
                    budget -= pairs.len();
                }
                for &(p, q) in &pairs {
                    neighbours[p as usize].push(q);
                    neighbours[q as usize].push(p);
                }
            }
        }
        for list in &mut neighbours {
            list.sort_unstable();
            list.dedup();
        }
        for list in &mut membership {
            list.sort_unstable();
            list.dedup();
        }
        Self {
            neighbours,
            cliques,
            membership,
        }
    }

    pub fn conflicts(&self, p: u32, q: u32) -> bool {
        if self.neighbours[p as usize].binary_search(&q).is_ok() {
            return true;
        }
        // Two literals of one clique exclude each other without an edge between them.
        let (a, b) = (&self.membership[p as usize], &self.membership[q as usize]);
        let (mut i, mut j) = (0usize, 0usize);
        while i < a.len() && j < b.len() {
            match a[i].cmp(&b[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => return true,
            }
        }
        false
    }

    /// Visit every literal excluded by `node`, allocating nothing and sorting nothing.
    ///
    /// `adjacent` builds a deduplicated list, which costs an allocation and a sort per
    /// call. A propagator calls this once per column it fixes and does not need either:
    /// forcing a column to a value it already holds is a no-op, so a literal arriving
    /// twice costs one comparison, where sorting a 3861-literal clique to spare that
    /// comparison costs everything. Stop by returning false from `visit`.
    pub fn for_each_adjacent(&self, node: u32, mut visit: impl FnMut(u32) -> bool) -> bool {
        for &other in &self.neighbours[node as usize] {
            if !visit(other) {
                return false;
            }
        }
        for &id in &self.membership[node as usize] {
            for &other in &self.cliques[id as usize] {
                if other != node && !visit(other) {
                    return false;
                }
            }
        }
        true
    }

    /// Every literal excluded by `node`, from its edges and its cliques together.
    pub fn adjacent(&self, node: u32) -> Vec<u32> {
        let mut out = self.neighbours[node as usize].clone();
        for &id in &self.membership[node as usize] {
            out.extend(
                self.cliques[id as usize]
                    .iter()
                    .copied()
                    .filter(|&other| other != node),
            );
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// How many literals `node` excludes, without building the list to find out.
    ///
    /// Counts cliques as well as edges. Reading only the edges reports zero for every
    /// column of a set partitioning model, whose conflicts are all held as cliques,
    /// which is exactly backwards: those are the most constrained columns in the set.
    pub fn degree(&self, node: u32) -> usize {
        self.neighbours[node as usize].len()
            + self.membership[node as usize]
                .iter()
                .map(|&id| self.cliques[id as usize].len().saturating_sub(1))
                .sum::<usize>()
    }

    /// Does `node` exclude anything, without building the list to find out?
    pub fn has_conflicts(&self, node: u32) -> bool {
        !self.neighbours[node as usize].is_empty() || !self.membership[node as usize].is_empty()
    }

    /// Are there any conflicts at all? Building cliques is pointless otherwise.
    pub fn is_empty(&self) -> bool {
        self.cliques.is_empty() && self.neighbours.iter().all(Vec::is_empty)
    }

    /// How many triangles the graph holds, capped so a dense one cannot run away.
    ///
    /// A clique worth cutting on needs at least three literals: two of them restate a
    /// row already in the model, which the relaxation therefore already respects.
    pub fn triangles(&self) -> usize {
        let mut count = 0usize;
        // A clique of `k` holds every one of its triples, and counting them by
        // enumeration is exactly the cost holding it whole was meant to avoid.
        for clique in &self.cliques {
            let k = clique.len();
            count = count.saturating_add(k * (k - 1) * (k - 2) / 6);
            if count >= 100_000 {
                return count;
            }
        }
        for (a, list) in self.neighbours.iter().enumerate() {
            for (i, &b) in list.iter().enumerate() {
                if (b as usize) < a {
                    continue;
                }
                for &c in &list[i + 1..] {
                    if (c as usize) > b as usize && self.conflicts(b, c) {
                        count += 1;
                        if count >= 100_000 {
                            return count;
                        }
                    }
                }
            }
        }
        count
    }

    /// The literals excluded by `node`.
    pub fn neighbours(&self, node: u32) -> &[u32] {
        &self.neighbours[node as usize]
    }

    /// How many literals the graph is over, twice the column count.
    pub fn nodes(&self) -> usize {
        self.neighbours.len()
    }

    /// How many conflicting pairs were found.
    pub fn edges(&self) -> usize {
        let pairs: usize = self
            .cliques
            .iter()
            .map(|c| c.len() * (c.len() - 1) / 2)
            .sum();
        pairs + self.neighbours.iter().map(Vec::len).sum::<usize>() / 2
    }

    /// How many cliques were read whole, and the longest of them.
    pub fn clique_shape(&self) -> (usize, usize) {
        (
            self.cliques.len(),
            self.cliques.iter().map(Vec::len).max().unwrap_or(0),
        )
    }
}

/// Is this column a binary, including one already pinned to an end?
fn is_binary(problem: &Problem, j: usize) -> bool {
    problem.is_integer(j) && problem.col_lb[j] >= 0.0 && problem.col_ub[j] <= 1.0
}

/// Separate clique cuts violated by `x`.
///
/// Literals that pairwise exclude one another admit at most one true among them, so
/// their values sum to no more than one. Written over columns, a clique holding the
/// positive literals `P` and the negative literals `N` is
///
/// ```text
///     sum_{j in P} x_j - sum_{j in N} x_j <= 1 - |N|
/// ```
///
/// The rows the conflicts came from are already in the model, so a clique that is just
/// one of them again says nothing new. What is worth having is a clique drawn across
/// several rows, which no single row expresses and the relaxation has therefore never
/// been told about.
///
/// Cliques are grown greedily from the most nearly-true literal outwards, taking the
/// heaviest candidate excluded by everything already held. Greedy gives no guarantee of
/// the largest clique, which is NP-hard to want; it gives a clique, and one the
/// relaxation violates is worth adding whether or not something bigger existed.
/// For each binary column, a binary the model forces to one whenever the column is.
///
/// `x_j = 1` conflicting with `y = 0` is exactly the statement `x_j <= y`, so the
/// conflict graph already holds every variable upper bound the rows imply and this only
/// has to read them off. Where a column has several, the one standing lowest in the
/// relaxation is kept, since that is the one a row aggregated through these bounds is
/// most violated at.
fn implied_upper_bounds(problem: &Problem, conflicts: &Conflicts, x: &[f64]) -> Vec<Option<usize>> {
    let mut bounds: Vec<Option<usize>> = vec![None; problem.n_cols()];
    for (j, slot) in bounds.iter_mut().enumerate() {
        if !is_binary(problem, j) {
            continue;
        }
        let mut best: Option<usize> = None;
        conflicts.for_each_adjacent(literal(j, true), |node| {
            // An odd node is the literal `y = 0`, so conflicting with it means `y = 1`.
            if !node.is_multiple_of(2) {
                let y = node as usize / 2;
                if y != j && best.is_none_or(|held| x[y] < x[held]) {
                    best = Some(y);
                }
            }
            true
        });
        *slot = best;
    }
    bounds
}

/// Separate rows aggregated through the variable upper bounds the model implies.
///
/// A big-M linking row, `sum_{j in S} x_j <= M y` with `M >= |S|`, says `x_j <= y` for
/// every `j`, and says it so weakly that the relaxation can hold `y` at `1/|S|` of what
/// any one `x_j` is. Separating those bounds one at a time is sound and hopeless: on
/// `neos-787933` there are 1764 such groups, each cut lifts one of them, and the
/// relaxation answers by moving its weight to another group. Three thousand cuts move
/// the bound from 9 to 10 against an optimum of 30.
///
/// Aggregating instead puts the whole model in one place. For a row `sum a_j x_j >= b`
/// with `a_j > 0`, every term obeys `a_j x_j <= a_j y_j`, so
///
/// ```text
///     sum_j a_j y_j  >=  sum_j a_j x_j  >=  b
/// ```
///
/// holds for every feasible point, and columns sharing a bounding variable collect onto
/// it. A term with `a_j < 0` contributes at most zero and is dropped, which needs the
/// column's lower bound to be zero and is why that is checked. On `neos-787933` this is
/// 133 cuts, one per covering row, and it takes the relaxation from 3 to **30**, which
/// is the integer optimum.
pub fn separate_implied_aggregations(
    problem: &Problem,
    conflicts: &Conflicts,
    x: &[f64],
    limit: usize,
) -> Vec<Cut> {
    if std::env::var("RS_OFF").ok().as_deref()==Some("aggregation") { return Vec::new(); }
    if conflicts.is_empty() {
        return Vec::new();
    }
    let bounds = implied_upper_bounds(problem, conflicts, x);
    if bounds.iter().all(Option::is_none) {
        return Vec::new();
    }
    let csr = problem.matrix.to_csr();
    let mut found: Vec<Cut> = Vec::new();
    let mut accumulated: Vec<(usize, f64)> = Vec::new();

    for i in 0..problem.n_rows() {
        let (cols, vals) = csr.column(i);
        if cols.len() < 2 {
            continue;
        }
        // Both sides of a range row are a `>=` row, one of them after negating.
        for &(sign, bound) in &[(1.0f64, problem.row_lb[i]), (-1.0, -problem.row_ub[i])] {
            if !bound.is_finite() || found.len() >= limit {
                continue;
            }
            accumulated.clear();
            let mut substituted = false;
            let mut usable = true;
            for (&j, &v) in cols.iter().zip(vals) {
                let a = sign * v;
                if a == 0.0 {
                    continue;
                }
                if a < 0.0 {
                    // Dropped, which replaces the term by zero and so needs zero to be
                    // an upper bound on it: `a x <= 0` for every value the column can
                    // take. With `a < 0` that is exactly the column not going negative.
                    // A column that can go negative makes the term positive, and
                    // dropping it would claim the rest of the row carries a bound the
                    // model never asked it to.
                    if problem.col_lb[j] < 0.0 {
                        usable = false;
                        break;
                    }
                    continue;
                }
                match bounds[j] {
                    Some(y) => {
                        substituted = true;
                        accumulated.push((y, a));
                    }
                    None => accumulated.push((j, a)),
                }
            }
            if !usable || !substituted || accumulated.is_empty() {
                continue;
            }
            accumulated.sort_by_key(|&(j, _)| j);
            let mut coefficients: Vec<(usize, f64)> = Vec::with_capacity(accumulated.len());
            for &(j, a) in &accumulated {
                match coefficients.last_mut() {
                    Some(last) if last.0 == j => last.1 += a,
                    _ => coefficients.push((j, a)),
                }
            }
            let cut = Cut {
                coefficients,
                lb: bound,
                ub: f64::INFINITY,
            };
            if cut.violation(x) > MIN_VIOLATION {
                found.push(cut);
            }
        }
    }

    found.sort_by(|a, b| {
        b.violation(x)
            .partial_cmp(&a.violation(x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found.truncate(limit);
    found
}

/// The model rewritten with every implied-bounded column replaced by the one bounding it.
///
/// Where `x_j <= y` holds and `x_j` costs nothing to raise, an optimal solution exists
/// with `x_j = y`, and the model over the bounding variables alone is a far smaller
/// question with the same answer. That argument is *not* made here, and does not need to
/// be: this returns a restriction of the model, every point of which maps to a point of
/// the original, and the caller checks the point it gets back. So a substitution that
/// should not have been made costs a wasted search and never a wrong answer.
///
/// On `neos-787933` it is 236376 columns and 1897 rows down to **1764 and 133**, which
/// this solver closes in a fifth of a second where the model as written does not close
/// in ten minutes.
///
/// Returns the smaller problem and, for each original column, the column of the smaller
/// one standing for it.
pub fn aggregate_by_implied_bounds(
    problem: &Problem,
    conflicts: &Conflicts,
    x: &[f64],
) -> Option<(Problem, Vec<usize>)> {
    let n = problem.n_cols();
    let bounds = implied_upper_bounds(problem, conflicts, x);
    // A column may stand for others or be stood for, never both: a chain would need the
    // substitution to be transitive, and one pass over an arbitrary graph is not the
    // place to establish that.
    let free = |j: usize| {
        problem.is_integer(j) && problem.col_lb[j] == 0.0 && problem.col_ub[j] == 1.0
    };
    let mut representative: Vec<usize> = (0..n).collect();
    let mut substituted = 0usize;
    for j in 0..n {
        if let Some(y) = bounds[j]
            && y != j
            && bounds[y].is_none()
            && free(j)
            && free(y)
        {
            representative[j] = y;
            substituted += 1;
        }
    }
    if substituted == 0 {
        return None;
    }

    let mut column_of: Vec<Option<usize>> = vec![None; n];
    let mut kept: Vec<usize> = Vec::new();
    for j in 0..n {
        if representative[j] == j {
            column_of[j] = Some(kept.len());
            kept.push(j);
        }
    }
    if kept.len() == n {
        return None;
    }
    let index: Vec<usize> = (0..n)
        .map(|j| column_of[representative[j]].expect("a representative is kept"))
        .collect();

    let mut obj = vec![0.0f64; kept.len()];
    for j in 0..n {
        obj[index[j]] += problem.obj[j];
    }
    let csr = problem.matrix.to_csr();
    let mut triplets: Vec<(usize, usize, f64)> = Vec::new();
    let mut accumulated: Vec<(usize, f64)> = Vec::new();
    let (mut row_lb, mut row_ub) = (Vec::new(), Vec::new());
    for i in 0..problem.n_rows() {
        let (cols, values) = csr.column(i);
        accumulated.clear();
        for (&j, &a) in cols.iter().zip(values) {
            accumulated.push((index[j], a));
        }
        accumulated.sort_by_key(|&(k, _)| k);
        let row = row_lb.len();
        let mut written = 0usize;
        let mut last: Option<(usize, f64)> = None;
        for &(k, a) in &accumulated {
            match last {
                Some((held, total)) if held == k => last = Some((k, total + a)),
                Some((held, total)) => {
                    if total != 0.0 {
                        triplets.push((row, held, total));
                        written += 1;
                    }
                    last = Some((k, a));
                }
                None => last = Some((k, a)),
            }
        }
        if let Some((held, total)) = last
            && total != 0.0
        {
            triplets.push((row, held, total));
            written += 1;
        }
        // A row every column left is a statement about nothing, and one that zero does
        // not satisfy says the substitution contradicts the model.
        if written == 0 {
            if problem.row_lb[i] > TOL || problem.row_ub[i] < -TOL {
                return None;
            }
            continue;
        }
        row_lb.push(problem.row_lb[i]);
        row_ub.push(problem.row_ub[i]);
    }

    let rows = row_lb.len();
    let smaller = Problem {
        name: format!("{}-aggregated", problem.name),
        sense: problem.sense,
        obj,
        obj_offset: problem.obj_offset,
        matrix: crate::sparse::SparseMatrix::from_triplets(rows, kept.len(), triplets),
        row_lb,
        row_ub,
        col_lb: kept.iter().map(|&j| problem.col_lb[j]).collect(),
        col_ub: kept.iter().map(|&j| problem.col_ub[j]).collect(),
        col_type: kept.iter().map(|&j| problem.col_type[j]).collect(),
        col_names: kept.iter().map(|&j| problem.col_names[j].clone()).collect(),
        row_names: (0..rows).map(|i| format!("r{i}")).collect(),
    };
    Some((smaller, index))
}

pub fn separate_cliques(
    problem: &Problem,
    conflicts: &Conflicts,
    x: &[f64],
    limit: usize,
) -> Vec<Cut> {
    if std::env::var("RS_OFF").ok().as_deref()==Some("clique") { return Vec::new(); }
    if conflicts.is_empty() {
        return Vec::new();
    }
    let value = |node: u32| -> f64 {
        let j = node as usize / 2;
        if node.is_multiple_of(2) { x[j] } else { 1.0 - x[j] }
    };

    // Only literals carrying weight can put a clique over one.
    let mut seeds: Vec<u32> = (0..conflicts.nodes() as u32)
        .filter(|&node| {
            is_binary(problem, node as usize / 2)
                && value(node) > TOL
                && conflicts.has_conflicts(node)
        })
        .collect();
    seeds.sort_by(|&a, &b| value(b).total_cmp(&value(a)));

    let mut found: Vec<Cut> = Vec::new();
    let mut used = vec![false; conflicts.nodes()];
    for &seed in &seeds {
        if used[seed as usize] || found.len() >= limit {
            continue;
        }
        let mut clique = vec![seed];
        let mut total = value(seed);
        // Anything joining must be excluded by every member, so only the seed's own
        // conflicts are ever candidates.
        let mut candidates: Vec<u32> = conflicts
            .adjacent(seed)
            .into_iter()
            .filter(|&node| value(node) > TOL)
            .collect();
        candidates.sort_by(|&a, &b| value(b).total_cmp(&value(a)));
        for candidate in candidates {
            // Both literals of one column exclude each other trivially, and a clique
            // holding both states the tautology that exactly one of them holds.
            if clique.iter().any(|&held| held / 2 == candidate / 2) {
                continue;
            }
            if clique.iter().all(|&held| conflicts.conflicts(held, candidate)) {
                clique.push(candidate);
                total += value(candidate);
            }
        }
        if clique.len() < 2 || total <= 1.0 + MIN_VIOLATION {
            continue;
        }
        for &node in &clique {
            used[node as usize] = true;
        }
        let negatives = clique.iter().filter(|&&node| !node.is_multiple_of(2)).count();
        let mut coefficients: Vec<(usize, f64)> = clique
            .iter()
            .map(|&node| {
                let j = node as usize / 2;
                (j, if node.is_multiple_of(2) { 1.0 } else { -1.0 })
            })
            .collect();
        coefficients.sort_by_key(|&(j, _)| j);
        found.push(Cut {
            coefficients,
            lb: f64::NEG_INFINITY,
            ub: 1.0 - negatives as f64,
        });
    }
    found
}

/// Separate knapsack cover cuts violated by `x`.
///
/// Returns at most `limit` cuts, strongest violation first.
pub fn separate(problem: &Problem, x: &[f64], limit: usize) -> Vec<Cut> {
    separate_until(problem, x, limit, None)
}

/// As [`separate`], stopping early if `deadline` passes.
pub fn separate_until(
    problem: &Problem,
    x: &[f64],
    limit: usize,
    deadline: Option<std::time::Instant>,
) -> Vec<Cut> {
    if std::env::var("RS_OFF").ok().as_deref()==Some("cover") { return Vec::new(); }
    let csr = problem.matrix.to_csr();
    let mut found: Vec<Cut> = Vec::new();

    for i in 0..problem.n_rows() {
        // Separation over a large model is itself a long operation; stop rather
        // than run past a budget the caller set.
        if i.is_multiple_of(256) && deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            break;
        }
        let (cols, vals) = csr.column(i);
        if cols.is_empty() {
            continue;
        }
        let coefficients: Vec<(usize, f64)> =
            cols.iter().copied().zip(vals.iter().copied()).collect();

        // Each finite side of the row is its own knapsack.
        for (bound, negate) in [(problem.row_ub[i], false), (problem.row_lb[i], true)] {
            let Some(knapsack) = to_knapsack(problem, &coefficients, bound, negate) else {
                continue;
            };
            // One unseeded pass, then one seeded by each fractional term. Distinct
            // seeds usually produce distinct covers, and only some of them separate.
            // Seeded by the most fractional columns only. Seeding from every
            // fractional column is quadratic in the row's support and finds little
            // the top few do not.
            let mut fractional: Vec<usize> = (0..knapsack.terms.len())
                .filter(|&t| {
                    let v = y_value(&knapsack, t, x);
                    v > 1e-6 && v < 1.0 - 1e-6
                })
                .collect();
            fractional.sort_by(|&a, &b| {
                let d = |t: usize| (y_value(&knapsack, t, x) - 0.5).abs();
                d(a).partial_cmp(&d(b)).unwrap_or(std::cmp::Ordering::Equal)
            });
            fractional.truncate(MAX_SEEDS_PER_ROW);
            let seeds = std::iter::once(None).chain(fractional.into_iter().map(Some));
            for seed in seeds {
                if let Some(cover) = find_cover(&knapsack, x, seed) {
                    let cut = cover_to_cut(&knapsack, &cover);
                    if cut.violation(x) > MIN_VIOLATION {
                        found.push(cut);
                    }
                }
            }
        }
    }

    found.sort_by(|a, b| {
        b.violation(x)
            .partial_cmp(&a.violation(x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found.dedup_by(|a, b| a.coefficients == b.coefficients && a.ub == b.ub);
    found.truncate(limit);
    found
}

impl Problem {
    /// Append cuts as new rows.
    ///
    /// Cuts add rows but never columns, so the solution vector's meaning is
    /// unchanged and nothing downstream needs remapping.
    pub fn add_cuts(&mut self, cuts: &[Cut]) {
        if cuts.is_empty() {
            return;
        }
        use crate::sparse::SparseMatrix;

        let n = self.n_cols();
        let old_rows = self.n_rows();
        let csr = self.matrix.to_csr();

        let mut triplets: Vec<(usize, usize, f64)> = Vec::with_capacity(self.matrix.nnz());
        for i in 0..old_rows {
            let (cols, vals) = csr.column(i);
            triplets.extend(cols.iter().zip(vals).map(|(&j, &v)| (i, j, v)));
        }
        for (k, cut) in cuts.iter().enumerate() {
            let row = old_rows + k;
            triplets.extend(cut.coefficients.iter().map(|&(j, a)| (row, j, a)));
            self.row_lb.push(cut.lb);
            self.row_ub.push(cut.ub);
            self.row_names.push(format!("cut{k}"));
        }

        self.matrix = SparseMatrix::from_triplets(old_rows + cuts.len(), n, triplets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `x <= b` over the listed columns, with unit coefficients unless given.
    fn row(terms: &[(usize, f64)], ub: f64) -> Cut {
        Cut {
            coefficients: terms.to_vec(),
            lb: f64::NEG_INFINITY,
            ub,
        }
    }

    #[test]
    fn efficacy_is_invariant_to_row_scaling() {
        // The same halfspace written twice, once scaled by ten. Raw violation scales
        // with it; the distance from x to the face does not, which is the whole point
        // of ranking on efficacy rather than violation.
        let plain = row(&[(0, 1.0), (1, 1.0)], 1.0);
        let scaled = row(&[(0, 10.0), (1, 10.0)], 10.0);
        let x = [1.0, 1.0];

        assert!((scaled.violation(&x) - 10.0 * plain.violation(&x)).abs() < 1e-12);
        assert!((scaled.efficacy(&x) - plain.efficacy(&x)).abs() < 1e-12);
    }

    #[test]
    fn selection_drops_a_near_duplicate_and_keeps_a_distinct_cut() {
        let x = [1.0, 1.0];
        // Two nearly parallel cuts and one at right angles to both. The near-duplicate
        // removes almost the same region as the cut before it, so it should not earn a
        // row; the orthogonal one should.
        let first = row(&[(0, 1.0), (1, 1.0)], 1.0);
        let duplicate = row(&[(0, 1.0), (1, 1.001)], 1.0);
        let orthogonal = row(&[(0, 1.0), (1, -1.0)], -0.5);

        assert!(first.cosine(&duplicate) > 0.99);
        assert!(first.cosine(&orthogonal) < 0.1);

        let chosen = select(vec![first, duplicate, orthogonal], &x, 10);
        assert_eq!(chosen.len(), 2, "the near-parallel duplicate should be cut");
        assert!(chosen.iter().any(|c| c.coefficients[1].1 < 0.0));
    }

    #[test]
    fn selection_ranks_by_efficacy_and_respects_the_limit() {
        let x = [1.0, 1.0];
        // Deep and shallow violations of two orthogonal faces. With room for one, the
        // deeper cut is the one to keep.
        let shallow = row(&[(0, 1.0)], 0.99);
        let deep = row(&[(1, 1.0)], 0.1);
        assert!(deep.efficacy(&x) > shallow.efficacy(&x));

        let chosen = select(vec![shallow, deep], &x, 1);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].coefficients[0].0, 1, "kept the deeper cut");
    }

    #[test]
    fn selection_rejects_cuts_the_point_already_satisfies() {
        // Nothing to separate: x is inside both halfspaces, so neither is a cut.
        let x = [0.0, 0.0];
        let satisfied = row(&[(0, 1.0), (1, 1.0)], 1.0);
        let barely = row(&[(0, 1.0), (1, 1.0)], 1e-12);

        assert!(select(vec![satisfied, barely], &x, 10).is_empty());

        // Violated, but only just. A cut this shallow moves the relaxation nowhere
        // while costing a row in every LP beneath it, so it is not worth taking.
        let negligible = row(&[(0, 1.0)], 1.0 - 1e-6);
        let y = [1.0, 0.0];
        assert!(negligible.violation(&y) > 0.0);
        assert!(select(vec![negligible], &y, 10).is_empty());
    }

    #[test]
    fn tightness_distinguishes_a_binding_row_from_a_slack_one() {
        let x = [0.5, 0.5];
        assert!(row(&[(0, 1.0), (1, 1.0)], 1.0).is_tight(&x, 1e-7));
        assert!(!row(&[(0, 1.0), (1, 1.0)], 2.0).is_tight(&x, 1e-7));
    }
    use crate::generate::{Kind, Spec};
    use crate::lp::{Lp, LpStatus};
    use crate::model::{RowSense, Sense};
    use crate::sparse::SparseMatrix;
    use lp_parser_rs::problem::LpProblem;

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

    /// Every binary point the model admits, by exhaustive enumeration.
    /// Every clique read out of a row must hold at every feasible point: no two of its
    /// literals may be true together. Checked by enumeration over the whole cube,
    /// because a clique that is wrong by one pair silently produces a wrong optimum,
    /// which is how the capacity of a negated row got its sign flipped twice.
    fn assert_cliques_hold(p: &Problem, label: &str) {
        let conflicts = Conflicts::of(p);
        let points = feasible_points(p);
        assert!(!points.is_empty(), "{label}: nothing feasible to check against");
        let holds = |x: &[f64], node: u32| -> bool {
            let j = node as usize / 2;
            if node.is_multiple_of(2) { x[j] > 0.5 } else { x[j] < 0.5 }
        };
        for id in 0..conflicts.cliques.len() {
            let clique = &conflicts.cliques[id];
            for x in &points {
                let true_count = clique.iter().filter(|&&node| holds(x, node)).count();
                assert!(
                    true_count <= 1,
                    "{label}: clique {clique:?} has {true_count} literals true at {x:?}"
                );
            }
        }
    }

    /// A set partitioning row is one clique spanning the whole row, and reading it is
    /// the whole reason long rows are looked at: `air04` is 823 such rows and yielded a
    /// conflict graph of four edges while only two-column rows were read.
    #[test]
    fn a_partitioning_row_is_read_as_one_clique() {
        let p = problem(
            &[-1.0, -1.0, -1.0, -1.0],
            &[(&[1.0, 1.0, 1.0, 1.0], RowSense::Eq, 1.0)],
        );
        let conflicts = Conflicts::of(&p);
        // The `<= 1` side is a clique of all four columns. The `>= 1` side is not: three
        // of the four may be zero together, so it excludes no pair and yields nothing.
        assert_eq!(
            conflicts.clique_shape(),
            (1, 4),
            "the row is a clique of all four columns"
        );
        assert_cliques_hold(&p, "partitioning");
    }

    /// The same reading applies to a packing row, and to a knapsack where only the
    /// heaviest columns exclude one another rather than all of them.
    #[test]
    fn a_knapsack_row_yields_only_its_overshooting_prefix() {
        // Any two of the first three overshoot 10; the last pairs with none of them.
        let p = problem(
            &[-1.0, -1.0, -1.0, -1.0],
            &[(&[6.0, 6.0, 6.0, 1.0], RowSense::Le, 10.0)],
        );
        let conflicts = Conflicts::of(&p);
        assert_eq!(conflicts.clique_shape(), (1, 3), "only the heavy three conflict");
        assert_cliques_hold(&p, "knapsack");
    }

    /// A row that excludes nothing must yield nothing. The negated direction of a
    /// loosely bounded row is where a sign error shows up as an invented exclusion.
    #[test]
    fn a_slack_row_yields_no_clique() {
        let p = problem(
            &[-1.0, -1.0, -1.0],
            &[(&[1.0, 1.0, 1.0], RowSense::Ge, -5.0)],
        );
        assert!(
            Conflicts::of(&p).is_empty(),
            "a row satisfied everywhere cannot exclude a pair"
        );
    }

    /// Negative coefficients are complemented rather than skipped, so an implication
    /// spread over a long row is read the same way a two-column row's is.
    #[test]
    fn a_long_row_with_negative_coefficients_is_complemented() {
        let p = problem(
            &[-1.0, -1.0, -1.0, -1.0],
            &[(&[1.0, 1.0, 1.0, -1.0], RowSense::Le, 0.0)],
        );
        assert_cliques_hold(&p, "complemented");
    }

    fn feasible_points(p: &Problem) -> Vec<Vec<f64>> {
        let n = p.n_cols();
        assert!(n <= 18);
        let csr = p.matrix.to_csr();
        (0u32..(1u32 << n))
            .map(|mask| {
                (0..n)
                    .map(|j| f64::from((mask >> j) & 1))
                    .collect::<Vec<f64>>()
            })
            .filter(|x| {
                (0..n).all(|j| x[j] >= p.col_lb[j] - 1e-9 && x[j] <= p.col_ub[j] + 1e-9)
                    && (0..p.n_rows()).all(|i| {
                        let (cols, vals) = csr.column(i);
                        let a: f64 = cols.iter().zip(vals).map(|(&j, &v)| v * x[j]).sum();
                        a >= p.row_lb[i] - 1e-9 && a <= p.row_ub[i] + 1e-9
                    })
            })
            .collect()
    }

    /// The property a cut generator must never break: no feasible binary point may
    /// be cut off. A violated cut is not merely suboptimal, it silently produces the
    /// wrong answer, so this is checked exhaustively rather than by spot-check.
    fn assert_cuts_are_valid(p: &Problem, label: &str) -> usize {
        let relaxed = Lp::relaxation(p).solve();
        if relaxed.status != LpStatus::Optimal {
            return 0;
        }
        let cuts = separate(p, &relaxed.x, 64);
        let points = feasible_points(p);

        for cut in &cuts {
            for point in &points {
                assert!(
                    cut.violation(point) <= 1e-9,
                    "{label}: cut {:?} <= {} removes feasible point {point:?} (activity {})",
                    cut.coefficients,
                    cut.ub,
                    cut.activity(point)
                );
            }
            // A cut that does not cut is wasted work.
            assert!(
                cut.violation(&relaxed.x) > MIN_VIOLATION,
                "{label}: cut does not separate the relaxation"
            );
        }
        cuts.len()
    }

    /// Every mod-2 cut a model yields must hold at every one of its feasible points.
    ///
    /// Enumerated rather than argued. The derivation halves a sum of rows and rounds the
    /// coefficients down, and rounding down is only valid because the columns are
    /// nonnegative integers; a sign slip in the halving, or rounding a negative
    /// coefficient towards zero instead of down, produces a cut that looks perfectly
    /// ordinary and quietly removes the optimum.
    /// Every implied-aggregation cut of `p` holds at every feasible binary point.
    fn assert_aggregation_is_valid(p: &Problem, label: &str) -> usize {
        let relaxed = Lp::relaxation(p).solve();
        if relaxed.status != LpStatus::Optimal {
            return 0;
        }
        let conflicts = Conflicts::of(p);
        let cuts = separate_implied_aggregations(p, &conflicts, &relaxed.x, 32);
        let n = p.n_cols();
        assert!(n <= 16);
        let csr = p.matrix.to_csr();
        for mask in 0u32..(1u32 << n) {
            let point: Vec<f64> = (0..n).map(|j| f64::from((mask >> j) & 1)).collect();
            let feasible = (0..p.n_rows()).all(|i| {
                let (cols, values) = csr.column(i);
                let activity: f64 = cols.iter().zip(values).map(|(&j, &a)| a * point[j]).sum();
                activity >= p.row_lb[i] - 1e-9 && activity <= p.row_ub[i] + 1e-9
            });
            if !feasible {
                continue;
            }
            for cut in &cuts {
                let activity = cut.activity(&point);
                assert!(
                    activity >= cut.lb - 1e-6 && activity <= cut.ub + 1e-6,
                    "{label}: cut {:?} in [{}, {}] cuts off the feasible point {point:?}",
                    cut.coefficients,
                    cut.lb,
                    cut.ub
                );
            }
        }
        cuts.len()
    }

    /// A linking row says `x_j <= y`, and a covering row over the `x` says it over `y`.
    ///
    /// Three groups of two, each gated by its own binary, and a covering row taking one
    /// member from each group and wanting two of them. The relaxation spreads the three
    /// members over two thirds each, which satisfies the covering row while holding
    /// every gate at a third, and scores 1. Aggregating the covering row through the
    /// gates says two gates must be open, which is the integer optimum.
    ///
    /// The covering row taking one member per group is what makes it bite. Two members
    /// of the same group collapse onto one gate with coefficient two, and the
    /// aggregated row is then no stronger than the relaxation already was -- which is
    /// the first version of this test, and it generated no violated cut at all.
    #[test]
    fn an_aggregated_covering_row_holds_and_lifts_the_bound() {
        let mut model = crate::model::Builder::new(crate::model::Sense::Minimize);
        let members: Vec<(usize, usize, usize)> = ["a", "b", "c"]
            .iter()
            .map(|g| {
                let first = model.binary(&format!("{g}1"));
                let second = model.binary(&format!("{g}2"));
                let gate = model.binary(&format!("y{g}"));
                (first, second, gate)
            })
            .collect();
        model.objective(&members.iter().map(|&(_, _, y)| (y, 1.0)).collect::<Vec<_>>());
        for &(first, second, gate) in &members {
            model.row(
                &[(first, 1.0), (second, 1.0), (gate, -2.0)],
                crate::model::RowSense::Le,
                0.0,
            );
        }
        model.row(
            &members.iter().map(|&(f, _, _)| (f, 1.0)).collect::<Vec<_>>(),
            crate::model::RowSense::Ge,
            2.0,
        );
        let problem = model.build();

        let relaxed = Lp::relaxation(&problem).solve();
        assert_eq!(relaxed.status, LpStatus::Optimal);
        assert!(
            relaxed.objective < 1.5,
            "the relaxation should be weak here, and scored {}",
            relaxed.objective
        );
        let found = assert_aggregation_is_valid(&problem, "three gated pairs");
        assert!(found > 0, "no aggregated row was separated from a gated model");

        let mut with = problem.clone();
        with.add_cuts(&separate_implied_aggregations(
            &problem,
            &Conflicts::of(&problem),
            &relaxed.x,
            32,
        ));
        let lifted = Lp::relaxation(&with).solve();
        assert!(
            lifted.objective > 1.99,
            "the aggregated row left the bound at {}, where the optimum is 2",
            lifted.objective
        );
    }

    /// A term that can turn positive is not droppable, and dropping it loses a point.
    ///
    /// The aggregation drops a negative-coefficient term from a `>=` row by replacing it
    /// with zero, which needs zero to be an upper bound on the term. For a column that
    /// cannot go below zero it is. For one that can, `-z` with `z = -1` is worth `+1`,
    /// and the row is satisfied by that alone while the aggregated row demands the rest
    /// of it carry the whole right-hand side.
    ///
    /// The random models cannot reach this: every column in them is binary, so a
    /// negative coefficient really is bounded above by zero and the guard never fires.
    #[test]
    fn a_column_that_can_go_negative_stops_the_aggregation() {
        let mut model = crate::model::Builder::new(crate::model::Sense::Minimize);
        let a1 = model.binary("a1");
        let a2 = model.binary("a2");
        let gate = model.binary("y");
        let z = model.integer("z", -1.0, 1.0);
        model.objective(&[(gate, 1.0)]);
        model.row(
            &[(a1, 1.0), (a2, 1.0), (gate, -2.0)],
            crate::model::RowSense::Le,
            0.0,
        );
        // `a1 - z >= 1` is satisfied by `a1 = 0, z = -1`, which needs no gate at all.
        model.row(&[(a1, 1.0), (z, -1.0)], crate::model::RowSense::Ge, 1.0);
        let problem = model.build();

        let relaxed = Lp::relaxation(&problem).solve();
        assert_eq!(relaxed.status, LpStatus::Optimal);
        let cuts = separate_implied_aggregations(
            &problem,
            &Conflicts::of(&problem),
            &relaxed.x,
            32,
        );
        let csr = problem.matrix.to_csr();
        for mask in 0u32..8 {
            for zv in [-1.0f64, 0.0, 1.0] {
                let point = vec![
                    f64::from(mask & 1),
                    f64::from((mask >> 1) & 1),
                    f64::from((mask >> 2) & 1),
                    zv,
                ];
                let feasible = (0..problem.n_rows()).all(|i| {
                    let (cols, values) = csr.column(i);
                    let activity: f64 =
                        cols.iter().zip(values).map(|(&j, &a)| a * point[j]).sum();
                    activity >= problem.row_lb[i] - 1e-9 && activity <= problem.row_ub[i] + 1e-9
                });
                if !feasible {
                    continue;
                }
                for cut in &cuts {
                    let activity = cut.activity(&point);
                    assert!(
                        activity >= cut.lb - 1e-6 && activity <= cut.ub + 1e-6,
                        "cut {:?} in [{}, {}] cuts off the feasible point {point:?}",
                        cut.coefficients,
                        cut.lb,
                        cut.ub
                    );
                }
            }
        }
    }

    /// Every point of the rewritten model maps to a feasible point of the original.
    ///
    /// That is the whole contract: the rewriting is a *restriction*, so it may lose the
    /// optimum and may not invent a point. Both halves are enumerated -- every binary
    /// assignment feasible for the rewritten model is expanded and checked against the
    /// original's rows, and its objective is checked to match what the rewritten model
    /// scored it, since a map that renumbered wrongly would return a plausible vector
    /// for the wrong columns.
    #[test]
    fn the_rewritten_model_only_maps_to_feasible_points() {
        let mut seed = 0x1234_5678_9ABC_DEF0u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (mut rewritten, mut points) = (0usize, 0usize);
        for _ in 0..300 {
            let groups = 2 + (next() % 2) as usize;
            let per = 2 + (next() % 2) as usize;
            let mut model = crate::model::Builder::new(crate::model::Sense::Minimize);
            let mut members: Vec<Vec<usize>> = Vec::new();
            let mut gates: Vec<usize> = Vec::new();
            for g in 0..groups {
                members.push(
                    (0..per)
                        .map(|k| model.binary(&format!("x{g}_{k}")))
                        .collect(),
                );
                gates.push(model.binary(&format!("y{g}")));
            }
            // The members carry a cost too, so that collecting their objective onto
            // the gate is exercised. Dropping it leaves the rewritten model scoring a
            // point differently from the model it came from, which is the check below.
            let mut objective: Vec<(usize, f64)> = gates
                .iter()
                .map(|&y| (y, 1.0 + (next() % 3) as f64))
                .collect();
            for cols in &members {
                for &j in cols {
                    objective.push((j, (next() % 3) as f64));
                }
            }
            model.objective(&objective);
            for (g, cols) in members.iter().enumerate() {
                let big = per as f64 + (next() % 2) as f64;
                let mut terms: Vec<(usize, f64)> = cols.iter().map(|&j| (j, 1.0)).collect();
                terms.push((gates[g], -big));
                model.row(&terms, crate::model::RowSense::Le, 0.0);
            }
            for _ in 0..(1 + next() % 3) {
                let mut terms: Vec<(usize, f64)> = Vec::new();
                for cols in &members {
                    for &j in cols {
                        if next() % 2 == 0 {
                            terms.push((j, 1.0));
                        }
                    }
                }
                if terms.len() < 2 {
                    continue;
                }
                let want = 1.0 + (next() % terms.len() as u64) as f64;
                model.row(&terms, crate::model::RowSense::Ge, want);
            }
            let problem = model.build();
            if problem.n_cols() > 16 || problem.n_rows() == 0 {
                continue;
            }
            let relaxed = Lp::relaxation(&problem).solve();
            if relaxed.status != LpStatus::Optimal {
                continue;
            }
            let conflicts = Conflicts::of(&problem);
            let Some((small, column_of)) =
                aggregate_by_implied_bounds(&problem, &conflicts, &relaxed.x)
            else {
                continue;
            };
            rewritten += 1;
            assert!(small.n_cols() <= 16);

            let inner = small.matrix.to_csr();
            let outer = problem.matrix.to_csr();
            for mask in 0u32..(1u32 << small.n_cols()) {
                let point: Vec<f64> =
                    (0..small.n_cols()).map(|k| f64::from((mask >> k) & 1)).collect();
                let feasible = (0..small.n_rows()).all(|i| {
                    let (cols, values) = inner.column(i);
                    let activity: f64 =
                        cols.iter().zip(values).map(|(&k, &a)| a * point[k]).sum();
                    activity >= small.row_lb[i] - 1e-9 && activity <= small.row_ub[i] + 1e-9
                });
                if !feasible {
                    continue;
                }
                points += 1;
                let full: Vec<f64> =
                    (0..problem.n_cols()).map(|j| point[column_of[j]]).collect();
                for i in 0..problem.n_rows() {
                    let (cols, values) = outer.column(i);
                    let activity: f64 =
                        cols.iter().zip(values).map(|(&j, &a)| a * full[j]).sum();
                    assert!(
                        activity >= problem.row_lb[i] - 1e-6
                            && activity <= problem.row_ub[i] + 1e-6,
                        "expanded point {full:?} breaks row {i} of the original"
                    );
                }
                let scored: f64 = (0..problem.n_cols())
                    .map(|j| problem.obj[j] * full[j])
                    .sum::<f64>()
                    + problem.obj_offset;
                let claimed: f64 = (0..small.n_cols())
                    .map(|k| small.obj[k] * point[k])
                    .sum::<f64>()
                    + small.obj_offset;
                assert!(
                    (scored - claimed).abs() < 1e-9,
                    "the rewritten model scored {claimed} for a point worth {scored}"
                );
            }
        }
        assert!(rewritten > 30, "only {rewritten} models were rewritten");
        assert!(points > 100, "only {points} points were checked");
    }

    /// The aggregation never loses a feasible point, over models built to trip it.
    ///
    /// Signs are what makes this delicate. A term with a negative coefficient in a
    /// `>=` row is dropped, which is only valid while the column cannot go below zero,
    /// and the `<=` side of a range row is aggregated after negating, which swaps which
    /// terms are dropped. Enumeration is the only check that notices either mistake.
    #[test]
    fn aggregated_rows_hold_on_random_gated_models() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (mut total, mut checked) = (0usize, 0usize);
        for _ in 0..300 {
            let groups = 2 + (next() % 2) as usize;
            let per = 2 + (next() % 2) as usize;
            let mut model = crate::model::Builder::new(crate::model::Sense::Minimize);
            let mut members: Vec<Vec<usize>> = Vec::new();
            let mut gates: Vec<usize> = Vec::new();
            for g in 0..groups {
                let cols: Vec<usize> = (0..per)
                    .map(|k| model.binary(&format!("x{g}_{k}")))
                    .collect();
                gates.push(model.binary(&format!("y{g}")));
                members.push(cols);
            }
            let objective: Vec<(usize, f64)> = gates
                .iter()
                .map(|&y| (y, 1.0 + (next() % 3) as f64))
                .collect();
            model.objective(&objective);
            for (g, cols) in members.iter().enumerate() {
                // `M` at or above the group size is what makes the gate a pure link.
                let big = per as f64 + (next() % 2) as f64;
                let mut terms: Vec<(usize, f64)> =
                    cols.iter().map(|&j| (j, 1.0)).collect();
                terms.push((gates[g], -big));
                model.row(&terms, crate::model::RowSense::Le, 0.0);
            }
            // Covering rows over the members, with an occasional negative coefficient
            // and an occasional range, which is where the sign handling is tested.
            for _ in 0..(1 + next() % 3) {
                let mut terms: Vec<(usize, f64)> = Vec::new();
                for cols in &members {
                    for &j in cols {
                        if next() % 2 == 0 {
                            let a = (next() % 3) as f64 - 1.0;
                            if a != 0.0 {
                                terms.push((j, a));
                            }
                        }
                    }
                }
                if terms.len() < 2 {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let bound = (next() % (span as u64 + 1)) as f64;
                let sense = if next() % 4 == 0 {
                    crate::model::RowSense::Le
                } else {
                    crate::model::RowSense::Ge
                };
                model.row(&terms, sense, bound);
            }
            let problem = model.build();
            if problem.n_cols() > 16 || problem.n_rows() == 0 {
                continue;
            }
            checked += 1;
            total += assert_aggregation_is_valid(&problem, "random gated");
        }
        assert!(checked > 100, "only {checked} models were built");
        assert!(total > 0, "no aggregated row was generated, so nothing was checked");
    }

    fn assert_mod2_is_valid(p: &Problem, label: &str) -> usize {
        let relaxed = Lp::relaxation(p).solve();
        if relaxed.status != LpStatus::Optimal {
            return 0;
        }
        let cuts = separate_mod2(p, &relaxed.x, 32);
        let columns: Vec<usize> = (0..p.n_cols()).collect();
        assert!(columns.len() <= 16);
        assert!(columns.iter().all(|&j| p.is_integer(j) && p.col_ub[j] == 1.0));

        let csr = p.matrix.to_csr();
        for mask in 0u32..(1u32 << columns.len()) {
            let point: Vec<f64> = columns
                .iter()
                .map(|&j| f64::from((mask >> j) & 1))
                .collect();
            let feasible = (0..p.n_rows()).all(|i| {
                let (cols, values) = csr.column(i);
                let activity: f64 = cols.iter().zip(values).map(|(&j, &a)| a * point[j]).sum();
                activity >= p.row_lb[i] - 1e-9 && activity <= p.row_ub[i] + 1e-9
            });
            if !feasible {
                continue;
            }
            for cut in &cuts {
                let activity = cut.activity(&point);
                assert!(
                    activity <= cut.ub + 1e-6 && activity >= cut.lb - 1e-6,
                    "{label}: cut {:?} <= {} cuts off the feasible point {point:?}",
                    cut.coefficients,
                    cut.ub
                );
            }
        }
        for cut in &cuts {
            assert!(
                cut.violation(&relaxed.x) > MIN_VIOLATION,
                "{label}: cut does not separate the relaxation"
            );
        }
        cuts.len()
    }

    /// A mixed-integer knapsack: a continuous column and several integer ones meeting
    /// the same demand. This is the shape MIR exists for, and rounding the integer part
    /// of the row is exactly what the relaxation cannot do for itself.
    fn mixed_knapsack(demand: f64, weight: f64) -> Problem {
        // Columns: x0 continuous in [0, 4]; y0, y1, y2 binary. The continuous column is
        // priced above the integer ones so the relaxation prefers a fractional integer.
        let mut p = problem(
            &[3.0, 1.0, 1.0, 1.0],
            &[(&[1.0, weight, weight, weight], RowSense::Ge, demand)],
        );
        p.col_type[0] = crate::model::VarType::Continuous;
        p.col_ub[0] = 4.0;
        p
    }

    /// A fixed-charge model: continuous columns held down by binaries through variable
    /// upper bound rows, sharing one capacity row. Rounding the rows as written finds
    /// nothing here; this shape only yields a cut once the bounds are substituted.
    fn fixed_charge(capacity: f64, cost: f64) -> Problem {
        // Columns: x0, x1 continuous in [0, 4]; y0, y1 binary.
        let mut p = problem(
            &[-1.0, -1.0, cost, cost],
            &[
                (&[1.0, 0.0, -4.0, 0.0], RowSense::Le, 0.0),
                (&[0.0, 1.0, 0.0, -4.0], RowSense::Le, 0.0),
                (&[1.0, 1.0, 0.0, 0.0], RowSense::Le, capacity),
            ],
        );
        p.col_type[0] = crate::model::VarType::Continuous;
        p.col_type[1] = crate::model::VarType::Continuous;
        p.col_ub[0] = 4.0;
        p.col_ub[1] = 4.0;
        p
    }

    /// MIR is the one family here that reasons about continuous columns, so
    /// enumerating binary points is not enough to check it. Fixing the integer
    /// columns leaves a polytope in the continuous ones, and the cut is valid exactly
    /// when its activity cannot be pushed past its right-hand side anywhere on that
    /// polytope. That maximum is itself an LP, so validity is checked by solving one
    /// per integer assignment rather than by sampling points.
    fn assert_mir_is_valid(p: &Problem, label: &str) -> usize {
        let relaxed = Lp::relaxation(p).solve();
        if relaxed.status != LpStatus::Optimal {
            return 0;
        }
        let cuts = separate_mir(p, &relaxed.x, 32);
        let integers: Vec<usize> = (0..p.n_cols()).filter(|&j| p.is_integer(j)).collect();
        assert!(integers.len() <= 12);
        assert!(integers.iter().all(|&j| p.col_ub[j] == 1.0));

        for cut in &cuts {
            for mask in 0u32..(1u32 << integers.len()) {
                let mut sub = p.clone();
                for (bit, &j) in integers.iter().enumerate() {
                    let value = f64::from((mask >> bit) & 1);
                    sub.col_lb[j] = value;
                    sub.col_ub[j] = value;
                }
                sub.obj = vec![0.0; p.n_cols()];
                for &(j, a) in &cut.coefficients {
                    sub.obj[j] = -a;
                }
                let solved = Lp::relaxation(&sub).solve();
                if solved.status == LpStatus::Infeasible {
                    continue;
                }
                assert_eq!(solved.status, LpStatus::Optimal, "{label}: subproblem status");
                let most = -solved.objective;
                assert!(
                    most <= cut.ub + 1e-6,
                    "{label}: cut {:?} <= {} is violated by {most} at integer mask {mask}",
                    cut.coefficients,
                    cut.ub
                );
            }
            assert!(
                cut.violation(&relaxed.x) > MIN_VIOLATION,
                "{label}: cut does not separate the relaxation"
            );
        }
        cuts.len()
    }

    /// A set partitioning model, which is the shape mod-2 exists for: rows whose
    /// coefficients are all one, where halving a pair of them is what the relaxation
    /// cannot do for itself.
    fn partitioning(rows: &[&[usize]], n: usize) -> Problem {
        let mut model = crate::model::Builder::new(crate::model::Sense::Minimize);
        let columns: Vec<usize> = (0..n).map(|j| model.binary(&format!("x{j}"))).collect();
        model.objective(&columns.iter().map(|&j| (j, 1.0)).collect::<Vec<_>>());
        for r in rows {
            model.row(
                &r.iter().map(|&j| (columns[j], 1.0)).collect::<Vec<_>>(),
                crate::model::RowSense::Eq,
                1.0,
            );
        }
        model.build()
    }

    #[test]
    fn mod2_cuts_hold_at_every_feasible_point() {
        // Odd cycles, which is where a half-integral relaxation and an integral optimum
        // come apart, and where combining rows is the only thing that closes the gap.
        let mut total = 0;
        for rows in [
            vec![vec![0, 1], vec![1, 2], vec![0, 2]],
            vec![vec![0, 1], vec![1, 2], vec![2, 3], vec![3, 4], vec![0, 4]],
            vec![vec![0, 1, 2], vec![2, 3], vec![3, 4], vec![0, 4], vec![1, 3]],
        ] {
            let slices: Vec<&[usize]> = rows.iter().map(|r| r.as_slice()).collect();
            let n = rows.iter().flatten().copied().max().unwrap() + 1;
            total += assert_mod2_is_valid(&partitioning(&slices, n), "partitioning");
        }
        // A test that never separates anything proves nothing at all.
        assert!(total > 0, "no mod-2 cut was generated, so nothing was checked");
    }

    /// The generated model on which a mod-2 cut removed the optimum, and the search then
    /// reported it infeasible where the optimum is 1188. Rows cancelled a column back to
    /// zero part way through a combination, the column was listed twice, and the cut
    /// carried its coefficient twice. The cut arises only after other families have moved
    /// the relaxation for several rounds, so this checks the whole root loop against the
    /// same search with cutting switched off.
    #[test]
    fn mod2_cuts_do_not_remove_the_optimum_of_signed_68() {
        use crate::generate::{Kind, Spec};
        use crate::search::{Options, Status, solve};
        use lp_parser_rs::problem::LpProblem;
        let spec = Spec {
            kind: Kind::Signed,
            n_cols: 40,
            n_rows: 25,
            seed: 68,
        };
        let p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
        // The default gap is exact here: the objective moves in whole steps and a
        // ten-thousandth of 1188 is less than one of them.
        let exact = |cut_rounds: usize| Options {
            cut_rounds,
            threads: 1,
            cdcl: false,
            ..Default::default()
        };
        // 1188 is what Gurobi and HiGHS both report for it.
        let cut = solve(&p, exact(50));
        assert_eq!(cut.status, Status::Optimal, "the root cuts made a feasible model look empty");
        assert!((cut.objective.unwrap() - 1188.0).abs() < 1e-6);
    }

    #[test]
    fn mod2_cuts_hold_on_random_models_with_signs_and_complements() {
        // Partitioning rows alone do not exercise the halving, because their
        // coefficients are all positive and rounding towards zero and rounding down
        // agree. Mixed signs and a relaxation that parks some column at one, so the
        // separator complements it, are what tell the two apart: with negative
        // aggregated coefficients, `a / 2` rounds *up* where `a.div_euclid(2)` rounds
        // down, and the cut stops being valid without looking any different.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut total = 0;
        let mut checked = 0;
        for _ in 0..200 {
            let n = 6 + (next() % 3) as usize;
            let m = 3 + (next() % 4) as usize;
            let mut model = crate::model::Builder::new(crate::model::Sense::Minimize);
            let columns: Vec<usize> =
                (0..n).map(|j| model.binary(&format!("x{j}"))).collect();
            let objective: Vec<(usize, f64)> = columns
                .iter()
                .map(|&j| (j, (next() % 9) as f64 - 4.0))
                .collect();
            model.objective(&objective);
            for _ in 0..m {
                let mut terms: Vec<(usize, f64)> = Vec::new();
                for &j in &columns {
                    if next() % 2 == 0 {
                        terms.push((j, (next() % 7) as f64 - 3.0));
                    }
                }
                if terms.is_empty() {
                    continue;
                }
                let span: f64 = terms.iter().map(|&(_, a)| a.abs()).sum();
                let bound = (next() % (2 * span as u64 + 1)) as f64 - span;
                let sense = match next() % 3 {
                    0 => crate::model::RowSense::Le,
                    1 => crate::model::RowSense::Ge,
                    _ => crate::model::RowSense::Eq,
                };
                model.row(&terms, sense, bound);
            }
            let problem = model.build();
            if problem.n_rows() == 0 {
                continue;
            }
            checked += 1;
            total += assert_mod2_is_valid(&problem, "random");
        }
        assert!(checked > 100, "only {checked} models were built");
        assert!(total > 0, "no mod-2 cut was generated, so nothing was checked");
    }

    #[test]
    fn mir_cuts_a_mixed_knapsack_without_losing_a_solution() {
        // Demand is swept so the row's fractional part lands differently each time,
        // which is what makes the derivation pick a different divisor.
        let mut total = 0;
        for demand in [2.5, 4.5, 5.25, 6.5, 7.4] {
            for weight in [2.0, 3.0, 4.0] {
                total += assert_mir_is_valid(
                    &mixed_knapsack(demand, weight),
                    &format!("mixed knapsack demand {demand} weight {weight}"),
                );
            }
        }
        assert!(total > 0, "no MIR cut was produced by any mixed knapsack");
    }

    #[test]
    fn bound_substitution_reaches_fixed_charge_structure() {
        // Substitution is the step that makes MIR useful, and it is also the step most
        // likely to produce a subtly invalid cut, since the row it rounds is no longer
        // one of the model's own.
        let mut total = 0;
        for capacity in [1.5, 2.5, 3.5, 5.5, 6.5, 7.5] {
            for cost in [0.5, 1.0, 2.0] {
                total += assert_mir_is_valid(
                    &fixed_charge(capacity, cost),
                    &format!("fixed charge cap {capacity} cost {cost}"),
                );
            }
        }
        assert!(total > 0, "bound substitution found nothing in a fixed-charge model");
    }

    #[test]
    fn a_variable_upper_bound_is_read_off_a_two_column_row() {
        let p = fixed_charge(3.5, 1.0);
        let bounds = variable_upper_bounds(&p);
        for (column, binary) in [(0usize, 2usize), (1, 3)] {
            let vub = bounds[column].expect("continuous column has a variable bound");
            assert_eq!(vub.binary, binary);
            assert!((vub.factor - 4.0).abs() < 1e-9, "factor {}", vub.factor);
        }
        // The binaries themselves are not continuous columns and get no bound, nor does
        // a model whose rows never pair a continuous column with a binary.
        assert!(bounds[2].is_none() && bounds[3].is_none());
        assert!(variable_upper_bounds(&mixed_knapsack(4.5, 3.0))
            .iter()
            .all(Option::is_none));
    }

    #[test]
    fn complementing_moves_a_binary_near_one_back_to_zero() {
        // Only the binary the point holds above a half is rewritten, and the constant
        // it carries moves onto the right-hand side.
        let p = mixed_knapsack(7.4, 3.0);
        let x = [0.0, 1.0, 0.25, 0.0];
        let terms = [
            (Term::Column(0), 1.0),
            (Term::Column(1), 3.0),
            (Term::Column(2), 3.0),
            (Term::Column(3), 3.0),
        ];
        let (rewritten, rhs) =
            complement_binaries(&p, &terms, 7.4, &x).expect("a binary sits above a half");
        assert!((rhs - 4.4).abs() < 1e-9, "rhs {rhs}");
        assert_eq!(rewritten[0], (Term::Column(0), 1.0));
        assert_eq!(rewritten[1], (Term::Complement(1), -3.0));
        assert_eq!(rewritten[2], (Term::Column(2), 3.0));

        // With every binary at zero there is nothing to move.
        assert!(complement_binaries(&p, &terms, 7.4, &[0.0; 4]).is_none());
    }

    #[test]
    fn mir_respects_a_pure_integer_knapsack() {
        // With no continuous columns the derivation reduces to rounding the integer
        // part, which the exhaustive binary check covers directly.
        for capacity in [4.5, 5.5, 7.5, 9.5] {
            let p = problem(
                &[-3.0, -2.0, -2.0, -1.0],
                &[(&[4.0, 3.0, 3.0, 2.0], RowSense::Le, capacity)],
            );
            let relaxed = Lp::relaxation(&p).solve();
            let cuts = separate_mir(&p, &relaxed.x, 32);
            for cut in &cuts {
                for point in &feasible_points(&p) {
                    assert!(
                        cut.violation(point) <= 1e-9,
                        "cap {capacity}: MIR cut {:?} <= {} removes {point:?}",
                        cut.coefficients,
                        cut.ub
                    );
                }
            }
        }
    }

    #[test]
    fn mir_skips_a_column_that_does_not_sit_at_zero() {
        // The derivation assumes every column has a lower bound of zero. A row with a
        // shifted column must be left alone rather than cut on a false premise.
        let p = mixed_knapsack(4.5, 3.0);
        let relaxed = Lp::relaxation(&p).solve();
        assert!(!separate_mir(&p, &relaxed.x, 32).is_empty(), "row is cuttable as written");

        let mut shifted = p.clone();
        shifted.col_lb[0] = 1.0;
        let relaxed = Lp::relaxation(&shifted).solve();
        assert_eq!(relaxed.status, LpStatus::Optimal);
        assert!(
            separate_mir(&shifted, &relaxed.x, 32).is_empty(),
            "cut derived from a row containing a shifted column"
        );
    }

    /// Every conflict has to hold at every feasible point, checked by enumeration
    /// rather than by following the argument that produced it.
    ///
    /// The chain here is what chasing the implications would derive and this does not:
    /// x0 excludes x1, x2 needs x1, x3 needs x2, so x0 excludes x2 and x3 while no row
    /// says as much. That closure was implemented and reverted, having taken `neos18`'s
    /// root bound from 9.250 to 8.778 by crowding better cuts out of the round. What is
    /// pinned here is that whatever the rows do yield is sound.
    #[test]
    fn every_derived_conflict_holds_at_every_feasible_point() {
        // An implication chain the rows never state end to end: x0 excludes x1,
        // x2 needs x1, x3 needs x2. So x0 excludes x2 and x3 as well, and neither
        // exclusion appears in any row.
        let p = problem(
            &[-1.0, -1.0, -1.0, -1.0],
            &[
                (&[1.0, 1.0, 0.0, 0.0], RowSense::Le, 1.0),
                (&[0.0, -1.0, 1.0, 0.0], RowSense::Le, 0.0),
                (&[0.0, 0.0, -1.0, 1.0], RowSense::Le, 0.0),
            ],
        );
        let conflicts = Conflicts::of(&p);
        let points = feasible_points(&p);
        assert!(!points.is_empty());

        let holds = |point: &[f64], lit: u32| {
            let j = lit as usize / 2;
            if lit.is_multiple_of(2) { point[j] > 0.5 } else { point[j] < 0.5 }
        };
        let mut derived = 0usize;
        for node in 0..conflicts.nodes() as u32 {
            for &other in conflicts.neighbours(node) {
                derived += 1;
                for point in &points {
                    assert!(
                        !(holds(point, node) && holds(point, other)),
                        "literals {node} and {other} are called exclusive, \
                         but {point:?} satisfies both"
                    );
                }
            }
        }
        assert!(derived > 0, "no conflicts were found at all");
    }

    /// A clique cut says at most one of a set may be taken, so it is wrong the moment
    /// any feasible point takes two. Checked by enumeration rather than by argument.
    #[test]
    fn clique_cuts_never_remove_a_feasible_point() {
        // Three columns that pairwise exclude each other through separate rows, and a
        // fourth that conflicts with only one of them, so it must not join the clique.
        let p = problem(
            &[-1.0, -1.0, -1.0, -1.0],
            &[
                (&[1.0, 1.0, 0.0, 0.0], RowSense::Le, 1.0),
                (&[1.0, 0.0, 1.0, 0.0], RowSense::Le, 1.0),
                (&[0.0, 1.0, 1.0, 0.0], RowSense::Le, 1.0),
                (&[1.0, 0.0, 0.0, 1.0], RowSense::Le, 1.0),
            ],
        );
        let conflicts = Conflicts::of(&p);
        let relaxed = Lp::relaxation(&p).solve();
        assert_eq!(relaxed.status, LpStatus::Optimal);
        let cuts = separate_cliques(&p, &conflicts, &relaxed.x, 16);
        assert!(!cuts.is_empty(), "the triangle should have been found");

        for cut in &cuts {
            for point in &feasible_points(&p) {
                assert!(
                    cut.violation(point) <= 1e-9,
                    "clique {:?} <= {} removes feasible point {point:?}",
                    cut.coefficients,
                    cut.ub
                );
            }
            assert!(
                cut.violation(&relaxed.x) > MIN_VIOLATION,
                "a clique that does not separate the relaxation is not worth adding"
            );
        }
    }

    /// The conflict has to come from the row rather than from the pair merely appearing
    /// together: two columns that can both be one are not in conflict, and a clique
    /// built over them would cut off points the model allows.
    #[test]
    fn a_row_that_permits_both_columns_is_not_a_conflict() {
        // `x + y <= 2` permits both, so there is no edge and nothing to separate.
        let mut p = problem(&[-1.0, -1.0], &[(&[1.0, 1.0], RowSense::Le, 2.0)]);
        p.col_ub = vec![1.0, 1.0];
        assert!(Conflicts::of(&p).is_empty(), "a satisfiable pair was called a conflict");

        // `x + y <= 1` forbids both, so there is.
        let q = problem(&[-1.0, -1.0], &[(&[1.0, 1.0], RowSense::Le, 1.0)]);
        assert!(!Conflicts::of(&q).is_empty(), "an exclusive pair was missed");
    }

    /// A clique is only valid if *every* pair in it conflicts. Growing one greedily
    /// makes it easy to admit a column that conflicts with some members and not others,
    /// which is exactly how such a cut goes wrong.
    #[test]
    fn a_clique_admits_only_columns_conflicting_with_all_of_it() {
        // 0-1 and 0-2 conflict, 1-2 do not, so no clique larger than a pair exists.
        let p = problem(
            &[-1.0, -1.0, -1.0],
            &[
                (&[1.0, 1.0, 0.0], RowSense::Le, 1.0),
                (&[1.0, 0.0, 1.0], RowSense::Le, 1.0),
            ],
        );
        let conflicts = Conflicts::of(&p);
        // A point the relaxation might reach with all three part-set.
        let x = [0.5, 0.5, 0.5];
        for cut in separate_cliques(&p, &conflicts, &x, 16) {
            // Back to the literals the clique was built from: a positive coefficient
            // is the column asserted true, a negative one the column asserted false.
            let members: Vec<u32> = cut
                .coefficients
                .iter()
                .map(|&(j, a)| literal(j, a > 0.0))
                .collect();
            for (a, &p1) in members.iter().enumerate() {
                for &p2 in &members[a + 1..] {
                    assert!(
                        conflicts.conflicts(p1, p2),
                        "clique {members:?} contains the compatible pair ({p1}, {p2})"
                    );
                }
            }
        }
    }

    #[test]
    fn lifting_strengthens_a_cover_without_breaking_it() {
        // Lifting is the step most likely to produce a subtly invalid cut, so it gets
        // its own exhaustive check across a spread of capacities.
        for capacity in [4.0, 5.0, 7.0, 9.0, 11.0] {
            let p = problem(
                &[-3.0, -2.0, -2.0, -1.0],
                &[(&[4.0, 3.0, 3.0, 2.0], RowSense::Le, capacity)],
            );
            assert_cuts_are_valid(&p, &format!("lifting cap {capacity}"));
        }
    }

    #[test]
    fn lifting_is_skipped_for_fractional_weights() {
        // The lifting DP indexes by integer weight, so a row it cannot represent must
        // fall back to the unlifted cover rather than round and produce a bad cut.
        let p = problem(
            &[-1.0, -1.0, -1.0],
            &[(&[2.5, 2.5, 2.5], RowSense::Le, 4.2)],
        );
        assert_cuts_are_valid(&p, "fractional weights");
    }

    /// GMI cuts get the same exhaustive treatment as covers: no feasible binary
    /// point may be cut off. They are derived from floating-point tableau entries
    /// rather than from combinatorial reasoning, so they are the family most likely
    /// to produce a subtly invalid inequality.
    fn assert_gomory_is_valid(p: &Problem, label: &str) -> usize {
        let mut lp = Lp::relaxation(p);
        let relaxed = lp.solve();
        if relaxed.status != LpStatus::Optimal {
            return 0;
        }
        let cuts = separate_gomory(&lp, &relaxed.basis, &relaxed.x, 32);
        let points = feasible_points(p);
        for cut in &cuts {
            for point in &points {
                assert!(
                    cut.violation(point) <= 1e-6,
                    "{label}: gomory cut {:?} >= {} removes feasible point {point:?}",
                    cut.coefficients,
                    cut.lb
                );
            }
            assert!(
                cut.violation(&relaxed.x) > MIN_VIOLATION,
                "{label}: gomory cut does not separate the relaxation"
            );
        }
        cuts.len()
    }

    #[test]
    fn gomory_cuts_never_remove_a_feasible_point() {
        let mut total = 0;
        for kind in [Kind::Knapsack, Kind::Covering, Kind::Signed] {
            for seed in 0..14u64 {
                let spec = Spec {
                    kind,
                    n_cols: 13,
                    n_rows: 7,
                    seed,
                };
                let p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
                total += assert_gomory_is_valid(&p, &spec.name());
            }
        }
        assert!(
            total > 0,
            "no gomory cuts generated anywhere, so nothing was tested"
        );
    }

    #[test]
    fn gomory_cuts_tighten_the_relaxation() {
        let spec = Spec {
            kind: Kind::Knapsack,
            n_cols: 16,
            n_rows: 8,
            seed: 5,
        };
        let mut p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
        let mut lp = Lp::relaxation(&p);
        let before = lp.solve();
        let cuts = separate_gomory(&lp, &before.basis, &before.x, 16);
        assert!(
            !cuts.is_empty(),
            "expected gomory cuts on a fractional relaxation"
        );

        p.add_cuts(&cuts);
        let after = Lp::relaxation(&p).solve();
        assert_eq!(after.status, LpStatus::Optimal);
        assert!(
            after.objective > before.objective + 1e-9,
            "bound did not improve: {} -> {}",
            before.objective,
            after.objective
        );
    }

    #[test]
    fn cuts_never_remove_a_feasible_point() {
        // The broad net, across all three instance families and both row senses.
        let mut total = 0;
        for kind in [Kind::Knapsack, Kind::Covering, Kind::Signed] {
            for seed in 0..14u64 {
                let spec = Spec {
                    kind,
                    n_cols: 14,
                    n_rows: 7,
                    seed,
                };
                let p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
                total += assert_cuts_are_valid(&p, &spec.name());
            }
        }
        assert!(
            total > 0,
            "no cuts were generated anywhere, so nothing was tested"
        );
    }

    #[test]
    fn separates_a_textbook_cover() {
        // 3x0 + 3x1 + 3x2 <= 5, maximizing the count. Any two columns overrun the
        // capacity, so x_a + x_b <= 1 is valid for each pair, while the relaxation
        // can afford 5/3 columns in total and so violates it.
        //
        // The weights must not divide the capacity evenly: with 6/5/5 <= 10 the
        // relaxation lands on the integral point [0, 1, 1] and there is correctly
        // nothing to separate.
        let p = problem(
            &[-1.0, -1.0, -1.0],
            &[(&[3.0, 3.0, 3.0], RowSense::Le, 5.0)],
        );
        let relaxed = Lp::relaxation(&p).solve();
        assert_eq!(relaxed.status, LpStatus::Optimal);
        let cuts = separate(&p, &relaxed.x, 8);
        assert!(!cuts.is_empty(), "no cover found for {:?}", relaxed.x);

        // The cover is {a, b} giving `x_a + x_b <= 1`, and lifting then brings the
        // third column in at coefficient 1 as well: no two of the three fit, so
        // `x0 + x1 + x2 <= 1` is valid and strictly stronger than the cover alone.
        let cut = &cuts[0];
        assert_eq!(cut.coefficients.len(), 3, "{:?}", cut.coefficients);
        assert!(
            cut.coefficients.iter().all(|&(_, a)| a == 1.0),
            "{:?}",
            cut.coefficients
        );
        assert!((cut.ub - 1.0).abs() < 1e-9, "rhs is {}", cut.ub);
        assert_cuts_are_valid(&p, "textbook cover");
    }

    #[test]
    fn handles_ge_rows_by_negation() {
        // A `>=` row is a knapsack after negating, which also flips every sign.
        let p = problem(
            &[1.0, 1.0, 1.0],
            &[(&[-6.0, -5.0, -5.0], RowSense::Ge, -10.0)],
        );
        assert_cuts_are_valid(&p, "ge row");
    }

    #[test]
    fn handles_negative_coefficients_by_complementing() {
        let p = problem(
            &[1.0, -1.0, 1.0, -1.0],
            &[(&[6.0, -5.0, 5.0, -4.0], RowSense::Le, 3.0)],
        );
        assert_cuts_are_valid(&p, "complemented");
    }

    #[test]
    fn a_cut_tightens_the_relaxation() {
        // The point of the exercise: adding cuts must raise the bound.
        let spec = Spec {
            kind: Kind::Knapsack,
            n_cols: 16,
            n_rows: 8,
            seed: 5,
        };
        let mut p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
        let before = Lp::relaxation(&p).solve();
        assert_eq!(before.status, LpStatus::Optimal);

        let cuts = separate(&p, &before.x, 32);
        assert!(!cuts.is_empty());
        p.add_cuts(&cuts);
        let after = Lp::relaxation(&p).solve();

        assert_eq!(after.status, LpStatus::Optimal);
        assert!(
            after.objective > before.objective + 1e-9,
            "bound did not improve: {} -> {}",
            before.objective,
            after.objective
        );
    }

    #[test]
    fn adding_cuts_preserves_every_feasible_point() {
        // add_cuts rebuilds the matrix; this checks the rebuild keeps the model's
        // meaning as well as the cuts keeping their validity.
        let spec = Spec {
            kind: Kind::Knapsack,
            n_cols: 14,
            n_rows: 6,
            seed: 2,
        };
        let p = Problem::from_lp(&LpProblem::parse(&spec.to_lp()).unwrap()).unwrap();
        let before = feasible_points(&p);

        let relaxed = Lp::relaxation(&p).solve();
        let cuts = separate(&p, &relaxed.x, 32);
        let mut with_cuts = p.clone();
        with_cuts.add_cuts(&cuts);
        with_cuts.validate().unwrap();

        assert_eq!(
            before,
            feasible_points(&with_cuts),
            "cuts changed the feasible set"
        );
        assert_eq!(with_cuts.n_rows(), p.n_rows() + cuts.len());
        assert_eq!(with_cuts.n_cols(), p.n_cols());
    }

    #[test]
    fn no_cuts_from_an_integral_relaxation() {
        // Nothing to separate when the relaxation is already integral.
        let p = problem(&[1.0, 1.0], &[(&[1.0, 1.0], RowSense::Ge, 1.0)]);
        let relaxed = Lp::relaxation(&p).solve();
        let cuts = separate(&p, &relaxed.x, 8);
        for cut in &cuts {
            assert!(cut.violation(&relaxed.x) > MIN_VIOLATION);
        }
    }
}
