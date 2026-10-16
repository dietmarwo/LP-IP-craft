"""Sparse wrapper, limits and independently checked solutions."""
import math
from concurrent.futures import ThreadPoolExecutor
import pytest
from lp_ip_craft import Model, NativeOptions, Cancellation, solve, lp_residuals


def fixture():
    return Model(objective=[3., 2.], lower=[0., 0.], upper=[math.inf]*2,
                 row_lower=[-math.inf]*2, row_upper=[4., 5.],
                 indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[1., 1., 2., 1.],
                 sense="maximize")


@pytest.mark.parametrize("interval", [1, 2, 64, 10000])
@pytest.mark.parametrize("presolve", [False, True])
def test_sparse_solution_and_duals(interval, presolve):
    m = fixture()
    r = solve(m, backend="native", backend_options=NativeOptions(
        sparse=True, sparse_refactor_interval=interval, max_tableau_entries=4, presolve=presolve))
    assert r.status == "optimal" and r.evaluation.objective == pytest.approx(9.)
    k = lp_residuals(m, r.candidate, r.duals)
    assert max(k.primal, k.duality_gap, k.dual_sign, k.stationarity) < 1e-7
    assert r.backend.effective_options["linear_kernel"] == "sparse_revised_simplex"


@pytest.mark.parametrize("field,value", [("sparse_refactor_interval", 0), ("max_sparse_entries", 0)])
def test_sparse_option_validation(field, value):
    with pytest.raises(ValueError):
        solve(fixture(), backend="native", backend_options=NativeOptions(sparse=True, **{field: value}))


def test_sparse_cap_cancel_and_limits():
    with pytest.raises(NotImplementedError):
        solve(fixture(), backend="native", backend_options=NativeOptions(sparse=True, max_sparse_entries=4))
    token = Cancellation()
    token.cancel()
    assert solve(fixture(), backend="native", cancellation=token,
                 backend_options=NativeOptions(sparse=True)).status == "cancelled"
    assert solve(fixture(), backend="native", backend_options=NativeOptions(
        sparse=True, iteration_limit=0)).status == "iteration_limit"


def test_sparse_independent_concurrent_calls():
    m = fixture()
    with ThreadPoolExecutor(4) as pool:
        results = list(pool.map(lambda _: solve(m, backend="native", backend_options=NativeOptions(sparse=True)), range(12)))
    assert all(r.status == "optimal" and r.evaluation.objective == pytest.approx(9.) for r in results)
