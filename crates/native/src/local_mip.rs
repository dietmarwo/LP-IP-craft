//! Single-column tight moves on sampled violated rows, after Lin, Zou and Cai
//! (CP 2024, doi:10.4230/LIPIcs.CP.2024.19). Search scores have no proof role.
use crate::heuristic_budget::{Meter, Stop, Work};
use crate::oriented;
use lp_ip_craft::{Bounds, Model, Tolerances};

const SAMPLE_ROWS: usize = 8;
const SAMPLE_TERMS: usize = 64;
const TABU_MOVES: u64 = 3;
const WEIGHT_CAP: i32 = 1024;
const REFRESH_EVERY: u64 = 32;

#[derive(Default)]
pub(crate) struct Stats {
    pub proposals: u64,
    pub moves: u64,
    pub weight_bumps: u64,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn index(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

struct Move {
    column: usize,
    value: f64,
    score: i32,
    objective_delta: f64,
    tie: u64,
}

struct Search<'a> {
    model: &'a Model,
    tolerance: f64,
    box_: Vec<(f64, f64)>,
    point: Vec<f64>,
    incident: Vec<Vec<(usize, f64)>>,
    activity: Vec<f64>,
    violated: Vec<usize>,
    slot: Vec<usize>,
    weights: Vec<i32>,
    tabu: Vec<(bool, u64)>,
}

fn violation(model: &Model, row: usize, value: f64) -> f64 {
    (model.rows().lower[row] - value)
        .max(value - model.rows().upper[row])
        .max(0.)
}

impl<'a> Search<'a> {
    fn new(
        model: &'a Model,
        bounds: &Bounds,
        seed: &[f64],
        tolerance: f64,
        meter: &mut Meter<'_, '_>,
    ) -> Result<Self, Stop> {
        meter.ready()?;
        let mut state = Self {
            model,
            tolerance,
            box_: Vec::with_capacity(model.n()),
            point: Vec::with_capacity(model.n()),
            incident: vec![Vec::new(); model.n()],
            activity: vec![0.; model.m()],
            violated: Vec::new(),
            slot: vec![usize::MAX; model.m()],
            weights: vec![1; model.m()],
            tabu: vec![(false, 0); model.n()],
        };
        for (j, &value) in seed.iter().enumerate() {
            meter.charge(Work::Bookkeeping, 1)?;
            let (mut lo, mut hi) = (bounds.lower[j], bounds.upper[j]);
            let rounded = if model.kinds()[j].is_integral() {
                lo = lo.ceil();
                hi = hi.floor();
                value.round()
            } else {
                value
            };
            if !lo.is_finite() || !hi.is_finite() || lo > hi || !rounded.is_finite() {
                return Err(Stop::Numerical);
            }
            state.box_.push((lo, hi));
            state.point.push(rounded.clamp(lo, hi));
        }
        let matrix = model.matrix();
        for row in 0..model.m() {
            meter.charge(Work::Bookkeeping, 1)?;
            for k in matrix.pattern.indptr[row]..matrix.pattern.indptr[row + 1] {
                meter.charge(Work::Setup, 1)?;
                let (j, a) = (matrix.pattern.indices[k], matrix.values[k]);
                if a != 0. {
                    state.incident[j].push((row, a));
                }
            }
        }
        state.refresh(meter)?;
        Ok(state)
    }

    fn set_activity(&mut self, row: usize, value: f64) -> Result<(), Stop> {
        if !value.is_finite() {
            return Err(Stop::Numerical);
        }
        self.activity[row] = value;
        let position = self.slot[row];
        if violation(self.model, row, value) > self.tolerance {
            if position == usize::MAX {
                self.slot[row] = self.violated.len();
                self.violated.push(row);
            }
        } else if position != usize::MAX {
            self.violated.swap_remove(position);
            if let Some(&moved) = self.violated.get(position) {
                self.slot[moved] = position;
            }
            self.slot[row] = usize::MAX;
        }
        Ok(())
    }

    fn refresh(&mut self, meter: &mut Meter<'_, '_>) -> Result<(), Stop> {
        let matrix = self.model.matrix();
        for row in 0..self.model.m() {
            meter.charge(Work::Bookkeeping, 1)?;
            let mut activity = 0.;
            for k in matrix.pattern.indptr[row]..matrix.pattern.indptr[row + 1] {
                meter.charge(Work::Refresh, 1)?;
                activity += matrix.values[k] * self.point[matrix.pattern.indices[k]];
            }
            self.set_activity(row, activity)?;
        }
        Ok(())
    }

    fn sample_rows(&self, rng: &mut Rng) -> Vec<usize> {
        // Floyd sampling selects distinct active rows without scanning all rows.
        let n = self.violated.len();
        let mut sampled = Vec::with_capacity(n.min(SAMPLE_ROWS));
        for last in n.saturating_sub(SAMPLE_ROWS)..n {
            let mut row = self.violated[rng.index(last + 1)];
            if sampled.contains(&row) {
                row = self.violated[last];
            }
            sampled.push(row);
        }
        sampled
    }

    fn score(&self, j: usize, delta: f64, meter: &mut Meter<'_, '_>) -> Result<i32, Stop> {
        let mut score = 0i32;
        for &(row, a) in &self.incident[j] {
            meter.charge(Work::Scoring, 1)?;
            let next = self.activity[row] + a * delta;
            if !next.is_finite() {
                return Err(Stop::Numerical);
            }
            let old = violation(self.model, row, self.activity[row]);
            let new = violation(self.model, row, next);
            let noise = 16. * f64::EPSILON * old.max(new).max(1.);
            let progress = match (old > self.tolerance, new > self.tolerance) {
                (true, false) => 1,
                (false, true) => -1,
                (true, true) if old - new > noise => 1,
                (true, true) if new - old > noise => -1,
                _ => 0,
            };
            score = score.saturating_add(self.weights[row] * progress);
        }
        Ok(score)
    }

    fn choose(
        &self,
        rows: &[usize],
        rng: &mut Rng,
        meter: &mut Meter<'_, '_>,
        stats: &mut Stats,
    ) -> Result<Option<Move>, Stop> {
        let matrix = self.model.matrix();
        let mut best: Option<Move> = None;
        let mut seen = Vec::with_capacity(2 * SAMPLE_TERMS);
        for &row in rows {
            meter.charge(Work::Bookkeeping, 1)?;
            let activity = self.activity[row];
            let target = activity.clamp(self.model.rows().lower[row], self.model.rows().upper[row]);
            let start = matrix.pattern.indptr[row];
            let size = matrix.pattern.indptr[row + 1] - start;
            if size == 0 {
                continue;
            }
            let first = rng.index(size);
            for offset in 0..size.min(SAMPLE_TERMS / rows.len()) {
                meter.charge(Work::Generation, 1)?;
                let k = start + (first + offset) % size;
                let (j, a) = (matrix.pattern.indices[k], matrix.values[k]);
                let (lo, hi) = self.box_[j];
                if a == 0. || lo == hi {
                    continue;
                }
                let old = self.point[j];
                let boundary = (old + (target - activity) / a).clamp(lo, hi);
                let values = if self.model.kinds()[j].is_integral() {
                    [boundary.floor(), boundary.ceil()]
                } else {
                    [boundary, boundary]
                };
                for value in values {
                    let delta = value - old;
                    let (tabu_up, until) = self.tabu[j];
                    if !value.is_finite()
                        || delta == 0.
                        || (stats.moves < until && (delta > 0.) == tabu_up)
                    {
                        continue;
                    }
                    let key = (j, value);
                    meter.charge(Work::Bookkeeping, 1)?;
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    stats.proposals += 1;
                    let candidate = Move {
                        column: j,
                        value,
                        score: self.score(j, delta, meter)?,
                        objective_delta: oriented(self.model, self.model.objective()[j] * delta),
                        tie: rng.next(),
                    };
                    if candidate.objective_delta.is_finite()
                        && best.as_ref().is_none_or(|b| {
                            candidate
                                .score
                                .cmp(&b.score)
                                .then(b.objective_delta.total_cmp(&candidate.objective_delta))
                                .then(candidate.tie.cmp(&b.tie))
                                .is_gt()
                        })
                    {
                        best = Some(candidate);
                    }
                }
            }
        }
        Ok(best)
    }

    fn apply(&mut self, step: Move, meter: &mut Meter<'_, '_>, moves: u64) -> Result<(), Stop> {
        let j = step.column;
        let delta = step.value - self.point[j];
        for k in 0..self.incident[j].len() {
            meter.charge(Work::Update, 1)?;
            let (row, a) = self.incident[j][k];
            self.set_activity(row, self.activity[row] + a * delta)?;
        }
        self.point[j] = step.value;
        self.tabu[j] = (delta < 0., moves.saturating_add(TABU_MOVES));
        Ok(())
    }
}

pub(crate) fn search(
    model: &Model,
    original: &Model,
    bounds: &Bounds,
    seed: &[f64],
    tolerances: &Tolerances,
    meter: &mut Meter<'_, '_>,
    random_seed: u64,
) -> (Result<Vec<f64>, Stop>, Stats) {
    let mut stats = Stats::default();
    let result = (|| {
        let mut state = Search::new(model, bounds, seed, tolerances.feasibility, meter)?;
        let mut rng = Rng(random_seed);
        meter.progress(state.violated.len());
        loop {
            if state.violated.is_empty() {
                if meter.valid_point(model, original, &state.point, tolerances)? {
                    return Ok(state.point);
                }
                state.refresh(meter)?;
                if state.violated.is_empty() {
                    return Err(Stop::Numerical);
                }
            }
            let violations = state.violated.len();
            meter.progress(violations);
            meter.ready()?;
            let rows = state.sample_rows(&mut rng);
            let step = state
                .choose(&rows, &mut rng, meter, &mut stats)?
                .ok_or(Stop::Stalled)?;
            if step.score <= 0 {
                stats.weight_bumps += 1;
                for row in rows {
                    meter.charge(Work::Bookkeeping, 1)?;
                    state.weights[row] = (state.weights[row] + 1).min(WEIGHT_CAP);
                }
            }
            state.apply(step, meter, stats.moves + 1)?;
            stats.moves += 1;
            let violations = state.violated.len();
            meter.progress(violations);
            if violations == 0 {
                // Check at the top of the loop, including a last-budget move.
                continue;
            }
            if stats.moves % REFRESH_EVERY == 0 {
                state.refresh(meter)?;
            }
        }
    })();
    if result.is_ok() {
        meter.progress(0);
    }
    (result, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Budget, NativeOptions, NativeSolver};
    use lp_ip_craft::{
        CancellationToken, CsrMatrix, LpIpSolver, ModelData, Options, Sense, Status, VariableKind,
        WarmStart, check,
    };
    use std::time::Instant;

    fn mixed(sense: Sense) -> Model {
        Model::new(ModelData {
            sense,
            objective: vec![2., -1., 0.],
            objective_offset: 7.,
            variables: Bounds::new(vec![-1.25, 0., 1.], vec![2.5, 3., 1.]),
            kinds: vec![
                VariableKind::Integer,
                VariableKind::Continuous,
                VariableKind::Integer,
            ],
            matrix: CsrMatrix::from_dense(3, 3, &[-1., 1., 0., 1., 1., 0., 1., 0., 1.]).unwrap(),
            rows: Bounds::new(vec![1., -1., f64::NEG_INFINITY], vec![1., 5., 3.]),
        })
        .unwrap()
    }

    #[test]
    fn mixed_moves_and_incremental_activities_match_recomputation() {
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let tol = Tolerances::default();
        for sense in [Sense::Minimize, Sense::Maximize] {
            let model = mixed(sense);
            let mut meter = Meter::new(&budget, 1000, 0);
            let mut state = Search::new(
                &model,
                model.variables(),
                &[2.7, 0., 1.],
                tol.feasibility,
                &mut meter,
            )
            .unwrap();
            assert_eq!(state.point, [2., 0., 1.]);
            assert_eq!(state.violated, [0]);
            let mut stats = Stats::default();
            let chosen = state
                .choose(&[0], &mut Rng(29), &mut meter, &mut stats)
                .unwrap()
                .unwrap();
            state.apply(chosen, &mut meter, 1).unwrap();
            let mut full = vec![0.; model.m()];
            model.matrix().multiply(&state.point, &mut full);
            assert_eq!(state.activity, full);
            let evaluation = check::evaluate(&model, &state.point, &tol).unwrap();
            assert!(evaluation.feasible_candidate);
            assert_eq!(
                evaluation.objective,
                7. + 2. * state.point[0] - state.point[1]
            );
            let (result, stats) = search(
                &model,
                &model,
                model.variables(),
                &[2.7, 0., 1.],
                &tol,
                &mut Meter::new(&budget, 1000, 0),
                29,
            );
            assert_eq!(result.unwrap(), state.point);
            assert_eq!(stats.moves, 1);
        }
    }

    #[test]
    fn infeasible_on_original_or_exhausted_work_does_not_propose() {
        let model = mixed(Sense::Minimize);
        let mut altered = model.data().clone();
        altered.rows.lower[0] = 2.;
        altered.rows.upper[0] = 2.;
        let input = Model::new(altered).unwrap();
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let tol = Tolerances::default();
        let seed = [0., 1., 1.];
        for effort in [0, 7, 1000] {
            let mut meter = Meter::new(&budget, effort, 0);
            let (point, _) = search(
                &model,
                &input,
                model.variables(),
                &seed,
                &tol,
                &mut meter,
                47,
            );
            assert_eq!(
                point,
                Err(if effort < 1000 {
                    Stop::Effort
                } else {
                    Stop::Numerical
                })
            );
            assert!(meter.used() <= effort);
        }
    }

    #[test]
    fn fractional_root_runs_once_and_preserves_a_checked_start_on_limits() {
        let model = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![1., 3.],
            objective_offset: 2.,
            variables: Bounds::new(vec![0.; 2], vec![1.; 2]),
            kinds: vec![VariableKind::Binary; 2],
            matrix: CsrMatrix::from_dense(1, 2, &[0.7, 0.9]).unwrap(),
            rows: Bounds::new(vec![1.2], vec![f64::INFINITY]),
        })
        .unwrap();
        let options = Options::default();
        let cancel = CancellationToken::default();
        let settings = NativeOptions {
            incumbent_heuristic: false,
            root_propagation: false,
            mip_presolve: false,
            coefficient_tightening: false,
            decompose: false,
            local_mip_effort: 1000,
            ..NativeOptions::default()
        };
        let has_local = |result: &lp_ip_craft::SolveResult| {
            result.backend.effective_options.iter().any(|(key, value)| {
                key == "diagnostics.root_local_search.source" && value == "local_mip"
            })
        };
        let result = NativeSolver::new(settings.clone())
            .solve(&model, &options, None, &cancel)
            .unwrap();
        assert!(has_local(&result));
        assert_eq!(result.evaluation.unwrap().objective, 6.);

        let start = WarmStart::from_primal(vec![1.; 2]);
        for (node_limit, iteration_limit, expected) in [
            (Some(0), None, Status::NodeLimit),
            (None, Some(0), Status::IterationLimit),
        ] {
            let mut solver = NativeSolver::new(NativeOptions {
                node_limit,
                iteration_limit,
                ..settings.clone()
            });
            let result = solver
                .solve(&model, &options, Some(&start), &cancel)
                .unwrap();
            assert!(!has_local(&result));
            assert_eq!(result.status, expected);
            assert_eq!(result.candidate.unwrap(), [1., 1.]);
        }
        cancel.cancel();
        let result = NativeSolver::new(settings)
            .solve(&model, &options, Some(&start), &cancel)
            .unwrap();
        assert!(!has_local(&result));
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(result.candidate.unwrap(), [1., 1.]);
    }
}
