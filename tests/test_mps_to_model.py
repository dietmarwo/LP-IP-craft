"""scripts/mps_to_model.py: MPS (fixed and free format) to lp-ip.model/1.

Each fixture in tests/fixtures is converted, checked against the model schema
of the worker's discovery data and solved with the native backend against a
hand-computed optimum.
"""
import gzip
import hashlib
import importlib.util
import json
import subprocess
import sys

import lp_ip_craft as lp
import pytest

from discovery_files import INPUT_SCHEMAS, REPO, model_from_json, validator

SCRIPT = REPO / "scripts" / "mps_to_model.py"
FIXTURES = REPO / "tests" / "fixtures"
_spec = importlib.util.spec_from_file_location("mps_to_model", SCRIPT)
mps = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mps)


def text(name):
    return (FIXTURES / name).read_text(encoding="utf-8")


def converted(source, fmt="auto", **kwargs):
    model, names = mps.parse_mps(source, fmt, **kwargs)
    schema = INPUT_SCHEMAS["lp-ip.model/1"]
    assert [e.message for e in validator(schema).iter_errors(model)] == []
    return model, names


def solved(model):
    r = lp.solve(model_from_json(model), backend="native", backend_options=lp.NativeOptions())
    assert r.status == "optimal" and r.evaluation.feasible_candidate, r
    return r


def test_lp_with_ranges_free_and_negative_bounds():
    source = text("lp_ranges.mps")
    model, names = converted(source)
    assert names["rows"] == ["LIM A", "ROWB", "ROWC", "ROWD", "ROWE"]
    assert names["columns"] == ["X1", "X2", "X3", "X4", "X5"]
    assert (names["problem"], names["objective_row"], names["dropped_rows"]) == ("RANGELP", "COST", ["ALT"])
    assert len(names["warnings"]) == 1 and "'X3'" in names["warnings"][0]
    # The ALT row and its RHS are dropped; RHS -2.5 on COST is the constant +2.5.
    assert model["objective"] == [1.0, 1.0, 1.0, 2.0, -1.0]
    assert model["objective_offset"] == 2.5
    # FR; MI and UP 10; UP -2 without a lower bound; the default; LO -5 and UP 5.
    assert model["variable_lower"] == [None, None, None, 0.0, -5.0]
    assert model["variable_upper"] == [None, 10.0, -2.0, None, 5.0]
    # G 2 range 3; E 1 range -4; L 7 range 4; E 4 range 3; G without an RHS entry.
    assert model["row_lower"] == [2.0, -3.0, 3.0, 4.0, 0.0]
    assert model["row_upper"] == [5.0, 1.0, 7.0, 7.0, None]
    # The column-wise COLUMNS entries become CSR by rows.
    assert model["matrix"] == {"indptr": [0, 1, 3, 4, 6, 8], "indices": [0, 1, 3, 2, 0, 4, 3, 4],
                               "values": [1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0]}
    # Only the fixed reading accepts the row name with a space (line 8).
    assert mps.parse_mps(source, "fixed") == (model, names)
    with pytest.raises(mps.MpsError, match="^line 8: "):
        mps.parse_mps(source, "free")
    # Each variable has one binding limit: x1 >= 2 (LIM A), x2 + x4 >= -3 with
    # x4 cheaper at 0 (ROWB), x3 >= -7 (ROWC), x5 <= 5 and x1 + x5 <= 7 (ROWD).
    r = solved(model)
    assert r.evaluation.objective == pytest.approx(2 - 3 - 7 + 0 - 5 + 2.5, abs=1e-7)
    assert r.candidate == pytest.approx([2.0, -3.0, -7.0, 0.0, 5.0], abs=1e-7)


def test_mip_with_markers_bv_and_integer_bounds():
    model, names = converted(text("mip_markers.mps"))
    assert names["columns"] == ["A", "B", "C", "D", "E"]
    assert model["kinds"] == ["integer", "integer", "continuous", "binary", "integer"]
    # Marker columns default to [0, +inf) (B); UP 3; UP 2.5; BV; LI 1 and UI 4.
    assert model["variable_lower"] == [0.0, 0.0, 0.0, 0.0, 1.0]
    assert model["variable_upper"] == [3.0, None, 2.5, 1.0, 4.0]
    assert model["row_lower"] == [None, None, 2.0] and model["row_upper"] == [12.0, 4.0, None]
    # Maximize 5A + 4B + 3C + 8D + E (as a minimization) with 2A + 3B + C + 4D
    # + E <= 12, A + B <= 4, D + E >= 2. C fills the capacity left, up to 2.5;
    # enumerating A, B, D and E gives 27 at (3, 0, 1, 1, 1), then 26.5.
    r = solved(model)
    assert r.evaluation.objective == pytest.approx(-27.0, abs=1e-7)
    assert r.candidate == pytest.approx([3.0, 0.0, 1.0, 1.0, 1.0], abs=1e-7)


def test_maximization_with_objsense_on_the_next_line():
    model, _ = converted(text("max_objsense.mps"))
    assert model["sense"] == "maximize" and model["objective"] == [3.0, 2.0]
    assert model["objective_offset"] == -10.0
    # Maximize 3x + 2y - 10 with x + y <= 4, x + 3y <= 6 and x <= 3: (3, 1).
    r = solved(model)
    assert r.evaluation.objective == pytest.approx(1.0, abs=1e-7)
    assert r.candidate == pytest.approx([3.0, 1.0], abs=1e-7)


def test_free_format_names_sets_and_constant():
    source = text("free_format.mps")
    model, names = converted(source)
    assert mps.parse_mps(source, "free") == (model, names)
    with pytest.raises(mps.MpsError):
        mps.parse_mps(source, "fixed")
    assert names["columns"] == ["units_alpha", "units_beta", "slack_gamma"]
    assert names["rows"] == ["capacity_limit", "minimum_alpha", "balance_beta_gamma"]
    assert model["sense"] == "maximize" and model["objective_offset"] == 3.0
    assert model["kinds"] == ["integer", "integer", "continuous"]
    assert model["variable_upper"] == [4.0, 5.0, 10.0]
    assert model["row_lower"] == [None, 1.0, 2.0] and model["row_upper"] == [9.0, None, 2.0]
    # gamma = beta - 2, so the objective is 5 alpha + 2 beta + 5 with 2 alpha +
    # beta <= 9 and beta >= 2: 26 at (3, 3); (2, 5) gives 25, alpha = 4 is infeasible.
    r = solved(model)
    assert r.evaluation.objective == pytest.approx(26.0, abs=1e-7)
    assert r.candidate == pytest.approx([3.0, 3.0, 1.0], abs=1e-7)


# A free-format model; with objsense="" the BOUNDS entry is on line 10.
BASE = """NAME t
{objsense}ROWS
 N obj
 L c
COLUMNS
 x obj 1 c 1
RHS
 rhs c 4
BOUNDS
 UP bnd x 3
ENDATA
"""
PLAIN = BASE.format(objsense="")


@pytest.mark.parametrize("objsense, sense", [
    ("", "minimize"),
    ("OBJSENSE\n    MAX\n", "maximize"),
    ("OBJSENSE\n    MAXIMIZE\n", "maximize"),
    ("OBJSENSE MAX\n", "maximize"),
    ("OBJSENSE MAXIMIZE\n", "maximize"),
    ("OBJSENSE\n    MIN\n", "minimize"),
    ("OBJSENSE MINIMIZE\n", "minimize"),
])
def test_objsense_forms(objsense, sense):
    model, _ = converted(BASE.format(objsense=objsense), "free")
    assert model["sense"] == sense and model["objective"] == [1.0]


def test_negative_upper_bound_without_lower_bound_frees_the_lower_side():
    model, names = converted(PLAIN.replace(" UP bnd x 3", " UP bnd x -2"), "free")
    assert (model["variable_lower"], model["variable_upper"]) == ([None], [-2.0])
    assert "lower bound set to -infinity" in names["warnings"][0]
    # An explicit lower bound is kept, and [0, -2] is empty.
    with pytest.raises(mps.MpsError, match="^line 11: .*lower bound 0.0 above upper bound -2.0"):
        mps.parse_mps(PLAIN.replace(" UP bnd x 3", " LO bnd x 0\n UP bnd x -2"), "free")


def test_large_values_are_infinite_bounds():
    source = PLAIN.replace(" UP bnd x 3", " UP bnd x 1e30\n LO bnd x -Infinity")
    model, _ = converted(source, "free")
    assert (model["variable_lower"], model["variable_upper"]) == ([None], [None])
    model, _ = converted(source.replace("1e30", "1e20"), "free")
    assert model["variable_upper"] == [1e20]
    model, _ = converted(source.replace("1e30", "1e20"), "free", infinity=1e20)
    assert model["variable_upper"] == [None]


def add(after, lines):
    return PLAIN.replace(after, after + "\n" + lines)


@pytest.mark.parametrize("source, line, message", [
    (PLAIN.replace(" UP bnd", " SC bnd"), 10, "semi-continuous (SC) bounds are not supported"),
    (PLAIN.replace("ENDATA", "QUADOBJ\n x x 2\nENDATA"), 11, "quadratic objectives (QUADOBJ)"),
    (PLAIN.replace("ENDATA", "QSECTION obj\n x x 2\nENDATA"), 11, "quadratic objectives (QSECTION)"),
    (PLAIN.replace("ENDATA", "OBJNAME\n obj\nENDATA"), 11, "OBJNAME is not supported"),
    (PLAIN.replace("ENDATA", "FOO\nENDATA"), 11, "unknown section 'FOO'"),
    (PLAIN.replace("x 3", "x -1e30"), 10, "upper bound of -infinity"),
    (PLAIN.replace("UP bnd x 3", "LO bnd x inf"), 10, "lower bound of +infinity"),
    (PLAIN.replace("UP bnd x 3", "FX bnd x -inf"), 10, "upper bound of -infinity"),
    (PLAIN.replace("rhs c 4", "rhs c -inf"), 8, "L row 'c' has right-hand side -inf"),
    (PLAIN.replace(" L c", " G c").replace("rhs c 4", "rhs c 1e31"), 8, "G row 'c' has right-hand side inf"),
    (PLAIN.replace(" L c", " E c").replace("rhs c 4", "rhs c inf"), 8, "infinite right-hand side"),
    (PLAIN.replace("rhs c 4", "rhs c 4 obj 1e30"), 8, "objective constant"),
    (PLAIN.replace("obj 1 c 1", "obj 1e30 c 1"), 6, "is infinite"),
    (PLAIN.replace("obj 1 c 1", "obj nan c 1"), 6, "is NaN"),
    (PLAIN.replace("rhs c 4", "rhs c four"), 8, "'four' is not a number"),
    (add(" L c", " G c"), 5, "duplicate row 'c'"),
    (add(" x obj 1 c 1", " x c 2"), 7, "duplicate entry for column 'x' in row 'c' (first at line 6)"),
    (PLAIN.replace(" x obj 1 c 1", " x obj 1\n y c 1\n x c 1"), 8, "column 'x' appears again"),
    (add(" rhs c 4", " rhs c 5"), 9, "duplicate RHS entry for row 'c'"),
    (add(" rhs c 4", " other obj 1"), 9, "a second RHS set 'other'"),
    (add(" UP bnd x 3", " UP bnd x 4"), 11, "duplicate UP bound for column 'x'"),
    (PLAIN.replace("obj 1 c 1", "obj 1 d 1"), 6, "unknown row 'd'"),
    (PLAIN.replace("UP bnd x", "UP bnd z"), 10, "unknown column 'z'"),
    (PLAIN.replace("UP bnd", "XX bnd"), 10, "unknown bound type 'XX'"),
    (add(" UP bnd x 3", " LO bnd x 5"), 11, "lower bound 5.0 above upper bound 3.0"),
    (PLAIN.replace(" UP bnd x 3", " BV bnd x\n UP bnd x 2"), 11, "outside [0, 1]"),
    (PLAIN.replace(" x obj", " m 'MARKER' 'INTORG'\n x obj"), 8, "'INTORG' at line 6 has no 'INTEND'"),
    (PLAIN.replace("BOUNDS", "RANGES\n rng obj 1\nBOUNDS"), 10, "RANGES on the objective row 'obj'"),
    (PLAIN.replace("ENDATA\n", ""), None, "missing ENDATA"),
])
def test_errors_name_the_line(source, line, message):
    with pytest.raises(mps.MpsError) as e:
        mps.parse_mps(source, "free")
    assert e.value.line == line and message in e.value.message, str(e.value)
    assert str(e.value).startswith(f"line {line}: " if line else message)
    # The free reading got further than the fixed one, so auto reports it.
    with pytest.raises(mps.MpsError) as auto:
        mps.parse_mps(source)
    assert (auto.value.line, auto.value.message, auto.value.format) == (line, e.value.message, "free")


def test_fixed_format_rejects_misaligned_fields():
    source = text("max_objsense.mps").replace("    X         PROFIT", "    X          PROFIT")
    with pytest.raises(mps.MpsError, match="^line 11: .*misaligned"):
        mps.parse_mps(source, "fixed")


def test_cli_writes_the_model_names_and_hash(tmp_path):
    source = tmp_path / "free.mps.gz"
    source.write_bytes(gzip.compress((FIXTURES / "free_format.mps").read_bytes()))
    out, names = tmp_path / "model.json", tmp_path / "names.json"
    run = subprocess.run([sys.executable, str(SCRIPT), str(source), "-o", str(out), "--names", str(names)],
                         capture_output=True, text=True, check=True)
    summary = json.loads(run.stdout)
    assert summary["sha256"] == hashlib.sha256(out.read_bytes()).hexdigest()
    assert (summary["variables"], summary["rows"], summary["nonzeros"], summary["integer_variables"]) == (3, 3, 5, 2)
    model, expected_names = mps.parse_mps(text("free_format.mps"))
    assert json.loads(out.read_text(encoding="utf-8")) == model
    assert json.loads(names.read_text(encoding="utf-8")) == expected_names
    bad = tmp_path / "bad.mps"
    bad.write_text(PLAIN.replace("UP bnd", "SC bnd"), encoding="utf-8")
    run = subprocess.run([sys.executable, str(SCRIPT), str(bad), "-o", str(out)], capture_output=True, text=True)
    assert run.returncode == 1 and "line 10: semi-continuous" in run.stderr
