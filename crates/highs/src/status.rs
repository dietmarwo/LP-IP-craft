//! Mapping of HiGHS model statuses to the facade's [`Status`].

use highs::HighsModelStatus as H;
use lp_ip_craft::Status;

/// What a native status means for the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mapped {
    /// A termination reason.
    Status(Status),
    /// The interrupt callback stopped the run; the caller decides whether the
    /// token or the time budget asked for it.
    Interrupted,
    /// HiGHS did not run the model (setup or model error). Reported as an
    /// [`Error::Backend`](lp_ip_craft::Error::Backend), never as a finding
    /// about the model.
    NotRun,
}

/// Maps every status of the pinned `highs` 2.4.0 explicitly.
///
/// `ReachedSolutionLimit` is HiGHS's report for `mip_max_nodes` too; the
/// caller refines it to [`Status::NodeLimit`] when a node limit was set and
/// reached.
pub(crate) fn map(raw: H) -> Mapped {
    use Mapped::{Interrupted, NotRun, Status as S};
    match raw {
        H::Optimal => S(Status::Optimal),
        H::Infeasible => S(Status::Infeasible),
        H::Unbounded => S(Status::Unbounded),
        H::UnboundedOrInfeasible => S(Status::InfeasibleOrUnbounded),
        H::ReachedTimeLimit => S(Status::TimeLimit),
        H::ReachedIterationLimit => S(Status::IterationLimit),
        H::ReachedSolutionLimit => S(Status::SolutionLimit),
        H::ObjectiveBound | H::ObjectiveTarget => S(Status::ObjectiveLimit),
        H::ReachedMemoryLimit => S(Status::MemoryLimit),
        H::PresolveError | H::SolveError | H::PostsolveError => S(Status::NumericalFailure),
        H::Unknown => S(Status::Unknown),
        H::ReachedInterrupt => Interrupted,
        H::NotSet | H::LoadError | H::ModelError | H::ModelEmpty => NotRun,
        // `HighsModelStatus` is non-exhaustive; the crate is pinned, so this
        // only matters after an upgrade, which must revisit this table.
        _ => S(Status::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_status_has_an_explicit_meaning() {
        let table = [
            (H::Optimal, Mapped::Status(Status::Optimal)),
            (H::Infeasible, Mapped::Status(Status::Infeasible)),
            (H::Unbounded, Mapped::Status(Status::Unbounded)),
            (
                H::UnboundedOrInfeasible,
                Mapped::Status(Status::InfeasibleOrUnbounded),
            ),
            (H::ReachedTimeLimit, Mapped::Status(Status::TimeLimit)),
            (
                H::ReachedIterationLimit,
                Mapped::Status(Status::IterationLimit),
            ),
            (
                H::ReachedSolutionLimit,
                Mapped::Status(Status::SolutionLimit),
            ),
            (H::ObjectiveBound, Mapped::Status(Status::ObjectiveLimit)),
            (H::ObjectiveTarget, Mapped::Status(Status::ObjectiveLimit)),
            (H::ReachedMemoryLimit, Mapped::Status(Status::MemoryLimit)),
            (H::PresolveError, Mapped::Status(Status::NumericalFailure)),
            (H::SolveError, Mapped::Status(Status::NumericalFailure)),
            (H::PostsolveError, Mapped::Status(Status::NumericalFailure)),
            (H::Unknown, Mapped::Status(Status::Unknown)),
            (H::ReachedInterrupt, Mapped::Interrupted),
            (H::NotSet, Mapped::NotRun),
            (H::LoadError, Mapped::NotRun),
            (H::ModelError, Mapped::NotRun),
            (H::ModelEmpty, Mapped::NotRun),
        ];
        for (raw, expected) in table {
            assert_eq!(map(raw), expected, "{raw:?}");
        }
    }
}
