//! Root guided diving and L1 feasibility pump. Their LPs never supply proof bounds.
use crate::*;
use lp_ip_craft::VariableKind;
#[derive(Default)]
pub(crate) struct Stats {
    pub calls: u64,
    pub candidates: u64,
    pub cycles: u64,
    pub failures: u64,
}
pub(crate) fn candidate(
    model: &Model,
    x: Vec<f64>,
    options: &Options,
    best: &mut Option<Vec<f64>>,
) -> Result<bool> {
    if !check::evaluate(model, &x, &options.tolerances)?.feasible_candidate {
        return Ok(false);
    }
    let value = |v: &[f64]| {
        oriented(
            model,
            model.objective_offset()
                + model
                    .objective()
                    .iter()
                    .zip(v)
                    .map(|(c, x)| c * x)
                    .sum::<f64>(),
        )
    };
    if best.as_ref().is_none_or(|old| value(&x) < value(old)) {
        *best = Some(x);
        return Ok(true);
    }
    Ok(false)
}
fn allowed(budget: &Budget<'_>, nodes: u64) -> bool {
    budget.stop().is_none() && budget.settings.node_limit.is_none_or(|n| nodes < n)
}
pub(crate) fn run(
    model: &Model,
    lp: &mut Lp,
    options: &Options,
    budget: &mut Budget<'_>,
    nodes: &mut u64,
    best: &mut Option<Vec<f64>>,
) -> Result<Stats> {
    let mut stats = Stats::default();
    if (0..model.n()).any(|j| {
        model.kinds()[j].is_integral()
            && model.variables().lower[j].ceil() > model.variables().upper[j].floor()
    }) {
        return Ok(stats);
    }
    let initial = lp.point();
    let rounded = |x: &[f64]| -> Vec<f64> {
        (0..model.n())
            .map(|j| {
                if model.kinds()[j].is_integral() {
                    x[j].round().clamp(
                        model.variables().lower[j].ceil(),
                        model.variables().upper[j].floor(),
                    )
                } else {
                    x[j]
                }
            })
            .collect()
    };
    if candidate(model, rounded(&initial), options, best)? {
        stats.candidates += 1;
    }
    let cap = budget.settings.heuristic_lp_calls;
    let dive_cap = (cap / 2).min(8);
    let snapshot = lp.snapshot();
    let mut lower = model.variables().lower.clone();
    let mut upper = model.variables().upper.clone();
    for _ in 0..dive_cap {
        if !allowed(budget, *nodes) {
            break;
        }
        let x = lp.point();
        let j = (0..model.n())
            .filter(|&j| {
                model.kinds()[j].is_integral()
                    && lower[j] != upper[j]
                    && (x[j] - x[j].round()).abs() > 1e-6
            })
            .max_by(|&a, &b| {
                (x[a] - x[a].round())
                    .abs()
                    .total_cmp(&(x[b] - x[b].round()).abs())
            });
        let Some(j) = j else {
            break;
        };
        let value = x[j].round().clamp(lower[j].ceil(), upper[j].floor());
        lower[j] = value;
        upper[j] = value;
        lp.set_bounds(&lower, &upper)?;
        *nodes += 1;
        stats.calls += 1;
        let status = lp.solve(budget, false).unwrap_or(Status::NumericalFailure);
        if status != Status::Optimal {
            stats.failures += 1;
            break;
        }
        if candidate(model, rounded(&lp.point()), options, best)? {
            stats.candidates += 1;
        }
    }
    // Parent bounds and combinatorial basis are restored on every normal outcome,
    // including infeasible/limited probes. They are not used as proof information.
    lp.set_bounds(&model.variables().lower, &model.variables().upper)?;
    lp.restore(&snapshot);
    let integers: Vec<_> = (0..model.n())
        .filter(|&j| model.kinds()[j].is_integral())
        .collect();
    if integers.is_empty()
        || (0..model.n()).any(|j| {
            !model.variables().lower[j].is_finite() || !model.variables().upper[j].is_finite()
        })
    {
        return Ok(stats);
    }
    let mut point = initial;
    let mut seen = std::collections::BTreeSet::new();
    while (stats.calls as usize) < cap && allowed(budget, *nodes) {
        let mut target = rounded(&point);
        let key: Vec<_> = integers.iter().map(|&j| target[j].to_bits()).collect();
        if !seen.insert(key) {
            stats.cycles += 1;
            if stats.cycles > 3 {
                break;
            }
            let at = (stats.cycles as usize - 1) % integers.len();
            let j = integers[at];
            let l = model.variables().lower[j].ceil();
            let u = model.variables().upper[j].floor();
            if l < u {
                target[j] = if target[j] < u {
                    target[j] + 1.
                } else {
                    target[j] - 1.
                };
            } else {
                break;
            }
        }
        if candidate(model, target.clone(), options, best)? {
            stats.candidates += 1;
            break;
        }
        let mut data = model.data().clone();
        let n = model.n();
        data.sense = Sense::Minimize;
        data.objective.fill(0.);
        data.objective_offset = 0.;
        data.kinds.fill(VariableKind::Continuous);
        for (at, &j) in integers.iter().enumerate() {
            let k = n + at;
            data.objective.push(1.);
            data.variables.lower.push(0.);
            data.variables.upper.push(
                (target[j] - model.variables().lower[j])
                    .abs()
                    .max((model.variables().upper[j] - target[j]).abs()),
            );
            data.kinds.push(VariableKind::Continuous);
            for (sign, rhs) in [(1., target[j]), (-1., -target[j])] {
                data.matrix.pattern.indices.extend([j, k]);
                data.matrix.values.extend([sign, -1.]);
                data.matrix.pattern.indptr.push(data.matrix.values.len());
                data.matrix.pattern.rows += 1;
                data.rows.lower.push(f64::NEG_INFINITY);
                data.rows.upper.push(rhs);
            }
        }
        data.matrix.pattern.cols = data.objective.len();
        let auxiliary = Model::new(data)?;
        let mut projection = Lp::new(
            &auxiliary,
            &vec![true; auxiliary.m()],
            &vec![true; auxiliary.n()],
            controls(&auxiliary, options, budget.settings),
        )?;
        *nodes += 1;
        stats.calls += 1;
        let status = projection
            .solve(budget, true)
            .unwrap_or(Status::NumericalFailure);
        if status != Status::Optimal {
            stats.failures += 1;
            break;
        }
        point = projection.point()[..n].to_vec();
        if candidate(model, rounded(&point), options, best)? {
            stats.candidates += 1;
            break;
        }
    }
    Ok(stats)
}
