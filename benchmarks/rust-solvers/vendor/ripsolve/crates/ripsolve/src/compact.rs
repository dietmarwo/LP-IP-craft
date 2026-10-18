//! Physically remove what a model no longer varies over.
//!
//! Every reduction elsewhere in this solver works **in place**: a fixed column becomes
//! `lb == ub` and a redundant row has its bounds freed, and nothing is renumbered, so a
//! solution vector maps one-to-one onto the original columns and there is no postsolve
//! step to get wrong. That trade is explained under "Presolve" and is the right one for
//! presolve, which removes a few per cent of a typical model.
//!
//! It stops being the right one when most of the model has gone. After the root's reduced
//! cost fixing, `neos-820879` has 6871 of its 9522 columns pinned and carries every one of
//! them through every LP of the search. This is the other half of that trade: the columns
//! and rows are actually removed, an index map comes back with the smaller model, and a
//! solution of it is read back through the map.
//!
//! The map is the whole risk. A postsolve that renumbers wrongly returns a plausible
//! vector for the wrong columns, which scores as a valid objective and is not one, so
//! everything here is checked by solving both models and comparing, rather than by
//! reading the code and believing it.

use crate::model::Problem;
use crate::sparse::SparseMatrix;

/// How to read a solution of a compacted model as a solution of the model it came from.
#[derive(Clone, Debug)]
pub struct Compaction {
    /// Original column index of each column of the compacted model.
    columns: Vec<usize>,
    /// The value every column of the original takes: for a kept column this is
    /// overwritten by [`Compaction::expand`], for a removed one it is the value it was
    /// fixed at.
    values: Vec<f64>,
}

impl Compaction {
    /// A solution of the compacted model, read as a solution of the original.
    pub fn expand(&self, x: &[f64]) -> Vec<f64> {
        debug_assert_eq!(x.len(), self.columns.len());
        let mut full = self.values.clone();
        for (&j, &value) in self.columns.iter().zip(x) {
            full[j] = value;
        }
        full
    }

    /// Columns of the original that survived, in the compacted model's order.
    pub fn columns(&self) -> &[usize] {
        &self.columns
    }
}

/// A model whose bounds prove it has no feasible point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Infeasible;

/// Remove every fixed column and every row that no longer constrains anything.
///
/// A column pinned to `v` contributes `a_ij v` to row `i` and `c_j v` to the objective
/// whatever else happens, so it is removed by moving those constants into the row bounds
/// and the objective offset. A row left with no columns is then either satisfied by that
/// constant, and carries no information, or contradicted by it, and the model has no
/// feasible point.
///
/// Returns `None` when nothing would be removed, so the caller keeps the model it has
/// rather than paying for a copy of it.
pub fn compact(
    problem: &Problem,
    tolerance: f64,
) -> Result<Option<(Problem, Compaction)>, Infeasible> {
    let (n, m) = (problem.n_cols(), problem.n_rows());
    let fixed: Vec<bool> = (0..n)
        .map(|j| problem.col_ub[j] - problem.col_lb[j] <= tolerance)
        .collect();
    let free_row = |i: usize| !problem.row_lb[i].is_finite() && !problem.row_ub[i].is_finite();
    let removable_rows = (0..m).filter(|&i| free_row(i)).count();
    let removable_columns = fixed.iter().filter(|&&f| f).count();
    if removable_columns == 0 && removable_rows == 0 {
        return Ok(None);
    }

    // What the fixed columns contribute, once and for all.
    let mut row_lb = problem.row_lb.clone();
    let mut row_ub = problem.row_ub.clone();
    let mut offset = problem.obj_offset;
    let csr = problem.matrix.to_csr();
    for (j, &is_fixed) in fixed.iter().enumerate() {
        if !is_fixed {
            continue;
        }
        // The value it is pinned at, taken from the lower bound so that a range narrower
        // than the tolerance still yields a point inside it.
        let value = problem.col_lb[j];
        offset += problem.obj[j] * value;
        if value == 0.0 {
            continue;
        }
        let (rows, coefficients) = problem.matrix.column(j);
        for (&i, &a) in rows.iter().zip(coefficients) {
            if row_lb[i].is_finite() {
                row_lb[i] -= a * value;
            }
            if row_ub[i].is_finite() {
                row_ub[i] -= a * value;
            }
        }
    }

    // Which rows survive: a row still holding a free column, and one that does not but
    // whose remaining bounds still say something, are both kept.
    let mut kept_rows: Vec<usize> = Vec::with_capacity(m);
    for i in 0..m {
        if free_row(i) {
            continue;
        }
        let (cols, _) = csr.column(i);
        if cols.iter().any(|&j| !fixed[j]) {
            kept_rows.push(i);
            continue;
        }
        // Empty now. Its constant is already in the bounds, so zero has to lie inside
        // them or nothing does.
        if row_lb[i] > tolerance || row_ub[i] < -tolerance {
            return Err(Infeasible);
        }
    }

    let kept_columns: Vec<usize> = (0..n).filter(|&j| !fixed[j]).collect();
    let mut row_of = vec![usize::MAX; m];
    for (new, &old) in kept_rows.iter().enumerate() {
        row_of[old] = new;
    }

    let row_of = &row_of;
    let triplets = kept_columns
        .iter()
        .enumerate()
        .flat_map(move |(new_j, &j)| {
            let (rows, coefficients) = problem.matrix.column(j);
            rows.iter()
                .zip(coefficients)
                .filter(move |&(&i, _)| row_of[i] != usize::MAX)
                .map(move |(&i, &a)| (row_of[i], new_j, a))
        });

    let smaller = Problem {
        name: problem.name.clone(),
        sense: problem.sense,
        obj: kept_columns.iter().map(|&j| problem.obj[j]).collect(),
        obj_offset: offset,
        matrix: SparseMatrix::from_triplets(kept_rows.len(), kept_columns.len(), triplets),
        row_lb: kept_rows.iter().map(|&i| row_lb[i]).collect(),
        row_ub: kept_rows.iter().map(|&i| row_ub[i]).collect(),
        col_lb: kept_columns.iter().map(|&j| problem.col_lb[j]).collect(),
        col_ub: kept_columns.iter().map(|&j| problem.col_ub[j]).collect(),
        col_type: kept_columns.iter().map(|&j| problem.col_type[j]).collect(),
        col_names: kept_columns
            .iter()
            .map(|&j| problem.col_names[j].clone())
            .collect(),
        row_names: kept_rows
            .iter()
            .map(|&i| problem.row_names[i].clone())
            .collect(),
    };
    let compaction = Compaction {
        columns: kept_columns,
        values: problem.col_lb.clone(),
    };
    Ok(Some((smaller, compaction)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Builder, RowSense, Sense};
    use crate::search::{self, Options, Status};

    /// A random binary model, some of whose columns are pinned.
    ///
    /// Pinned rather than merely bounded, because that is what compaction removes, and
    /// pinned at both ends and at both values, because a column fixed at zero drops out
    /// of every row without changing a bound and a column fixed at one does not.
    fn model(seed: u64, pin: bool) -> Problem {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let n = 6 + (next() % 5) as usize;
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
        let mut problem = b.build();
        if pin {
            for j in 0..problem.n_cols() {
                if next() % 3 == 0 {
                    let value = f64::from((next() % 2) as u32);
                    problem.col_lb[j] = value;
                    problem.col_ub[j] = value;
                }
            }
        }
        problem
    }

    #[test]
    fn a_compacted_model_has_the_same_optimum_and_the_map_recovers_it() {
        // The map is what makes this dangerous: renumbering wrongly returns a plausible
        // vector for the wrong columns, which scores as a valid objective and is not
        // one. So the answer is checked twice over -- the compacted model's optimum
        // against the original's, and the *expanded point* scored on the original model
        // against what the compacted search claimed for it. The second is the one that
        // catches a permutation, because a permuted vector still scores something.
        let mut compacted_any = 0;
        for seed in 0..300u64 {
            let problem = model(seed, true);
            let direct = search::solve(&problem, Options::default());
            let Ok(Some((smaller, map))) = compact(&problem, 1e-9) else {
                continue;
            };
            compacted_any += 1;
            let small = search::solve(&smaller, Options::default());
            assert_eq!(small.status, direct.status, "seed {seed}: status");
            match (small.objective, direct.objective) {
                (Some(a), Some(b)) => {
                    assert!(
                        (a - b).abs() <= 1e-6 * b.abs().max(1.0),
                        "seed {seed}: compacted {a}, original {b}"
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
                         compacted search claimed {a}"
                    );
                    assert!(
                        crate::heuristic::is_feasible(&problem, &expanded, 1e-6),
                        "seed {seed}: the expanded point is not feasible for the original"
                    );
                }
                (None, None) => {}
                (a, b) => panic!("seed {seed}: compacted {a:?}, original {b:?}"),
            }
        }
        assert!(
            compacted_any > 100,
            "only {compacted_any} models compacted, so little was checked"
        );
    }

    #[test]
    fn a_model_with_nothing_pinned_is_left_alone() {
        // Returning a copy of the model would be correct and pointless, and the caller
        // is entitled to know that keeping what it has costs nothing.
        for seed in 0..50u64 {
            let problem = model(seed, false);
            if (0..problem.n_rows())
                .any(|i| !problem.row_lb[i].is_finite() && !problem.row_ub[i].is_finite())
            {
                continue;
            }
            assert!(matches!(compact(&problem, 1e-9), Ok(None)), "seed {seed}");
        }
    }

    #[test]
    fn a_row_its_fixed_columns_contradict_is_reported_infeasible() {
        // `x + y >= 2` with both pinned to zero leaves an empty row demanding 2, which
        // is the one case where removing columns proves something rather than losing it.
        let mut b = Builder::new(Sense::Minimize);
        let x = b.binary("x");
        let y = b.binary("y");
        b.objective(&[(x, 1.0), (y, 1.0)]);
        b.row(&[(x, 1.0), (y, 1.0)], RowSense::Ge, 2.0);
        let mut problem = b.build();
        for j in [x, y] {
            problem.col_lb[j] = 0.0;
            problem.col_ub[j] = 0.0;
        }
        assert!(matches!(compact(&problem, 1e-9), Err(Infeasible)));
        // And the search agrees, which is what makes the claim more than an assertion.
        assert_eq!(
            search::solve(&problem, Options::default()).status,
            Status::Infeasible
        );
    }
}
