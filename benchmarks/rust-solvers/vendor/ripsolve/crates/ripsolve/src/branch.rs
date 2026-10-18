//! Branching variable selection.
//!
//! Pseudocost scoring is the default and is a clear win: on v081c162n018 it takes
//! 3828 nodes where most-fractional branching took 14602.
//!
//! # Strong branching, and why it is off
//!
//! Strong branching probes a column by actually solving both child LPs. It is
//! implemented here and defaults to **off**, but the reason changed once the search
//! around it changed, and the history is worth keeping because the first
//! explanation was wrong.
//!
//! Under depth-first node selection it was catastrophic, v128c256n100 went from 10
//! nodes to 917, v081c162n018 from 3828 to 23686. The suspicion at the time was
//! that these instances are dual degenerate enough that most probes return a zero
//! degradation, leaving the score to break ties arbitrarily.
//!
//! That was measured and is false. Instrumenting the probe outcomes shows *every*
//! probe returning a finite, strictly positive degradation: zero of them come back
//! degenerate, truncated, or infeasible. The probes were always informative.
//!
//! The real cause was the interaction with depth-first selection. Under a plunge a
//! branching decision compounds: the search descends into whichever subtree the
//! rule chose and stays there, so any change to the rule swings the tree enormously
//! and a locally better decision can be globally much worse. Best-bound selection
//! is self-correcting, because the next node comes from the global pool regardless
//! of what the last decision was. Re-measured under best-bound, strong branching
//! reduces node counts on seven of eight instances, by 10% to 32%.
//!
//! It stays off because the node savings do not pay for the probes: two extra LPs
//! per candidate costs more wall-clock time than 10-30% fewer nodes saves, on five
//! of eight instances. Worth enabling on large models, v128c1000n100 goes from
//! 13.3s to 9.5s at a budget of 100, which is what the budget option is for.

/// Per-column history of how much branching on it actually cost./// Per-column history of how much branching on it actually cost.
///
/// A pseudocost is the objective degradation per unit of fractionality, averaged
/// over the times a column has been branched on. It is a far better predictor of a
/// column's usefulness than its fractional value: "closest to 0.5" is a guess about
/// the shape of the tree, whereas this is a measurement of it.
#[derive(Clone, Debug)]
pub struct Pseudocosts {
    down_sum: Vec<f64>,
    down_count: Vec<u32>,
    up_sum: Vec<f64>,
    up_count: Vec<u32>,
}

/// Observations of a direction needed before its pseudocost is trusted.
///
/// Below this the column is probed by strong branching instead, which is the
/// "reliability" in reliability branching. Zero would be pure pseudocost (cheap, poor early
/// decisions); a large value approaches full strong branching (excellent decisions,
/// far too expensive).
const RELIABILITY: u32 = 4;
/// Columns to probe at one node before falling back to pseudocost estimates.
const STRONG_CANDIDATES: usize = 8;
/// Simplex iterations allowed per strong-branching probe. Probes are advisory, so
/// a truncated one still yields a usable bound on the degradation.
const STRONG_ITERATIONS: usize = 100;

impl Pseudocosts {
    pub fn new(n: usize) -> Self {
        Self {
            down_sum: vec![0.0; n],
            down_count: vec![0; n],
            up_sum: vec![0.0; n],
            up_count: vec![0; n],
        }
    }

    /// Record that branching column `j` in one direction cost `degradation`, when
    /// the column had to move `step` to get there.
    ///
    /// `step` is clamped away from zero before dividing. A column sitting at 1e-6
    /// from integral is still a legal branching candidate, and dividing by that
    /// would book a degradation inflated a millionfold, which then poisons the
    /// global average every unobserved column is scored against. Measured, the
    /// unclamped version made strong branching a net loss on every instance tried.
    pub fn record(&mut self, j: usize, up: bool, degradation: f64, step: f64) {
        const MIN_STEP: f64 = 1e-2;
        if !degradation.is_finite() || degradation < 0.0 {
            return;
        }
        let unit_degradation = degradation / step.max(MIN_STEP);
        if up {
            self.up_sum[j] += unit_degradation;
            self.up_count[j] += 1;
        } else {
            self.down_sum[j] += unit_degradation;
            self.down_count[j] += 1;
        }
    }

    fn reliable(&self, j: usize) -> bool {
        self.down_count[j] >= RELIABILITY && self.up_count[j] >= RELIABILITY
    }

    /// Average unit degradation in one direction, or `None` if never observed.
    fn average(&self, j: usize, up: bool) -> Option<f64> {
        let (sum, count) = if up {
            (self.up_sum[j], self.up_count[j])
        } else {
            (self.down_sum[j], self.down_count[j])
        };
        (count > 0).then(|| sum / f64::from(count))
    }

    /// Mean unit degradation across every column that has any history.
    ///
    /// Used for columns with none of their own, so an unobserved column is scored
    /// as typical rather than as free.
    fn global_average(&self, up: bool) -> f64 {
        let (sums, counts) = if up {
            (&self.up_sum, &self.up_count)
        } else {
            (&self.down_sum, &self.down_count)
        };
        let total: f64 = sums.iter().sum();
        let n: u32 = counts.iter().sum();
        if n > 0 { total / f64::from(n) } else { 1.0 }
    }

    /// Predicted degradation on each side, for a column sitting at `fraction`.
    fn estimates(&self, j: usize, fraction: f64) -> (f64, f64) {
        let down = self
            .average(j, false)
            .unwrap_or_else(|| self.global_average(false))
            * fraction;
        let up = self
            .average(j, true)
            .unwrap_or_else(|| self.global_average(true))
            * (1.0 - fraction);
        (down, up)
    }
}

/// Combine the two sides into one score.
///
/// The product rule, which is standard: it rewards columns that are costly in
/// *both* directions, since those close the gap whichever way the search goes. A
/// sum would let one huge side mask a side that prunes nothing. The floor keeps a
/// zero on one side from collapsing the product to zero.
fn branch_score(down: f64, up: f64) -> f64 {
    const FLOOR: f64 = 1e-6;
    down.max(FLOOR) * up.max(FLOOR)
}

use crate::lp::{BasisState, Lp, LpStatus};
use crate::model::Problem;

/// The columns eligible for branching: integer columns sitting at a fractional
/// value. A continuous column is never a candidate, however fractional it is.
fn candidates(problem: &Problem, x: &[f64], tolerance: f64) -> Vec<(usize, f64)> {
    x.iter()
        .enumerate()
        .filter(|&(j, _)| problem.is_integer(j))
        .filter_map(|(j, &v)| {
            let fraction = v - v.floor();
            (fraction > tolerance && fraction < 1.0 - tolerance).then_some((j, fraction))
        })
        .collect()
}

/// How much a probe degraded the objective, or `None` if it told us nothing.
///
/// An infeasible probe is the most informative outcome there is: that branch is
/// dead, so it reports an infinite degradation rather than no information.
fn degradation(status: LpStatus, objective: f64, parent: f64) -> Option<f64> {
    match status {
        LpStatus::Optimal => Some((objective - parent).max(0.0)),
        LpStatus::Infeasible => Some(f64::INFINITY),
        _ => None,
    }
}

/// What branching decided.
pub struct Decision {
    pub column: usize,
    pub fraction: f64,
    /// Set when a probe proved one direction infeasible: that column can be fixed
    /// to the other value outright, with no branching at all.
    pub forced: Option<u8>,
    /// Set when a probe proved *both* directions infeasible, so the node itself has
    /// no feasible completion and can be discarded without branching.
    pub dead: bool,
}

/// Choose a column to branch on, by reliability branching.
///
/// The shortlist comes first, and that ordering is the whole trick. Candidates are
/// ranked by their pseudocost estimate, the best few are taken, and only those are
/// probed. Selection then happens *within* the shortlist, where every member is
/// either reliable or has just been measured.
///
/// Probing a prefix of the candidates and then scoring the rest from estimates does
/// not work, even though it looks equivalent: a probed column reports its true
/// degradation, which is usually modest, while an unprobed one reports the global
/// average, which is optimistic. The search then reliably picks a column it knows
/// nothing about over one it just measured, and the probes are worse than wasted,
/// measured at 10 nodes to 917 on v128c256n100.
#[allow(clippy::too_many_arguments)]
pub fn select(
    problem: &Problem,
    lp: &mut Lp,
    basis: &BasisState,
    x: &[f64],
    parent_objective: f64,
    pseudocosts: &mut Pseudocosts,
    tolerance: f64,
    strong_budget: &mut usize,
    iterations: &mut usize,
) -> Option<Decision> {
    let candidates = candidates(problem, x, tolerance);
    if candidates.is_empty() {
        return None;
    }

    // Rank by estimate, then keep only the head of the list.
    let mut ranked: Vec<(usize, f64, f64)> = candidates
        .iter()
        .map(|&(j, fraction)| {
            let (down, up) = pseudocosts.estimates(j, fraction);
            (j, fraction, branch_score(down, up))
        })
        .collect();
    ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(STRONG_CANDIDATES);

    let mut best: Option<Decision> = None;
    let mut best_score = f64::NEG_INFINITY;

    for &(j, fraction, estimated) in &ranked {
        let score = if pseudocosts.reliable(j) || *strong_budget == 0 {
            estimated
        } else {
            *strong_budget -= 1;

            // The same split the search itself would make, so what the probe
            // measures is what branching would actually cost.
            let saved = lp.column_bounds(j);
            let value = x[j];
            lp.set_column_bounds(j, saved.0, value.floor());
            let down_probe = lp.solve_warm(basis, None, STRONG_ITERATIONS);
            lp.set_column_bounds(j, value.ceil(), saved.1);
            let up_probe = lp.solve_warm(basis, None, STRONG_ITERATIONS);
            lp.set_column_bounds(j, saved.0, saved.1);
            *iterations += down_probe.iterations + up_probe.iterations;

            let down = degradation(down_probe.status, down_probe.objective, parent_objective);
            let up = degradation(up_probe.status, up_probe.objective, parent_objective);

            // A finite observation teaches the pseudocosts something about the column;
            // an infinite one is about this subtree, not the column in general.
            if let Some(d) = down.filter(|d| d.is_finite()) {
                pseudocosts.record(j, false, d, fraction);
            }
            if let Some(d) = up.filter(|d| d.is_finite()) {
                pseudocosts.record(j, true, d, 1.0 - fraction);
            }

            match (down, up) {
                // Both sides dead: so is this node, and the caller can stop here.
                (Some(d), Some(u)) if d.is_infinite() && u.is_infinite() => {
                    return Some(Decision {
                        column: j,
                        fraction,
                        forced: None,
                        dead: true,
                    });
                }
                (Some(d), _) if d.is_infinite() => {
                    return Some(Decision {
                        column: j,
                        fraction,
                        forced: Some(1),
                        dead: false,
                    });
                }
                (_, Some(u)) if u.is_infinite() => {
                    return Some(Decision {
                        column: j,
                        fraction,
                        forced: Some(0),
                        dead: false,
                    });
                }
                // A probe that ran out of iterations proves nothing, so fall back to
                // the estimate rather than scoring the column as costing nothing.
                _ => match (down, up) {
                    (Some(d), Some(u)) => branch_score(d, u),
                    _ => estimated,
                },
            }
        };

        if score > best_score {
            best_score = score;
            best = Some(Decision {
                column: j,
                fraction,
                forced: None,
                dead: false,
            });
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_only_the_direction_observed() {
        let mut p = Pseudocosts::new(3);
        p.record(0, true, 4.0, 0.5);
        p.record(0, true, 2.0, 0.5);
        // 4/0.5 = 8 and 2/0.5 = 4, averaging 6.
        assert_eq!(p.average(0, true), Some(6.0));
        assert_eq!(p.average(0, false), None);
    }

    #[test]
    fn the_step_divisor_is_clamped() {
        // Without the clamp a column a millionth away from integral would book a
        // degradation inflated a millionfold and swamp every other observation.
        let mut p = Pseudocosts::new(1);
        p.record(0, true, 1.0, 1e-6);
        assert_eq!(
            p.average(0, true),
            Some(100.0),
            "step was not clamped to 1e-2"
        );
    }

    #[test]
    fn nonsense_observations_are_ignored() {
        let mut p = Pseudocosts::new(1);
        p.record(0, true, f64::NAN, 0.5);
        p.record(0, true, f64::INFINITY, 0.5);
        p.record(0, true, -1.0, 0.5);
        assert_eq!(p.average(0, true), None);
    }

    #[test]
    fn an_unobserved_column_is_scored_as_typical() {
        // Not as free: an unobserved column must not outrank a measured one purely
        // for lack of evidence.
        let mut p = Pseudocosts::new(2);
        p.record(0, true, 5.0, 1.0);
        p.record(0, false, 5.0, 1.0);
        let (down, up) = p.estimates(1, 0.5);
        assert_eq!(
            (down, up),
            (2.5, 2.5),
            "unobserved column did not use the global average"
        );
    }

    #[test]
    fn reliability_needs_both_directions() {
        let mut p = Pseudocosts::new(1);
        for _ in 0..RELIABILITY {
            p.record(0, true, 1.0, 0.5);
        }
        assert!(
            !p.reliable(0),
            "one direction should not make a column reliable"
        );
        for _ in 0..RELIABILITY {
            p.record(0, false, 1.0, 0.5);
        }
        assert!(p.reliable(0));
    }

    #[test]
    fn the_score_rewards_columns_costly_in_both_directions() {
        // The product rule's point: a column that prunes only one side is worth less
        // than one that prunes both, even at equal total degradation.
        assert!(branch_score(5.0, 5.0) > branch_score(9.0, 1.0));
        // And a zero on one side must not collapse the score to exactly zero.
        assert!(branch_score(10.0, 0.0) > 0.0);
    }

    #[test]
    fn candidates_are_the_fractional_integer_columns_only() {
        use crate::model::{Sense, VarType};
        use crate::sparse::SparseMatrix;
        let n = 5;
        let mut problem = Problem {
            name: "t".into(),
            sense: Sense::Minimize,
            obj: vec![0.0; n],
            obj_offset: 0.0,
            matrix: SparseMatrix::from_triplets(0, n, []),
            row_lb: Vec::new(),
            row_ub: Vec::new(),
            col_lb: vec![0.0; n],
            col_ub: vec![1.0; n],
            col_type: vec![VarType::Integer; n],
            col_names: (0..n).map(|j| format!("x{j}")).collect(),
            row_names: Vec::new(),
        };
        let x = [0.0, 0.5, 1.0, 0.25, 1e-9];
        assert_eq!(candidates(&problem, &x, 1e-6), vec![(1, 0.5), (3, 0.25)]);

        // A continuous column is never a branching candidate, however fractional.
        problem.col_type[1] = VarType::Continuous;
        assert_eq!(candidates(&problem, &x, 1e-6), vec![(3, 0.25)]);
    }

    #[test]
    fn an_infeasible_probe_reports_an_infinite_degradation() {
        // The most informative outcome a probe can have: that branch is dead.
        assert_eq!(
            degradation(LpStatus::Infeasible, f64::NAN, 1.0),
            Some(f64::INFINITY)
        );
        assert_eq!(degradation(LpStatus::Optimal, 5.0, 3.0), Some(2.0));
        // A truncated probe proves nothing.
        assert_eq!(degradation(LpStatus::IterationLimit, 5.0, 3.0), None);
        // Numerical noise must not become a negative degradation.
        assert_eq!(degradation(LpStatus::Optimal, 2.9999, 3.0), Some(0.0));
    }
}
