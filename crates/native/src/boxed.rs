//! Exact bound selection for continuous models with no constraints.
use crate::{Budget, NativeOptions, accepted, effective_options, oriented};
use lp_ip_craft::*;
use std::time::Instant;

pub(crate) fn solve(
    model: &Model,
    options: &Options,
    settings: &NativeOptions,
    start: Option<&WarmStart>,
    cancel: &CancellationToken,
    began: Instant,
) -> Result<SolveResult> {
    let budget = Budget {
        start: began,
        settings,
        cancel,
        pivots: 0,
    };
    let mut report = BackendReport::new("native");
    report.version = Some(env!("CARGO_PKG_VERSION").into());
    report.effective_options = effective_options(settings);
    report.effective_options.extend([
        ("diagnostics.component_count".into(), "1".into()),
        ("diagnostics.component_workers".into(), "1".into()),
        ("diagnostics.direct_box".into(), "true".into()),
    ]);
    report.start_status = if start.is_some() {
        StartStatus::Rejected
    } else {
        StartStatus::NotProvided
    };
    let mut out = SolveResult::empty(Status::Optimal, "", report);
    let mut point = Vec::with_capacity(model.n());
    for j in 0..model.n() {
        if j % 1024 == 0 {
            if let Some(s) = budget.stop() {
                out.status = s;
                break;
            }
        }
        let c = oriented(model, model.objective()[j]);
        let (l, u) = (model.variables().lower[j], model.variables().upper[j]);
        let x = if c > 0. {
            l
        } else if c < 0. {
            u
        } else {
            0f64.max(l).min(u)
        };
        if !x.is_finite() {
            out.status = Status::Unbounded;
            break;
        }
        point.push(x);
    }
    out.backend.solve_seconds = began.elapsed().as_secs_f64();
    let checks = Instant::now();
    if out.status == Status::Optimal {
        let duals = Duals {
            row_duals: vec![],
            reduced_costs: model.objective().to_vec(),
        };
        if accepted(model, model.variables(), &point, &duals, options, settings) {
            out.best_bound = safe_dual_bound(model, &[])?.bound;
            out.evaluation = Some(check::evaluate(model, &point, &options.tolerances)?);
            out.candidate = Some(point);
            out.duals = Some(duals);
        } else {
            out.status = Status::NumericalFailure;
        }
    }
    if let Some(s) = budget.stop() {
        out.status = s;
    }
    out.backend.check_seconds = checks.elapsed().as_secs_f64();
    out.backend.total_seconds = began.elapsed().as_secs_f64();
    out.backend.time_budget_exceeded = settings
        .time_limit_seconds
        .is_some_and(|t| out.backend.total_seconds > t);
    out.backend.raw_status = Some(out.status.as_str().into());
    out.message = format!(
        "Native box solve: {} (not an exact certificate)",
        out.status.as_str()
    );
    Ok(out)
}
