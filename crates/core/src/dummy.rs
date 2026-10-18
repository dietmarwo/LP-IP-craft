//! The placeholder backend.

use crate::check::{check_point, evaluate};
use crate::error::Result;
use crate::model::Model;
use crate::solver::{
    BackendReport, CancellationToken, LpIpSolver, Options, SolveResult, StartStatus, Status,
    WarmStart,
};

/// Evaluates one candidate and stops. **It does not optimize.**
///
/// Exact behavior, which the tests pin down:
///
/// 1. Validate options and the start (length `n`, finite), even if
///    cancellation was already requested.
/// 2. If cancelled: return [`Status::Cancelled`] without a candidate. This is
///    the only checkpoint; the remaining work is a single pass over the data.
/// 3. Take the start's primal point unchanged, or else the point closest to
///    zero within the variable bounds (zero clamped to each bound pair).
///    Neither path rounds integer variables or repairs rows.
/// 4. Evaluate it with [`evaluate`] and return it with [`Status::Stub`].
///
/// In every case `duals` and `best_bound` are `None` and `nodes` and
/// `iterations` are zero, even when the candidate happens to be optimal.
#[derive(Clone, Copy, Debug, Default)]
pub struct DummySolver;

const STUB_MESSAGE: &str =
    "DUMMY backend: one candidate evaluated; no LP/MIP solve, bound or optimality claim";

impl LpIpSolver for DummySolver {
    fn solve(
        &mut self,
        model: &Model,
        options: &Options,
        start: Option<&WarmStart>,
        cancel: &CancellationToken,
    ) -> Result<SolveResult> {
        options.validate()?;
        if options.threads != 1 {
            return Err(crate::Error::Unsupported(format!(
                "threads = {}: dummy supports one worker",
                options.threads
            )));
        }
        if let Some(start) = start {
            check_point(model, &start.primal, "start")?;
        }

        let mut report = BackendReport::new("dummy");
        if start.is_some() {
            report.start_status = StartStatus::Evaluated;
        }
        let mut result = SolveResult::empty(
            Status::Cancelled,
            "cancelled before a candidate was evaluated",
            report,
        );
        if cancel.is_cancelled() {
            return Ok(result);
        }

        let x = match start {
            Some(start) => start.primal.clone(),
            None => {
                let bounds = model.variables();
                bounds
                    .lower
                    .iter()
                    .zip(&bounds.upper)
                    .map(|(&l, &u)| 0.0_f64.clamp(l, u))
                    .collect()
            }
        };
        let evaluation = evaluate(model, &x, &options.tolerances)?;
        result.candidate = Some(x);
        result.evaluation = Some(evaluation);
        result.status = Status::Stub;
        result.message = STUB_MESSAGE.into();
        Ok(result)
    }
}
