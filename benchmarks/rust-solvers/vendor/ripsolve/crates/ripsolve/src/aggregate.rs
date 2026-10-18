//! Doubleton equality aggregation, and the postsolve it needs.
//!
//! An equality row on exactly two columns, `a x_p + b x_q = r`, says one of them is an
//! affine function of the other. Substituting it out removes a column *and* the row that
//! defined it, which is the one reduction in this solver that cannot be done in place:
//! `presolve` fixes a column by setting `lb == ub` and frees a row by widening its
//! bounds, so nothing is renumbered and no postsolve is needed, but an eliminated column
//! has no value of its own to be fixed at. Its value is whatever the surviving column
//! turns out to take, which is not known until the search has finished.
//!
//! So this is the first reduction here that renumbers, and [`Aggregation`] is the
//! postsolve. The map is the whole risk -- a postsolve that renumbers wrongly returns a
//! plausible vector for the wrong columns, which scores as a valid objective and is not
//! one -- so, as in `compact`, the tests solve both models and compare rather than
//! reading the code and believing it.
//!
//! **What it is for.** MIPLIB's `neos-633273` is 21781 rows by 11154 columns, and
//! **10648 of those rows are the identical doubleton `x_p - x_q = 0`**: half the model
//! says two binaries are equal. Our presolve reports zero reductions on it and a
//! reference solver closes it in half a second from a model less than a third the size.
//!
//! **What it will not do.** Eliminating `x_p = alpha + beta x_q` has to keep `x_p`
//! integral whenever `x_q` is, and for a general `beta` it does not: `x_p = x_q / 2`
//! turns an integer column into one that is only integral for even `x_q`, which the
//! search would never enforce. Only `beta = +-1` with an integral `alpha` is taken, which
//! is exactly "these two are equal" and "these two are opposite". That covers the
//! structure above and is sound without a case analysis.

use crate::compact::Infeasible;
use crate::model::{Problem, VarType};
use crate::sparse::SparseMatrix;

/// How to read a solution of an aggregated model as a solution of the model it came from.
#[derive(Clone, Debug)]
pub struct Aggregation {
    /// Original column index of each column of the aggregated model.
    columns: Vec<usize>,
    /// `x[column] = alpha + beta * x[source]`, in the order the substitutions were made.
    ///
    /// A substitution's source is a live column at the moment it is recorded, so it is
    /// either a column that survives or one eliminated by a *later* entry. Applying the
    /// list in reverse therefore always finds the source already known, and no
    /// topological sort is needed; see [`Aggregation::expand`].
    substitutions: Vec<(usize, usize, f64, f64)>,
    n_original: usize,
}

impl Aggregation {
    /// A solution of the aggregated model, read as a solution of the original.
    pub fn expand(&self, x: &[f64]) -> Vec<f64> {
        debug_assert_eq!(x.len(), self.columns.len());
        let mut full = vec![0.0; self.n_original];
        for (&j, &value) in self.columns.iter().zip(x) {
            full[j] = value;
        }
        // Backwards: see the field's own note on why this order is the right one.
        for &(column, source, alpha, beta) in self.substitutions.iter().rev() {
            full[column] = alpha + beta * full[source];
        }
        full
    }

    /// A solution of the original, read as one of the aggregated model.
    ///
    /// Used for a caller-supplied starting point, which is given in the original's
    /// columns. A point that satisfies the original satisfies the aggregation, since
    /// every eliminated column's defining row is one of the model's own.
    pub fn restrict(&self, x: &[f64]) -> Vec<f64> {
        self.columns.iter().map(|&j| x[j]).collect()
    }

    /// Columns of the original that survived, in the aggregated model's order.
    pub fn columns(&self) -> &[usize] {
        &self.columns
    }

    /// How many columns were substituted away.
    pub fn eliminated(&self) -> usize {
        self.substitutions.len()
    }
}

/// `x_j = alpha[j] + beta[j] * x_parent[j]`, with a root standing for itself.
///
/// A weighted union-find. Eliminating `p` into `q` when `q` has itself already been
/// eliminated has to compose the two affine maps rather than point at a column that is
/// no longer there, which is what the weights are for.
struct Links {
    parent: Vec<usize>,
    alpha: Vec<f64>,
    beta: Vec<f64>,
}

impl Links {
    fn new(n: usize) -> Self {
        Links {
            parent: (0..n).collect(),
            alpha: vec![0.0; n],
            beta: vec![1.0; n],
        }
    }

    /// The live column `j` reduces to, and the map onto it. Compresses the path.
    fn find(&mut self, j: usize) -> (usize, f64, f64) {
        if self.parent[j] == j {
            return (j, 0.0, 1.0);
        }
        let mut path = Vec::new();
        let mut cursor = j;
        while self.parent[cursor] != cursor {
            path.push(cursor);
            cursor = self.parent[cursor];
        }
        let root = cursor;
        // Rewrite each link on the path to point straight at the root, composing as it
        // goes. Walking from the far end means the parent is already compressed.
        for &p in path.iter().rev() {
            let parent = self.parent[p];
            if parent != root {
                let (outer_alpha, outer_beta) = (self.alpha[p], self.beta[p]);
                let (inner_alpha, inner_beta) = (self.alpha[parent], self.beta[parent]);
                self.alpha[p] = outer_alpha + outer_beta * inner_alpha;
                self.beta[p] = outer_beta * inner_beta;
                self.parent[p] = root;
            }
        }
        (root, self.alpha[j], self.beta[j])
    }

    fn is_root(&self, j: usize) -> bool {
        self.parent[j] == j
    }
}

/// Whether `value` is a whole number, to within `tolerance`.
fn is_integral(value: f64, tolerance: f64) -> bool {
    (value - value.round()).abs() <= tolerance
}

/// Substitute out every doubleton equality this can do soundly.
///
/// Returns `None` when there is nothing to aggregate, so the caller keeps the model it
/// has rather than paying for a copy of it.
pub fn aggregate(
    problem: &Problem,
    tolerance: f64,
) -> Result<Option<(Problem, Aggregation)>, Infeasible> {
    let (n, m) = (problem.n_cols(), problem.n_rows());
    let csr = problem.matrix.to_csr();
    let mut links = Links::new(n);
    let mut col_lb = problem.col_lb.clone();
    let mut col_ub = problem.col_ub.clone();
    let mut substitutions: Vec<(usize, usize, f64, f64)> = Vec::new();
    // Rows whose whole content is a substitution already applied. They are dropped; the
    // information in them is carried by the map.
    let mut consumed = vec![false; m];
    let length: Vec<usize> = (0..n).map(|j| problem.matrix.column(j).0.len()).collect();

    for i in 0..m {
        let (lo, hi) = (problem.row_lb[i], problem.row_ub[i]);
        if !lo.is_finite() || !hi.is_finite() || (hi - lo).abs() > tolerance {
            continue;
        }
        let (columns, values) = csr.column(i);
        if columns.len() != 2 {
            continue;
        }
        let rhs = 0.5 * (lo + hi);
        let (p, a) = (columns[0], values[0]);
        let (q, b) = (columns[1], values[1]);

        // Both endpoints may already stand for something else.
        let (root_p, alpha_p, beta_p) = links.find(p);
        let (root_q, alpha_q, beta_q) = links.find(q);
        // a(alpha_p + beta_p X) + b(alpha_q + beta_q Y) = rhs
        let residual = rhs - a * alpha_p - b * alpha_q;
        let (coefficient_x, coefficient_y) = (a * beta_p, b * beta_q);

        if root_p == root_q {
            // One unknown. Either it is pinned, or the row says nothing, or nothing
            // satisfies it.
            let coefficient = coefficient_x + coefficient_y;
            if coefficient.abs() > tolerance {
                let mut value = residual / coefficient;
                // An integer column pinned at a fraction has no feasible point, and
                // saying so is the whole content of the row. Writing the fraction into
                // the bounds instead hands the search a column it cannot satisfy and
                // cannot branch on, and it answers `Optimal` on a model that has no
                // solution -- which is what the differential test caught at seed 55.
                if problem.col_type[root_p] == VarType::Integer {
                    if !is_integral(value, tolerance) {
                        return Err(Infeasible);
                    }
                    value = value.round();
                }
                if value < col_lb[root_p] - tolerance || value > col_ub[root_p] + tolerance {
                    return Err(Infeasible);
                }
                col_lb[root_p] = value;
                col_ub[root_p] = value;
                consumed[i] = true;
            } else if residual.abs() > tolerance {
                return Err(Infeasible);
            } else {
                consumed[i] = true;
            }
            continue;
        }
        if coefficient_x.abs() <= tolerance || coefficient_y.abs() <= tolerance {
            continue;
        }

        // Eliminate the cheaper of the two to move, deterministically.
        let (keep, drop, coefficient_drop, coefficient_keep) =
            if (length[root_p], root_p) <= (length[root_q], root_q) {
                (root_q, root_p, coefficient_x, coefficient_y)
            } else {
                (root_p, root_q, coefficient_y, coefficient_x)
            };
        // x_drop = alpha + beta * x_keep
        let alpha = residual / coefficient_drop;
        let beta = -coefficient_keep / coefficient_drop;

        // Integrality is the whole reason this is restricted; see the module note.
        let integral = |j: usize| problem.col_type[j] == VarType::Integer;
        if integral(drop) != integral(keep)
            || (integral(drop)
                && (!is_integral(alpha, tolerance) || (beta.abs() - 1.0).abs() > tolerance))
        {
            continue;
        }

        // The eliminated column's bounds become the survivor's, through the map.
        let (from, to) = ((col_lb[drop] - alpha) / beta, (col_ub[drop] - alpha) / beta);
        let (mut implied_lb, mut implied_ub) = if beta > 0.0 { (from, to) } else { (to, from) };
        if integral(keep) {
            implied_lb = (implied_lb - tolerance).ceil();
            implied_ub = (implied_ub + tolerance).floor();
        }
        let new_lb = col_lb[keep].max(implied_lb);
        let new_ub = col_ub[keep].min(implied_ub);
        if new_lb > new_ub + tolerance {
            return Err(Infeasible);
        }
        col_lb[keep] = new_lb;
        col_ub[keep] = new_ub;

        links.parent[drop] = keep;
        links.alpha[drop] = alpha;
        links.beta[drop] = beta;
        substitutions.push((drop, keep, alpha, beta));
        consumed[i] = true;
    }

    if substitutions.is_empty() {
        return Ok(None);
    }

    let kept_columns: Vec<usize> = (0..n).filter(|&j| links.is_root(j)).collect();
    let mut column_of = vec![usize::MAX; n];
    for (new, &old) in kept_columns.iter().enumerate() {
        column_of[old] = new;
    }

    // Rewrite the rows. Two columns of the original can now be the same column, so the
    // coefficients are accumulated before the row is judged: a row whose terms cancel is
    // either vacuous or contradictory, and telling those apart needs the exact sum.
    let mut row_lb = Vec::with_capacity(m);
    let mut row_ub = Vec::with_capacity(m);
    let mut row_names = Vec::with_capacity(m);
    let mut triplets: Vec<(usize, usize, f64)> = Vec::with_capacity(problem.matrix.nnz());
    let mut accumulator = vec![0.0f64; kept_columns.len()];
    let mut touched: Vec<usize> = Vec::new();

    for i in 0..m {
        if consumed[i] {
            continue;
        }
        let (columns, values) = csr.column(i);
        let mut constant = 0.0;
        for (&j, &a) in columns.iter().zip(values) {
            let (root, alpha, beta) = links.find(j);
            constant += a * alpha;
            let slot = column_of[root];
            if accumulator[slot] == 0.0 {
                touched.push(slot);
            }
            accumulator[slot] += a * beta;
        }
        let (mut lo, mut hi) = (problem.row_lb[i], problem.row_ub[i]);
        if lo.is_finite() {
            lo -= constant;
        }
        if hi.is_finite() {
            hi -= constant;
        }
        let surviving = touched.iter().filter(|&&s| accumulator[s] != 0.0).count();
        if surviving == 0 {
            // Nothing left to vary: the constant alone has to lie inside the bounds.
            if lo > tolerance || hi < -tolerance {
                return Err(Infeasible);
            }
        } else {
            let new_row = row_lb.len();
            for &slot in &touched {
                if accumulator[slot] != 0.0 {
                    triplets.push((new_row, slot, accumulator[slot]));
                }
            }
            row_lb.push(lo);
            row_ub.push(hi);
            row_names.push(problem.row_names[i].clone());
        }
        for &slot in &touched {
            accumulator[slot] = 0.0;
        }
        touched.clear();
    }

    // The objective moves the same way.
    let mut obj = vec![0.0f64; kept_columns.len()];
    let mut obj_offset = problem.obj_offset;
    for j in 0..n {
        let c = problem.obj[j];
        if c == 0.0 {
            continue;
        }
        let (root, alpha, beta) = links.find(j);
        obj_offset += c * alpha;
        obj[column_of[root]] += c * beta;
    }

    let smaller = Problem {
        name: problem.name.clone(),
        sense: problem.sense,
        obj,
        obj_offset,
        matrix: SparseMatrix::from_triplets(row_lb.len(), kept_columns.len(), triplets),
        row_lb,
        row_ub,
        col_lb: kept_columns.iter().map(|&j| col_lb[j]).collect(),
        col_ub: kept_columns.iter().map(|&j| col_ub[j]).collect(),
        col_type: kept_columns.iter().map(|&j| problem.col_type[j]).collect(),
        col_names: kept_columns
            .iter()
            .map(|&j| problem.col_names[j].clone())
            .collect(),
        row_names,
    };
    let aggregation = Aggregation {
        columns: kept_columns,
        substitutions,
        n_original: n,
    };
    Ok(Some((smaller, aggregation)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Builder, RowSense, Sense};
    use crate::search::{self, Options, Status};

    /// A random binary model, some of whose columns are tied together by doubletons.
    ///
    /// The ties are the point, so they are generated as the two forms this accepts --
    /// `x = y` and `x = 1 - y` -- and sometimes chained, because a chain is what makes
    /// the postsolve's ordering matter: eliminating `p` into `q` and later `q` into `s`
    /// leaves `p` pointing at a column that is itself gone.
    fn model(seed: u64, tie: bool) -> Problem {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let n = 6 + (next() % 6) as usize;
        let mut b = Builder::new(if next() % 2 == 0 {
            Sense::Minimize
        } else {
            Sense::Maximize
        });
        let columns: Vec<usize> = (0..n).map(|j| b.binary(&format!("x{j}"))).collect();
        let objective: Vec<(usize, f64)> = columns
            .iter()
            .map(|&j| (j, (next() % 11) as f64 - 5.0))
            .collect();
        b.objective(&objective);
        for _ in 0..3 + (next() % 4) as usize {
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
            let rhs = (next() % (2 * span as u64 + 1)) as f64 - span;
            let sense = match next() % 3 {
                0 => RowSense::Le,
                1 => RowSense::Ge,
                _ => RowSense::Eq,
            };
            b.row(&terms, sense, rhs);
        }
        if tie {
            for _ in 0..1 + (next() % 4) {
                let p = (next() % n as u64) as usize;
                let q = (next() % n as u64) as usize;
                if p == q {
                    continue;
                }
                if next() % 2 == 0 {
                    // x_p = x_q
                    b.row(&[(columns[p], 1.0), (columns[q], -1.0)], RowSense::Eq, 0.0);
                } else {
                    // x_p = 1 - x_q
                    b.row(&[(columns[p], 1.0), (columns[q], 1.0)], RowSense::Eq, 1.0);
                }
            }
        }
        b.build()
    }

    #[test]
    fn an_aggregated_model_has_the_same_optimum_and_the_map_recovers_it() {
        // The map is the risk, exactly as in `compact`: renumbering wrongly returns a
        // plausible vector for the wrong columns, which still scores an objective. So
        // the expanded point is scored on the *original* model and checked feasible
        // there, which is what catches a permutation; matching optima alone would not.
        let mut aggregated_any = 0;
        for seed in 0..400u64 {
            let problem = model(seed, true);
            let direct = search::solve(&problem, Options::default());
            let Ok(Some((smaller, map))) = aggregate(&problem, 1e-9) else {
                continue;
            };
            aggregated_any += 1;
            assert!(
                smaller.n_cols() < problem.n_cols(),
                "seed {seed}: aggregation kept every column"
            );
            let small = search::solve(&smaller, Options::default());
            assert_eq!(small.status, direct.status, "seed {seed}: status");
            match (small.objective, direct.objective) {
                (Some(a), Some(b)) => {
                    assert!(
                        (a - b).abs() <= 1e-6 * b.abs().max(1.0),
                        "seed {seed}: aggregated {a}, original {b}"
                    );
                    let expanded = map.expand(&small.x);
                    assert_eq!(expanded.len(), problem.n_cols(), "seed {seed}: width");
                    let scored: f64 = problem
                        .obj
                        .iter()
                        .zip(&expanded)
                        .map(|(c, v)| c * v)
                        .sum::<f64>();
                    let scored = problem.objective_value(scored);
                    assert!(
                        (scored - a).abs() <= 1e-6 * a.abs().max(1.0),
                        "seed {seed}: the expanded point scores {scored} where the \
                         aggregated search claimed {a}"
                    );
                    assert!(
                        crate::heuristic::is_feasible(&problem, &expanded, 1e-6),
                        "seed {seed}: the expanded point is not feasible for the original"
                    );
                    for j in problem.integer_columns() {
                        assert!(
                            (expanded[j] - expanded[j].round()).abs() <= 1e-6,
                            "seed {seed}: column {j} came back fractional at {}",
                            expanded[j]
                        );
                    }
                }
                (None, None) => {}
                (a, b) => panic!("seed {seed}: aggregated {a:?}, original {b:?}"),
            }
        }
        assert!(
            aggregated_any > 150,
            "only {aggregated_any} models aggregated, so little was checked"
        );
    }

    #[test]
    fn a_model_with_no_doubleton_equality_is_left_alone() {
        // The caller is entitled to know that keeping the model it has costs nothing.
        for seed in 0..80u64 {
            let problem = model(seed, false);
            let doubleton = (0..problem.n_rows()).any(|i| {
                problem.row_lb[i] == problem.row_ub[i]
                    && problem.matrix.to_csr().column(i).0.len() == 2
            });
            if doubleton {
                continue;
            }
            assert!(matches!(aggregate(&problem, 1e-9), Ok(None)), "seed {seed}");
        }
    }

    #[test]
    fn a_chain_of_ties_collapses_to_one_column() {
        // p = q, q = s, s = t: four columns, one survivor, and every eliminated column
        // has to come back through a source that was itself eliminated later.
        let mut b = Builder::new(Sense::Maximize);
        let x: Vec<usize> = (0..4).map(|j| b.binary(&format!("x{j}"))).collect();
        b.objective(&[(x[0], 1.0), (x[1], 1.0), (x[2], 1.0), (x[3], 1.0)]);
        b.row(&[(x[0], 1.0), (x[1], -1.0)], RowSense::Eq, 0.0);
        b.row(&[(x[1], 1.0), (x[2], -1.0)], RowSense::Eq, 0.0);
        b.row(&[(x[2], 1.0), (x[3], -1.0)], RowSense::Eq, 0.0);
        let problem = b.build();
        let (smaller, map) = aggregate(&problem, 1e-9).unwrap().unwrap();
        assert_eq!(smaller.n_cols(), 1);
        assert_eq!(smaller.n_rows(), 0);
        assert_eq!(map.eliminated(), 3);
        let solved = search::solve(&smaller, Options::default());
        let expanded = map.expand(&solved.x);
        assert_eq!(expanded, vec![1.0; 4], "all four are tied and all should be one");
        assert_eq!(solved.objective, Some(4.0));
    }

    #[test]
    fn an_opposing_tie_is_carried_through_the_map() {
        // x = 1 - y, so the survivor's value has to be negated and shifted on the way
        // back. A map that only ever copies would pass the equality test above.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.binary("x");
        let y = b.binary("y");
        b.objective(&[(x, 3.0), (y, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Eq, 1.0);
        let problem = b.build();
        let (smaller, map) = aggregate(&problem, 1e-9).unwrap().unwrap();
        assert_eq!(smaller.n_cols(), 1);
        let solved = search::solve(&smaller, Options::default());
        let expanded = map.expand(&solved.x);
        assert_eq!(expanded.len(), 2);
        assert!((expanded[x] + expanded[y] - 1.0).abs() <= 1e-9, "the tie must hold");
        assert_eq!(solved.objective, Some(3.0), "taking x is worth more than y");
        assert_eq!(expanded[x], 1.0);
        assert_eq!(expanded[y], 0.0);
    }

    #[test]
    fn contradictory_ties_are_reported_infeasible() {
        // x = y and x = 1 - y cannot both hold. The second reduces to a single unknown
        // and pins it; the two together leave 2x = 1 on a binary, which has no point.
        let mut b = Builder::new(Sense::Minimize);
        let x = b.binary("x");
        let y = b.binary("y");
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 1.0), (y, -1.0)], RowSense::Eq, 0.0);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Eq, 1.0);
        let problem = b.build();
        assert!(matches!(aggregate(&problem, 1e-9), Err(Infeasible)));
        // And the search agrees, which is what makes the claim more than an assertion.
        assert_eq!(
            search::solve(&problem, Options::default()).status,
            Status::Infeasible
        );
    }

    #[test]
    fn a_fractional_tie_is_declined_rather_than_broken() {
        // 2x - y = 0 makes x = y/2, which is integral only for even y. Eliminating it
        // would hand the search a column it cannot keep integral, so it is refused.
        let mut b = Builder::new(Sense::Maximize);
        let x = b.column("x", 0.0, 10.0, VarType::Integer);
        let y = b.column("y", 0.0, 10.0, VarType::Integer);
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 2.0), (y, -1.0)], RowSense::Eq, 0.0);
        let problem = b.build();
        assert!(matches!(aggregate(&problem, 1e-9), Ok(None)));
    }
}
