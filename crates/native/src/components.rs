//! Split independent restricted-row components; each is solved with private numerical state.
use crate::{NativeOptions, NativeSolver, accepted, incumbent, oriented};
use lp_ip_craft::*;
use std::time::Instant;

struct Part {
    columns: Vec<usize>,
    rows: Vec<usize>,
    model: Model,
    start: Option<WarmStart>,
}
fn root(parent: &mut [usize], mut j: usize) -> usize {
    while parent[j] != j {
        parent[j] = parent[parent[j]];
        j = parent[j];
    }
    j
}
fn split(
    model: &Model,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
    avoid_dominant: bool,
) -> Result<Option<Vec<Part>>> {
    let mut parent: Vec<_> = (0..model.n()).collect();
    let mut ranks = vec![0; model.n()];
    let mut groups = model.n();
    let matrix = model.matrix();
    let restricted =
        |i: usize| model.rows().lower[i].is_finite() || model.rows().upper[i].is_finite();
    for i in 0..model.m() {
        if i % 256 == 0 && cancel.is_cancelled() {
            return Ok(None);
        }
        if !restricted(i) {
            continue;
        }
        let mut first = None;
        for k in matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1] {
            if matrix.values[k] == 0. {
                continue;
            }
            let j = matrix.pattern.indices[k];
            if let Some(a) = first {
                let (mut a, mut b) = (root(&mut parent, a), root(&mut parent, j));
                if a != b {
                    if ranks[a] < ranks[b] {
                        std::mem::swap(&mut a, &mut b);
                    }
                    parent[b] = a;
                    groups -= 1;
                    if groups == 1 {
                        return Ok(None);
                    }
                    if ranks[a] == ranks[b] {
                        ranks[a] += 1;
                    }
                }
            } else {
                first = Some(j);
            }
        }
        if first.is_none() && !(model.rows().lower[i] <= 0. && model.rows().upper[i] >= 0.) {
            return Ok(None);
        }
    }
    let mut index = vec![usize::MAX; model.n()];
    let mut columns: Vec<Vec<usize>> = Vec::new();
    let mut owner = vec![0; model.n()];
    let mut local = vec![0; model.n()];
    for j in 0..model.n() {
        let r = root(&mut parent, j);
        if index[r] == usize::MAX {
            index[r] = columns.len();
            columns.push(Vec::new());
        }
        let p = index[r];
        owner[j] = p;
        local[j] = columns[p].len();
        columns[p].push(j);
    }
    if columns.len() < 2 || columns.iter().all(|c| c.len() == 1) {
        return Ok(None);
    }
    if avoid_dominant
        && model.n() >= 512
        && columns
            .iter()
            .any(|c| c.len().saturating_mul(10) >= model.n().saturating_mul(9))
    {
        return Ok(None);
    }
    let mut rows = vec![Vec::new(); columns.len()];
    for i in 0..model.m() {
        if !restricted(i) {
            continue;
        }
        if let Some(k) = (matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1])
            .find(|&k| matrix.values[k] != 0.)
        {
            rows[owner[matrix.pattern.indices[k]]].push(i);
        }
    }
    let mut parts = Vec::with_capacity(columns.len());
    for (columns, rows) in columns.into_iter().zip(rows) {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let mut indptr = vec![0];
        let mut indices = Vec::new();
        let mut values = Vec::new();
        for &i in &rows {
            for k in matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1] {
                if matrix.values[k] != 0. {
                    indices.push(local[matrix.pattern.indices[k]]);
                    values.push(matrix.values[k]);
                }
            }
            indptr.push(indices.len());
        }
        let child = Model::new(ModelData {
            sense: model.sense(),
            objective: columns.iter().map(|&j| model.objective()[j]).collect(),
            objective_offset: 0.,
            variables: Bounds::new(
                columns
                    .iter()
                    .map(|&j| model.variables().lower[j])
                    .collect(),
                columns
                    .iter()
                    .map(|&j| model.variables().upper[j])
                    .collect(),
            ),
            kinds: columns.iter().map(|&j| model.kinds()[j]).collect(),
            matrix: CsrMatrix::new(rows.len(), columns.len(), indptr, indices, values),
            rows: Bounds::new(
                rows.iter().map(|&i| model.rows().lower[i]).collect(),
                rows.iter().map(|&i| model.rows().upper[i]).collect(),
            ),
        })?;
        let warm =
            start.map(|s| WarmStart::from_primal(columns.iter().map(|&j| s.primal[j]).collect()));
        parts.push(Part {
            columns,
            rows,
            model: child,
            start: warm,
        });
    }
    Ok(Some(parts))
}

fn quota(limit: Option<u64>, index: usize, count: usize) -> Option<u64> {
    limit.map(|n| n / count as u64 + u64::from((index as u64) < n % count as u64))
}
fn controls(settings: &NativeOptions, started: Instant) -> NativeOptions {
    let mut c = settings.clone();
    // A child trace is not a global tree trace. The aggregate reports its
    // explicit scope instead of computing and silently discarding events.
    c.mip_diagnostics = false;
    c.decompose = false;
    c.packing_workers = 1;
    c.tree_threads = 1;
    c.time_limit_seconds = settings
        .time_limit_seconds
        .map(|t| (t - started.elapsed().as_secs_f64()).max(0.));
    c
}
fn worker_count(parts: &[Part], settings: &NativeOptions) -> usize {
    let size = parts
        .iter()
        .map(|p| p.model.nnz() + p.model.n() + p.model.m())
        .fold(0usize, usize::saturating_add);
    if size / parts.len() < settings.component_parallel_min_entries {
        1
    } else {
        settings.component_threads.min(parts.len())
    }
}
fn run_parts(
    parts: &[Part],
    settings: &NativeOptions,
    options: &Options,
    cancel: &CancellationToken,
    started: Instant,
) -> Result<(Vec<Option<SolveResult>>, usize)> {
    let workers = worker_count(parts, settings);
    if workers == 1 {
        let mut slots = vec![None; parts.len()];
        let (mut nodes, mut pivots) = (0u64, 0u64);
        for (i, part) in parts.iter().enumerate() {
            let mut c = controls(settings, started);
            c.node_limit = settings.node_limit.map(|n| n.saturating_sub(nodes));
            c.iteration_limit = settings.iteration_limit.map(|n| n.saturating_sub(pivots));
            let r =
                NativeSolver::new(c).solve(&part.model, options, part.start.as_ref(), cancel)?;
            nodes += r.nodes;
            pivots += r.iterations;
            let status = r.status;
            slots[i] = Some(r);
            if !matches!(status, Status::Optimal | Status::Unbounded) {
                break;
            }
        }
        return Ok((slots, workers));
    }
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let mut order: Vec<_> = (0..parts.len()).collect();
    // Work reservations remain tied to the original part, never queue order.
    if settings.node_limit.is_none() && settings.iteration_limit.is_none() {
        order.sort_by_key(|&i| {
            std::cmp::Reverse(
                parts[i]
                    .model
                    .nnz()
                    .saturating_add(parts[i].model.n())
                    .saturating_add(parts[i].model.m()),
            )
        });
    }
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let results = Mutex::new(vec![None; parts.len()]);
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let handle = std::thread::Builder::new()
                .spawn_scoped(scope, || -> Result<()> {
                    loop {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= parts.len() {
                            break;
                        }
                        let i = order[i];
                        let part = &parts[i];
                        let mut c = controls(settings, started);
                        c.node_limit = quota(settings.node_limit, i, parts.len());
                        c.iteration_limit = quota(settings.iteration_limit, i, parts.len());
                        let r = NativeSolver::new(c).solve(
                            &part.model,
                            options,
                            part.start.as_ref(),
                            cancel,
                        );
                        if !r
                            .as_ref()
                            .is_ok_and(|r| matches!(r.status, Status::Optimal | Status::Unbounded))
                        {
                            stop.store(true, Ordering::Relaxed);
                        }
                        results.lock().map_err(|_| {
                            Error::Backend("component result lock poisoned".into())
                        })?[i] = Some(r);
                    }
                    Ok(())
                })
                .map_err(|e| Error::Backend(format!("component worker creation failed: {e}")))?;
            handles.push(handle);
        }
        for h in handles {
            h.join()
                .map_err(|_| Error::Backend("component worker panicked".into()))??;
        }
        Ok(())
    })?;
    let slots = results
        .into_inner()
        .map_err(|_| Error::Backend("component result lock poisoned".into()))?
        .into_iter()
        .map(|r| r.transpose())
        .collect::<Result<Vec<_>>>()?;
    Ok((slots, workers))
}

pub(crate) fn solve_if_separable(
    settings: &NativeOptions,
    model: &Model,
    options: &Options,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
    started: Instant,
) -> Result<Option<SolveResult>> {
    let Some(parts) = split(model, start, cancel, settings.avoid_dominant_components)? else {
        return Ok(None);
    };
    let conversion = started.elapsed().as_secs_f64();
    let (slots, workers) = run_parts(&parts, settings, options, cancel, started)?;
    let results: Vec<_> = slots.iter().flatten().collect();
    let complete = results.len() == parts.len();
    let nodes = results.iter().map(|r| r.nodes).sum();
    let pivots = results.iter().map(|r| r.iterations).sum();
    let mut status = results
        .iter()
        .find(|r| !matches!(r.status, Status::Optimal | Status::Unbounded))
        .map_or(Status::Optimal, |r| r.status);
    if complete
        && status == Status::Optimal
        && results.iter().any(|r| r.status == Status::Unbounded)
    {
        status = Status::Unbounded;
    }
    if !complete && status == Status::Optimal {
        status = Status::Unknown;
    }
    let mut report = BackendReport::new("native");
    report.version = Some(env!("CARGO_PKG_VERSION").into());
    report.start_status = if start.is_none() {
        StartStatus::NotProvided
    } else if model.is_mip() {
        StartStatus::Evaluated
    } else {
        StartStatus::Rejected
    };
    if let Some(r) = results.first() {
        report.effective_options = r
            .backend
            .effective_options
            .iter()
            .filter(|(k, _)| !k.starts_with("diagnostics."))
            .cloned()
            .collect();
    }
    for (k, v) in &mut report.effective_options {
        if k == "decompose" {
            *v = "true".into();
        }
    }
    report.effective_options.retain(|(k, _)| k != "algorithm");
    report
        .effective_options
        .push(("algorithm".into(), "native_component_decomposition".into()));
    for (key, value) in [
        ("node_limit", format!("{:?}", settings.node_limit)),
        ("iteration_limit", format!("{:?}", settings.iteration_limit)),
        (
            "time_limit_seconds",
            format!("{:?}", settings.time_limit_seconds),
        ),
    ] {
        if let Some((_, v)) = report.effective_options.iter_mut().find(|(k, _)| k == key) {
            *v = value;
        }
    }
    report.effective_options.push((
        "component_budget_mode".into(),
        if workers == 1 {
            "sequential_shared"
        } else {
            "parallel_reserved"
        }
        .into(),
    ));
    report.effective_options.push((
        "component_timing_mode".into(),
        "worker_duration_sums".into(),
    ));
    let mut counters = std::collections::BTreeMap::<String, u64>::new();
    for r in &results {
        report.conversion_seconds += r.backend.conversion_seconds;
        report.solve_seconds += r.backend.solve_seconds;
        report.check_seconds += r.backend.check_seconds;
        for (k, v) in &r.backend.effective_options {
            if k.starts_with("diagnostics.")
                && k != "diagnostics.component_count"
                && k != "diagnostics.component_workers"
            {
                if let Ok(v) = v.parse::<u64>() {
                    *counters.entry(k.clone()).or_default() += v;
                }
            }
        }
    }
    report.conversion_seconds += conversion;
    report
        .effective_options
        .extend(counters.into_iter().map(|(k, v)| (k, v.to_string())));
    report.effective_options.extend([
        (
            "diagnostics.component_count".into(),
            parts.len().to_string(),
        ),
        ("diagnostics.component_workers".into(), workers.to_string()),
        (
            "diagnostics.objective_lattice".into(),
            (settings.objective_lattice && incumbent::integer_lattice(model)).to_string(),
        ),
    ]);
    if settings.mip_diagnostics && model.is_mip() {
        for (key, value) in &mut report.effective_options {
            if key == "mip_diagnostics" {
                *value = "true".into();
            }
        }
        report.effective_options.push(("diagnostics.mip_search".into(),
            format!("{{\"schema_version\":1,\"scope\":\"component_aggregate\",\"components\":{},\"workers\":{},\"events\":[],\"note\":\"no global serial search trace; use decompose=false for diagnosis\"}}", parts.len(), workers)));
    }
    let mut out = SolveResult::empty(status, "", report);
    out.nodes = nodes;
    out.iterations = pivots;
    let checks = Instant::now();
    if complete && status == Status::Optimal {
        let mut x = vec![0.; model.n()];
        let mut y = vec![0.; model.m()];
        for (part, result) in parts.iter().zip(&results) {
            let Some(point) = &result.candidate else {
                return Err(Error::Backend("optimal component has no candidate".into()));
            };
            for (&j, &v) in part.columns.iter().zip(point) {
                x[j] = v;
            }
            if let Some(duals) = &result.duals {
                for (&i, &v) in part.rows.iter().zip(&duals.row_duals) {
                    y[i] = v;
                }
            }
        }
        let evaluation = check::evaluate(model, &x, &options.tolerances)?;
        if !evaluation.feasible_candidate {
            status = Status::NumericalFailure;
        }
        if !model.is_mip() {
            let mut at = vec![0.; model.n()];
            model.matrix().multiply_transpose(&y, &mut at);
            let duals = Duals {
                row_duals: y,
                reduced_costs: model
                    .objective()
                    .iter()
                    .zip(at)
                    .map(|(&c, a)| c - a)
                    .collect(),
            };
            if !accepted(model, model.variables(), &x, &duals, options, settings) {
                status = Status::NumericalFailure;
            }
            out.best_bound = safe_dual_bound(model, &duals.row_duals)?.bound;
            out.duals = Some(duals);
        } else if crate::presolve::supported_arithmetic()
            && results.iter().all(|r| r.best_bound.is_some())
        {
            out.best_bound = incumbent::dot(
                results
                    .iter()
                    .map(|r| (oriented(model, r.best_bound.unwrap()), 1.)),
                oriented(model, model.objective_offset()),
            )
            .map(|v| oriented(model, v.0));
        }
        out.candidate = Some(x);
        out.evaluation = Some(evaluation);
    } else if model.is_mip() && status != Status::Infeasible {
        if let Some(s) = start {
            let e = check::evaluate(model, &s.primal, &options.tolerances)?;
            if e.feasible_candidate {
                out.candidate = Some(s.primal.clone());
                out.evaluation = Some(e);
            }
        }
        out.best_bound = safe_dual_bound(model, &vec![0.; model.m()])?.bound;
    }
    if status == Status::Optimal {
        if let (Some(e), Some(b)) = (&out.evaluation, out.best_bound) {
            if oriented(model, e.objective) - oriented(model, b)
                < -settings.dual_tolerance * (1. + e.objective.abs())
            {
                status = Status::NumericalFailure;
            }
        }
    }
    out.status = status;
    out.message = format!(
        "Native {}-component numerical solve: {} (not an exact certificate)",
        parts.len(),
        status.as_str()
    );
    out.backend.raw_status = Some(status.as_str().into());
    out.backend.check_seconds += checks.elapsed().as_secs_f64();
    out.backend.total_seconds = started.elapsed().as_secs_f64();
    out.backend.time_budget_exceeded = settings
        .time_limit_seconds
        .is_some_and(|t| out.backend.total_seconds > t);
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservations_never_exceed_the_global_budget() {
        for count in 1..=16 {
            for limit in [0, 1, 17, u64::MAX] {
                let values: Vec<_> = (0..count)
                    .map(|i| quota(Some(limit), i, count).unwrap())
                    .collect();
                assert_eq!(values.iter().copied().sum::<u64>(), limit);
                assert!(values.iter().max().unwrap() - values.iter().min().unwrap() <= 1);
            }
        }
    }
}
