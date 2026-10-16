"""Analytic and independent Python checks of the compiled HiGHS adapter."""
from concurrent.futures import ProcessPoolExecutor
from multiprocessing import get_context
from threading import Thread
from time import sleep
import math
import pytest
from lp_ip_craft import Model, Options, HighsOptions, Cancellation, Duals, solve, lp_residuals, EvaluationError


def make_ip(offset=0.0):
    return Model(objective=[3., 2.], objective_offset=offset, sense="maximize", lower=[0., 0.], upper=[math.inf, math.inf], kinds=["integer"]*2,
                 indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[2., 1., 1., 2.], row_lower=[-math.inf]*2, row_upper=[4., 4.])


def worker(offset):
    return solve(make_ip(offset), backend="highs").evaluation.objective


@pytest.mark.parametrize("offset", [-7., 0., 7.])
def test_integer_solution_bound_and_independent_oracle(offset):
    result = solve(make_ip(offset), backend="highs", start=[0.5, 0.5])
    reference = max(3*x + 2*y + offset for x in range(3) for y in range(3) if 2*x+y <= 4 and x+2*y <= 4)
    assert result.status == "optimal"
    assert abs(result.evaluation.objective-reference) < 1e-8
    assert abs(result.best_bound-reference) < 1e-8
    x, y = result.candidate
    assert 2*x+y <= 4+1e-7 and x+2*y <= 4+1e-7
    assert max(abs(x-round(x)), abs(y-round(y))) <= 1e-7
    assert result.start_status in ("submitted", "rejected")
    assert result.backend == "highs" and result.backend_version
    assert result.effective_options["threads"] == "1"


def test_lp_duals_and_zero_objective_gap():
    model = Model(objective=[-1.], lower=[0.], upper=[1.], indptr=[0], indices=[], values=[], row_lower=[], row_upper=[])
    r = solve(model, backend="highs")
    assert r.status == "optimal" and r.candidate == pytest.approx([1.])
    assert max(vars(lp_residuals(model, r.candidate, r.duals)).values()) < 1e-7
    assert r.best_bound is None and r.gap is None
    r = solve(make_ip(-6.), backend="highs")
    assert r.gap.absolute == 0 and r.gap.relative == 0


def test_overflow_is_rejected_by_public_checker():
    m = Model(objective=[1.], lower=[-math.inf], upper=[math.inf], indptr=[0,1,2], indices=[0,0], values=[1e308,-1e308], row_lower=[0.,0.], row_upper=[0.,0.])
    with pytest.raises(EvaluationError):
        lp_residuals(m, [0.], Duals([1e308,1e308], [0.]))


def test_limits_dispatch_and_invalid_inputs():
    assert solve(make_ip(), backend="highs", backend_options=HighsOptions(time_limit_seconds=0)).status == "time_limit"
    with pytest.raises(NotImplementedError):
        solve(make_ip(), backend_options=HighsOptions())
    with pytest.raises(ValueError):
        solve(make_ip(), backend="unknown")
    with pytest.raises(ValueError):
        solve(make_ip(), backend="highs", backend_options=HighsOptions(time_limit_seconds=-1))
    token = Cancellation(); token.cancel()
    assert solve(make_ip(), backend="highs", cancellation=token).status == "cancelled"
    with pytest.raises(ValueError):
        solve(make_ip(), backend="highs", cancellation=token, start=[float("nan"),0.])


def test_two_process_workers_are_isolated():
    with ProcessPoolExecutor(2, mp_context=get_context("spawn")) as pool:
        assert list(pool.map(worker, [0., 7.])) == pytest.approx([6., 13.])


def test_cooperative_thread_cancellation():
    # Enough combinatorial work to exercise an interrupt, with a finite fallback deadline.
    n, m = 500, 35
    a = [[float((i*71+j*37+i*j*17)%97+1) for j in range(n)] for i in range(m)]
    model = Model(objective=[float((j*19)%71+1) for j in range(n)], sense="maximize", lower=[0.]*n, upper=[1.]*n, kinds=["binary"]*n,
                  indptr=[i*n for i in range(m+1)], indices=list(range(n))*m, values=[v for row in a for v in row], row_lower=[-math.inf]*m, row_upper=[sum(row)*.45 for row in a])
    token = Cancellation()
    thread = Thread(target=lambda: (sleep(.05), token.cancel()))
    thread.start()
    try:
        result = solve(model, backend="highs", cancellation=token, backend_options=HighsOptions(time_limit_seconds=3))
    finally:
        thread.join()
    assert result.status in ("cancelled", "optimal")  # completing before the request is legitimate
    if result.candidate is not None:
        assert result.evaluation.feasible_candidate


def test_zero_node_limit_with_and_without_a_submitted_incumbent():
    import random
    rng = random.Random(3)
    n, m = 100, 12
    values = [rng.randint(1, 50) for _ in range(n * m)]
    model = Model(
        sense="maximize", objective=[rng.randint(1, 100) for _ in range(n)],
        lower=[0] * n, upper=[1] * n, kinds=["binary"] * n,
        indptr=[i * n for i in range(m + 1)], indices=list(range(n)) * m,
        values=values, row_lower=[-float("inf")] * m,
        row_upper=[sum(values[i*n:(i+1)*n]) * .4 for i in range(m)],
    )
    for start in (None, [0] * n):
        result = solve(model, start=start, backend="highs",
                       backend_options=HighsOptions(node_limit=0, time_limit_seconds=10))
        assert result.status == "node_limit"
        assert result.raw_status == "ReachedSolutionLimit"
        assert result.nodes == 0
        assert (result.candidate is not None) == (start is not None)
        if result.evaluation is not None:
            assert result.evaluation.feasible_candidate
            assert result.evaluation.objective == 0
        assert result.best_bound is None and result.gap is None
