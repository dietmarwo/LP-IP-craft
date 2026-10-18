import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import pytest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "parallel028", ROOT / "scripts/parallel028.py"
)
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
CASE = ["packing", 8, "batch", 1, 8, 3]


def valid():
    return dict(
        status="optimal",
        completed=3,
        seconds=0.01,
        objective=3.0,
        residual=0.0,
        family="packing",
        size=8,
        mode="batch",
        inner=1,
        workers=8,
        jobs=3,
    )


@pytest.mark.parametrize(
    "field,value",
    [
        ("status", "time_limit"),
        ("completed", 2),
        ("seconds", float("nan")),
        ("seconds", -1.0),
        ("objective", float("inf")),
        ("residual", 1.0),
        ("workers", 4),
    ],
)
def test_false_speedups_fail_closed(field, value):
    p = valid()
    p[field] = value
    with pytest.raises(ValueError):
        driver.validate(p, CASE)


def test_budget_and_missing_certificate_are_rejected():
    with pytest.raises(ValueError):
        driver.validate(valid(), ["packing", 8, "batch", 2, 8, 3])
    p = valid()
    del p["residual"]
    with pytest.raises(ValueError):
        driver.validate(p, CASE)


@pytest.mark.skipif(
    sys.platform == "win32", reason="the failing child is a POSIX script"
)
def test_actual_driver_persists_failed_child(tmp_path):
    child = tmp_path / "bad.py"
    child.write_text("#!/usr/bin/env python3\nprint('{}')\n")
    child.chmod(0o755)
    out = tmp_path / "failed.json"
    r = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/parallel028.py"),
            str(out),
            "bad=" + str(child),
            "--case",
            "packing:8:batch:1:8:3",
        ],
        capture_output=True,
    )
    assert r.returncode != 0
    record = json.loads(out.read_text())
    assert record["status"] == "failed"
    assert len(record["records"]) == 1 and record["records"][0]["error"]
    assert record["records"][0]["build"] == "bad"


def test_profiling_binary_is_rejected():
    p = valid()
    p["kernel_profile"] = True
    with pytest.raises(ValueError, match="profiling-enabled"):
        driver.validate(p, CASE)
