"""Python interface to the lp-ip-craft LP / mixed-integer LP contracts.

**Interface draft. No optimizer is implemented.** ``solve`` currently runs a
dummy backend that evaluates one candidate and returns it with
``status == "stub"``. ``evaluate`` is the solver-independent candidate check;
it can already be used on solutions from other solvers.

The model is::

    minimize or maximize   objective_offset + objective @ x
    subject to             row_lower <= A @ x <= row_upper
                           lower     <=   x   <= upper
                           x[j] integer where kinds[j] is "integer" or "binary"

with ``A`` given in canonical zero-based CSR form (``indptr``, ``indices``,
``values``). Sequences may be lists, tuples or 1-D NumPy arrays.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable, Literal, Optional

from ._native import Cancellation, EvaluationError, Model
from ._native import evaluate as _evaluate
from ._native import solve_dummy as _solve_dummy

__version__ = "0.1.0"

__all__ = [
    "Cancellation",
    "Duals",
    "Evaluation",
    "EvaluationError",
    "Gap",
    "Model",
    "Options",
    "SolveResult",
    "evaluate",
    "solve",
]

DEFAULT_FEASIBILITY_TOLERANCE = 1e-7
DEFAULT_INTEGRALITY_TOLERANCE = 1e-7


@dataclass(frozen=True, kw_only=True)
class Options:
    """Solver settings. Only settings a backend honours are exposed."""

    feasibility_tolerance: float = DEFAULT_FEASIBILITY_TOLERANCE
    """Largest accepted bound violation, absolute and in original units."""
    integrality_tolerance: float = DEFAULT_INTEGRALITY_TOLERANCE
    """Largest accepted distance of an integer variable to an integer, in (0, 0.5)."""
    threads: int = 1
    """Only 1 is supported; run independent solves for parallelism."""


@dataclass(frozen=True)
class Evaluation:
    """Independently recomputed values at a point. ``feasible_candidate``
    compares both violations with the tolerances; it says nothing about
    optimality, and ``False`` does not show that the model is infeasible."""

    objective: float
    row_activity: list[float]
    primal_violation: float
    integrality_violation: float
    feasible_candidate: bool


@dataclass(frozen=True)
class Duals:
    """LP duals. Convention, in the original sense: ``c = A.T @ row_duals +
    reduced_costs`` at an optimum. Minimizing, positive values belong to
    active lower bounds; maximizing, to active upper bounds (Gurobi's
    Pi/RC and HiGHS's row_dual/col_dual)."""

    row_duals: list[float]
    reduced_costs: list[float]


@dataclass(frozen=True)
class Gap:
    """``absolute`` is ``objective - best_bound`` when minimizing and
    ``best_bound - objective`` when maximizing; ``relative`` divides it by
    ``|objective|`` (0 if both are 0, else +-inf when the objective is 0)."""

    absolute: float
    relative: float


@dataclass(frozen=True)
class SolveResult:
    status: Literal["stub", "cancelled"]
    """More statuses arrive with real backends."""
    message: str
    candidate: Optional[list[float]]
    """The returned point (the incumbent, for a MIP); None if none was evaluated."""
    evaluation: Optional[Evaluation]
    """Independent evaluation of ``candidate``; present exactly when it is."""
    duals: Optional[Duals]
    best_bound: Optional[float]
    """A proven bound in the original sense (lower when minimizing, upper when
    maximizing), including the offset. Never estimated."""
    gap: Optional[Gap]
    """Present when there is a ``best_bound`` and a feasible candidate."""
    nodes: int
    iterations: int


def _evaluation(raw: Optional[dict]) -> Optional[Evaluation]:
    return None if raw is None else Evaluation(**raw)


def solve(
    model: Model,
    *,
    start: Optional[Iterable[float]] = None,
    options: Optional[Options] = None,
    cancellation: Optional[Cancellation] = None,
) -> SolveResult:
    """Solve ``model``, optionally from a complete primal ``start`` point.

    The start is used as given: it need not be feasible or integral and is
    never rounded or repaired. The interpreter is released during the solve.

    Raises ``ValueError`` for malformed input, ``TypeError`` for wrongly typed
    input, ``NotImplementedError`` for unsupported options, and
    ``EvaluationError`` if floating-point evaluation overflows.
    """
    options = options or Options()
    raw = _solve_dummy(
        model,
        start,
        options.feasibility_tolerance,
        options.integrality_tolerance,
        options.threads,
        cancellation if cancellation is not None else Cancellation(),
    )
    duals = raw["duals"]
    gap = raw["gap"]
    return SolveResult(
        status=raw["status"],
        message=raw["message"],
        candidate=raw["candidate"],
        evaluation=_evaluation(raw["evaluation"]),
        duals=None if duals is None else Duals(**duals),
        best_bound=raw["best_bound"],
        gap=None if gap is None else Gap(**gap),
        nodes=raw["nodes"],
        iterations=raw["iterations"],
    )


def evaluate(
    model: Model,
    x: Iterable[float],
    *,
    feasibility_tolerance: float = DEFAULT_FEASIBILITY_TOLERANCE,
    integrality_tolerance: float = DEFAULT_INTEGRALITY_TOLERANCE,
) -> Evaluation:
    """Check the point ``x`` against ``model`` independently of any solver.

    Recomputes the objective (with offset, original sense), the row
    activities, the largest bound violation and the largest integrality
    violation, in original units. Nothing is rounded or repaired. Use it to
    check solutions from any solver, including commercial reference solvers.
    """
    raw = _evaluate(model, x, feasibility_tolerance, integrality_tolerance)
    return Evaluation(**raw)
