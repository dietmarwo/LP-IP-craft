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
    /// Cut rows slack at the final root LP optimum, removed before search.
    pub pruned: u64,
    /// RENS sub-MIP nodes, and whether it improved the root seed.
    pub rens_nodes: u64,
    pub rens_found: u64,
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
    let mut families = [settings.gmi_cuts, settings.mir_cuts];
    // Rows from here on are root cuts; `last` is the latest accepted optimum
    // and the row count it belongs to.
    let cuts_from = model.m();
    let mut last: Option<(usize, Vec<f64>)> = None;
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
        last = Some((model.m(), x.clone()));
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
        // While both families keep contributing after the first round,
        // dense cuts are dropped (support above max(50, n/4), like SCIP's
        // relative support limit). Combined rounds otherwise accumulate
        // dense rank-2 cuts whose exact derivations dominate root time on
        // assignment models. Single-family policies, and combined once a
        // family has stopped, keep every cut.
        let max_terms = if round >= 1 && families == [true, true] {
            50.max(model.n() / 4)
        } else {
            usize::MAX
        };
        let (selected, cuts) =
            certified_cuts::separate(model, &mut lp, &x, &budget, families, max_terms);
        // A family whose cuts were not selected this round is not separated
        // again; the other family then gets the whole work budget.
        for (active, family) in families.iter_mut().zip(["gmi", "mir"]) {
            *active &= selected.iter().any(|c| c.family == family);
        }
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
    if let Some((rows, x)) = last {
        if rows == model.m() && rows > cuts_from {
            stats.pruned = prune_slack(model, cuts_from, &x)?;
        }
        if settings.root_heuristics && budget.stop().is_none() {
            rens(model, &x, options, &mut budget, began, &mut stats)?;
        }
    }
    stats.pivots = budget.pivots;
    Ok(stats)
}
/// RENS: the sub-MIP of `model` in which every integer column that is
/// integral at the root optimum `x` is fixed and every other one is
/// restricted to its two neighbouring integers, when at least half of the
/// integer columns are fixed. It is solved with a node budget of 50 per
/// heuristic LP call. Its incumbents are feasible for
/// `model`, and only those are kept, as the search's start; its status and
/// bound say nothing about `model` and are discarded.
fn rens(
    model: &Model,
    x: &[f64],
    options: &Options,
    budget: &mut Budget<'_>,
    began: Instant,
    stats: &mut Stats,
) -> Result<()> {
    let settings = budget.settings;
    let mut data = model.data().clone();
    let (mut open, mut fixed) = (0, 0);
    for (j, &v) in x.iter().enumerate() {
        if !model.kinds()[j].is_integral() {
            continue;
        }
        let (lo, hi) = (
            data.variables.lower[j].ceil(),
            data.variables.upper[j].floor(),
        );
        if (v - v.round()).abs() <= 1e-6 {
            fixed += 1;
            let r = v.round().clamp(lo, hi);
            data.variables.lower[j] = r;
            data.variables.upper[j] = r;
        } else {
            data.variables.lower[j] = lo.max(v.floor());
            data.variables.upper[j] = hi.min(v.ceil());
            open += 1;
        }
    }
    let nodes = 50 * settings.heuristic_lp_calls as u64;
    let nodes = settings
        .node_limit
        .map_or(nodes, |n| n.saturating_sub(stats.nodes).min(nodes));
    // Like SCIP's minimum fixing rate: with fewer than half the integer
    // columns fixed, the sub-MIP is nearly the whole problem.
    if open == 0 || nodes == 0 || fixed < open {
        return Ok(());
    }
    let mut s = packing::child_settings(settings, began);
    s.node_limit = Some(nodes);
    s.iteration_limit = settings
        .iteration_limit
        .map(|p| p.saturating_sub(budget.pivots));
    s.mip_trace_limit = 0;
    s.branch_trace_limit = 0;
    let Ok(sub) = Model::new(data) else {
        return Ok(());
    };
    // A failed heuristic solve is not an error of the main search.
    let Ok(r) = NativeSolver::new(s).solve(&sub, options, None, budget.cancel) else {
        return Ok(());
    };
    stats.nodes += r.nodes;
    stats.rens_nodes += r.nodes;
    budget.pivots += r.iterations;
    if let Some(c) = r.candidate {
        if primal::candidate(model, c, options, &mut stats.seed)? {
            stats.rens_found += 1;
        }
    }
    Ok(())
}
/// Removes cut rows (index `from` on) that are strictly slack at the root LP
/// optimum `x`. Each has a zero dual there, so `x` and its duals stay
/// optimal for the smaller LP and the root relaxation value is unchanged;
/// dropping valid inequalities never removes a feasible point.
fn prune_slack(model: &mut Model, from: usize, x: &[f64]) -> Result<u64> {
    let a = model.matrix();
    let rows = model.rows();
    let slack = |i: usize| {
        let activity: f64 = (a.pattern.indptr[i]..a.pattern.indptr[i + 1])
            .map(|k| a.values[k] * x[a.pattern.indices[k]])
            .sum();
        let margin = |b: f64| 1e-6 * (1. + b.abs());
        (!rows.lower[i].is_finite() || activity > rows.lower[i] + margin(rows.lower[i]))
            && (!rows.upper[i].is_finite() || activity < rows.upper[i] - margin(rows.upper[i]))
    };
    let keep: Vec<bool> = (0..model.m()).map(|i| i < from || !slack(i)).collect();
    let removed = keep.iter().filter(|k| !**k).count() as u64;
    if removed == 0 {
        return Ok(0);
    }
    let mut data = model.data().clone();
    let (mut indptr, mut indices, mut values) = (vec![0], Vec::new(), Vec::new());
    let (mut lower, mut upper) = (Vec::new(), Vec::new());
    for i in (0..model.m()).filter(|&i| keep[i]) {
        let range = a.pattern.indptr[i]..a.pattern.indptr[i + 1];
        indices.extend_from_slice(&a.pattern.indices[range.clone()]);
        values.extend_from_slice(&a.values[range]);
        indptr.push(values.len());
        lower.push(rows.lower[i]);
        upper.push(rows.upper[i]);
    }
    data.matrix.pattern.rows = lower.len();
    data.matrix.pattern.indptr = indptr;
    data.matrix.pattern.indices = indices;
    data.matrix.values = values;
    data.rows.lower = lower;
    data.rows.upper = upper;
    *model = Model::new(data)?;
    Ok(removed)
}
