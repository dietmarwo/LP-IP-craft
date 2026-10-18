"""Python interface to lp-ip-craft: LP and mixed-integer LP in Rust.

``solve(model, backend="native")`` optimizes with the native Rust engine. The
default ``backend="dummy"`` is kept for compatibility with v0.1: it evaluates
one candidate and returns ``status == "stub"``. ``evaluate`` and
``lp_residuals`` are solver-independent checks that also work on solutions
from other solvers.

The model is::

    minimize or maximize   objective_offset + objective @ x
    subject to             row_lower <= A @ x <= row_upper
                           lower     <=   x   <= upper
                           x[j] integer where kinds[j] is "integer" or "binary"

with ``A`` given in canonical zero-based CSR form (``indptr``, ``indices``,
``values``). Sequences may be lists, tuples or 1-D NumPy arrays.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Iterable, Literal, Optional

from ._native import BackendError, Cancellation, EvaluationError, Model
from ._native import backends as _backends
from ._native import evaluate as _evaluate
from ._native import lp_residuals as _lp_residuals
from ._native import safe_dual_bound as _safe_dual_bound
from ._native import solve_native as _solve_native
from ._native import solve_batch_native as _solve_batch_native

__version__ = "0.28.0"

__all__ = [
    "BACKENDS",
    "solve_batch",
    "BackendError",
    "BackendReport",
    "Cancellation",
    "Duals",
    "Evaluation",
    "EvaluationError",
    "Gap",
    "NativeOptions",
    "LpResiduals",
    "Model",
    "Options",
    "SafeBoundStatus",
    "SafeDualBound",
    "SolveResult",
    "Status",
    "evaluate",
    "lp_residuals",
    "safe_dual_bound",
    "solve",
]

BACKENDS: tuple[str, ...] = tuple(_backends())
"""Backends compiled into this build, for example ``("dummy", "native")``."""

DEFAULT_FEASIBILITY_TOLERANCE = 1e-7
DEFAULT_INTEGRALITY_TOLERANCE = 1e-7

Status = Literal[
    "stub",
    "cancelled",
    "optimal",
    "infeasible",
    "unbounded",
    "infeasible_or_unbounded",
    "time_limit",
    "node_limit",
    "iteration_limit",
    "solution_limit",
    "objective_limit",
    "memory_limit",
    "numerical_failure",
    "unknown",
]
"""Why a solve ended. Candidate quality is reported separately."""


@dataclass(frozen=True, kw_only=True)
class Options:
    """Settings every backend honours."""

    feasibility_tolerance: float = DEFAULT_FEASIBILITY_TOLERANCE
    """Largest accepted bound violation, absolute and in original units."""
    integrality_tolerance: float = DEFAULT_INTEGRALITY_TOLERANCE
    """Largest accepted distance of an integer variable to an integer, in (0, 0.5)."""
    threads: int = 1
    """Maximum native numerical workers, 1 through 8. A width above 1 serializes
    the outer mechanisms (component, residual and tree workers)."""


@dataclass(frozen=True, kw_only=True)
class NativeOptions:
    """Controls for the native Rust engine: a bounded-variable dual revised
    simplex on a sparse LU factorization inside best-bound branch and bound.
    Field meanings and units are documented in ``docs/api.md``."""
    time_limit_seconds: Optional[float] = None
    node_limit: Optional[int] = 100_000
    iteration_limit: Optional[int] = 1_000_000
    pivot_tolerance: float = 1e-9
    dual_tolerance: float = 1e-7
    max_factor_entries: int = 4_000_000
    refactor_interval: int = 256
    adaptive_refactor: bool = True
    basis_update: Literal["auto", "eta", "forrest_tomlin"] = "auto"
    factor_pivot_search: int = 4
    factor_ordering: Literal["auto", "natural", "min_degree"] = "auto"
    factor_replay: bool = False
    mip_diagnostics: bool = False
    mip_trace_limit: int = 256
    adaptive_pricing: bool = True
    devex: bool = False
    avoid_dominant_components: bool = True
    singleton_presolve: bool = True
    root_propagation: bool = True
    mixed_propagation: bool = False
    root_cuts: bool = False
    root_separation: bool = False
    mip_presolve: bool = True
    coefficient_tightening: bool = True
    dual_presolve: bool = False
    mip_presolve_rounds: int = 8
    gmi_cuts: bool = False
    mir_cuts: bool = False
    mip_cut_rounds: int = 8
    mip_cut_limit: int = 16
    cut_exact_work: int = 4_000_000
    root_heuristics: bool = False
    heuristic_lp_calls: int = 16
    branching_score: str = "weighted_sum"
    pseudocost_min_step: float = 0.0
    branch_trace_limit: int = 0
    root_cut_rounds: int = 2
    cover_lifting: bool = True
    steepest_edge: bool = True
    presolve: bool = True
    redundant_rows: bool = True
    redundant_row_min_ratio: float = 1.0
    node_reoptimization: bool = True
    strong_branching: bool = False
    reliability_branching: bool = False
    reliability_threshold: int = 2
    reliability_max_probes: int = 64
    reduced_cost_fixing: bool = True
    incumbent_heuristic: bool = True
    rational_certification: bool = True
    feasibility_jump: bool = False
    feasibility_jump_steps: int = 2000
    objective_lattice: bool = True
    decompose: bool = True
    decomposition_min_variables: int = 32
    component_threads: int = 16
    component_parallel_min_entries: int = 2048
    packing_root: bool = False
    packing_root_iterations: int = 1000
    packing_root_work: int = 200_000_000
    packing_neighborhoods: int = 0
    packing_neighborhood_size: int = 192
    packing_workers: int = 1
    packing_neighborhood_nodes: int = 128
    packing_cuts: bool = False
    packing_cut_work: int = 2_000_000
    packing_combine_repairs: bool = False
    packing_rounds: int = 1
    packing_neighborhood_growth: int = 0
    packing_seed_strategy: str = "incumbent"
    packing_residual_cliques: bool = False
    packing_clique_work: int = 50_000
    packing_price_diagnostics: bool = False
    packing_repair_seconds: Optional[float] = None
    packing_proof_seconds: Optional[float] = None
    partial_dual_bounds: bool = False
    scalable_branching: bool = False
    branch_probe_candidates: int = 4
    branch_probe_pivots: int = 32
    branch_probe_work: int = 2_000_000
    tree_threads: int = 1
    tree_split_depth: int = 2


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
class LpResiduals:
    """LP residuals of a point with duals; see ``lp_residuals``."""

    primal: float
    stationarity: float
    complementarity: float
    dual_sign: float
    duality_gap: float


SafeBoundStatus = Literal[
    "finite", "unbounded_row", "unbounded_variable",
    "arithmetic_overflow", "unsupported_arithmetic",
]


@dataclass(frozen=True)
class SafeDualBound:
    """An independent bound for the exact binary64 model, or an explanation
    of why this calculation could not supply one. Not a solver status."""

    bound: Optional[float]
    """Lower bound when minimizing, upper when maximizing; includes offset.
    Present exactly for status ``finite``."""
    status: SafeBoundStatus
    index: Optional[int]
    """Zero-based row/variable for ``unbounded_row``/``unbounded_variable``;
    otherwise None. These outcomes do not prove the model unbounded."""


@dataclass(frozen=True)
class BackendReport:
    """Provenance, applied settings and timings of one solve."""

    name: str
    version: Optional[str]
    """Native engine package version."""
    raw_status: Optional[str]
    """The engine's own status name; ``None`` if the engine never ran."""
    start_status: Literal["not_provided", "submitted", "rejected", "evaluated"]
    """``submitted`` means the engine took the start, not that it became the incumbent."""
    effective_options: dict[str, str]
    """Native settings applied, by native name."""
    total_seconds: float
    conversion_seconds: float
    solve_seconds: float
    check_seconds: float
    time_budget_exceeded: bool
    """The call outlasted its time budget; ``status`` still says why it stopped."""


@dataclass(frozen=True)
class SolveResult:
    status: Status
    """Why the solve ended; see the ``Status`` values."""
    message: str
    candidate: Optional[list[float]]
    """The returned point (the incumbent, for a MIP); None if none was declared."""
    evaluation: Optional[Evaluation]
    """Independent evaluation of ``candidate``; present exactly when it is."""
    duals: Optional[Duals]
    """LP duals; never for a MIP."""
    best_bound: Optional[float]
    """A proven bound in the original sense (lower when minimizing, upper when
    maximizing), including the offset. Never estimated."""
    gap: Optional[Gap]
    """Present when there is a ``best_bound`` and a feasible candidate."""
    nodes: int
    iterations: int
    backend: BackendReport


def solve(
    model: Model,
    *,
    backend: Literal["dummy", "native"] = "dummy",
    start: Optional[Iterable[float]] = None,
    options: Optional[Options] = None,
    backend_options: Optional[NativeOptions] = None,
    cancellation: Optional[Cancellation] = None,
) -> SolveResult:
    """Solve ``model`` with ``backend``, optionally from a complete ``start``.

    Use ``backend="native"`` to optimize; the default ``"dummy"`` only evaluates
    one candidate. The start is used as given: it need not be feasible or
    integral and is never rounded or repaired. The interpreter is released
    during the solve, so solves from several threads run in parallel.

    Raises ``ValueError`` for malformed input, ``TypeError`` for wrongly typed
    input, ``NotImplementedError`` for unsupported settings or a backend that
    is not compiled in (see ``BACKENDS``), ``EvaluationError`` if
    floating-point evaluation overflows and ``BackendError`` if the engine
    fails to set up or run the model.
    """
    options = options or Options()
    raw = _solve_native(
        model,
        start,
        options.feasibility_tolerance,
        options.integrality_tolerance,
        options.threads,
        cancellation if cancellation is not None else Cancellation(),
        backend,
        None if backend_options is None else asdict(backend_options),
    )
    return _result_from_raw(raw)


def _result_from_raw(raw: dict) -> SolveResult:
    evaluation, duals, gap = raw["evaluation"], raw["duals"], raw["gap"]
    return SolveResult(
        status=raw["status"],
        message=raw["message"],
        candidate=raw["candidate"],
        evaluation=None if evaluation is None else Evaluation(**evaluation),
        duals=None if duals is None else Duals(**duals),
        best_bound=raw["best_bound"],
        gap=None if gap is None else Gap(**gap),
        nodes=raw["nodes"],
        iterations=raw["iterations"],
        backend=BackendReport(**raw["backend"]),
    )


def solve_batch(models: Iterable[Model], *, workers: int = 1, options: Optional[Options] = None,
                backend_options: Optional[NativeOptions] = None,
                cancellation: Optional[Cancellation] = None) -> list[SolveResult | Exception]:
    """Solve independent native jobs in order with 1..16 Rust workers.

    Budgets apply per job. Nested component, residual and tree workers are disabled inside the pool.
    ``options.threads`` is each job's numerical width; above 1, ``workers * threads`` must not
    exceed 8.
    Returns an exception value for a failed job; batch argument errors raise.
    Cancellation is shared, and queued jobs report cancellation too.
    """
    options = options or Options()
    if isinstance(workers, bool) or not isinstance(workers, int):
        raise TypeError("workers must be an integer")
    if not 1 <= workers <= 16:
        raise ValueError("workers must be in 1..16")
    if options.threads > 1 and options.threads * workers > 8:
        raise ValueError("batch outer workers times numerical threads must not exceed 8")
    raw = _solve_batch_native(list(models), workers, options.feasibility_tolerance,
                             options.integrality_tolerance, options.threads,
                             cancellation if cancellation is not None else Cancellation(),
                             None if backend_options is None else asdict(backend_options))
    return [r if isinstance(r, Exception) else _result_from_raw(r) for r in raw]


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


def lp_residuals(model: Model, x: Iterable[float], duals: Duals) -> LpResiduals:
    """Check ``x`` and ``duals`` as a solution of the LP, ignoring integrality.

    ``stationarity`` is ``max |c - A.T @ y - z|``; ``complementarity`` the
    largest ``|dual| * distance to the bound its sign selects``;
    ``dual_sign`` the largest dual that selects an infinite bound;
    ``duality_gap`` the relative primal-dual objective gap
    ``|P - D| / (1 + |P| + |D|)``. All are zero at an optimal pair.
    """
    raw = _lp_residuals(model, x, duals.row_duals, duals.reduced_costs)
    return LpResiduals(**raw)


def safe_dual_bound(model: Model, row_duals: Iterable[float]) -> SafeDualBound:
    """Bound the objective using arbitrary finite row duals in the original
    sense, with outward-rounded residual correction over the variable box.

    No primal point, reduced costs or tolerance is used. Integrality is relaxed;
    the bound applies to every feasible LP/MIP point of the stored binary64
    model. It may be weak or unavailable for poor duals or infinite bounds:
    with solver duals, a column with an infinite bound usually makes it
    ``unbounded_variable``. Duals of the LP with one-sided costs shifted by a
    small margin avoid that (``margin_duals`` in ``examples/safe_bound.py``);
    free columns need finite bounds.
    Requires standard IEEE binary64 arithmetic with gradual underflow; runtime
    probes reject flush-to-zero modes. The interpreter is released.

    This does not certify a tolerance-feasible incumbent, a solver's search
    tree, or ``solve(...).best_bound``. It makes no pruning decision and does
    not modify solve results. Available even in dummy-only builds.
    """
    return SafeDualBound(**_safe_dual_bound(model, row_duals))
