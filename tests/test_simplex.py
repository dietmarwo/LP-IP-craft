"""Dual simplex controls, limits and independently checked solutions."""
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
@pytest.mark.parametrize("steepest_edge", [False, True])
def test_solution_and_duals(interval, presolve, steepest_edge):
    m = fixture()
    r = solve(m, backend="native", backend_options=NativeOptions(
        refactor_interval=interval, presolve=presolve, steepest_edge=steepest_edge))
    assert r.status == "optimal" and r.evaluation.objective == pytest.approx(9.)
    k = lp_residuals(m, r.candidate, r.duals)
    assert max(k.primal, k.duality_gap, k.dual_sign, k.stationarity) < 1e-7
    assert r.backend.effective_options["linear_kernel"] == "bounded_dual_revised_simplex+markowitz_lu"


@pytest.mark.parametrize("field,value", [("refactor_interval", 0), ("max_factor_entries", 0)])
def test_option_validation(field, value):
    with pytest.raises(ValueError):
        solve(fixture(), backend="native", backend_options=NativeOptions(**{field: value}))


def test_factor_cap_cancel_and_limits():
    with pytest.raises(NotImplementedError, match="max_factor_entries"):
        solve(fixture(), backend="native", backend_options=NativeOptions(max_factor_entries=1))
    token = Cancellation()
    token.cancel()
    assert solve(fixture(), backend="native", cancellation=token).status == "cancelled"
    assert solve(fixture(), backend="native", backend_options=NativeOptions(
        iteration_limit=0)).status == "iteration_limit"


def test_independent_concurrent_calls():
    m = fixture()
    with ThreadPoolExecutor(4) as pool:
        results = list(pool.map(lambda _: solve(m, backend="native"), range(12)))
    assert all(r.status == "optimal" and r.evaluation.objective == pytest.approx(9.) for r in results)


def test_diagnostics_are_reported():
    r = solve(fixture(), backend="native")
    d = r.backend.effective_options
    for key in ["factor_peak_entries", "refactorizations", "basis_updates", "bound_flips",
                "basis_repairs", "presolve_removed_rows", "presolve_removed_columns",
                "certification_solves", "certified_incumbents", "unverified_infeasible_nodes"]:
        assert int(d[f"diagnostics.{key}"]) >= 0
    assert int(d["diagnostics.refactorizations"]) >= 1
