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
                "batch outer workers times numerical threads must not exceed 8".into(),
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
        let results = run_shared(workers, work)?;
        let mut out: Vec<_> = (0..models.len()).map(|_| None).collect();
        for (i, result) in results {
            out[i] = Some(result);
        }
        out.into_iter()
            .map(|r| r.ok_or_else(|| Error::Backend("batch result missing".into())))
            .collect()
    }
}

/// Runs `work` on `workers` threads, the calling thread being one of them,
/// and concatenates their outputs. Every thread is joined before returning,
/// and a panic in any share, the caller's included, is reported as
/// `Error::Backend`, as when every worker was a spawned thread.
fn run_shared<T: Send>(workers: usize, work: impl Fn() -> Vec<T> + Sync) -> Result<Vec<T>> {
    let panicked = || Error::Backend("batch worker panicked".into());
    std::thread::scope(|scope| {
        let work = &work;
        let mut handles = Vec::new();
        let mut creation = None;
        for _ in 1..workers {
            match std::thread::Builder::new()
                .name("lpip-batch".into())
                .spawn_scoped(scope, work)
            {
                Ok(h) => handles.push(h),
                Err(e) => {
                    creation = Some(Error::Backend(format!("batch worker creation: {e}")));
                    break;
                }
            }
        }
        if let Some(e) = creation {
            for h in handles {
                let _ = h.join();
            }
            return Err(e);
        }
        let own = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        let joined: Vec<_> = handles.into_iter().map(|h| h.join()).collect();
        let mut out = own.map_err(|_| panicked())?;
        for share in joined {
            out.extend(share.map_err(|_| panicked())?);
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panicked(r: Result<Vec<usize>>) -> bool {
        matches!(r, Err(Error::Backend(m)) if m == "batch worker panicked")
    }

    #[test]
    fn every_share_runs_once() {
        let calls = AtomicUsize::new(0);
        let mut out = run_shared(4, || vec![calls.fetch_add(1, Ordering::Relaxed)]).unwrap();
        out.sort_unstable();
        assert_eq!(out, [0, 1, 2, 3]);
    }

    #[test]
    fn a_panic_on_the_calling_thread_is_an_error_like_a_worker_panic() {
        let caller = std::thread::current().id();
        let on_caller = || {
            assert_ne!(std::thread::current().id(), caller, "injected");
            vec![1]
        };
        let on_worker = || {
            assert_eq!(std::thread::current().id(), caller, "injected");
            vec![1]
        };
        assert!(panicked(run_shared(3, on_caller)));
        assert!(panicked(run_shared(3, on_worker)));
    }
}
