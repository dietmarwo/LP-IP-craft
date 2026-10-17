"""Isolated Linux process measurements of the family benchmark.

Build first: cargo build --release -p lp-ip-craft-native --example family_benchmark

Optionally pass a baseline binary of the 0.15.0 `sparse_benchmark` example
(arguments `family size sparse|dense warm|cold`) to measure both builds on
the same cases in alternating order. Solver time excludes model
construction; peak RSS covers the whole process. Every optimal record must
match the family's independently known optimum (checked by the binaries).
No third-party Python packages are required.
"""
import argparse
import json
import platform
import statistics
import subprocess
from pathlib import Path

CASES = [("packing", [256, 1024, 4096]), ("network", [128, 512, 2048]),
         ("irregular", [256, 1024]), ("selection", [20, 32, 64])]


def runs(args):
    cases = []
    for family, sizes in CASES:
        for size in sizes:
            for warm in ([True, False] if family == "selection" else [True]):
                cases.append(("new", family, size, warm))
                if args.baseline:
                    for kernel in ["sparse", "dense"]:
                        cases.append((f"0.15-{kernel}", family, size, warm))
    return cases


def command(args, build, family, size, warm):
    mode = "warm" if warm else "cold"
    if build == "new":
        return [args.binary, family, str(size), mode]
    return [args.baseline, family, str(size), build.split("-")[1], mode]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", default="target/release/examples/family_benchmark")
    p.add_argument("--baseline", help="0.15.0 sparse_benchmark binary")
    p.add_argument("--output", default="docs/simplex/benchmarks.json")
    p.add_argument("--repeats", type=int, default=3)
    args = p.parse_args()
    assert args.repeats > 0
    args.binary = str(Path(args.binary).resolve())
    if args.baseline:
        args.baseline = str(Path(args.baseline).resolve())
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    cases = runs(args)
    cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")), "unknown")
    result = {"cpu": cpu, "platform": platform.platform(),
              "rustc": subprocess.check_output(["rustc", "-V"], text=True).strip(),
              "repeats": args.repeats, "warmups_per_case": 1,
              "method": "isolated processes, rotated/reversed case order, single thread, "
                        "no decomposition, 30 s solver time limit; no CPU affinity guarantee",
              "records": []}
    for run in range(args.repeats + 1):
        order = cases[run % len(cases):] + cases[:run % len(cases)]
        if run % 2:
            order.reverse()
        for build, family, size, warm in order:
            cmd = command(args, build, family, size, warm)
            done = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
            if done.returncode:
                raise RuntimeError(f"{cmd}: {done.stderr}")
            record = json.loads(done.stdout)
            record.update(build=build, run=run, warm=warm)
            record.setdefault("iterations", record.get("pivots"))
            result["records"].append(record)
        print(f"completed round {run}/{args.repeats}", flush=True)
    result["summary"] = summarize(result["records"], cases)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(table(result["summary"]))


def summarize(records, cases):
    rows = []
    for build, family, size, warm in cases:
        group = [r for r in records if r["run"] > 0 and
                 (r["build"], r["family"], r["size"], r["warm"]) == (build, family, size, warm)]
        statuses = sorted({r["status"] for r in group})
        rows.append({"build": build, "family": family, "size": size, "warm": warm,
                     "n": group[0].get("n"), "m": group[0].get("m"), "nnz": group[0].get("nnz"),
                     "status": statuses[0] if len(statuses) == 1 else "mixed:" + ",".join(statuses),
                     "median_ms": round(1e3 * statistics.median(r["seconds"] for r in group), 3),
                     "iterations": group[0].get("iterations"), "nodes": group[0].get("nodes"),
                     "peak_rss_kib": statistics.median(r["peak_rss_kib"] for r in group)})
    return rows


def table(rows):
    lines = ["| case | build | status | median ms | iterations | nodes |", "|---|---|---|---:|---:|---:|"]
    for r in rows:
        case = f"{r['family']} {r['size']}" + ("" if r["family"] != "selection" else (" warm" if r["warm"] else " cold"))
        lines.append(f"| {case} | {r['build']} | {r['status']} | {r['median_ms']} | {r['iterations']} | {r['nodes']} |")
    return "\n".join(lines)


if __name__ == "__main__":
    main()
