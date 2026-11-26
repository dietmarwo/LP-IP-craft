//! Bounded deterministic Feasibility Jump prototype. Scores are heuristic only.
use crate::Budget;
use crate::heuristic_budget::{Meter, Stop, Work};
use lp_ip_craft::{Bounds, Model};
#[derive(Default)]
pub(crate) struct Stats {
    pub steps: u64,
    pub proposals: u64,
    pub work: u64,
    pub feasible: u64,
    pub bumps: u64,
}
fn violation(v: f64, l: f64, u: f64) -> f64 {
    (l - v).max(v - u).max(0.)
}
pub(crate) fn search(
    model: &Model,
    bounds: &Bounds,
    seed: &[f64],
    budget: &Budget<'_>,
    limit: u64,
) -> (Option<Vec<f64>>, Stats) {
    let mut stats = Stats::default();
    let point = search_impl::<false>(model, bounds, seed, budget, limit, None, &mut stats)
        .expect("unmetered Feasibility Jump does not return a local stop");
    (point, stats)
}

pub(crate) fn search_metered(
    model: &Model,
    bounds: &Bounds,
    seed: &[f64],
    budget: &Budget<'_>,
    limit: u64,
    meter: &mut Meter<'_, '_>,
) -> (Result<Vec<f64>, Stop>, Stats) {
    let mut stats = Stats::default();
    let point = search_impl::<true>(model, bounds, seed, budget, limit, Some(meter), &mut stats)
        .and_then(|point| point.ok_or(Stop::StepLimit));
    (point, stats)
}

fn metered_feasible(model: &Model, lhs: &[f64], meter: &mut Meter<'_, '_>) -> Result<bool, Stop> {
    let mut violated = 0;
    for (i, &value) in lhs.iter().enumerate() {
        meter.charge(Work::Bookkeeping, 1)?;
        if !value.is_finite() {
            return Err(Stop::Numerical);
        }
        violated +=
            usize::from(violation(value, model.rows().lower[i], model.rows().upper[i]) > 1e-10);
    }
    meter.progress(violated);
    Ok(violated == 0)
}

fn search_impl<const METERED: bool>(
    model: &Model,
    bounds: &Bounds,
    seed: &[f64],
    budget: &Budget<'_>,
    limit: u64,
    mut meter: Option<&mut Meter<'_, '_>>,
    stats: &mut Stats,
) -> Result<Option<Vec<f64>>, Stop> {
    fn multiply(
        a: &lp_ip_craft::CsrMatrix,
        x: &[f64],
        lhs: &mut [f64],
        meter: Option<&mut Meter<'_, '_>>,
    ) -> Result<(), Stop> {
        let Some(meter) = meter else {
            a.multiply(x, lhs);
            return Ok(());
        };
        for (i, value) in lhs.iter_mut().enumerate() {
            meter.charge(Work::Bookkeeping, 1)?;
            *value = 0.;
            for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                meter.charge(Work::Refresh, 1)?;
                *value += a.values[k] * x[a.pattern.indices[k]];
            }
        }
        Ok(())
    }
    macro_rules! charge {
        ($work:ident, $n:expr) => {
            if METERED {
                meter.as_deref_mut().unwrap().charge(Work::$work, $n)?;
            }
        };
    }
    if let Some(meter) = &meter {
        meter.ready()?;
    }
    let n = model.n();
    let m = model.m();
    let a = model.matrix();
    let mut incident = vec![Vec::new(); n];
    for i in 0..m {
        charge!(Bookkeeping, 1);
        for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
            charge!(Setup, 1);
            let j = a.pattern.indices[k];
            if a.values[k] != 0. {
                incident[j].push((i, a.values[k]));
            }
        }
    }
    // Integer columns move only within their integral bounds, so a clamped
    // value can never be fractional.
    let box_of = |j: usize| {
        if model.kinds()[j].is_integral() {
            (bounds.lower[j].ceil(), bounds.upper[j].floor())
        } else {
            (bounds.lower[j], bounds.upper[j])
        }
    };
    let initial = |j: usize| {
        let v = if model.kinds()[j].is_integral() {
            seed[j].round()
        } else {
            seed[j]
        };
        let (l, u) = box_of(j);
        v.max(l).min(u)
    };
    charge!(Bookkeeping, n as u64);
    let mut x: Vec<f64> = (0..n).map(initial).collect();
    let mut lhs = vec![0.; m];
    multiply(a, &x, &mut lhs, meter.as_deref_mut())?;
    let mut weight = vec![1.; m];
    let mut dirty = vec![true; n];
    let mut jumps = x.clone();
    let mut scores = vec![f64::NEG_INFINITY; n];
    let mut stall = 0usize;
    let cap = limit.saturating_mul(512).min(10_000_000);
    for step in 0..limit {
        if budget.stop().is_some() || stats.work >= cap {
            if let Some(meter) = &meter {
                meter.clock()?;
            }
            break;
        }
        if step % 32 == 0 {
            multiply(a, &x, &mut lhs, meter.as_deref_mut())?;
            charge!(Bookkeeping, n as u64);
            dirty.fill(true);
        }
        let feasible = if let Some(meter) = &mut meter {
            // Every completed move is checked below. A full refresh may change
            // the row classification, including on the initial iteration.
            let feasible = step % 32 == 0 && metered_feasible(model, &lhs, meter)?;
            if !feasible {
                meter.ready()?;
            }
            feasible
        } else {
            (0..m).all(|i| violation(lhs[i], model.rows().lower[i], model.rows().upper[i]) <= 1e-10)
        };
        if feasible {
            stats.feasible += 1;
            return Ok(Some(x));
        }
        for j in 0..n {
            charge!(Bookkeeping, 1);
            if !dirty[j] {
                continue;
            }
            dirty[j] = false;
            let old = x[j];
            let (lower, upper) = box_of(j);
            let mut candidates = vec![old, lower, upper];
            for &(i, c) in &incident[j] {
                charge!(Generation, 1);
                for b in [model.rows().lower[i], model.rows().upper[i]] {
                    if b.is_finite() {
                        let v = old + (b - lhs[i]) / c;
                        if model.kinds()[j].is_integral() {
                            candidates.push(v.floor());
                            candidates.push(v.ceil());
                        } else {
                            candidates.push(v);
                        }
                    }
                }
            }
            let mut best = (f64::NEG_INFINITY, old);
            for v in candidates {
                charge!(Bookkeeping, 1);
                let v = v.max(lower).min(upper);
                if !v.is_finite() || v == old {
                    continue;
                }
                if METERED {
                    stats.proposals += 1;
                }
                let mut score = 0.;
                for &(i, c) in &incident[j] {
                    charge!(Scoring, 1);
                    stats.work += 1;
                    score += weight[i]
                        * (violation(lhs[i], model.rows().lower[i], model.rows().upper[i])
                            - violation(
                                lhs[i] + c * (v - old),
                                model.rows().lower[i],
                                model.rows().upper[i],
                            ));
                    if stats.work >= cap {
                        break;
                    }
                }
                if stats.work >= cap {
                    break;
                }
                if score > best.0 {
                    best = (score, v);
                }
            }
            scores[j] = best.0;
            jumps[j] = best.1;
            if stats.work >= cap || budget.stop().is_some() {
                break;
            }
        }
        if stats.work >= cap || budget.stop().is_some() {
            break;
        }
        charge!(Bookkeeping, n as u64);
        let mut chosen = (0..n)
            .filter(|&j| jumps[j] != x[j])
            .max_by(|&j, &k| scores[j].total_cmp(&scores[k]).then(k.cmp(&j)));
        if chosen.is_none_or(|j| scores[j] <= 1e-10) {
            stats.bumps += 1;
            stall += 1;
            charge!(Bookkeeping, m as u64);
            let violated: Vec<_> = (0..m)
                .filter(|&i| {
                    violation(lhs[i], model.rows().lower[i], model.rows().upper[i]) > 1e-10
                })
                .collect();
            for &i in &violated {
                weight[i] = (weight[i] + 1.).min(1e6);
                for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                    charge!(Update, 1);
                    dirty[a.pattern.indices[k]] = true;
                }
            }
            if violated.is_empty() {
                break;
            }
            let row = violated[(stall - 1) % violated.len()];
            charge!(
                Generation,
                (a.pattern.indptr[row + 1] - a.pattern.indptr[row]) as u64
            );
            chosen = (a.pattern.indptr[row]..a.pattern.indptr[row + 1])
                .map(|k| a.pattern.indices[k])
                .filter(|&j| jumps[j] != x[j])
                .max_by(|&j, &k| scores[j].total_cmp(&scores[k]).then(k.cmp(&j)));
            // A deterministic restart explores a different corner while keeping fixed columns.
            if stall % 32 == 0 {
                for (j, value) in x.iter_mut().enumerate() {
                    charge!(Bookkeeping, 1);
                    let (lower, upper) = box_of(j);
                    let v = if (j + stall / 32) % 3 == 0 {
                        upper
                    } else {
                        lower
                    };
                    *value = if v.is_finite() {
                        if model.kinds()[j].is_integral() {
                            v.round()
                        } else {
                            v
                        }
                    } else {
                        initial(j)
                    };
                }
                multiply(a, &x, &mut lhs, meter.as_deref_mut())?;
                charge!(Bookkeeping, n as u64);
                dirty.fill(true);
                if let Some(meter) = &mut meter {
                    if metered_feasible(model, &lhs, meter)? {
                        stats.feasible += 1;
                        return Ok(Some(x));
                    }
                }
                continue;
            }
        }
        let Some(j) = chosen else {
            charge!(Bookkeeping, n as u64);
            dirty.fill(true);
            continue;
        };
        let delta = jumps[j] - x[j];
        x[j] = jumps[j];
        for &(i, c) in &incident[j] {
            charge!(Update, 1);
            lhs[i] += c * delta;
            for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                charge!(Update, 1);
                dirty[a.pattern.indices[k]] = true;
            }
        }
        dirty[j] = true;
        stats.steps += 1;
        if let Some(meter) = &mut meter {
            if metered_feasible(model, &lhs, meter)? {
                stats.feasible += 1;
                return Ok(Some(x));
            }
        }
    }
    if let Some(meter) = &meter {
        meter.clock()?;
        if stats.work >= cap && limit != 0 {
            return Err(Stop::ScoringLimit);
        }
    }
    Ok(None)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeOptions;
    use lp_ip_craft::*;
    #[test]
    fn repairs_rounding_with_sparse_jumps() {
        let m = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![0.; 3],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; 3], vec![1.; 3]),
            kinds: vec![VariableKind::Binary; 3],
            matrix: CsrMatrix::from_dense(1, 3, &[1., 1., 1.]).unwrap(),
            rows: Bounds::fixed(vec![1.]),
        })
        .unwrap();
        let s = NativeOptions::default();
        let c = CancellationToken::default();
        let b = Budget {
            start: std::time::Instant::now(),
            settings: &s,
            cancel: &c,
            pivots: 0,
        };
        let (x, t) = search(&m, m.variables(), &[0.4; 3], &b, 100);
        assert!(
            check::evaluate(&m, &x.unwrap(), &Tolerances::default())
                .unwrap()
                .feasible_candidate
        );
        assert!(t.steps > 0 && t.work <= 51200);
        let (x, t) = search(&m, m.variables(), &[0.4; 3], &b, 0);
        assert!(x.is_none() && t.steps == 0);
        c.cancel();
        assert!(search(&m, m.variables(), &[0.4; 3], &b, 100).0.is_none());
    }
    #[test]
    fn metered_fj_credits_a_feasible_final_move_at_the_exact_work_cap() {
        let model = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![0.; 3],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; 3], vec![1.; 3]),
            kinds: vec![VariableKind::Binary; 3],
            matrix: CsrMatrix::from_dense(1, 3, &[1.; 3]).unwrap(),
            rows: Bounds::fixed(vec![1.]),
        })
        .unwrap();
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: std::time::Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let mut meter = Meter::new(&budget, 1000, 0);
        let (point, stats) =
            search_metered(&model, model.variables(), &[0.4; 3], &budget, 1, &mut meter);
        assert!(point.is_ok());
        assert_eq!(stats.steps, 1);
        let cap = meter.used();
        let mut exact = Meter::new(&budget, cap, 0);
        let (same, _) =
            search_metered(&model, model.variables(), &[0.4; 3], &budget, 1, &mut exact);
        assert_eq!(same, point);
        assert_eq!(exact.used(), cap);
        assert_eq!(exact.counts.patience_resets, 1);
        let mut short = Meter::new(&budget, cap - 1, 0);
        assert_eq!(
            search_metered(&model, model.variables(), &[0.4; 3], &budget, 1, &mut short).0,
            Err(Stop::Effort)
        );
    }

    #[test]
    fn integer_columns_stay_integral_under_fractional_bounds() {
        // x is integer in [0, 2.5]; the seed rounds to 3, which the bound clamps.
        let m = Model::new(ModelData {
            sense: Sense::Minimize,
            objective: vec![0.],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.], vec![2.5]),
            kinds: vec![VariableKind::Integer],
            matrix: CsrMatrix::from_dense(1, 1, &[1.]).unwrap(),
            rows: Bounds::new(vec![1.], vec![f64::INFINITY]),
        })
        .unwrap();
        let s = NativeOptions::default();
        let c = CancellationToken::default();
        let b = Budget {
            start: std::time::Instant::now(),
            settings: &s,
            cancel: &c,
            pivots: 0,
        };
        let (x, _) = search(&m, m.variables(), &[2.7], &b, 100);
        let x = x.expect("a feasible integral point exists");
        assert_eq!(x, vec![2.]);
        assert!(
            check::evaluate(&m, &x, &Tolerances::default())
                .unwrap()
                .feasible_candidate
        );
    }
}
