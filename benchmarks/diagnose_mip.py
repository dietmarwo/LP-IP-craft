"""Paired trace overhead, work equivalence and labelled existing-policy probes.

All process launches use argv and explicit environments; failures are retained.
Timing requires no concurrent builds, tests or other benchmark workloads.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess


def run(binary, fixture, size, env):
    done = subprocess.run([binary, fixture, str(size), "warm", "1", "1"],
                          capture_output=True, text=True, timeout=90, env=env)
    if done.returncode:
        return dict(error=done.stderr, returncode=done.returncode)
    try:
        result = json.loads(done.stdout)
    except json.JSONDecodeError:
        return dict(error="invalid JSON", stdout=done.stdout, stderr=done.stderr)
    for solve in result["runs"]:
        options = dict(solve.get("effective_options", []))
        if solve["status"] != "error" and "LPIP_MIP_DIAGNOSTICS" in env:
            assert options.get("mip_diagnostics") == "true", options
            assert "diagnostics.mip_search" in options, (fixture, solve)
            solve["search_diagnostics"] = json.loads(options["diagnostics.mip_search"])
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--corpus", type=Path, action="append", default=[])
    p.add_argument("--rounds", type=int, default=3)
    p.add_argument("--ablations", action="store_true")
    a = p.parse_args()
    assert a.rounds > 0
    binary = str(Path(a.binary).resolve())
    cases = []
    for corpus in a.corpus:
        for path in sorted(corpus.glob("*.json")):
            if path.name == "manifest.json":
                continue
            d = json.loads(path.read_text())
            if any(k != "continuous" for k in d["model"]["kinds"]):
                cases.append(("json=" + str(path.resolve()), 0))
    cases += [("multi-knapsack", 12018), ("graph-mip", 13018),
              ("mixed-overload", 12016), ("integer-knapsack", 48), ("long-row-mip", 112)]
    env = os.environ.copy()
    for key in ["LPIP_MIP_DIAGNOSTICS", "LPIP_HEURISTIC", "LPIP_ROOT_CUTS", "LPIP_STRONG_BRANCHING", "LPIP_PRESOLVE"]:
        env.pop(key, None)
    env["LPIP_BENCH_NO_DECOMPOSE"] = "1"
    out = dict(schema_version=1, binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
               platform=platform.platform(), protocol="One discarded round, rotated/reversed fresh paired processes, one warmup and one measured solve, one numerical thread, decomposition off, shared host. Same 30s/100000-node/1000000-pivot caps. Traces are observational and enabled overhead counts against wall limits.",
               records=[], ablations=[], summary=[])
    order = [(name, size, enabled) for name, size in cases for enabled in [False, True]]
    fingerprints = {}
    for rnd in range(a.rounds + 1):
        todo = order[rnd % len(order):] + order[:rnd % len(order)]
        if rnd % 2:
            todo = todo[::-1]
        for name, size, enabled in todo:
            settings = env | ({"LPIP_MIP_DIAGNOSTICS": "1"} if enabled else {})
            try:
                d = run(binary, name, size, settings)
            except subprocess.TimeoutExpired as e:
                d = dict(error=str(e))
            if "model_fingerprint" in d:
                assert fingerprints.setdefault((name, size), d["model_fingerprint"]) == d["model_fingerprint"]
            out["records"].append(dict(fixture=name, size=size, round=rnd, trace_enabled=enabled, result=d))
            a.output.write_text(json.dumps(out, indent=2) + "\n")
            measured = d.get("runs", [{}])[-1]
            print(rnd, enabled, Path(name).stem, size, measured.get("status", "process_error"), measured.get("nodes"), flush=True)
    for name, size in cases:
        row = dict(fixture=name, size=size)
        for enabled, label in [(False, "off"), (True, "on")]:
            records = [r for r in out["records"] if r["round"] > 0 and
                       (r["fixture"], r["size"], r["trace_enabled"]) == (name, size, enabled)]
            runs = [r["result"]["runs"][1] for r in records if "runs" in r["result"]]
            row[label] = dict(process_errors=len(records)-len(runs), statuses=[r["status"] for r in runs],
                              nodes=[r.get("nodes") for r in runs], pivots=[r.get("iterations") for r in runs],
                              objectives=[r.get("checks", {}).get("objective") if isinstance(r.get("checks"), dict) else None for r in runs],
                              best_bounds=[r.get("best_bound") for r in runs], accepted=[r.get("accepted") for r in runs],
                              median_ms=1000*statistics.median(r["seconds"] for r in runs) if runs else None)
        if row["off"]["median_ms"]:
            row["overhead_percent"] = 100*(row["on"]["median_ms"]/row["off"]["median_ms"]-1)
        row["work_equal"] = all(row["off"][key] == row["on"][key] for key in ["statuses", "nodes", "pivots"])
        row["result_equal"] = row["work_equal"] and all(row["off"][key] == row["on"][key] for key in ["objectives", "best_bounds", "accepted"])
        out["summary"].append(row)
    if a.ablations:
        selected = [(n, s) for n, s in cases if Path(n).stem in ["egout", "flugpl", "bell5", "rgn", "gt2", "lseu"] or n == "multi-knapsack"]
        modes = {"heuristic-off": {"LPIP_HEURISTIC": "off"}, "presolve-off": {"LPIP_PRESOLVE": "off"},
                 "strong-branching": {"LPIP_STRONG_BRANCHING": "on"}, "root-covers": {"LPIP_ROOT_CUTS": "1"}}
        for name, size in selected:
            for mode, overrides in modes.items():
                try:
                    d = run(binary, name, size, env | {"LPIP_MIP_DIAGNOSTICS": "1"} | overrides)
                except subprocess.TimeoutExpired as e:
                    d = dict(error=str(e))
                out["ablations"].append(dict(fixture=name, size=size, mode=mode,
                                             note="Single fresh-process probe, not a repeated timing comparison; presolve-off disables several reductions and can lose finite-bound admission.", result=d))
                a.output.write_text(json.dumps(out, indent=2) + "\n")
                print("probe", mode, Path(name).stem, flush=True)
    a.output.write_text(json.dumps(out, indent=2) + "\n")


if __name__ == "__main__":
    main()
