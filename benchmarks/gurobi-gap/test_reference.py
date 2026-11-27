"""Test the original-coefficient path with an optional Gurobi installation."""
import importlib.util
import pytest

from run import TOL, checked_point, corrected_reference, validate
from gurobi_reference import solve


def test_gurobi_mixed_ranged_row_with_offset():
    if importlib.util.find_spec("gurobipy") is None:
        pytest.skip("optional gurobipy unavailable")
    spec = dict(sense="maximize", objective_offset=7., objective=[3., 1.],
                lower=[0., -2.], upper=[1., 2.], kinds=["binary", "continuous"],
                row_lower=[0.], row_upper=[1.5], indptr=[0, 2], indices=[0, 1], values=[2., 1.])
    result = solve(spec, dict(seconds=2.), observed=True)["result"]
    if result["status"] in ("license_limit", "unavailable"):
        pytest.skip("Gurobi license unavailable")
    checked, issues = validate(spec, result, 9.5, "optimal")
    assert result["status"] == "optimal" and not issues
    assert checked["feasible"] and checked["objective"] == pytest.approx(9.5, abs=TOL)


def test_checked_point_for_continuous_free_variable():
    spec = dict(objective_offset=2., objective=[3.], lower=[None], upper=[None],
                kinds=["continuous"], row_lower=[], row_upper=[], indptr=[0], indices=[], values=[])
    assert checked_point(spec, [1.])["objective"] == 5.

def test_verified_target_requires_matching_model_identity():
    mapping = {"abc": {"case": "old", "original_expected": -43, "expected": -41}}
    assert corrected_reference("old", "abc", -43, mapping) == -41
    assert corrected_reference("old", "def", -43, mapping) == -43
    with pytest.raises(ValueError, match="identity"):
        corrected_reference("other", "abc", -43, mapping)


def test_invalid_callback_point_is_reported_even_when_the_final_point_is_feasible():
    spec = dict(sense="maximize", objective=[1.], lower=[0.], upper=[1.], kinds=["binary"],
                row_lower=[], row_upper=[], indptr=[0], indices=[], values=[])
    result = dict(status="optimal", candidate=[1.], objective=1., bound=1.,
                  events=[dict(kind="candidate", candidate=[0.5])])
    _, issues = validate(spec, result, 1., "optimal")
    assert issues == ["callback candidate exceeds original feasibility tolerance"]


def test_gurobi_simplex_iteration_limit_is_forwarded():
    pytest.importorskip("gurobipy")
    spec = dict(sense="maximize", objective=[1., 1.], lower=[0., 0.], upper=[10., 10.],
                kinds=["continuous"] * 2, row_lower=[None], row_upper=[1.5],
                indptr=[0, 2], indices=[0, 1], values=[1., 1.])
    result = solve(spec, dict(seconds=5., pivots=0), switches={"Presolve": 0, "Method": 1})["result"]
    if result["status"] in ("license_limit", "unavailable"):
        pytest.skip("Gurobi license unavailable")
    assert result["status"] == "iteration_limit"
    assert result["pivots"] == 0

@pytest.mark.parametrize("change", ["hash", "cap", "repetition"])
def test_comparison_rejects_mismatched_pairs(change):
    from summarize import assess

    def record(solver, rep):
        return {"request": {"solver": solver, "case": "fixture", "profile": "default",
                            "repetition": rep, "input_sha256": "a", "settings": {"seconds": 10}}}

    records = [record(solver, rep) for solver in ("old", "new", "gurobi") for rep in (0, 1)]
    if change == "hash":
        records[-1]["request"]["input_sha256"] = "b"
    elif change == "cap":
        records[-1]["request"]["settings"]["nodes"] = 5
    else:
        records[-1]["request"]["repetition"] = 0
    with pytest.raises(ValueError, match="unmatched|unpaired"):
        assess(records, "old:default", "new:default", None, "gurobi:default")
