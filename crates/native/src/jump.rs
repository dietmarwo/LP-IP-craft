//! Bounded deterministic Feasibility Jump prototype. Scores are heuristic only.
use crate::Budget;
use lp_ip_craft::{Bounds, Model};
#[derive(Default)]
pub(crate) struct Stats {
    pub steps: u64,
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
    let n = model.n();
    let m = model.m();
    let a = model.matrix();
    let mut incident = vec![Vec::new(); n];
    for i in 0..m {
        for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
            let j = a.pattern.indices[k];
            if a.values[k] != 0. {
                incident[j].push((i, a.values[k]));
            }
        }
    }
    let initial = |j: usize| {
        let v = if model.kinds()[j].is_integral() {
            seed[j].round()
        } else {
            seed[j]
        };
        v.max(bounds.lower[j]).min(bounds.upper[j])
    };
    let mut x: Vec<f64> = (0..n).map(initial).collect();
    let mut lhs = vec![0.; m];
    a.multiply(&x, &mut lhs);
    let mut weight = vec![1.; m];
    let mut dirty = vec![true; n];
    let mut jumps = x.clone();
    let mut scores = vec![f64::NEG_INFINITY; n];
    let mut stall = 0usize;
    let cap = limit.saturating_mul(512).min(10_000_000);
    for step in 0..limit {
        if budget.stop().is_some() || stats.work >= cap {
            break;
        }
        if step % 32 == 0 {
            a.multiply(&x, &mut lhs);
            dirty.fill(true);
        }
        if (0..m).all(|i| violation(lhs[i], model.rows().lower[i], model.rows().upper[i]) <= 1e-10)
        {
            stats.feasible += 1;
            return (Some(x), stats);
        }
        for j in 0..n {
            if !dirty[j] {
                continue;
            }
            dirty[j] = false;
            let old = x[j];
            let mut candidates = vec![old, bounds.lower[j], bounds.upper[j]];
            for &(i, c) in &incident[j] {
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
                let v = v.max(bounds.lower[j]).min(bounds.upper[j]);
                if !v.is_finite() || v == old {
                    continue;
                }
                let mut score = 0.;
                for &(i, c) in &incident[j] {
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
        let mut chosen = (0..n)
            .filter(|&j| jumps[j] != x[j])
            .max_by(|&j, &k| scores[j].total_cmp(&scores[k]).then(k.cmp(&j)));
        if chosen.is_none_or(|j| scores[j] <= 1e-10) {
            stats.bumps += 1;
            stall += 1;
            let violated: Vec<_> = (0..m)
                .filter(|&i| {
                    violation(lhs[i], model.rows().lower[i], model.rows().upper[i]) > 1e-10
                })
                .collect();
            for &i in &violated {
                weight[i] = (weight[i] + 1.).min(1e6);
                for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                    dirty[a.pattern.indices[k]] = true;
                }
            }
            if violated.is_empty() {
                break;
            }
            let row = violated[(stall - 1) % violated.len()];
            chosen = (a.pattern.indptr[row]..a.pattern.indptr[row + 1])
                .map(|k| a.pattern.indices[k])
                .filter(|&j| jumps[j] != x[j])
                .max_by(|&j, &k| scores[j].total_cmp(&scores[k]).then(k.cmp(&j)));
            // A deterministic restart explores a different corner while keeping fixed columns.
            if stall % 32 == 0 {
                for (j, value) in x.iter_mut().enumerate() {
                    let v = if (j + stall / 32) % 3 == 0 {
                        bounds.upper[j]
                    } else {
                        bounds.lower[j]
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
                a.multiply(&x, &mut lhs);
                dirty.fill(true);
                continue;
            }
        }
        let Some(j) = chosen else {
            dirty.fill(true);
            continue;
        };
        let delta = jumps[j] - x[j];
        x[j] = jumps[j];
        for &(i, c) in &incident[j] {
            lhs[i] += c * delta;
            for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                dirty[a.pattern.indices[k]] = true;
            }
        }
        dirty[j] = true;
        stats.steps += 1;
    }
    (None, stats)
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
}
