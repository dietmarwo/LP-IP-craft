//! Optional, thread-local kernel timings. Build with `kernel-profile`.
//! Times are exclusive of nested measured operations. Read/reset on the
//! solving thread; component-worker timings are deliberately not aggregated.
#[cfg(feature = "kernel-profile")]
mod enabled {
    use std::{cell::RefCell, time::Instant};
    #[derive(Default)]
    struct Data {
        totals: Vec<(&'static str, u64, u64)>,
        children: Vec<u64>,
    }
    thread_local! { static DATA: RefCell<Data> = RefCell::new(Data::default()); }
    pub struct Guard {
        name: &'static str,
        start: Instant,
    }
    pub fn enter(name: &'static str) -> Guard {
        DATA.with(|d| d.borrow_mut().children.push(0));
        Guard {
            name,
            start: Instant::now(),
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            let elapsed = self.start.elapsed().as_nanos() as u64;
            DATA.with(|d| {
                let mut d = d.borrow_mut();
                let children = d.children.pop().unwrap();
                if let Some(parent) = d.children.last_mut() {
                    *parent += elapsed;
                }
                let exclusive = elapsed.saturating_sub(children);
                if let Some(entry) = d.totals.iter_mut().find(|e| e.0 == self.name) {
                    entry.1 += exclusive;
                    entry.2 += 1;
                } else {
                    d.totals.push((self.name, exclusive, 1));
                }
            });
        }
    }
    /// Reset on the current thread, outside a solve.
    pub fn reset() {
        DATA.with(|d| {
            let mut d = d.borrow_mut();
            assert!(d.children.is_empty());
            d.totals.clear();
        });
    }
    /// Operation name, exclusive nanoseconds, call count, on this thread.
    pub fn snapshot() -> Vec<(&'static str, u64, u64)> {
        DATA.with(|d| d.borrow().totals.clone())
    }
}
#[cfg(feature = "kernel-profile")]
pub use enabled::*;
#[cfg(not(feature = "kernel-profile"))]
pub(crate) struct Guard;
#[cfg(not(feature = "kernel-profile"))]
#[inline(always)]
pub(crate) fn enter(_: &'static str) -> Guard {
    Guard
}
