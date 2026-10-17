"""Search observations must preserve decisions and distinguish certificates."""
import json
import pytest
import lp_ip_craft as lp


def model(*, sense="maximize", offset=-50.0, integral=True):
    return lp.Model(objective=[1.0, 1.0], lower=[0.0, 0.0], upper=[1.0, 1.0],
                    row_lower=[-float("inf")], row_upper=[3.0],
                    indptr=[0, 2], indices=[0, 1], values=[2.0, 2.0],
                    kinds=["binary", "binary"] if integral else ["integer", "continuous"],
                    sense=sense, objective_offset=offset)


def solve(m, **kwargs):
    control = dict(decompose=False, mip_diagnostics=True, objective_lattice=False,
                   incumbent_heuristic=False, reduced_cost_fixing=False)
    control.update(kwargs)
    return lp.solve(m, backend="native", backend_options=lp.NativeOptions(**control))


def trace(result):
    return json.loads(result.backend.effective_options["diagnostics.mip_search"])


@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_trace_preserves_work_and_objectives_in_original_units(sense):
    m = model(sense=sense)
    a, b = solve(m, mip_diagnostics=False), solve(m)
    assert (a.status, a.candidate, a.best_bound, a.nodes, a.iterations) == (
        b.status, b.candidate, b.best_bound, b.nodes, b.iterations)
    d = trace(b)
    assert d["scope"] == "serial_search" and d["schema_version"] == 1
    assert d["root"]["lp_objective"] == pytest.approx(-50.0 if sense == "minimize" else -48.5)
    assert d["terminal"]["returned_objective"] == b.evaluation.objective
    assert d["terminal"]["best_bound"] == b.best_bound
    assert d["terminal"]["status"] == b.status == "optimal"
    assert d["first_accepted_candidate"] is not None
    assert d["first_certified_incumbent"] is not None
    assert d["totals"]["node_lp_pivots"] + d["totals"]["other_lp_pivots"] == b.iterations
    assert d["totals"]["tree_nodes_visited"] >= b.nodes
    assert [e["pivots"] for e in d["events"]] == sorted(e["pivots"] for e in d["events"])
    assert [e["elapsed_seconds"] for e in d["events"]] == sorted(e["elapsed_seconds"] for e in d["events"])
    assert d["events"][-1]["kind"] == "terminal"


@pytest.mark.parametrize("limits,status", [({"node_limit": 0}, "node_limit"),
                                         ({"node_limit": 1}, "node_limit"),
                                         ({"iteration_limit": 0}, "iteration_limit"),
                                         ({"time_limit_seconds": 0.0}, "time_limit")])
def test_limits_have_truthful_terminal_summaries(limits, status):
    r = solve(model(), **limits)
    d = trace(r)
    assert r.status == d["terminal"]["status"] == status
    assert d["terminal"]["nodes"] == r.nodes
    assert d["terminal"]["pivots"] == r.iterations
    if not r.nodes:
        assert d["root"]["lp_objective"] is None
        assert d["root"]["status"] == "not_solved"


@pytest.mark.parametrize("cap", [0, 1, 2])
def test_event_caps_keep_full_summary(cap):
    r = solve(model(), mip_trace_limit=cap)
    d = trace(r)
    assert len(d["events"]) <= cap and d["truncated"] and d["events_dropped"] > 0
    assert d["first_certified_incumbent"] is not None
    assert d["terminal"]["returned_objective"] == -49.0


def test_accepted_tolerance_start_is_not_a_certified_incumbent():
    m = model(integral=False)
    r = lp.solve(m, start=[0.0, 1.0 + 5e-8], backend="native",
                 backend_options=lp.NativeOptions(node_limit=0, mip_diagnostics=True))
    d = trace(r)
    assert r.evaluation.feasible_candidate
    assert d["first_accepted_candidate"] is not None
    assert d["first_certified_incumbent"] is None
    assert d["terminal"]["certified_incumbent_bound"] is None
    assert d["terminal"]["absolute_gap"] is None


def test_cancelled_and_infeasible_roots():
    token = lp.Cancellation()
    token.cancel()
    r = lp.solve(model(), backend="native", cancellation=token,
                 backend_options=lp.NativeOptions(mip_diagnostics=True))
    assert r.status == trace(r)["terminal"]["status"] == "cancelled"
    m = lp.Model(objective=[1.0], lower=[0.0], upper=[1.0],
                 row_lower=[2.0], row_upper=[float("inf")], indptr=[0, 1],
                 indices=[0], values=[1.0], kinds=["binary"])
    r = solve(m)
    d = trace(r)
    assert r.status == d["terminal"]["status"] == "infeasible"
    assert d["first_accepted_candidate"] is None
    assert d["propagation"]["infeasibilities"] > 0


def test_component_scope_is_explicit():
    m = lp.Model(objective=[1.0] * 4, lower=[0.0] * 4, upper=[1.0] * 4,
                 row_lower=[-float("inf")] * 2, row_upper=[3.0] * 2,
                 indptr=[0, 2, 4], indices=[0, 1, 2, 3], values=[2.0] * 4,
                 kinds=["binary"] * 4)
    d = trace(solve(m, decompose=True, decomposition_min_variables=1))
    assert d["scope"] == "component_aggregate" and d["components"] == 2
    assert d["events"] == []


def test_mixed_row_coverage_and_invalid_trace_limit():
    d = trace(solve(model(integral=False)))
    assert d["propagation"]["eligible_rows"] == 0
    assert d["propagation"]["ineligible_rows"] == 1
    with pytest.raises(ValueError, match="mip_trace_limit"):
        solve(model(), mip_trace_limit=4097)


def test_root_preprocessing_retains_original_acceptance_and_clock_offset():
    m = lp.Model(objective=[1.0, 1.0], lower=[0.0, 0.0], upper=[float("inf"), 1.0],
                 row_lower=[-float("inf"), -float("inf")], row_upper=[3.0, 4.0],
                 indptr=[0, 1, 3], indices=[0, 0, 1], values=[1.0, 1.0, 1.0],
                 kinds=["integer", "binary"], sense="maximize", objective_offset=7.0)
    r = solve(m)
    d = trace(r)
    assert d["first_accepted_candidate"]["objective"] >= 7.0
    assert d["original_rows"] == 2 and d["search_rows"] < 2
    assert d["totals"]["original_check_rejections"] == 0
    assert float(r.backend.effective_options["diagnostics.mip_search.preprocessing_seconds"]) >= 0
    assert lp.evaluate(m, r.candidate).feasible_candidate
