"""Contract tests against the compiled extension (no Python-side solver exists)."""

import dataclasses
import math
import pickle
from concurrent.futures import ThreadPoolExecutor

import pytest

from lp_ip_craft import (
    BACKENDS,
    Cancellation,
    EvaluationError,
    Model,
    Options,
    evaluate,
    solve,
)

ASSIGNMENT = dict(
    # minimize 2 x0 + 3 x1 + 5  subject to  x0 + x1 = 1,  x binary
    objective=[2.0, 3.0],
    objective_offset=5.0,
    lower=[0.0, 0.0],
    upper=[1.0, 1.0],
    kinds=["binary", "binary"],
    indptr=[0, 2],
    indices=[0, 1],
    values=[1.0, 1.0],
    row_lower=[1.0],
    row_upper=[1.0],
)

PRODUCTION = dict(
    # Example 1 of the LP/IP tutorial; optimum (30, 10, 0) with profit 2150.
    sense="maximize",
    objective=[45.0, 80.0, 110.0],
    lower=[0.0] * 3,
    upper=[math.inf] * 3,
    indptr=[0, 3, 6, 9],
    indices=[0, 1, 2] * 3,
    values=[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0],
    row_lower=[-math.inf] * 3,
    row_upper=[400.0, 450.0, 60.0],
)


def make(base=ASSIGNMENT, **overrides):
    return Model(**{**base, **overrides})


@pytest.fixture
def model():
    return make()


# --- dummy behavior -----------------------------------------------------------


def test_feasible_optimal_start_is_still_only_a_stub(model):
    r = solve(model, start=[1.0, 0.0])
    assert r.status == "stub" and r.message.startswith("DUMMY")
    assert r.candidate == [1.0, 0.0]
    assert r.evaluation.objective == 7.0 and r.evaluation.row_activity == [1.0]
    assert r.evaluation.feasible_candidate
    assert r.duals is None and r.best_bound is None and r.gap is None
    assert r.nodes == 0 and r.iterations == 0


@pytest.mark.parametrize(
    "kind,feasible", [("continuous", True), ("integer", False), ("binary", False)]
)
def test_fractional_start_is_never_rounded(kind, feasible):
    r = solve(make(kinds=[kind, kind]), start=[0.5, 0.5])
    assert r.candidate == [0.5, 0.5]
    assert r.evaluation.integrality_violation == (0.0 if kind == "continuous" else 0.5)
    assert r.evaluation.feasible_candidate is feasible


def test_without_start_zero_is_clamped_and_violation_reported(model):
    r = solve(model)
    assert r.status == "stub" and r.candidate == [0.0, 0.0]
    assert r.evaluation.primal_violation == 1.0
    assert not r.evaluation.feasible_candidate
    r = solve(make(kinds=None, lower=[2.0, -math.inf], upper=[5.0, -2.5]))
    assert r.candidate == [2.0, -2.5]


def test_start_outside_bounds_is_reported_not_repaired(model):
    r = solve(model, start=(2.0, -1.0))
    assert r.candidate == [2.0, -1.0] and r.evaluation.primal_violation == 1.0


def test_maximize_keeps_original_objective_sign():
    r = solve(make(sense="maximize"), start=[0.0, 1.0])
    assert r.evaluation.objective == 8.0


def test_pre_cancelled_solve_has_no_candidate(model):
    c = Cancellation()
    c.cancel()
    r = solve(model, cancellation=c)
    assert c.cancelled and r.status == "cancelled"
    assert r.candidate is None and r.evaluation is None
    assert repr(c) == "Cancellation(cancelled=true)"


def test_input_errors_take_precedence_over_cancellation(model):
    c = Cancellation()
    c.cancel()
    with pytest.raises(ValueError, match="start: expected 2 entries"):
        solve(model, start=[1.0], cancellation=c)


def test_parallel_solves_share_one_model(model):
    starts = [[1.0, 0.0], [0.0, 1.0], [0.5, 0.5], [1.0, 1.0]] * 8
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(lambda x: solve(model, start=x), starts))
    assert [r.evaluation.objective for r in results] == [7.0, 8.0, 7.5, 10.0] * 8


def test_dummy_reports_its_backend(model):
    r = solve(model, start=[1.0, 0.0])
    assert r.backend.name == "dummy"
    assert r.backend.version is None and r.backend.raw_status is None
    assert r.backend.start_status == "evaluated"
    assert r.backend.effective_options == {} and not r.backend.time_budget_exceeded
    assert solve(model).backend.start_status == "not_provided"


def test_unknown_or_missing_backend_is_explicit(model):
    assert "dummy" in BACKENDS
    with pytest.raises(ValueError, match="unknown backend"):
        solve(model, backend="cplex")
    if "highs" not in BACKENDS:
        with pytest.raises(ValueError, match="unknown backend"):
            solve(model, backend="highs")


# --- options and errors -------------------------------------------------------


def test_only_one_thread_is_supported(model):
    with pytest.raises(NotImplementedError, match="threads = 16"):
        solve(model, options=Options(threads=16))
    for threads in (0, -1):
        with pytest.raises(ValueError, match="threads must be at least 1"):
            solve(model, options=Options(threads=threads))


@pytest.mark.parametrize(
    "options",
    [
        Options(feasibility_tolerance=0.0),
        Options(feasibility_tolerance=math.nan),
        Options(integrality_tolerance=0.5),
        Options(integrality_tolerance=-1e-9),
    ],
)
def test_bad_tolerances(model, options):
    with pytest.raises(ValueError, match="tolerance"):
        solve(model, options=options)


def test_options_are_frozen_and_keyword_only():
    with pytest.raises(TypeError):
        Options(1e-6)  # type: ignore[misc]
    with pytest.raises(dataclasses.FrozenInstanceError):
        Options().threads = 2  # type: ignore[misc]


@pytest.mark.parametrize("start,match", [([1.0], "expected 2"), ([math.nan, 0.0], "entry 0 is NaN")])
def test_invalid_start(model, start, match):
    with pytest.raises(ValueError, match=match):
        solve(model, start=start)


def test_overflow_raises_evaluation_error():
    m = make(kinds=None, lower=[-math.inf] * 2, upper=[math.inf] * 2, objective=[1.7e308, 1.7e308])
    with pytest.raises(EvaluationError, match="objective overflowed"):
        solve(m, start=[1.0, 1.0])
    assert issubclass(EvaluationError, RuntimeError)


# --- model construction ---------------------------------------------------------


@pytest.mark.parametrize(
    "patch,match",
    [
        ({"indptr": [0, 3]}, "indptr must start at 0 and end at"),
        ({"indices": [0, 0]}, "strictly increasing"),
        ({"indices": [1, 0]}, "strictly increasing"),
        ({"indices": [0, 5]}, "column index 5"),
        ({"values": [1.0]}, "1 values for 2 stored entries"),
        ({"values": [1.0, math.inf]}, "matrix values: entry 1 is inf"),
        ({"lower": [math.nan, 0.0]}, "variable bounds: entry 0"),
        ({"upper": [-1.0, 1.0]}, "variable bounds: entry 0"),
        ({"objective": [math.inf, 1.0]}, "objective: entry 0 is inf"),
        ({"objective": [], "lower": [], "upper": [], "kinds": []}, "at least one variable"),
        ({"objective_offset": math.nan}, "objective offset"),
        ({"kinds": ["bogus", "binary"]}, "kinds: entry 0 is \"bogus\""),
        ({"kinds": ["binary"]}, "variable kinds: expected 2"),
        ({"upper": [1.0, 2.0]}, "variable 1 is binary"),
        ({"sense": "max"}, "sense is \"max\""),
        ({"row_upper": [-math.inf]}, "row bounds: entry 0"),
        ({"indices": [0, -1]}, "indices: entry 1 is -1"),
    ],
)
def test_invalid_models_name_the_problem(patch, match):
    with pytest.raises(ValueError, match=match):
        make(**patch)


@pytest.mark.parametrize(
    "patch",
    [
        {"objective": "12"},
        {"kinds": "binary"},
        {"indices": [0.0, 1.0]},
        {"values": [1.0, "x"]},
        {"lower": None},
    ],
)
def test_wrongly_typed_model_input(patch):
    with pytest.raises(TypeError):
        make(**patch)


def test_model_is_keyword_only_frozen_and_introspectable(model):
    with pytest.raises(TypeError):
        Model([1.0])  # type: ignore[misc]
    with pytest.raises(AttributeError):
        model.objective = [0.0, 0.0]  # type: ignore[misc]
    assert (model.n, model.m, model.nnz, model.is_mip) == (2, 1, 2, True)
    assert model.sense == "minimize" and model.objective_offset == 5.0
    assert model.kinds == ["binary", "binary"]
    assert (model.indptr, model.indices, model.values) == ([0, 2], [0, 1], [1.0, 1.0])
    assert (model.row_lower, model.row_upper) == ([1.0], [1.0])
    assert repr(model) == 'Model(sense="minimize", n=2, m=1, nnz=2, integral=2)'


def test_kinds_default_to_continuous():
    m = make(kinds=None)
    assert m.kinds == ["continuous", "continuous"] and not m.is_mip


def test_rowless_model():
    m = Model(objective=[1.0], lower=[-math.inf], upper=[math.inf],
              row_lower=[], row_upper=[], indptr=[0], indices=[], values=[])
    r = solve(m)
    assert r.candidate == [0.0] and r.evaluation.row_activity == []
    assert r.best_bound is None  # the dummy does not diagnose this unbounded LP


def test_model_pickles_and_compares_by_value(model):
    clone = pickle.loads(pickle.dumps(model))
    assert clone == model and clone is not model
    assert make(objective_offset=0.0) != model
    assert model != "not a model"


def test_numpy_arrays_are_accepted():
    np = pytest.importorskip("numpy")
    m = Model(
        objective=np.array([2.0, 3.0]),
        objective_offset=np.float64(5.0),
        lower=np.zeros(2),
        upper=np.ones(2),
        kinds=np.array(["binary", "binary"]),
        indptr=np.array([0, 2], dtype=np.int64),
        indices=np.array([0, 1], dtype=np.int32),
        values=np.ones(2),
        row_lower=np.ones(1),
        row_upper=np.ones(1),
    )
    assert m == make()
    r = solve(m, start=np.array([1.0, 0.0]))
    assert r.evaluation.objective == 7.0


# --- independent evaluation -----------------------------------------------------


def test_evaluate_checks_an_external_solution():
    production = make(PRODUCTION)
    e = evaluate(production, [30.0, 10.0, 0.0])
    assert e.objective == 2150.0 and e.row_activity == [350.0, 450.0, 60.0]
    assert e.primal_violation == 0.0 and e.feasible_candidate
    e = evaluate(production, [30.0, 10.0, 1e-3])  # labor row exceeded by 20 * 1e-3
    assert not e.feasible_candidate and math.isclose(e.primal_violation, 0.02, rel_tol=1e-9)


def test_evaluate_respects_tolerances(model):
    e = evaluate(model, [0.75, 0.0], feasibility_tolerance=0.25, integrality_tolerance=0.25)
    assert e.feasible_candidate
    assert not evaluate(model, [0.75, 0.0]).feasible_candidate
    with pytest.raises(ValueError, match="integrality tolerance"):
        evaluate(model, [1.0, 0.0], integrality_tolerance=0.6)
    with pytest.raises(ValueError, match="point: expected 2"):
        evaluate(model, [1.0])
