"""Isolated Linux process measurements. Build sparse_benchmark first.

Solver time excludes model construction; process peak RSS includes it.
No third-party Python packages are required. No solver dependencies are added.
"""
import argparse
import json
import platform
from pathlib import Path
import statistics
import subprocess


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", default="target/release/examples/sparse_benchmark")
    p.add_argument("--output", default="docs/sparse/benchmarks.json")
    p.add_argument("--repeats", type=int, default=5)
    args = p.parse_args()
    assert args.repeats > 0
    binary = str(Path(args.binary).resolve())
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    cases = [(f, n, sparse, warm)
             for f, sizes in [("packing", [64, 256, 1024, 2048, 4096]),
                              ("network", [32, 128, 512]),
                              ("irregular", [64, 256]),
                              ("selection", [12, 20])]
             for n in sizes for sparse in [False, True]
             for warm in ([False, True] if f == "selection" else [True])]
    cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")), "unknown")
    result = {"cpu": cpu, "platform": platform.platform(),
              "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
              "repeats": args.repeats, "warmups_per_case": 1,
              "method": "isolated processes, alternating/reversed case order, single thread, no decomposition; no CPU affinity guarantee",
              "solver_timing": "model construction excluded",
              "peak_rss": "whole process including model construction, Linux /proc/self/status VmHWM KiB read inside each benchmark process",
              "records": []}
    for run in range(args.repeats + 1):
        order = cases[run % len(cases):] + cases[:run % len(cases)]
        if run % 2:
            order = list(reversed(order))
        for family, n, sparse, warm in order:
            cmd = [binary, family, str(n), "sparse" if sparse else "dense",
                   "warm" if warm else "cold"]
            completed = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
            if completed.returncode:
                raise RuntimeError(f"{cmd}: {completed.stderr}")
            record = json.loads(completed.stdout)
            assert record["peak_rss_kib"] > 0, "Linux /proc VmHWM required for this measurement harness"
            record.update(run=run)
            result["records"].append(record)
            output.write_text(json.dumps(result, indent=2) + "\n")
        print(f"completed round {run}/{args.repeats}", flush=True)
    summarize(result, cases)
    output.write_text(json.dumps(result, indent=2) + "\n")


def summarize(result, cases):
    result["summary"] = []
    for family, n, sparse, warm in cases:
        records = [r for r in result["records"] if r["run"] > 0 and
                   (r["family"], r["size"], r["sparse"], r["warm"]) == (family, n, sparse, warm)]
        row = {k: records[0][k] for k in ["family", "size", "sparse", "warm", "status", "n", "m", "nnz"]}
        statuses = {r["status"]: sum(q["status"] == r["status"] for q in records) for r in records}
        row["status_counts"] = statuses
        if len(statuses) > 1:
            row["status"] = "mixed"
        row["median_seconds"] = statistics.median(r["seconds"] for r in records)
        row["median_peak_rss_kib"] = statistics.median(r["peak_rss_kib"] for r in records)
        for key in ["pivots", "nodes", "peak_entries", "refactors", "warm_nodes", "cold_fallbacks"]:
            if key in records[0]:
                values = [r[key] for r in records]
                row[key] = statistics.median(values)
                if all(r["status"] == "optimal" for r in records):
                    assert len(set(values)) == 1, (key, records)
                elif len(set(values)) > 1:
                    row[key + "_range"] = [min(values), max(values)]
        result["summary"].append(row)



if __name__ == "__main__":
    main()
