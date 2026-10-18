//! Independent-job parallelism; no shared basis, tree or numerical workspace.
use crate::*;
use std::sync::atomic::{AtomicUsize, Ordering};
impl NativeSolver {
    /// Solve independent models in input order using 1..=16 workers. Each job
    /// has its own limits and result/error. A shared cancellation token affects
    /// running and queued jobs. Nested component, residual and tree workers are disabled inside the pool.
    /// No warm starts are supplied by this API. Models are borrowed, not copied.
    pub fn solve_batch(
        &self,
        models: &[&Model],
        options: &Options,
        workers: usize,
        cancel: &CancellationToken,
    ) -> Result<Vec<Result<SolveResult>>> {
        self.settings.validate()?;
        options.validate()?;
        if !(1..=16).contains(&workers) {
            return Err(Error::InvalidInput(
                "batch workers must be in 1..=16".into(),
            ));
        }
        if options.threads > 1 && workers.saturating_mul(options.threads) > 8 {
            return Err(Error::InvalidInput(
                "batch outer workers times numerical threads must not exceed8".into(),
            ));
        }
        // Batch-owned resources keep mixed inner jobs from repeatedly rebuilding pools.
        let _numeric_execution = if options.threads > 1
            && !cancel.is_cancelled()
            && self.settings.time_limit_seconds != Some(0.)
            && models.iter().any(|m| m.nnz() >= 65_536)
        {
            crate::parallel::Execution::new(options.threads).map_err(Error::Backend)?
        } else {
            crate::parallel::Execution::default()
        };
        let mut settings = self.settings.clone();
        settings.component_threads = 1;
        settings.packing_workers = 1;
        settings.tree_threads = 1;
        let workers = if cancel.is_cancelled() {
            1
        } else {
            workers.min(models.len())
        };
        if workers <= 1 {
            let mut solver = NativeSolver::new(settings);
            return Ok(models
                .iter()
                .map(|model| solver.solve(model, options, None, cancel))
                .collect());
        }
        let mut order: Vec<_> = (0..models.len()).collect();
        if models.len() >= workers * 4 {
            let work = |i: usize| {
                models[i]
                    .nnz()
                    .saturating_add(models[i].n())
                    .saturating_add(models[i].m())
            };
            let lo = order.iter().map(|&i| work(i)).min().unwrap_or(0);
            let hi = order.iter().map(|&i| work(i)).max().unwrap_or(0);
            if hi > lo.saturating_mul(2) {
                order.sort_by_key(|&i| std::cmp::Reverse(work(i)));
            }
        }
        let order = &order;
        let next = AtomicUsize::new(0);
        let claim = (models.len() / (workers * 8)).clamp(1, 4);
        let work = || {
            let mut solver = NativeSolver::new(settings.clone());
            let mut out = Vec::with_capacity(models.len() / workers);
            loop {
                let first = next.fetch_add(claim, Ordering::Relaxed);
                if first >= models.len() {
                    break;
                }
                for &i in &order[first..models.len().min(first + claim)] {
                    out.push((i, solver.solve(models[i], options, None, cancel)));
                }
            }
            out
        };
        let results = std::thread::scope(|scope| -> Result<Vec<_>> {
            let mut handles = Vec::new();
            for _ in 1..workers {
                handles.push(
                    std::thread::Builder::new()
                        .name("lpip-batch".into())
                        .spawn_scoped(scope, &work)
                        .map_err(|e| Error::Backend(format!("batch worker creation: {e}")))?,
                );
            }
            let mut out = work();
            for h in handles {
                out.extend(
                    h.join()
                        .map_err(|_| Error::Backend("batch worker panicked".into()))?,
                );
            }
            Ok(out)
        })?;
        let mut out: Vec<_> = (0..models.len()).map(|_| None).collect();
        for (i, result) in results {
            out[i] = Some(result);
        }
        out.into_iter()
            .map(|r| r.ok_or_else(|| Error::Backend("batch result missing".into())))
            .collect()
    }
}
