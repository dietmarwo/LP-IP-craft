//! Root LP separation stage: checked solves, exact cuts and retained bases.
use crate::*;
#[derive(Default)]
pub(crate) struct Stats {
    pub pivots: u64,
    pub nodes: u64,
    pub gmi: u64,
    pub mir: u64,
    pub covers: u64,
    pub lifted: u64,
    pub exact_work: u64,
    pub candidates: u64,
    pub rejected: u64,
    pub vubs: u64,
    pub basis_reuses: u64,
    pub bound: Option<f64>,
    pub initial_objective: Option<f64>,
    pub stages: Vec<String>,
    pub seed: Option<Vec<f64>>,
    pub primal: primal::Stats,
}
fn number(v: Option<f64>) -> String {
    v.filter(|v| v.is_finite())
        .map_or_else(|| "null".into(), |v| v.to_string())
}
pub(crate) fn run(
    model: &mut Model,
    options: &Options,
    settings: &NativeOptions,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
    began: Instant,
) -> Result<Stats> {
    let mut stats = Stats {
        seed: start.map(|s| s.primal.clone()),
        ..Default::default()
    };
    let mut budget = Budget {
        start: began,
        settings,
        cancel,
        pivots: 0,
    };
    let mut lp = Lp::new(
        model,
        &vec![true; model.m()],
        &presolve::kept_columns(model, model.variables(), settings),
        controls(model, options, settings),
    )?;
    let rounds = if settings.gmi_cuts || settings.mir_cuts {
        settings.mip_cut_rounds
    } else {
        0
    };
    let mut stagnant = 0;
    let mut previous: Option<f64> = None;
    for round in 0..=rounds {
        if budget.stop().is_some() || settings.node_limit.is_some_and(|n| stats.nodes >= n) {
            break;
        }
        stats.nodes += 1;
        let t = Instant::now();
        let status = lp
            .solve(&mut budget, round == 0)
            .unwrap_or(Status::NumericalFailure);
        if status != Status::Optimal {
            stats.stages.push(format!("{{\"round\":{round},\"status\":\"{}\",\"bound\":null,\"objective\":null,\"seconds\":{}}}",status.as_str(),t.elapsed().as_secs_f64()));
            break;
        }
        let x = lp.point();
        let duals = lp.duals(model);
        if !accepted(model, model.variables(), &x, &duals, options, settings) {
            break;
        }
        let objective = check::evaluate(model, &x, &options.tolerances)?.objective;
        let bound = safe_dual_bound_with_bounds(model, model.variables(), &duals.row_duals)?.bound;
        if let Some(b) = bound {
            stats.bound = Some(stats.bound.map_or(b, |old| {
                if model.sense() == Sense::Minimize {
                    old.max(b)
                } else {
                    old.min(b)
                }
            }));
        }
        if round == 0 {
            stats.initial_objective = Some(objective);
        }
        stats.stages.push(format!("{{\"round\":{round},\"status\":\"optimal\",\"bound\":{},\"objective\":{objective},\"seconds\":{},\"rows\":{},\"pivots\":{}}}",number(bound),t.elapsed().as_secs_f64(),model.m(),budget.pivots));
        if let Some(old) = previous {
            let improvement = (objective - old).abs();
            if improvement < 0.001 * (1.0f64 + f64::abs(old)) {
                stagnant += 1;
            } else {
                stagnant = 0;
            }
        }
        previous = Some(objective);
        if round == 0 && settings.root_heuristics && budget.stop().is_none() {
            stats.primal = primal::run(
                model,
                &mut lp,
                options,
                &mut budget,
                &mut stats.nodes,
                &mut stats.seed,
            )?;
            if budget.stop().is_some() || settings.node_limit.is_some_and(|n| stats.nodes >= n) {
                break;
            }
            stats.nodes += 1;
            if lp
                .solve(&mut budget, false)
                .unwrap_or(Status::NumericalFailure)
                != Status::Optimal
                || !accepted(
                    model,
                    model.variables(),
                    &lp.point(),
                    &lp.duals(model),
                    options,
                    settings,
                )
            {
                break;
            }
        }
        if round == rounds || stagnant >= 3 {
            break;
        }
        let (selected, cuts) = certified_cuts::separate(model, &mut lp, &x, &budget);
        stats.exact_work += cuts.work;
        stats.candidates += cuts.candidates;
        stats.rejected += cuts.rejected;
        stats.vubs = stats.vubs.max(cuts.vubs);
        let old_rows = model.m();
        stats.gmi += selected.iter().filter(|c| c.family == "gmi").count() as u64;
        stats.mir += selected.iter().filter(|c| c.family == "mir").count() as u64;
        certified_cuts::append(model, &selected)?;
        if settings.root_separation {
            let c = cuts::separate(model, &x, settings.cover_lifting, cancel)?;
            stats.covers += c.added as u64;
            stats.lifted += c.lifted as u64;
        }
        if model.m() == old_rows {
            break;
        }
        lp = lp.extend_rows(model)?;
        stats.basis_reuses += 1;
    }
    stats.pivots = budget.pivots;
    Ok(stats)
}
