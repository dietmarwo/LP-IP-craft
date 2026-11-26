//! Solve-origin events for the single-process primal heuristic experiment.
use crate::oriented;
#[cfg(feature = "local-mip-bench")]
use crate::{NativeSolver, root};
#[cfg(feature = "local-mip-bench")]
use lp_ip_craft::{CancellationToken, Error, Result, SolveResult};
use lp_ip_craft::{Model, Options, check};
use std::time::Instant;

#[derive(Default, Clone, Copy)]
pub struct Controls {
    pub matched_fj_effort: u64,
    pub replace_root_fj: bool,
    pub stop_at_root: bool,
}

#[cfg_attr(not(feature = "local-mip-bench"), allow(dead_code))]
pub struct Event {
    pub kind: &'static str,
    pub source: &'static str,
    pub phase: &'static str,
    pub seconds: f64,
    pub nodes: u64,
    pub pivots: u64,
    pub objective: Option<f64>,
    pub bound: Option<f64>,
    pub stage_nodes: u64,
    pub stage_pivots: u64,
    pub stage_seconds: Option<f64>,
}

pub(crate) type Work = (u64, u64); // nodes, simplex pivots

pub(crate) struct Trace<'a> {
    original: &'a Model,
    sink: &'a mut dyn FnMut(Event),
    began: Instant,
    best: Option<f64>,
    pub controls: Controls,
    pub prefix: Work,
    pub skip: &'static str,
    in_root: bool,
}

impl Trace<'_> {
    fn emit(&mut self, mut event: Event, work: Work) {
        event.seconds = self.began.elapsed().as_secs_f64();
        event.phase = if self.in_root { "root" } else { "tree" };
        event.nodes = self.prefix.0 + work.0;
        event.pivots = self.prefix.1 + work.1;
        (self.sink)(event);
    }

    pub(crate) fn event(
        &mut self,
        kind: &'static str,
        source: &'static str,
        work: Work,
        objective: Option<f64>,
        bound: Option<f64>,
    ) {
        self.emit(
            Event {
                kind,
                source,
                phase: "",
                seconds: 0.,
                nodes: 0,
                pivots: 0,
                objective,
                bound,
                stage_nodes: 0,
                stage_pivots: 0,
                stage_seconds: None,
            },
            work,
        );
    }

    pub(crate) fn stage(&mut self, source: &'static str, work: Work, before: Work, began: Instant) {
        self.emit(
            Event {
                kind: "stage",
                source,
                phase: "",
                seconds: 0.,
                nodes: 0,
                pivots: 0,
                objective: None,
                bound: None,
                stage_nodes: work.0 - before.0,
                stage_pivots: work.1 - before.1,
                stage_seconds: Some(began.elapsed().as_secs_f64()),
            },
            work,
        );
    }

    pub(crate) fn checked_point(
        &mut self,
        point: &[f64],
        options: &Options,
        source: &'static str,
        work: Work,
    ) {
        match check::evaluate(self.original, point, &options.tolerances) {
            Ok(e) if e.feasible_candidate => self.candidate(e.objective, source, work),
            _ => self.event("original_check_rejection", source, work, None, None),
        }
    }

    pub(crate) fn candidate(&mut self, objective: f64, source: &'static str, work: Work) {
        if self
            .best
            .is_none_or(|old| oriented(self.original, objective) < oriented(self.original, old))
        {
            self.best = Some(objective);
            self.event("checked_incumbent", source, work, Some(objective), None);
        }
    }

    pub(crate) fn root_end(&mut self, work: Work) {
        if self.in_root {
            self.event("root_exit", self.skip, work, self.best, None);
            self.in_root = false;
        }
    }
}

/// Observe the general serial solve, with root-only replay available for diagnosis.
#[cfg(feature = "local-mip-bench")]
pub fn solve(
    solver: &mut NativeSolver,
    model: &Model,
    options: &Options,
    cancel: &CancellationToken,
    controls: Controls,
    mut sink: impl FnMut(Event),
) -> Result<SolveResult> {
    if options.threads != 1
        || solver.settings.decompose
        || solver.settings.tree_threads != 1
        || solver.settings.packing_root
        || solver.settings.packing_neighborhoods != 0
        || solver.settings.packing_cuts
    {
        return Err(Error::InvalidInput(
            "local MIP benchmark requires the general serial path".into(),
        ));
    }
    let mut trace = Trace {
        original: model,
        sink: &mut sink,
        began: Instant::now(),
        best: None,
        controls,
        prefix: (0, 0),
        skip: "no_usable_root_lp",
        in_root: true,
    };
    let result = root::solve_observed(solver, model, options, None, cancel, Some(&mut trace));
    trace.prefix = (0, 0);
    if let Ok(r) = &result {
        trace.root_end((r.nodes, r.iterations));
    }
    result
}
