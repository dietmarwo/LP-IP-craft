//! Opt-in unit-packing root relaxation and deterministic residual search.
//! Approximate points guide search only; bounds use the independent core.
use crate::*;
use lp_ip_craft::{CsrMatrix, ModelData, VariableKind, safe_dual_bound};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) struct Packing {
    pub rows: Vec<Vec<usize>>,
    pub cols: Vec<Vec<usize>>,
    pub weights: Vec<f64>,
}
impl Packing {
    pub fn new(model: &Model) -> Option<Self> {
        if model.sense() != Sense::Maximize
            || !model.is_mip()
            || model.objective().iter().any(|v| *v < 0. || !v.is_finite())
            || (0..model.n()).any(|j| {
                !model.kinds()[j].is_integral()
                    || ![0., 1.].contains(&model.variables().lower[j])
                    || ![0., 1.].contains(&model.variables().upper[j])
            })
        {
            return None;
        }
        let a = model.matrix();
        let mut rows = vec![Vec::new(); model.m()];
        let mut cols = vec![Vec::new(); model.n()];
        for (i, row) in rows.iter_mut().enumerate() {
            if model.rows().lower[i] > 0. || model.rows().upper[i] != 1. {
                return None;
            }
            for k in a.pattern.indptr[i]..a.pattern.indptr[i + 1] {
                if a.values[k] == 0. {
                    continue;
                }
                if a.values[k] != 1. {
                    return None;
                }
                let j = a.pattern.indices[k];
                row.push(j);
                cols[j].push(i);
            }
        }
        Some(Self {
            rows,
            cols,
            weights: model.objective().to_vec(),
        })
    }
    fn conflicts(&self, a: usize, b: usize) -> bool {
        self.cols[a].iter().any(|r| self.cols[b].contains(r))
    }
    fn owners(&self, x: &[f64]) -> Vec<Option<usize>> {
        let mut owner = vec![None; self.rows.len()];
        for (j, &v) in x.iter().enumerate() {
            if v > 0.5 {
                for &i in &self.cols[j] {
                    owner[i] = Some(j);
                }
            }
        }
        owner
    }
    fn greedy(&self, model: &Model, hint: Option<&[f64]>) -> Option<Vec<f64>> {
        let mut x = model.variables().lower.clone();
        let mut used = vec![false; model.m()];
        for (j, &v) in x.iter().enumerate() {
            if v == 1. {
                if self.cols[j].iter().any(|&i| used[i]) {
                    return None;
                }
                for &i in &self.cols[j] {
                    used[i] = true;
                }
            }
        }
        let score = |j: usize| self.weights[j] * hint.map_or(1., |h| 0.25 + h[j]);
        let mut order: Vec<_> = (0..model.n()).collect();
        order.sort_by(|&a, &b| score(b).total_cmp(&score(a)).then(a.cmp(&b)));
        for j in order {
            if x[j] == 0.
                && model.variables().upper[j] == 1.
                && self.weights[j] > 0.
                && self.cols[j].iter().all(|&i| !used[i])
            {
                x[j] = 1.;
                for &i in &self.cols[j] {
                    used[i] = true;
                }
            }
        }
        Some(x)
    }
}
fn stopped(settings: &NativeOptions, cancel: &CancellationToken, began: Instant) -> bool {
    cancel.is_cancelled()
        || settings
            .time_limit_seconds
            .is_some_and(|t| began.elapsed().as_secs_f64() >= t)
}
fn tighten(model: &Model, bound: f64, lattice: bool) -> f64 {
    if lattice && incumbent::integer_lattice(model) {
        bound.floor()
    } else {
        bound
    }
}
fn checked_bound(model: &Model, prices: &[f64], lattice: bool) -> Result<Option<f64>> {
    Ok(safe_dual_bound(model, prices)?
        .bound
        .map(|v| tighten(model, v, lattice)))
}
fn improve(
    model: &Model,
    options: &Options,
    best: &mut Option<Vec<f64>>,
    point: Vec<f64>,
) -> Result<bool> {
    let e = check::evaluate(model, &point, &options.tolerances)?;
    // A heuristic may be used for search, but only a conservatively feasible
    // candidate is retained here (and therefore never an invalid pruning seed).
    if !e.feasible_candidate || incumbent::upper(model, &point).is_none() {
        return Ok(false);
    }
    let old = best.as_ref().map(|x| {
        model.objective_offset()
            + model
                .objective()
                .iter()
                .zip(x)
                .map(|(c, v)| c * v)
                .sum::<f64>()
    });
    if old.is_none_or(|v| e.objective > v) {
        *best = Some(point);
        Ok(true)
    } else {
        Ok(false)
    }
}
#[derive(Default)]
struct RootStats {
    iterations: u64,
    work: u64,
}
fn relaxation(
    pack: &Packing,
    model: &Model,
    settings: &NativeOptions,
    cancel: &CancellationToken,
    began: Instant,
    seed: &[f64],
) -> Result<(Vec<f64>, Option<f64>, RootStats)> {
    let mut stats = RootStats::default();
    let mut initial = vec![0f64; model.m()];
    // Assign each column to one incident row: its maximum weight is a dual
    // cover. Uncovered columns are handled by the variable-bound residual.
    for j in 0..model.n() {
        if let Some(&i) = pack.cols[j].first() {
            initial[i] = initial[i].max(pack.weights[j]);
        }
    }
    let mut bound = checked_bound(model, &initial, settings.objective_lattice)?;
    let scale = pack.weights.iter().copied().fold(1., f64::max);
    let max_row = pack.rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let max_col = pack.cols.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let step = 0.9 / ((max_row as f64) * (max_col as f64)).sqrt();
    let mut x = seed.to_vec();
    let mut avg = vec![0.; model.n()];
    let mut y = vec![0.; model.m()];
    let mut avg_y = y.clone();
    let mut grad = vec![0.; model.n()];
    let mut extrapolated = x.clone();
    let step_work = (2 * model.nnz() + 4 * model.n() + 4 * model.m()) as u64;
    for iteration in 0..if settings.packing_root {
        settings.packing_root_iterations
    } else {
        0
    } {
        if stopped(settings, cancel, began)
            || step_work > settings.packing_root_work.saturating_sub(stats.work)
        {
            break;
        }
        grad.fill(0.);
        for (i, row) in pack.rows.iter().enumerate() {
            for &j in row {
                grad[j] += y[i];
            }
        }
        for j in 0..model.n() {
            let next = (x[j] + step * (pack.weights[j] / scale - grad[j]))
                .clamp(model.variables().lower[j], model.variables().upper[j]);
            extrapolated[j] = 2. * next - x[j];
            x[j] = next;
            avg[j] += (next - avg[j]) / ((iteration + 1) as f64);
        }
        for (i, row) in pack.rows.iter().enumerate() {
            let activity: f64 = row.iter().map(|&j| extrapolated[j]).sum();
            y[i] = (y[i] + step * (activity - 1.)).max(0.);
            avg_y[i] += (y[i] - avg_y[i]) / ((iteration + 1) as f64);
        }
        stats.iterations += 1;
        stats.work += step_work;
        if iteration % 32 == 31 || iteration + 1 == settings.packing_root_iterations {
            for prices in [&y, &avg_y] {
                let prices: Vec<_> = prices.iter().map(|v| v * scale).collect();
                if prices.iter().all(|v| v.is_finite()) {
                    if let Some(b) = checked_bound(model, &prices, settings.objective_lattice)? {
                        bound = Some(bound.map_or(b, |old| old.min(b)));
                    }
                }
            }
        }
    }
    if stats.iterations == 0 {
        avg = x;
    }
    Ok((avg, bound, stats))
}

fn neighborhood(
    pack: &Packing,
    model: &Model,
    incumbent: &[f64],
    hint: &[f64],
    task: usize,
    size: usize,
) -> Result<Option<(Model, Vec<usize>, WarmStart)>> {
    let owner = pack.owners(incumbent);
    let selected: Vec<_> = (0..model.n())
        .filter(|&j| incumbent[j] > 0.5 && model.variables().lower[j] < 1.)
        .collect();
    if selected.is_empty() {
        return Ok(None);
    }
    let center = selected[(task.wrapping_mul(104729).wrapping_add(task / 7)) % selected.len()];
    let mut released = vec![false; model.n()];
    let mut queue = VecDeque::from([center]);
    released[center] = true;
    let mut count = 1;
    let target = (size / 6).max(1);
    while let Some(j) = queue.pop_front() {
        for &i in &pack.cols[j] {
            for &other in &pack.rows[i] {
                for &r in &pack.cols[other] {
                    if let Some(k) = owner[r] {
                        if !released[k] && model.variables().lower[k] < 1. && count < target {
                            released[k] = true;
                            count += 1;
                            queue.push_back(k);
                        }
                    }
                }
            }
        }
        if count >= target {
            break;
        }
    }
    let mut columns: Vec<_> = (0..model.n()).filter(|&j| released[j]).collect();
    let mut available: Vec<_> = (0..model.n())
        .filter(|&j| {
            !released[j]
                && incumbent[j] < 0.5
                && model.variables().upper[j] > 0.
                && pack.cols[j]
                    .iter()
                    .all(|&i| owner[i].is_none_or(|k| released[k]))
        })
        .collect();
    available.sort_by(|&a, &b| {
        (pack.weights[b] * (0.25 + hint[b]))
            .total_cmp(&(pack.weights[a] * (0.25 + hint[a])))
            .then(a.cmp(&b))
    });
    available.truncate(size.saturating_sub(columns.len()));
    columns.extend(available);
    columns.sort_unstable();
    if columns.is_empty() {
        return Ok(None);
    }
    let mut rows: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (local, &j) in columns.iter().enumerate() {
        for &i in &pack.cols[j] {
            rows.entry(i).or_default().push(local);
        }
    }
    let mut indptr = vec![0];
    let mut indices = Vec::new();
    for row in rows.values() {
        indices.extend(row);
        indptr.push(indices.len());
    }
    let residual = Model::new(ModelData {
        sense: Sense::Maximize,
        objective: columns.iter().map(|&j| pack.weights[j]).collect(),
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
        kinds: vec![VariableKind::Binary; columns.len()],
        matrix: CsrMatrix {
            pattern: lp_ip_craft::CsrPattern {
                rows: rows.len(),
                cols: columns.len(),
                indptr,
                indices,
            },
            values: vec![1.; rows.values().map(Vec::len).sum()],
        },
        rows: Bounds::new(vec![f64::NEG_INFINITY; rows.len()], vec![1.; rows.len()]),
    })?;
    let start = WarmStart::from_primal(columns.iter().map(|&j| incumbent[j]).collect());
    Ok(Some((residual, columns, start)))
}
#[derive(Default)]
struct LnsStats {
    tasks: u64,
    improvements: u64,
    errors: u64,
    nodes: u64,
    pivots: u64,
    workers: usize,
}
#[allow(clippy::too_many_arguments)]
fn residual_search(
    pack: &Packing,
    model: &Model,
    options: &Options,
    settings: &NativeOptions,
    cancel: &CancellationToken,
    began: Instant,
    best: &mut Option<Vec<f64>>,
    hint: &[f64],
) -> Result<LnsStats> {
    let mut stats = LnsStats::default();
    const BATCH: usize = 16; // independent of worker count: fixed-work reproducibility
    for base in (0..settings.packing_neighborhoods).step_by(BATCH) {
        if stopped(settings, cancel, began) || best.is_none() {
            break;
        }
        let remaining = settings.packing_neighborhoods - base;
        let count = remaining.min(BATCH);
        let nodes = settings
            .node_limit
            .unwrap_or(u64::MAX)
            .saturating_sub(stats.nodes);
        let pivots = settings
            .iteration_limit
            .unwrap_or(u64::MAX)
            .saturating_sub(stats.pivots);
        let node_share = (nodes / (remaining as u64)).min(settings.packing_neighborhood_nodes);
        let pivot_share = (pivots / (remaining as u64)).min(4096);
        if node_share == 0 || pivot_share == 0 {
            break;
        }
        let snapshot = best.as_ref().unwrap().clone();
        let incumbent = &snapshot;
        let workers = settings.packing_workers.min(count);
        stats.workers = stats.workers.max(workers);
        let mut results = std::thread::scope(|scope| -> Result<Vec<_>> {
            let mut handles = Vec::new();
            for worker in 0..workers {
                handles.push(
                    std::thread::Builder::new()
                        .name("lpip-residual".into())
                        .spawn_scoped(scope, move || {
                            let mut out = Vec::new();
                            for at in (worker..count).step_by(workers) {
                                if stopped(settings, cancel, began) {
                                    break;
                                }
                                let result = (|| -> Result<Option<_>> {
                                    let Some((residual, columns, start)) = neighborhood(
                                        pack,
                                        model,
                                        incumbent,
                                        hint,
                                        base + at,
                                        settings.packing_neighborhood_size,
                                    )?
                                    else {
                                        return Ok(None);
                                    };
                                    let mut controls = child_settings(settings, began);
                                    controls.node_limit = Some(node_share);
                                    controls.iteration_limit = Some(pivot_share);
                                    controls.reliability_branching = true;
                                    controls.scalable_branching = true;
                                    let r = NativeSolver::new(controls).solve(
                                        &residual,
                                        options,
                                        Some(&start),
                                        cancel,
                                    )?;
                                    Ok(Some((columns, r)))
                                })();
                                out.push((at, result));
                            }
                            out
                        })
                        .map_err(|e| Error::Backend(format!("residual worker creation: {e}")))?,
                );
            }
            let mut out = Vec::new();
            for h in handles {
                out.extend(
                    h.join()
                        .map_err(|_| Error::Backend("residual worker panicked".into()))?,
                );
            }
            Ok(out)
        })?;
        results.sort_by_key(|v| v.0);
        for (_, result) in results {
            stats.tasks += 1;
            match result {
                Ok(Some((columns, r))) => {
                    stats.nodes += r.nodes;
                    stats.pivots += r.iterations;
                    if let Some(x) = r.candidate {
                        let mut point = incumbent.to_vec();
                        for (&j, &v) in columns.iter().zip(&x) {
                            point[j] = v;
                        }
                        if improve(model, options, best, point)? {
                            stats.improvements += 1;
                        }
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    stats.errors += 1;
                    stats.nodes += node_share;
                    stats.pivots += pivot_share;
                }
            }
        }
    }
    Ok(stats)
}

pub(crate) fn child_settings(settings: &NativeOptions, began: Instant) -> NativeOptions {
    let mut s = settings.clone();
    s.packing_root = false;
    s.packing_neighborhoods = 0;
    s.packing_cuts = false;
    s.tree_threads = 1;
    s.component_threads = 1;
    s.decompose = false;
    s.mip_diagnostics = false;
    if let Some(t) = s.time_limit_seconds {
        s.time_limit_seconds = Some((t - began.elapsed().as_secs_f64()).max(0.));
    }
    s
}

struct Separator<'a> {
    pack: &'a Packing,
    hint: &'a [f64],
    settings: &'a NativeOptions,
    cancel: &'a CancellationToken,
    began: Instant,
    cache: Vec<Option<Vec<usize>>>,
    work: u64,
    output: Vec<Vec<usize>>,
    seen: BTreeSet<Vec<usize>>,
}
impl Separator<'_> {
    fn stop(&self) -> bool {
        self.work >= self.settings.packing_cut_work
            || self.output.len() >= 64
            || stopped(self.settings, self.cancel, self.began)
    }
    fn neighbors(&mut self, j: usize) -> Vec<usize> {
        if let Some(list) = &self.cache[j] {
            return list.clone();
        }
        let mut out = Vec::new();
        for &i in &self.pack.cols[j] {
            for &k in &self.pack.rows[i] {
                if self.stop() {
                    break;
                }
                self.work += 1;
                if k != j {
                    out.push(k);
                }
            }
            if self.stop() {
                break;
            }
        }
        out.sort_unstable();
        out.dedup();
        self.cache[j] = Some(out.clone());
        out
    }
    fn consider(&mut self, path: &[usize]) {
        if path.len() < 3 || path.len() % 2 == 0 {
            return;
        }
        let rhs = (path.len() / 2) as f64;
        let valid =
            (0..path.len()).all(|k| self.pack.conflicts(path[k], path[(k + 1) % path.len()]));
        let dominated = self.pack.cols[path[0]]
            .iter()
            .any(|i| path.iter().all(|&j| self.pack.cols[j].contains(i)));
        let mut key = path.to_vec();
        key.sort_unstable();
        if key.windows(2).any(|p| p[0] == p[1]) {
            return;
        }
        if valid
            && !dominated
            && path.iter().map(|&j| self.hint[j]).sum::<f64>() > rhs + 1e-6
            && self.seen.insert(key.clone())
        {
            self.output.push(key);
        }
    }
    fn visit(&mut self, path: &mut Vec<usize>) {
        if self.stop() {
            return;
        }
        let first = path[0];
        let last = *path.last().unwrap();
        let cost: f64 = path
            .windows(2)
            .map(|e| (1. - self.hint[e[0]] - self.hint[e[1]]).max(0.))
            .sum();
        if cost >= 1. - 1e-7 {
            return;
        }
        if path.len() >= 3 && self.pack.conflicts(first, last) {
            self.consider(path);
            return;
        }
        if path.len() == 7 {
            return;
        }
        for next in self.neighbors(last) {
            if self.stop() {
                break;
            }
            self.work += 1;
            if next <= first || path.contains(&next) {
                continue;
            }
            // Only chordless cycles; chords yield shorter candidates instead.
            if path.len() > 2
                && path[1..path.len() - 1]
                    .iter()
                    .any(|&j| self.pack.conflicts(j, next))
            {
                continue;
            }
            path.push(next);
            self.visit(path);
            path.pop();
        }
    }
}
fn add_cuts(
    pack: &Packing,
    model: &Model,
    hint: &[f64],
    settings: &NativeOptions,
    cancel: &CancellationToken,
    began: Instant,
) -> Result<(Model, u64, u64, u64)> {
    if pack.cols.iter().any(|c| c.len() > 64) {
        return Ok((model.clone(), 0, 0, 0));
    }
    let mut sep = Separator {
        pack,
        hint,
        settings,
        cancel,
        began,
        cache: vec![None; model.n()],
        work: 0,
        output: Vec::new(),
        seen: BTreeSet::new(),
    };
    let mut order: Vec<_> = (0..model.n()).collect();
    order.sort_by(|&a, &b| hint[b].total_cmp(&hint[a]).then(a.cmp(&b)));
    // Spend at most half the search budget on a broad triangle pass before
    // exploring longer cycles. Deep DFS at the first vertex must not consume
    // every visit before any other part of the graph is inspected.
    let triangle_cap = settings.packing_cut_work / 2;
    for &first in &order {
        if sep.stop() || sep.work >= triangle_cap || hint[first] <= 1. / 3. {
            break;
        }
        let list = sep.neighbors(first);
        for (at, &a) in list.iter().enumerate() {
            for &b in &list[at + 1..] {
                if sep.stop() || sep.work >= triangle_cap {
                    break;
                }
                sep.work += 1;
                if hint[first] + hint[a] + hint[b] > 1. + 1e-6 {
                    sep.consider(&[first, a, b]);
                }
            }
            if sep.stop() || sep.work >= triangle_cap {
                break;
            }
        }
    }
    for first in order {
        if sep.stop() {
            break;
        }
        if hint[first] > 1e-8 {
            sep.visit(&mut vec![first]);
        }
    }
    let mut data = model.data().clone();
    let mut triangles = 0;
    let mut cycles = 0;
    for columns in &sep.output {
        if columns.len() == 3 {
            triangles += 1;
        } else {
            cycles += 1;
        }
        data.matrix.pattern.indices.extend(columns);
        data.matrix.values.extend(vec![1.; columns.len()]);
        data.matrix.pattern.indptr.push(data.matrix.values.len());
        data.matrix.pattern.rows += 1;
        data.rows.lower.push(f64::NEG_INFINITY);
        data.rows.upper.push((columns.len() / 2) as f64);
    }
    Ok((Model::new(data)?, triangles, cycles, sep.work))
}

pub(crate) fn solve(
    solver: &mut NativeSolver,
    model: &Model,
    options: &Options,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
) -> Result<Option<SolveResult>> {
    let settings = &solver.settings;
    let began = Instant::now();
    // Match the established zero-budget/cancellation behavior without doing work.
    if cancel.is_cancelled()
        || settings.time_limit_seconds == Some(0.)
        || settings.node_limit == Some(0)
        || settings.iteration_limit == Some(0)
    {
        return Ok(None);
    }
    let Some(pack) = Packing::new(model) else {
        return Ok(None);
    };
    let mut best = None;
    if let Some(s) = start {
        improve(model, options, &mut best, s.primal.clone())?;
    }
    if let Some(x) = pack.greedy(model, None) {
        improve(model, options, &mut best, x)?;
    }
    let Some(seed) = best.as_ref() else {
        return Ok(None);
    };
    let greedy_objective = model.objective_offset()
        + pack
            .weights
            .iter()
            .zip(seed)
            .map(|(w, v)| w * v)
            .sum::<f64>();
    let greedy_seconds = began.elapsed().as_secs_f64();
    let t = Instant::now();
    let (hint, bound, root_stats) = relaxation(&pack, model, settings, cancel, began, seed)?;
    let relaxation_seconds = t.elapsed().as_secs_f64();
    if let Some(x) = pack.greedy(model, Some(&hint)) {
        improve(model, options, &mut best, x)?;
    }
    let t = Instant::now();
    let lns = residual_search(
        &pack, model, options, settings, cancel, began, &mut best, &hint,
    )?;
    let neighborhood_seconds = t.elapsed().as_secs_f64();
    let neighborhood_objective = model.objective_offset()
        + pack
            .weights
            .iter()
            .zip(best.as_ref().unwrap())
            .map(|(w, v)| w * v)
            .sum::<f64>();
    let t = Instant::now();
    let (search, triangles, cycles, cut_work) =
        if settings.packing_cuts && !stopped(settings, cancel, began) {
            add_cuts(&pack, model, &hint, settings, cancel, began)?
        } else {
            (model.clone(), 0, 0, 0)
        };
    let cut_seconds = t.elapsed().as_secs_f64();
    let mut controls = child_settings(settings, began);
    controls.node_limit = controls.node_limit.map(|n| n.saturating_sub(lns.nodes));
    controls.iteration_limit = controls
        .iteration_limit
        .map(|n| n.saturating_sub(lns.pivots));
    controls.partial_dual_bounds = true;
    controls.mip_diagnostics = settings.mip_diagnostics;
    controls.tree_threads = settings.tree_threads;
    let seed = WarmStart::from_primal(best.as_ref().unwrap().clone());
    let mut child = NativeSolver::new(controls);
    let mut result = if settings.tree_threads > 1 {
        tree::solve(
            &mut child,
            &search,
            options,
            Some(&seed),
            cancel,
            bound,
            Some(&hint),
        )?
        .map(Ok)
        .unwrap_or_else(|| root::solve(&mut child, &search, options, Some(&seed), cancel))?
    } else {
        root::solve(&mut child, &search, options, Some(&seed), cancel)?
    };
    if let Some(point) = result.candidate.take() {
        improve(model, options, &mut best, point)?;
    }
    result.candidate = best;
    result.evaluation = result
        .candidate
        .as_ref()
        .map(|x| check::evaluate(model, x, &options.tolerances))
        .transpose()?;
    if let Some(b) = bound {
        result.best_bound = Some(result.best_bound.map_or(b, |v| v.min(b)));
    }
    result.nodes += lns.nodes;
    result.iterations += lns.pivots;
    result.backend.effective_options =
        merge_metadata(&solver.settings, result.backend.effective_options);
    for (key, value) in [
        ("packing_root_iterations", root_stats.iterations),
        ("packing_root_work", root_stats.work),
        ("packing_neighborhood_tasks", lns.tasks),
        ("packing_neighborhood_improvements", lns.improvements),
        ("packing_neighborhood_errors", lns.errors),
        ("packing_neighborhood_workers", lns.workers as u64),
        ("packing_neighborhood_nodes", lns.nodes),
        ("packing_neighborhood_pivots", lns.pivots),
        ("packing_triangles", triangles),
        ("packing_odd_cycles", cycles),
        ("packing_cut_work", cut_work),
    ] {
        result
            .backend
            .effective_options
            .push((format!("diagnostics.{key}"), value.to_string()));
    }
    result.backend.effective_options.push((
        "diagnostics.packing_root_bound".into(),
        bound.map_or_else(|| "none".into(), |v| v.to_string()),
    ));
    result.backend.effective_options.push((
        "diagnostics.packing_greedy_objective".into(),
        greedy_objective.to_string(),
    ));
    result.backend.effective_options.push((
        "diagnostics.packing_neighborhood_objective".into(),
        neighborhood_objective.to_string(),
    ));
    for (key, value) in [
        ("packing_greedy_seconds", greedy_seconds),
        ("packing_relaxation_seconds", relaxation_seconds),
        ("packing_neighborhood_seconds", neighborhood_seconds),
        ("packing_cut_seconds", cut_seconds),
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
    result.message = format!(
        "Native packing root/residual search + checked simplex: {} (numerical status)",
        result.status.as_str()
    );
    Ok(Some(result))
}
pub(crate) fn merge_metadata(
    settings: &NativeOptions,
    metadata: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let mut out = effective_options(settings);
    out.extend(
        metadata
            .into_iter()
            .filter(|(k, _)| k.starts_with("diagnostics.")),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packing(weights: Vec<f64>, rows: Vec<Vec<usize>>) -> Model {
        let n = weights.len();
        let m = rows.len();
        let mut indptr = vec![0];
        let mut indices = Vec::new();
        for row in rows {
            indices.extend(row);
            indptr.push(indices.len());
        }
        let values = vec![1.; indices.len()];
        Model::new(ModelData {
            objective: weights,
            objective_offset: 0.,
            sense: Sense::Maximize,
            variables: Bounds::new(vec![0.; n], vec![1.; n]),
            kinds: vec![VariableKind::Binary; n],
            rows: Bounds::new(vec![f64::NEG_INFINITY; m], vec![1.; m]),
            matrix: CsrMatrix {
                pattern: lp_ip_craft::CsrPattern {
                    rows: m,
                    cols: n,
                    indptr,
                    indices,
                },
                values,
            },
        })
        .unwrap()
    }
    fn feasible(m: &Model, x: &[f64]) -> bool {
        check::evaluate(m, x, &Options::default().tolerances)
            .unwrap()
            .feasible_candidate
    }
    fn optimum(m: &Model) -> f64 {
        (0..1usize << m.n())
            .filter_map(|mask| {
                let x: Vec<_> = (0..m.n()).map(|j| ((mask >> j) & 1) as f64).collect();
                feasible(m, &x).then(|| {
                    check::evaluate(m, &x, &Options::default().tolerances)
                        .unwrap()
                        .objective
                })
            })
            .fold(f64::NEG_INFINITY, f64::max)
    }
    fn lp_value(m: &Model) -> f64 {
        let mut d = m.data().clone();
        d.kinds.fill(VariableKind::Continuous);
        let r = NativeSolver::default()
            .solve(
                &Model::new(d).unwrap(),
                &Options::default(),
                None,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(r.status, Status::Optimal);
        r.evaluation.unwrap().objective
    }
    #[test]
    fn triangles_and_five_cycles_strengthen_the_lp_and_preserve_all_integer_points() {
        for n in [3, 5, 7] {
            let m = packing(
                vec![1.; n],
                (0..n)
                    .map(|j| {
                        let mut r = vec![j, (j + 1) % n];
                        r.sort_unstable();
                        r
                    })
                    .collect(),
            );
            let pack = Packing::new(&m).unwrap();
            let s = NativeOptions {
                packing_cuts: true,
                ..Default::default()
            };
            let (cut, t, c, w) = add_cuts(
                &pack,
                &m,
                &vec![0.5; n],
                &s,
                &CancellationToken::default(),
                Instant::now(),
            )
            .unwrap();
            assert_eq!(t + c, 1);
            assert!(w <= s.packing_cut_work);
            assert!((lp_value(&m) - (n as f64) / 2.).abs() < 1e-7);
            assert!((lp_value(&cut) - (n / 2) as f64).abs() < 1e-7);
            for mask in 0..1usize << n {
                let x: Vec<_> = (0..n).map(|j| ((mask >> j) & 1) as f64).collect();
                if feasible(&m, &x) {
                    assert!(feasible(&cut, &x));
                }
            }
        }
    }
    #[test]
    fn full_resource_clique_is_not_readded_as_a_triangle() {
        let m = packing(vec![1.; 4], vec![vec![0, 1, 2, 3]]);
        let p = Packing::new(&m).unwrap();
        let (_, t, c, _) = add_cuts(
            &p,
            &m,
            &[0.5; 4],
            &NativeOptions::default(),
            &CancellationToken::default(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(t + c, 0);
    }
    #[test]
    fn pdhg_bound_is_conservative_and_retained_without_simplex_work() {
        let m = packing(vec![6., 5., 5.], vec![vec![0, 1], vec![0, 2]]);
        let s = NativeOptions {
            packing_root: true,
            packing_root_iterations: 128,
            iteration_limit: Some(1),
            decompose: false,
            ..Default::default()
        };
        let p = Packing::new(&m).unwrap();
        let seed = p.greedy(&m, None).unwrap();
        let (_, bound, stats) = relaxation(
            &p,
            &m,
            &s,
            &CancellationToken::default(),
            Instant::now(),
            &seed,
        )
        .unwrap();
        assert!(bound.unwrap() >= optimum(&m));
        assert!(bound.unwrap() <= 11.);
        assert!(stats.work <= s.packing_root_work);
        let r = NativeSolver::new(s)
            .solve(&m, &Options::default(), None, &CancellationToken::default())
            .unwrap();
        assert!(r.evaluation.as_ref().unwrap().feasible_candidate);
        assert!(r.best_bound.unwrap() >= optimum(&m));
    }
    #[test]
    fn residuals_preserve_outside_assignments_exhaustively() {
        let m = packing(
            vec![6., 5., 5., 4., 3., 2.],
            vec![vec![0, 1], vec![0, 2, 3], vec![1, 4], vec![3, 5]],
        );
        let p = Packing::new(&m).unwrap();
        let x = p.greedy(&m, None).unwrap();
        for task in 0..24 {
            let Some((r, cols, start)) = neighborhood(&p, &m, &x, &x, task, 4).unwrap() else {
                continue;
            };
            assert!(feasible(&r, &start.primal));
            for mask in 0..1usize << r.n() {
                let local: Vec<_> = (0..r.n()).map(|j| ((mask >> j) & 1) as f64).collect();
                if feasible(&r, &local) {
                    let mut full = x.clone();
                    for (&j, &v) in cols.iter().zip(&local) {
                        full[j] = v;
                    }
                    assert!(feasible(&m, &full));
                }
            }
        }
    }
    #[test]
    fn residual_parallelism_improves_greedy_and_is_schedule_independent() {
        let m = packing(vec![6., 5., 5.], vec![vec![0, 1], vec![0, 2]]);
        let p = Packing::new(&m).unwrap();
        let greedy = p.greedy(&m, None).unwrap();
        assert_eq!(
            check::evaluate(&m, &greedy, &Options::default().tolerances)
                .unwrap()
                .objective,
            6.
        );
        let mut records = Vec::new();
        for workers in [1, 4, 16] {
            let s = NativeOptions {
                packing_neighborhoods: 16,
                packing_workers: workers,
                packing_neighborhood_size: 12,
                ..Default::default()
            };
            let mut best = Some(greedy.clone());
            let stats = residual_search(
                &p,
                &m,
                &Options::default(),
                &s,
                &CancellationToken::default(),
                Instant::now(),
                &mut best,
                &greedy,
            )
            .unwrap();
            assert_eq!(
                check::evaluate(&m, best.as_ref().unwrap(), &Options::default().tolerances)
                    .unwrap()
                    .objective,
                10.
            );
            assert!(stats.improvements > 0);
            records.push((best, stats.nodes, stats.pivots));
        }
        assert_eq!(records[0], records[1]);
        assert_eq!(records[0], records[2]);
    }
    #[test]
    fn random_packing_bounds_and_cuts_are_valid_against_exhaustive_oracles() {
        let mut rng = 7u64;
        for _ in 0..120 {
            let mut next = || {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                rng >> 32
            };
            let weights = (0..8).map(|_| (next() % 20) as f64).collect();
            let rows = (0..9)
                .map(|_| (0..8).filter(|_| next() % 4 == 0).collect())
                .collect();
            let m = packing(weights, rows);
            let p = Packing::new(&m).unwrap();
            let seed = p.greedy(&m, None).unwrap();
            let s = NativeOptions {
                packing_root: true,
                packing_root_iterations: 128,
                ..Default::default()
            };
            let (_, bound, _) = relaxation(
                &p,
                &m,
                &s,
                &CancellationToken::default(),
                Instant::now(),
                &seed,
            )
            .unwrap();
            assert!(bound.unwrap() >= optimum(&m));
            let (cut, _, _, _) = add_cuts(
                &p,
                &m,
                &[0.5; 8],
                &s,
                &CancellationToken::default(),
                Instant::now(),
            )
            .unwrap();
            assert_eq!(optimum(&m), optimum(&cut));
        }
    }
}
