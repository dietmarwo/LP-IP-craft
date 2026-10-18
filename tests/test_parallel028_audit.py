import copy
import importlib.util
import json
from pathlib import Path
import pytest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "audit028", ROOT / "scripts/parallel028_validate.py"
)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def fixture(tmp_path):
    paths = {}
    for name, threads in [("baseline", 1), ("serial", 1), ("parallel", 8)]:
        p = tmp_path / (name + ".json")
        row = dict(
            key="case",
            input_sha256="input",
            model_sha256="model",
            status="optimal",
            objective=1.0,
            feasible=True,
            bound=1.0,
            nodes=1,
            pivots=1,
            point=[1.0],
            independent_problems=[],
        )
        p.write_text(
            json.dumps(
                dict(
                    version="0.27.0",
                    threads=threads,
                    frozen_sources={"frozen": "hash"},
                    extension_sha256=("a" if name == "baseline" else "b") * 64,
                    expected_calls=1,
                    calls=[row],
                    generated_checks=[],
                )
            )
        )
        paths[name] = p
    return paths


def test_complete_three_way_audit(tmp_path):
    paths = fixture(tmp_path)
    audit.audit(paths, tmp_path / "audit.json")
    result = json.loads((tmp_path / "audit.json").read_text())
    assert result["total_solver_calls"] == 3 and result["paired_comparisons"] == 2
    assert not result["issues"] and not result["status_or_acceptance_changes"]


@pytest.mark.parametrize(
    "kind", ["missing", "input", "objective", "status", "extension"]
)
def test_incomplete_or_false_agreement_fails_closed(tmp_path, kind):
    paths = fixture(tmp_path)
    d = json.loads(paths["parallel"].read_text())
    if kind == "missing":
        d["calls"] = []
    elif kind == "input":
        d["calls"][0]["model_sha256"] = "changed"
    elif kind == "objective":
        d["calls"][0]["objective"] = 7.0
    elif kind == "status":
        d["calls"][0]["status"] = "time_limit"
    else:
        d["extension_sha256"] = "c" * 64
    paths["parallel"].write_text(json.dumps(d))
    with pytest.raises(AssertionError):
        audit.audit(paths, tmp_path / "audit.json")
