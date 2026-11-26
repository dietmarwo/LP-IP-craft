//! Cooperative, solve-local work limits for root local search.
use crate::Budget;
use lp_ip_craft::{Model, Tolerances, check};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Feasible,
    Stalled,
    Effort,
    LocalTime,
    Global,
    Numerical,
    StepLimit,
    ScoringLimit,
}

impl Stop {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Feasible => "feasible",
            Self::Stalled => "stalled",
            Self::Effort => "effort_exhausted",
            Self::LocalTime => "local_timeout",
            Self::Global => "global_stop",
            Self::Numerical => "numerical_rejection",
            Self::StepLimit => "step_limit",
            Self::ScoringLimit => "fj_scoring_limit",
        }
    }
}

#[derive(Default)]
pub(crate) struct Counts {
    pub setup: u64,
    pub refresh: u64,
    pub generation: u64,
    pub scoring: u64,
    pub updates: u64,
    pub bookkeeping: u64,
    pub checks: u64,
    pub check_seconds: f64,
    pub patience_resets: u64,
}

#[derive(Clone, Copy)]
pub(crate) enum Work {
    Setup,
    Refresh,
    Generation,
    Scoring,
    Update,
    Bookkeeping,
}

pub(crate) struct Meter<'a, 's> {
    global: &'a Budget<'s>,
    started: Instant,
    duration: Duration,
    effort: u64,
    patience: u64,
    used: u64,
    poll_in: u64,
    last_progress: u64,
    best_at_reset: Option<usize>,
    violations: Option<(usize, usize)>,
    pub counts: Counts,
}

impl<'a, 's> Meter<'a, 's> {
    pub(crate) fn new(global: &'a Budget<'s>, effort: u64, patience: u64) -> Self {
        Self {
            global,
            started: Instant::now(),
            duration: Duration::from_millis(250),
            effort,
            patience,
            used: 0,
            poll_in: 256,
            last_progress: 0,
            best_at_reset: None,
            violations: None,
            counts: Counts::default(),
        }
    }

    pub(crate) fn used(&self) -> u64 {
        self.used
    }

    pub(crate) fn seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub(crate) fn clock(&self) -> Result<(), Stop> {
        if self.global.stop().is_some() {
            Err(Stop::Global)
        } else if self.started.elapsed() >= self.duration {
            Err(Stop::LocalTime)
        } else {
            Ok(())
        }
    }

    pub(crate) fn charge(&mut self, work: Work, entries: u64) -> Result<(), Stop> {
        if entries >= self.poll_in {
            self.clock()?;
            self.poll_in = 256;
        } else {
            self.poll_in -= entries;
        }
        let counter = match work {
            Work::Bookkeeping => {
                self.counts.bookkeeping = self.counts.bookkeeping.saturating_add(entries);
                return Ok(());
            }
            Work::Setup => &mut self.counts.setup,
            Work::Refresh => &mut self.counts.refresh,
            Work::Generation => &mut self.counts.generation,
            Work::Scoring => &mut self.counts.scoring,
            Work::Update => &mut self.counts.updates,
        };
        if entries > self.effort - self.used {
            return Err(Stop::Effort);
        }
        *counter += entries;
        self.used += entries;
        Ok(())
    }

    /// Call after an initial scan and after every completed move, including
    /// the last move of an attempt, before applying the patience check.
    pub(crate) fn progress(&mut self, violated: usize) {
        let (_, best) = self.violations.get_or_insert((violated, violated));
        *best = (*best).min(violated);
        match self.best_at_reset {
            None => {
                self.best_at_reset = Some(violated);
                self.last_progress = self.used;
            }
            Some(old) if violated < old && violated <= old - old.div_ceil(10) => {
                self.best_at_reset = Some(violated);
                self.last_progress = self.used;
                self.counts.patience_resets += 1;
            }
            _ => {}
        }
    }

    pub(crate) fn ready(&self) -> Result<(), Stop> {
        self.clock()?;
        if self.used == self.effort {
            return Err(Stop::Effort);
        }
        if self.patience != 0 && self.used - self.last_progress >= self.patience {
            return Err(Stop::Stalled);
        }
        Ok(())
    }

    pub(crate) fn check<T>(&mut self, evaluate: impl FnOnce() -> T) -> Result<T, Stop> {
        self.clock()?;
        let started = Instant::now();
        let result = evaluate();
        self.counts.checks += 1;
        self.counts.check_seconds += started.elapsed().as_secs_f64();
        self.clock()?;
        Ok(result)
    }

    pub(crate) fn valid_point(
        &mut self,
        model: &Model,
        original: &Model,
        point: &[f64],
        tolerances: &Tolerances,
    ) -> Result<bool, Stop> {
        self.check(|| {
            let valid =
                |m| check::evaluate(m, point, tolerances).is_ok_and(|e| e.feasible_candidate);
            valid(model) && (std::ptr::eq(model, original) || valid(original))
        })
    }

    pub(crate) fn report(
        &self,
        metadata: &mut Vec<(String, String)>,
        source: &str,
        stop: Stop,
        extra: &[(&str, u64)],
    ) {
        let prefix = "diagnostics.root_local_search";
        if let Some((initial, best)) = self.violations {
            metadata.push((format!("{prefix}.initial_violations"), initial.to_string()));
            metadata.push((format!("{prefix}.best_violations"), best.to_string()));
        }
        metadata.extend([
            (format!("{prefix}.source"), source.into()),
            (format!("{prefix}.stop"), stop.name().into()),
            (format!("{prefix}.seconds"), self.seconds().to_string()),
            (
                format!("{prefix}.check_seconds"),
                self.counts.check_seconds.to_string(),
            ),
        ]);
        metadata.extend(
            [
                ("effort_cap", self.effort),
                ("patience", self.patience),
                ("effort", self.used()),
                ("setup", self.counts.setup),
                ("refresh", self.counts.refresh),
                ("generation", self.counts.generation),
                ("scoring", self.counts.scoring),
                ("updates", self.counts.updates),
                ("bookkeeping", self.counts.bookkeeping),
                ("checks", self.counts.checks),
                ("patience_resets", self.counts.patience_resets),
            ]
            .into_iter()
            .chain(extra.iter().copied())
            .map(|(k, v)| (format!("{prefix}.{k}"), v.to_string())),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeOptions;
    use lp_ip_craft::CancellationToken;

    #[test]
    fn progress_at_patience_boundary_and_hard_cap() {
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let mut meter = Meter::new(&budget, 20, 10);
        meter.progress(10);
        meter.charge(Work::Update, 10).unwrap();
        assert_eq!(meter.ready(), Err(Stop::Stalled));
        meter.progress(9);
        assert_eq!(meter.ready(), Ok(()));
        meter.charge(Work::Update, 10).unwrap();
        meter.progress(8);
        assert_eq!(meter.ready(), Err(Stop::Effort));
        assert_eq!(meter.charge(Work::Scoring, 1), Err(Stop::Effort));
        assert_eq!(meter.used(), 20);
        assert_eq!(meter.counts.patience_resets, 2);

        let mut no_patience = Meter::new(&budget, 20, 0);
        no_patience.progress(4);
        no_patience.charge(Work::Generation, 19).unwrap();
        assert_eq!(no_patience.ready(), Ok(()));
        assert_eq!(no_patience.charge(Work::Scoring, 2), Err(Stop::Effort));
        cancel.cancel();
        assert_eq!(no_patience.ready(), Err(Stop::Global));
    }

    #[test]
    fn cancellation_is_polled_between_chunks_of_a_dense_scan() {
        let settings = NativeOptions::default();
        let cancel = CancellationToken::default();
        let budget = Budget {
            start: Instant::now(),
            settings: &settings,
            cancel: &cancel,
            pivots: 0,
        };
        let mut meter = Meter::new(&budget, 4096, 0);
        meter.charge(Work::Scoring, 255).unwrap();
        cancel.cancel();
        assert_eq!(meter.charge(Work::Scoring, 1), Err(Stop::Global));
        assert_eq!(meter.used(), 255);
    }
}
