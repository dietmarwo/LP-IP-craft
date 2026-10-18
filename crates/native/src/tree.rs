//! Exhaustive disjoint binary-frontier parallel B&B. No shared mutable LP state.
use crate::*;

fn partition_columns(model: &Model, hint: Option<&[f64]>, depth: usize) -> Vec<usize> {
    let mut degree = vec![0usize; model.n()];
    for &j in &model.matrix().pattern.indices {
        degree[j] += 1;
    }
    let mut free: Vec<_> = (0..model.n())
        .filter(|&j| model.variables().lower[j] < model.variables().upper[j])
        .collect();
    let score =
        |j: usize| hint.map_or(0., |h| 0.5 - (h[j] - 0.5).abs()) * 1e6 + (degree[j] as f64).ln_1p();
    free.sort_by(|&a, &b| score(b).total_cmp(&score(a)).then(a.cmp(&b)));
    free.truncate(depth);
    free
}
/// The more informative of two unfinished-subtree statuses: a numerical
/// failure or an exhausted wall clock must not be hidden by a later subtree
/// that merely reached its node share.
fn worse(a: Status, b: Status) -> Status {
    let rank = |s: Status| match s {
        Status::NumericalFailure => 5,
        Status::Cancelled => 4,
        Status::TimeLimit => 3,
        Status::IterationLimit => 2,
        Status::NodeLimit => 1,
        _ => 0,
    };
    if rank(b) > rank(a) { b } else { a }
}
fn share(limit: Option<u64>, index: usize, count: usize) -> Option<u64> {
    limit.map(|v| v / (count as u64) + u64::from((index as u64) < v % (count as u64)))
}
pub(crate) fn solve(
    solver: &mut NativeSolver,
    model: &Model,
    options: &Options,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
    inherited: Option<f64>,
    hint: Option<&[f64]>,
) -> Result<Option<SolveResult>> {
    let settings = &solver.settings;
    if settings.tree_threads <= 1
        || settings.tree_split_depth == 0
        || cancel.is_cancelled()
        || settings.time_limit_seconds == Some(0.)
        || settings.node_limit == Some(0)
        || settings.iteration_limit == Some(0)
        || (0..model.n()).any(|j| {
            !model.kinds()[j].is_integral()
                || ![0., 1.].contains(&model.variables().lower[j])
                || ![0., 1.].contains(&model.variables().upper[j])
        })
    {
        return Ok(None);
    }
    let began = Instant::now();
    let columns = partition_columns(model, hint, settings.tree_split_depth);
    if columns.is_empty() {
        return Ok(None);
    }
    let count = 1usize << columns.len();
    if settings.node_limit.is_some_and(|n| n < (count as u64))
        || settings.iteration_limit.is_some_and(|n| n < (count as u64))
    {
        return Ok(None);
    }
    let workers = settings.tree_threads.min(count);
    let columns = &columns;
    // Subtrees differ a lot in size, so workers take the next one as they
    // finish. Shares depend on the subtree index and results are combined in
    // index order, so fixed-work outcomes do not depend on the schedule.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let next = &next;
    let outputs = std::thread::scope(|scope| -> Result<Vec<_>> {
        let mut handles = Vec::new();
        for _ in 0..workers {
            handles.push(
                std::thread::Builder::new()
                    .name("lpip-tree".into())
                    .spawn_scoped(scope, move || {
                        let mut out = Vec::new();
                        loop {
                            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if index >= count {
                                break;
                            }
                            let r = (|| -> Result<SolveResult> {
                                let mut controls = packing::child_settings(settings, began);
                                controls.node_limit = share(settings.node_limit, index, count);
                                controls.iteration_limit =
                                    share(settings.iteration_limit, index, count);
                                if cancel.is_cancelled() || controls.time_limit_seconds == Some(0.)
                                {
                                    return Ok(SolveResult::empty(
                                        if cancel.is_cancelled() {
                                            Status::Cancelled
                                        } else {
                                            Status::TimeLimit
                                        },
                                        "frontier job not started",
                                        BackendReport::new("native"),
                                    ));
                                }
                                // Allocate only one full private model per active worker.
                                let mut data = model.data().clone();
                                for (bit, &j) in columns.iter().enumerate() {
                                    let value = ((index >> bit) & 1) as f64;
                                    data.variables.lower[j] = value;
                                    data.variables.upper[j] = value;
                                }
                                let submodel = Model::new(data)?;
                                root::solve(
                                    &mut NativeSolver::new(controls),
                                    &submodel,
                                    options,
                                    start,
                                    cancel,
                                )
                            })();
                            out.push((index, r));
                        }
                        out
                    })
                    .map_err(|e| Error::Backend(format!("tree worker creation: {e}")))?,
            );
        }
        let mut out = Vec::new();
        for h in handles {
            out.extend(
                h.join()
                    .map_err(|_| Error::Backend("tree worker panicked".into()))?,
            );
        }
        Ok(out)
    })?;
    let outputs = crate::ordered::gather(count, outputs)?;
    if outputs.len() != count {
        return Err(Error::Backend("tree task output missing".into()));
    }
    let mut result = SolveResult::empty(
        Status::Unknown,
        "disjoint-frontier parallel native search",
        BackendReport::new("native"),
    );
    result.backend.version = Some(env!("CARGO_PKG_VERSION").into());
    result.backend.start_status = if start.is_some() {
        StartStatus::Evaluated
    } else {
        StartStatus::NotProvided
    };
    if let Some(s) = start {
        consider_candidate(model, options, &mut result, s.primal.clone())?;
    }
    let (mut bound, mut missing, mut closed, mut errors) = (None::<f64>, false, 0usize, 0usize);
    let mut terminal = Status::Unknown;
    let mut first_error = None;
    for (index, outcome) in outputs {
        match outcome {
            Err(e) => {
                errors += 1;
                first_error.get_or_insert_with(|| format!("subtree {index}: {e}"));
                result.nodes += share(settings.node_limit, index, count).unwrap_or(0);
                result.iterations += share(settings.iteration_limit, index, count).unwrap_or(0);
                missing |= inherited.is_none();
                if let Some(b) = inherited {
                    bound = Some(combine(model, bound, b));
                }
            }
            Ok(mut r) => {
                result.nodes += r.nodes;
                result.iterations += r.iterations;
                result.backend.conversion_seconds += r.backend.conversion_seconds;
                result.backend.solve_seconds += r.backend.solve_seconds;
                result.backend.check_seconds += r.backend.check_seconds;
                if let Some(x) = r.candidate.take() {
                    consider_candidate(model, options, &mut result, x)?;
                }
                if r.status == Status::Optimal || r.status == Status::Infeasible {
                    closed += 1;
                } else {
                    terminal = worse(terminal, r.status);
                }
                if r.status != Status::Infeasible {
                    match r.best_bound.or(inherited) {
                        Some(b) => bound = Some(combine(model, bound, b)),
                        None => missing = true,
                    }
                }
            }
        }
    }
    result.best_bound = if missing { None } else { bound };
    if let Some(b) = inherited {
        result.best_bound = Some(result.best_bound.map_or(b, |v| {
            if model.sense() == Sense::Maximize {
                v.min(b)
            } else {
                v.max(b)
            }
        }));
    }
    result.status = if errors > 0 {
        Status::NumericalFailure
    } else if cancel.is_cancelled() {
        Status::Cancelled
    } else if closed == count {
        if result.candidate.is_some() {
            Status::Optimal
        } else {
            Status::Infeasible
        }
    } else if settings
        .time_limit_seconds
        .is_some_and(|t| began.elapsed().as_secs_f64() >= t)
    {
        Status::TimeLimit
    } else {
        terminal
    };
    // A globally feasible start contradicts an all-infeasible frontier.
    if closed == count && result.candidate.is_some() && bound.is_none() {
        result.status = Status::NumericalFailure;
    }
    result.backend.effective_options = effective_options(settings);
    for (key, value) in [
        ("tree_subtrees", count),
        ("tree_workers", workers),
        ("tree_closed_subtrees", closed),
        ("tree_errors", errors),
    ] {
        result
            .backend
            .effective_options
            .push((format!("diagnostics.{key}"), value.to_string()));
    }
    result.backend.total_seconds = began.elapsed().as_secs_f64();
    result.backend.time_budget_exceeded = settings
        .time_limit_seconds
        .is_some_and(|t| result.backend.total_seconds > t);
    result.backend.raw_status = Some(result.status.as_str().into());
    result.message = format!(
        "Parallel disjoint binary frontier: {} (numerical status){}",
        result.status.as_str(),
        first_error.map_or_else(String::new, |e| format!("; first error in {e}"))
    );
    Ok(Some(result))
}
fn combine(model: &Model, bound: Option<f64>, value: f64) -> f64 {
    bound.map_or(value, |old| {
        if model.sense() == Sense::Maximize {
            old.max(value)
        } else {
            old.min(value)
        }
    })
}
fn consider_candidate(
    model: &Model,
    options: &Options,
    result: &mut SolveResult,
    x: Vec<f64>,
) -> Result<()> {
    let e = check::evaluate(model, &x, &options.tolerances)?;
    if e.feasible_candidate
        && result
            .evaluation
            .as_ref()
            .is_none_or(|old| oriented(model, e.objective) < oriented(model, old.objective))
    {
        result.candidate = Some(x);
        result.evaluation = Some(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_ip_craft::{CsrMatrix, ModelData, VariableKind};
    #[test]
    fn unfinished_status_keeps_the_most_informative_reason() {
        let all = [
            Status::Unknown,
            Status::NodeLimit,
            Status::IterationLimit,
            Status::TimeLimit,
            Status::Cancelled,
            Status::NumericalFailure,
        ];
        for (i, &a) in all.iter().enumerate() {
            for (j, &b) in all.iter().enumerate() {
                let expect = all[i.max(j)];
                assert_eq!(worse(a, b), expect);
                assert_eq!(worse(b, a), expect);
            }
        }
        // Frontier order no longer matters: a numerical failure in an early
        // subtree survives later node-limited ones.
        let order = [
            Status::NumericalFailure,
            Status::NodeLimit,
            Status::NodeLimit,
        ];
        assert_eq!(
            order.iter().fold(Status::Unknown, |t, &s| worse(t, s)),
            Status::NumericalFailure
        );
    }
    fn cycle(n: usize, extra: usize) -> Model {
        let mut a = vec![0.; n * (n + extra)];
        for i in 0..n {
            a[i * (n + extra) + i] = 1.;
            a[i * (n + extra) + (i + 1) % n] = 1.;
        }
        Model::new(ModelData {
            sense: Sense::Maximize,
            objective: vec![1.; n + extra],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n + extra], vec![1.; n + extra]),
            kinds: vec![VariableKind::Binary; n + extra],
            matrix: CsrMatrix::from_dense(n, n + extra, &a).unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY; n], vec![1.; n]),
        })
        .unwrap()
    }
    #[test]
    fn disjoint_frontier_covers_binary_points_once_and_closes_both_senses() {
        for n in [3, 5, 7] {
            for sense in [Sense::Maximize, Sense::Minimize] {
                let mut data = cycle(n, 0).into_data();
                data.sense = sense;
                if sense == Sense::Minimize {
                    data.objective.fill(-1.);
                }
                let m = Model::new(data).unwrap();
                let s = NativeOptions {
                    tree_threads: 4,
                    tree_split_depth: 2,
                    decompose: false,
                    ..Default::default()
                };
                let cols = partition_columns(&m, None, 2);
                let mut covered = vec![0usize; 1usize << n];
                for index in 0..1usize << cols.len() {
                    for (mask, count) in covered.iter_mut().enumerate() {
                        if cols
                            .iter()
                            .enumerate()
                            .all(|(bit, &j)| ((mask >> j) & 1) == ((index >> bit) & 1))
                        {
                            *count += 1;
                        }
                    }
                }
                assert!(covered.iter().all(|&v| v == 1));
                let r = NativeSolver::new(s)
                    .solve(&m, &Options::default(), None, &CancellationToken::default())
                    .unwrap();
                assert_eq!(r.status, Status::Optimal, "{r:?}");
                assert_eq!(r.evaluation.unwrap().objective.abs(), (n / 2) as f64);
                assert!(r.nodes > 0);
            }
        }
    }
    #[test]
    fn limited_frontier_keeps_safe_bounds_and_global_reservations() {
        let m = cycle(15, 0);
        for partial in [false, true] {
            let s = NativeOptions {
                tree_threads: 4,
                tree_split_depth: 2,
                node_limit: Some(8),
                iteration_limit: Some(4),
                partial_dual_bounds: partial,
                decompose: false,
                ..Default::default()
            };
            let seed = WarmStart::from_primal(vec![0.; m.n()]);
            let r = NativeSolver::new(s)
                .solve(
                    &m,
                    &Options::default(),
                    Some(&seed),
                    &CancellationToken::default(),
                )
                .unwrap();
            assert!(r.nodes <= 8);
            assert!(r.iterations <= 4);
            assert!(r.evaluation.unwrap().feasible_candidate);
            if let Some(b) = r.best_bound {
                assert!(b >= 7.);
            }
            if !partial {
                assert!(r.best_bound.is_none());
            }
        }
    }
    #[test]
    fn scalable_probes_activate_above_the_old_gate_and_obey_work_limits() {
        let mut data = cycle(3, 1030).into_data();
        data.objective.fill(10.);
        let m = Model::new(data).unwrap();
        for (scalable, work) in [(false, 2_000_000), (true, 2_000_000), (true, 0)] {
            let s = NativeOptions {
                reliability_branching: true,
                scalable_branching: scalable,
                branch_probe_work: work,
                decompose: false,
                ..Default::default()
            };
            let r = NativeSolver::new(s)
                .solve(&m, &Options::default(), None, &CancellationToken::default())
                .unwrap();
            assert_eq!(r.status, Status::Optimal);
            assert_eq!(r.evaluation.unwrap().objective, 10310.);
            let meta: std::collections::BTreeMap<_, _> =
                r.backend.effective_options.into_iter().collect();
            let probes: u64 = meta["diagnostics.strong_branch_probes"].parse().unwrap();
            let reserved: u64 = meta["diagnostics.strong_branch_work"].parse().unwrap();
            assert!(reserved <= work);
            if scalable && work > 0 {
                assert!(probes > 0);
            } else {
                assert_eq!(probes, 0);
            }
        }
    }
    #[test]
    fn zero_limits_and_precancel_preserve_the_serial_contract() {
        let m = cycle(5, 0);
        let token = CancellationToken::default();
        token.cancel();
        for s in [
            NativeOptions {
                tree_threads: 4,
                ..Default::default()
            },
            NativeOptions {
                packing_root: true,
                packing_workers: 4,
                packing_neighborhoods: 8,
                tree_threads: 4,
                ..Default::default()
            },
        ] {
            let r = NativeSolver::new(s)
                .solve(&m, &Options::default(), None, &token)
                .unwrap();
            assert_eq!(r.status, Status::Cancelled);
            assert_eq!(r.iterations, 0);
        }
    }
}
