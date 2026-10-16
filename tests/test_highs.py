"""End-to-end checks of the HiGHS backend through the compiled extension.

Reference values come from analytic solutions, exhaustive enumeration and a
pure-Python residual check that does not use the Rust checker.
"""

import math
import random
import typing
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
from multiprocessing import get_context
from threading import Timer

import pytest

from lp_ip_craft import (
    BACKENDS,
    Cancellation,
    Duals,
    EvaluationError,
    HighsOptions,
    Model,
    Status,
    lp_residuals,
    solve,
)

pytestmark = pytest.mark.skipif("highs" not in BACKENDS, reason="built without HiGHS")

INF = math.inf

PRODUCTION = dict(
    # LP/IP tutorial, Example 1: optimum (30, 10, 0), profit 2150.
    sense="maximize",
    objective=[45.0, 80.0, 110.0],
    lower=[0.0] * 3,
    upper=[INF] * 3,
    indptr=[0, 3, 6, 9],
    indices=[0, 1, 2] * 3,
    values=[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0],
    row_lower=[-INF] * 3,
    row_upper=[400.0, 450.0, 60.0],
)


def make_ip(offset=0.0, sense="maximize"):
    """2x + y <= 4, x + 2y <= 4, x, y >= 0 integer; maximize 3x + 2y: 6 at (2, 0)."""
    sign = 1.0 if sense == "maximize" else -1.0
    return Model(
        objective=[3.0 * sign, 2.0 * sign], objective_offset=offset, sense=sense,
        lower=[0.0, 0.0], upper=[INF, INF], kinds=["integer"] * 2,
        indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[2.0, 1.0, 1.0, 2.0],
        row_lower=[-INF] * 2, row_upper=[4.0, 4.0],
    )


def knapsack(n, m, seed):
    rng = random.Random(seed)
    values = [rng.randint(1, 50) for _ in range(n * m)]
    return Model(
        sense="maximize", objective=[rng.randint(1, 100) for _ in range(n)],
        lower=[0] * n, upper=[1] * n, kinds=["binary"] * n,
        indptr=[i * n for i in range(m + 1)], indices=list(range(n)) * m, values=values,
        row_lower=[-INF] * m, row_upper=[sum(values[i * n:(i + 1) * n]) * 0.4 for i in range(m)],
    )


def worker(offset):
    return solve(make_ip(offset), backend="highs").evaluation.objective


def independent_lp_check(data, x, y, z):
    """Pure-Python residuals from the original arrays, in the crate's convention."""
    n, m = len(data["objective"]), len(data["row_lower"])
    c, sign = data["objective"], 1.0 if data.get("sense", "minimize") == "minimize" else -1.0
    rows = [[] for _ in range(m)]
    for i in range(m):
        for k in range(data["indptr"][i], data["indptr"][i + 1]):
            rows[i].append((data["indices"][k], data["values"][k]))
    activity = [sum(a * x[j] for j, a in rows[i]) for i in range(m)]
    aty = [0.0] * n
    for i in range(m):
        for j, a in rows[i]:
            aty[j] += a * y[i]
    stationarity = max(abs(c[j] - aty[j] - z[j]) for j in range(n))
    primal, dual_objective = 0.0, 0.0
    for v, lo, hi, mu in [*zip(activity, data["row_lower"], data["row_upper"], y),
                          *zip(x, data["lower"], data["upper"], z)]:
        primal = max(primal, lo - v, v - hi)
        if sign * mu > 0:
            assert lo > -INF, "dual selects a missing lower bound"
            dual_objective += mu * lo
        elif sign * mu < 0:
            assert hi < INF, "dual selects a missing upper bound"
            dual_objective += mu * hi
    primal_objective = sum(cj * xj for cj, xj in zip(c, x))
    gap = abs(primal_objective - dual_objective) / (1 + abs(primal_objective) + abs(dual_objective))
    return primal, stationarity, gap


# --- optima -----------------------------------------------------------------------


@pytest.mark.parametrize("sense", ["maximize", "minimize"])
@pytest.mark.parametrize("offset", [-7.0, 0.0, 7.0])
def test_integer_optimum_bound_and_oracle(sense, offset):
    sign = 1.0 if sense == "maximize" else -1.0
    result = solve(make_ip(offset, sense), backend="highs", start=[0.5, 0.5])
    feasible = [(x, y) for x in range(3) for y in range(3) if 2 * x + y <= 4 and x + 2 * y <= 4]
    pick = max if sense == "maximize" else min
    reference = pick(sign * (3 * x + 2 * y) + offset for x, y in feasible)
    assert result.status == "optimal", result.message
    assert result.evaluation.objective == pytest.approx(reference, abs=1e-8)
    assert result.best_bound == pytest.approx(reference, abs=1e-8)
    assert result.gap.absolute == pytest.approx(0.0, abs=1e-8)
    x, y = result.candidate
    assert 2 * x + y <= 4 + 1e-7 and x + 2 * y <= 4 + 1e-7
    assert max(abs(x - round(x)), abs(y - round(y))) <= 1e-7
    assert result.duals is None
    report = result.backend
    assert report.name == "highs" and report.version and report.raw_status == "Optimal"
    assert report.start_status in ("submitted", "rejected")
    assert report.effective_options["threads"] == "1"
    assert not report.time_budget_exceeded


def test_lp_duals_agree_with_an_independent_python_check():
    model = Model(**PRODUCTION)
    r = solve(model, backend="highs")
    assert r.status == "optimal", r.message
    assert r.evaluation.objective == pytest.approx(2150.0)
    assert r.duals.row_duals == pytest.approx([0.0, 11 / 3, 25 / 3])
    assert r.duals.reduced_costs == pytest.approx([0.0, 0.0, -5.0])
    primal, stationarity, gap = independent_lp_check(
        PRODUCTION, r.candidate, r.duals.row_duals, r.duals.reduced_costs)
    assert max(primal, stationarity, gap) < 1e-9
    rust = lp_residuals(model, r.candidate, r.duals)
    assert rust.duality_gap == pytest.approx(gap, abs=1e-12)
    assert rust.stationarity == pytest.approx(stationarity, abs=1e-12)
    assert r.best_bound is None and r.gap is None  # no LP bound route yet


def test_mixed_binary_continuous():
    model = Model(objective=[2.0, 3.0], lower=[0.0, 0.0], upper=[3.0, 1.0],
                  kinds=["continuous", "binary"], indptr=[0, 2], indices=[0, 1],
                  values=[1.0, 2.0], row_lower=[2.5], row_upper=[INF])
    r = solve(model, backend="highs")
    assert r.status == "optimal"
    assert r.candidate == pytest.approx([0.5, 1.0])
    assert r.evaluation.objective == pytest.approx(4.0)


def test_zero_objective_gap_is_zero_not_nan():
    r = solve(make_ip(-6.0), backend="highs")
    assert r.evaluation.objective == pytest.approx(0.0, abs=1e-12)
    assert r.gap.absolute == 0 and r.gap.relative == 0


def test_infeasible_and_unbounded_are_not_optima():
    infeasible = Model(objective=[1.0], lower=[-INF], upper=[INF], indptr=[0, 1, 2],
                       indices=[0, 0], values=[1.0, 1.0], row_lower=[1.0, -INF],
                       row_upper=[INF, 0.0])
    r = solve(infeasible, backend="highs")
    assert r.status == "infeasible" and r.candidate is None and r.best_bound is None
    unbounded = Model(objective=[-1.0], lower=[0.0], upper=[INF], indptr=[0], indices=[],
                      values=[], row_lower=[], row_upper=[])
    assert solve(unbounded, backend="highs").status in ("unbounded", "infeasible_or_unbounded")


def test_every_status_is_typed():
    assert set(typing.get_args(Status)) >= {"optimal", "node_limit", "numerical_failure"}
    assert solve(make_ip(), backend="highs").status in typing.get_args(Status)


# --- dispatch, settings and errors ----------------------------------------------------


def test_dispatch_and_setting_errors():
    with pytest.raises(ValueError, match="unknown backend"):
        solve(make_ip(), backend="glpk")
    with pytest.raises(NotImplementedError, match="dummy"):
        solve(make_ip(), backend_options=HighsOptions())
    with pytest.raises(ValueError, match="time_limit_seconds"):
        solve(make_ip(), backend="highs", backend_options=HighsOptions(time_limit_seconds=-1))
    with pytest.raises(TypeError, match="backend_options.node_limit"):
        solve(make_ip(), backend="highs", backend_options=HighsOptions(node_limit="10"))
    with pytest.raises(NotImplementedError, match="node_limit"):
        solve(Model(**PRODUCTION), backend="highs", backend_options=HighsOptions(node_limit=5))
    with pytest.raises(NotImplementedError, match="iteration_limit"):
        solve(make_ip(), backend="highs", backend_options=HighsOptions(iteration_limit=5))
    with pytest.raises(NotImplementedError, match="matrix value"):
        solve(Model(objective=[1.0], lower=[0.0], upper=[1.0], indptr=[0, 1], indices=[0],
                    values=[1e-13], row_lower=[0.0], row_upper=[1.0]), backend="highs")


def test_overflow_is_rejected_by_the_public_checker():
    m = Model(objective=[1.0], lower=[-INF], upper=[INF], indptr=[0, 1, 2], indices=[0, 0],
              values=[1e308, -1e308], row_lower=[0.0, 0.0], row_upper=[0.0, 0.0])
    with pytest.raises(EvaluationError, match="overflowed"):
        lp_residuals(m, [0.0], Duals([1e308, 1e308], [0.0]))


# --- limits, starts and cancellation --------------------------------------------------------


def test_zero_time_budget():
    r = solve(make_ip(), backend="highs", backend_options=HighsOptions(time_limit_seconds=0))
    assert r.status == "time_limit" and r.candidate is None
    assert r.backend.time_budget_exceeded and r.backend.raw_status is None


def test_zero_node_limit_with_and_without_a_submitted_incumbent():
    model = knapsack(100, 12, 3)
    for start in (None, [0] * 100):
        result = solve(model, start=start, backend="highs",
                       backend_options=HighsOptions(node_limit=0, time_limit_seconds=10))
        assert result.status == "node_limit", result.message
        assert result.backend.raw_status == "ReachedSolutionLimit"
        assert result.nodes == 0
        assert (result.candidate is not None) == (start is not None)
        if result.evaluation is not None:
            assert result.evaluation.feasible_candidate and result.evaluation.objective == 0


def test_cancellation_before_the_run_and_input_precedence():
    token = Cancellation()
    token.cancel()
    r = solve(make_ip(), backend="highs", cancellation=token)
    assert r.status == "cancelled" and r.backend.raw_status is None
    with pytest.raises(ValueError, match="start"):
        solve(make_ip(), backend="highs", cancellation=token, start=[math.nan, 0.0])


def test_cancellation_from_another_thread_reaches_highs():
    token = Cancellation()
    timer = Timer(0.1, token.cancel)
    timer.start()
    try:
        result = solve(knapsack(400, 30, 11), backend="highs", cancellation=token,
                       backend_options=HighsOptions(time_limit_seconds=20))
    finally:
        timer.cancel()
    # Finishing before the request is legitimate; a stop must say why.
    assert result.status in ("cancelled", "optimal"), result.message
    if result.status == "cancelled":
        assert result.backend.raw_status == "ReachedInterrupt"
    if result.evaluation is not None:
        assert result.evaluation.feasible_candidate


# --- parallel use -------------------------------------------------------------------------


def test_threads_share_one_model():
    model = make_ip(1.0)
    with ThreadPoolExecutor(max_workers=4) as pool:
        objectives = list(pool.map(lambda _: solve(model, backend="highs").evaluation.objective,
                                   range(16)))
    assert objectives == pytest.approx([7.0] * 16)


def test_two_process_workers_are_isolated():
    with ProcessPoolExecutor(2, mp_context=get_context("spawn")) as pool:
        assert list(pool.map(worker, [0.0, 7.0])) == pytest.approx([6.0, 13.0])
