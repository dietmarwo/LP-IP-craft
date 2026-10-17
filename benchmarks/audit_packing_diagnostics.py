"""Audit retained 0.23 evidence without changing frozen expectations."""
import hashlib
import json
import re
from pathlib import Path

root = Path(__file__).resolve().parent.parent
data = root / "docs/packing-ftran-mip-diagnostics/data"
old = root / "docs/performance-ten/data"


def read(path):
    return json.loads(path.read_text())


inputs_bytes = (old / "oracle-inputs.json").read_bytes()
adjudication_bytes = (old / "oracle-adjudication.json").read_bytes()
assert hashlib.sha256(inputs_bytes).hexdigest() == "7da1d445743d20c29c70e5bbe658c017f755fcf8769214c30a519401dbc21bb9"
assert hashlib.sha256(adjudication_bytes).hexdigest() == "1ebc3c213212af2451801a965418520bfd402867f76db46b36a6b908a3cd1564"
inputs = {r["case_id"]: r for r in json.loads(inputs_bytes)["records"]}
adj = {r["case_id"]: r for r in json.loads(adjudication_bytes)["records"]}
out = dict(dataset_sha256=hashlib.sha256(inputs_bytes).hexdigest(),
           adjudication_sha256=hashlib.sha256(adjudication_bytes).hexdigest(), oracle_runs=[])
for name, count in [("default", 6330), ("ft", 6330), ("no-fix", 6330), ("refactor1", 1200),
                    ("workers4", 180), ("devex", 6330), ("root-cuts", 6330),
                    ("factor-replay", 6330), ("row-pricing", 6330)]:
    d = read(data / f"oracle-{name}.json")
    assert d["wrapper_version"] == "0.23.0"
    assert d["dataset_sha256"] == out["dataset_sha256"]
    assert len(d["records"]) == len({r["case_id"] for r in d["records"]}) == count
    failures = []
    for r in d["records"]:
        original, a = inputs[r["case_id"]], adj.get(r["case_id"], {})
        expected = a.get("adjudicated_status", original["expected_status"])
        reference = a.get("reference", original["reference"])
        issues = [s for s in r.get("problems", []) if s != "status mismatch"]
        if r["status"] != expected:
            issues.append("status mismatch")
        if r["status"] == "optimal" and reference is not None and abs(r["objective"] - reference) > 1e-6*(1+abs(reference)):
            issues.append("objective mismatch")
        if issues:
            failures.append(dict(case_id=r["case_id"], issues=issues))
    out["oracle_runs"].append(dict(file=name, cases=count, raw_flags=sum(bool(r.get("problems")) for r in d["records"]), failures=failures))
assert all(not r["failures"] for r in out["oracle_runs"]), out["oracle_runs"]
out["oracle_cases"] = sum(r["cases"] for r in out["oracle_runs"])
out["unsafe_finite_bounds"] = read(data / "bounds.json")["unsafe_finite_bounds"]
assert out["unsafe_finite_bounds"] == 0
out["rust_checks"] = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", (data / "final-rust-tests.txt").read_text())))
out["python_checks"] = int(re.search(r"(\d+) passed", (data / "final-python-tests.txt").read_text()).group(1))
assert "error:" not in (data / "final-clippy.txt").read_text()
assert "Finished" in (data / "final-clippy.txt").read_text()
out["benchmarks"] = []
for path in sorted(data.glob("final-*.json")):
    d = read(path)
    assert not d.get("objective_mismatches"), (path.name, d.get("objective_mismatches"))
    if "summary" not in d or path.stem in ["final-profiles", "final-replay"]:
        continue
    if path.stem == "final-large":
        failures = [r for r in d["summary"] if r["accepted"] != d["rounds"]]
        out["benchmarks"].append(dict(file=path.name, summaries=len(d["summary"]), failures=failures))
        assert not failures, failures
        continue
    failures = [r for r in d["summary"] if r.get("accepted") != r.get("measured_solves", r.get("processes"))
                or r.get("process_errors", r.get("errors", 0))]
    out["benchmarks"].append(dict(file=path.name, summaries=len(d["summary"]), failures=failures))
    if path.stem not in ["final-public", "final-new-public"]:
        assert not failures, (path.name, failures)
    # A candidate regression from a solved baseline is a release failure.
    for baseline in [r for r in d["summary"] if r["backend"] == "baseline"]:
        candidates = [r for r in d["summary"] if r["backend"] == "native" and
                      (r["fixture"], r["size"]) == (baseline["fixture"], baseline["size"])]
        for candidate in candidates:
            if baseline.get("accepted") == baseline.get("measured_solves", baseline.get("processes")):
                assert candidate.get("accepted") == candidate.get("measured_solves", candidate.get("processes")), candidate
replay = read(data / "final-replay.json")
assert all(all(run["accepted"] for run in row["runs"]) for row in replay["records"])
out["basis_replay_records"] = len(replay["records"])
out["max_backward_error"] = max(row["backward_error"] for row in replay["summary"])
assert out["max_backward_error"] < 1e-9
d = read(data / "mip-diagnostics.json")
out["diagnostic_comparisons"] = d["summary"]
for row in d["summary"]:
    limited = any(status == "time_limit" for side in ["off", "on"] for status in row[side]["statuses"])
    assert row["work_equal"] or limited, row
    assert row["result_equal"] or limited, row
out["diagnostic_ablations"] = len(d["ablations"])
conversion = read(data / "mps-conversion-check.json")
assert len(conversion["records"]) == 6 and all(not r["differences"] for r in conversion["records"])
out["verified_mps_conversions"] = len(conversion["records"])
for row in d["records"] + d["ablations"]:
    for run in row["result"].get("runs", []):
        if "search_diagnostics" not in run:
            continue
        trace = run["search_diagnostics"]
        assert trace["scope"] == "serial_search"
        assert len(trace["events"]) <= trace["event_limit"]
        assert trace["terminal"]["status"] == run["status"]
        assert trace["terminal"]["pivots"] == run["iterations"]
        assert trace["totals"]["node_lp_pivots"] <= run["iterations"]
        assert trace["root"]["conservative_bound"] is None or trace["root"]["lp_objective"] is not None
        assert trace["propagation"]["eligible_rows"] + trace["propagation"]["ineligible_rows"] == trace["search_rows"]
        expected = row["result"].get("expected")
        if expected is not None:
            direction = 1 if trace["sense"] == "minimize" else -1
            tolerance = 1e-6*(1+abs(expected))
            bounds = [trace["root"]["conservative_bound"], trace["terminal"]["best_bound"]]
            bounds += [e["global_bound"] for e in trace["events"]]
            assert all(direction*(expected-bound) >= -tolerance for bound in bounds if bound is not None), (row["fixture"], bounds, expected)
            proofs = [trace["terminal"]["certified_incumbent_bound"]]
            proofs += [e["certified_incumbent_bound"] for e in trace["events"]]
            assert all(direction*(proof-expected) >= -tolerance for proof in proofs if proof is not None), (row["fixture"], proofs, expected)
(data / "audit.json").write_text(json.dumps(out, indent=2) + "\n")
print(json.dumps(out, indent=2))
