//! Explicit local worker budgets. No global Rayon pool is configured.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
static POOLS: OnceLock<Mutex<BTreeMap<usize, Weak<rayon::ThreadPool>>>> = OnceLock::new();

/// Validates a requested maximum without allocating or starting workers.
pub fn validate_threads(threads: usize) -> Result<(), String> {
    if !(1..=8).contains(&threads) {
        Err(format!("parallel threads must lie in 1..=8, got {threads}"))
    } else {
        Ok(())
    }
}

/// Reusable owned execution resources; one means no worker pool.
#[derive(Clone, Debug, Default)]
pub struct Execution {
    pool: Option<Arc<rayon::ThreadPool>>,
}

impl Execution {
    /// Creates at most `threads` workers. Supported budgets are 1 through 8.
    pub fn new(threads: usize) -> Result<Self, String> {
        validate_threads(threads)?;
        if threads == 1 {
            return Ok(Self::default());
        }
        let mut pools = POOLS
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "worker registry lock poisoned".to_string())?;
        if let Some(pool) = pools.get(&threads).and_then(Weak::upgrade) {
            return Ok(Self { pool: Some(pool) });
        }
        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|i| format!("lpip-worker-{i}"))
                .build()
                .map_err(|e| format!("native worker creation: {e}"))?,
        );
        pools.insert(threads, Arc::downgrade(&pool));
        Ok(Self { pool: Some(pool) })
    }

    /// Configured maximum numerical workers.
    pub fn threads(&self) -> usize {
        self.pool.as_ref().map_or(1, |p| p.current_num_threads())
    }

    /// Runs disjoint chunks with deterministic chunk offsets. Scalar reductions
    /// inside a chunk remain the responsibility of the kernel.
    pub fn chunks_mut<T, F>(&self, data: &mut [T], chunk: usize, work: usize, f: F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Send + Sync,
    {
        assert!(chunk > 0);
        if let Some(pool) = &self.pool {
            if work >= 65536 {
                pool.install(|| {
                    data.par_chunks_mut(chunk)
                        .enumerate()
                        .for_each(|(i, part)| f(i * chunk, part))
                });
                return;
            }
        }
        for (i, part) in data.chunks_mut(chunk).enumerate() {
            f(i * chunk, part);
        }
    }

    /// Maps disjoint output ranges using cumulative nonnegative column work.
    /// Each leaf keeps its original scalar arithmetic; no numeric reduction occurs.
    pub fn weighted_mut<T, F>(&self, data: &mut [T], prefix: &[usize], f: F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Send + Sync,
    {
        assert_eq!(prefix.len(), data.len() + 1);
        fn visit<T: Send, F: Fn(usize, &mut [T]) + Send + Sync>(
            data: &mut [T],
            prefix: &[usize],
            offset: usize,
            lanes: usize,
            f: &F,
        ) {
            if lanes <= 1 || data.len() <= 1 {
                f(offset, data);
                return;
            }
            let total = prefix[prefix.len() - 1] - prefix[0];
            let target = prefix[0] + total / 2;
            let split = if total == 0 {
                data.len() / 2
            } else {
                prefix
                    .partition_point(|&w| w < target)
                    .clamp(1, data.len() - 1)
            };
            let (left, right) = data.split_at_mut(split);
            rayon::join(
                || visit(left, &prefix[..=split], offset, lanes / 2, f),
                || {
                    visit(
                        right,
                        &prefix[split..],
                        offset + split,
                        lanes - lanes / 2,
                        f,
                    )
                },
            );
        }
        if let Some(pool) = &self.pool {
            pool.install(|| visit(data, prefix, 0, self.threads() * 2, &f));
        } else {
            f(0, data);
        }
    }

    /// Creates private state on each worker and reuses it for dynamically
    /// assigned tasks. State itself need not be Send or Sync; only outputs do.
    pub fn map_with_init<S, T, I, F>(&self, count: usize, init: I, f: F) -> Vec<T>
    where
        T: Send,
        I: Fn(usize) -> S + Send + Sync,
        F: Fn(&mut S, usize) -> T + Send + Sync,
    {
        use std::sync::atomic::{AtomicUsize, Ordering};
        if count == 0 {
            return Vec::new();
        }
        let next = AtomicUsize::new(0);
        let workers = self.threads().min(count);
        let claim = (count / (workers * 8)).clamp(1, 4);
        let parts = self.map(workers, |worker| {
            let mut state = init(worker);
            let mut local = Vec::with_capacity(count / workers);
            loop {
                let first = next.fetch_add(claim, Ordering::Relaxed);
                if first >= count {
                    break;
                }
                for i in first..count.min(first + claim) {
                    local.push((i, f(&mut state, i)));
                }
            }
            local
        });
        let mut slots: Vec<_> = (0..count).map(|_| None).collect();
        for (index, value) in parts.into_iter().flatten() {
            assert!(slots[index].is_none());
            slots[index] = Some(value);
        }
        slots
            .into_iter()
            .map(|value| value.expect("one result per task"))
            .collect()
    }

    /// Maps independent tasks, returning results in input-index order.
    pub fn map<T, F>(&self, count: usize, f: F) -> Vec<T>
    where
        T: Send,
        F: Fn(usize) -> T + Send + Sync,
    {
        match &self.pool {
            Some(pool) => pool.install(|| (0..count).into_par_iter().map(f).collect()),
            None => (0..count).map(f).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn width_order_disjoint_outputs_and_budget() {
        for width in [1, 2, 4, 8] {
            let exec = Execution::new(width).unwrap();
            assert_eq!(exec.threads(), width);
            assert_eq!(
                exec.map(97, |i| i * i),
                (0..97).map(|i| i * i).collect::<Vec<_>>()
            );
            let mut out = vec![0; 103];
            exec.chunks_mut(&mut out, 7, 100_000, |start, part| {
                for (i, v) in part.iter_mut().enumerate() {
                    *v = start + i;
                }
            });
            assert_eq!(out, (0..103).collect::<Vec<_>>());
            let active = AtomicUsize::new(0);
            let peak = AtomicUsize::new(0);
            exec.map(64, |_| {
                let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                std::thread::yield_now();
                active.fetch_sub(1, Ordering::SeqCst);
            });
            assert!(peak.load(Ordering::SeqCst) <= width);
        }
        assert!(Execution::new(0).is_err());
        assert!(Execution::new(9).is_err());
    }
    #[test]
    fn panic_returns_to_caller_without_poisoning_pool() {
        let exec = Execution::new(8).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| exec.map(16, |i| {
                assert_ne!(i, 7);
                i
            })))
            .is_err()
        );
        assert_eq!(exec.map(4, |i| i), vec![0, 1, 2, 3]);
    }
}

#[cfg(test)]
mod configuration_tests {
    #[test]
    fn explicit_widths_are_bounded() {
        for width in 1..=8 {
            assert_eq!(super::Execution::new(width).unwrap().threads(), width);
        }
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;
    #[test]
    fn compatible_owners_share_without_permanent_retention() {
        let a = Execution::new(8).unwrap();
        let b = Execution::new(8).unwrap();
        assert!(Arc::ptr_eq(
            a.pool.as_ref().unwrap(),
            b.pool.as_ref().unwrap()
        ));
        let weak = Arc::downgrade(a.pool.as_ref().unwrap());
        drop(a);
        assert!(weak.upgrade().is_some());
        drop(b);
        let c = Execution::new(4).unwrap();
        assert_eq!(c.threads(), 4);
        let d = Execution::new(8).unwrap();
        assert!(!Arc::ptr_eq(
            c.pool.as_ref().unwrap(),
            d.pool.as_ref().unwrap()
        ));
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc, thread};
    #[test]
    fn state_is_local_and_results_are_ordered() {
        let execution = Execution::new(8).unwrap();
        let results = execution.map_with_init(
            1000,
            |_| (thread::current().id(), Rc::new(Cell::new(0))),
            |state, i| {
                assert_eq!(state.0, thread::current().id());
                state.1.set(state.1.get() + 1);
                i * i
            },
        );
        assert_eq!(results, (0..1000).map(|i| i * i).collect::<Vec<_>>());
        assert!(
            execution
                .map_with_init::<(), usize, _, _>(0, |_| panic!("empty init"), |_, _| 0)
                .is_empty()
        );
    }
}

#[cfg(test)]
mod weighted_tests {
    #[test]
    fn skewed_empty_columns_and_offsets_have_exactly_one_owner() {
        for width in [1, 2, 4, 8] {
            let e = super::Execution::new(width).unwrap();
            for count in [0, 1, 7, 65, 1000] {
                let mut prefix = vec![0];
                for i in 0..count {
                    prefix.push(prefix[i] + if i > count * 3 / 4 { 10000 } else { 0 });
                }
                let mut out = vec![usize::MAX; count];
                e.weighted_mut(&mut out, &prefix, |first, part| {
                    for (i, v) in part.iter_mut().enumerate() {
                        *v = first + i;
                    }
                });
                assert_eq!(out, (0..count).collect::<Vec<_>>());
            }
        }
    }
}
