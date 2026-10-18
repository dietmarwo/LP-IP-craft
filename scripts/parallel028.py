"""Isolated, CPU-budgeted known-answer timing for the LP parallel series."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import statistics
import subprocess
import time


def validate(payload, case):
    family, size, mode, inner, outer, jobs = case
    if (
        payload.get("status") != ("checked" if family == "factor" else "optimal")
        or payload.get("completed") != jobs
    ):
        raise ValueError("incomplete/nonoptimal child results")
    for k in ["seconds", "objective", "residual"]:
        if (
            isinstance(payload.get(k), bool)
            or not isinstance(payload.get(k), (float, int))
            or not math.isfinite(payload[k])
        ):
            raise ValueError("nonfinite/missing " + k)
    if payload["seconds"] <= 0 or payload["residual"] < 0 or payload["residual"] > 1e-4:
        raise ValueError("negative protocol field")
    expected = dict(
        family=family, size=size, mode=mode, inner=inner, workers=outer, jobs=jobs
    )
    if any(payload.get(k) != v for k, v in expected.items()):
        raise ValueError("child case mismatch")
    if inner * outer > 8 or min(inner, outer, jobs) < 1:
        raise ValueError("CPU budget must be positive and at most8")
    return payload


def default_cases(full=False, numeric=False):
    cases = [
        ["packing", 512, "single", 1, 1, 1],
        ["packing", 32, "batch", 1, 8, 64],
        ["packing", 8, "batch", 1, 8, 256],
        ["packing", 32, "skewbatch", 1, 8, 32],
        ["packing", 64, "skewcomponents", 1, 8, 1],
    ]
    if numeric:
        cases += [["dense", 192, "single", t, 1, 1] for t in [1, 8]]
    if full:
        cases = []
        for family, sizes in [
            ("packing", [512, 1024]),
            ("irregular", [128, 256]),
            ("selection", [20, 28]),
            ("dense", [192, 384]),
        ]:
            for n in sizes:
                for t in ([1, 2, 4, 8] if numeric else [1]):
                    cases.append([family, n, "single", t, 1, 1])
        for mode in ["batch", "skewbatch"]:
            for t in [1, 2, 4, 8]:
                cases.append(["packing", 32, mode, 1, t, 32])
        for mode in ["components", "skewcomponents"]:
            for t in [1, 2, 4, 8]:
                cases.append(["packing", 64, mode, 1, t, 1])
        for mode in ["tree", "residual"]:
            for t in [1, 2, 4, 8]:
                cases.append(["selection", 28, mode, 1, t, 1])
    return cases


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("builds", nargs="+", help="label=path")
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--case", action="append", help="family:size:mode:inner:outer:jobs")
    p.add_argument("--full", action="store_true")
    p.add_argument("--numeric", action="store_true")
    p.add_argument("--timeout", type=float, default=45)
    a = p.parse_args()
    if a.repeats < 1:
        p.error("at least one measured repetition is required")
    builds = {}
    for item in a.builds:
        name, path = item.split("=", 1)
        if name in builds:
            p.error("duplicate build label")
        builds[name] = Path(path).resolve()
    cases = []
    for s in a.case or []:
        f, n, m, i, w, j = s.split(":")
        cases.append([f, int(n), m, int(i), int(w), int(j)])
    if not cases:
        cases = default_cases(a.full, a.numeric)
    for c in cases:
        if c[3] * c[4] > 8 or min(c[3:]) < 1:
            p.error("case exceeds eight-thread budget")
    affinity = sorted(os.sched_getaffinity(0))[:8]
    quota = Path("/sys/fs/cgroup/cpu.max")
    result = {
        "schema_version": 1,
        "status": "running",
        "repeats": a.repeats,
        "cases": cases,
        "affinity": affinity,
        "cpu_max": quota.read_text().strip() if quota.exists() else None,
        "builds": {
            k: {"path": str(v), "sha256": hashlib.sha256(v.read_bytes()).hexdigest()}
            for k, v in builds.items()
        },
        "records": [],
    }
    a.output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        a.output.write_text(json.dumps(result, indent=2) + "\n")

    save()
    try:
        for c in cases:
            labels = list(builds)
            for repetition in range(a.repeats + 1):
                for label in (
                    labels[repetition % len(labels) :]
                    + labels[: repetition % len(labels)]
                ):
                    command = [str(builds[label]), *map(str, c)]
                    before = resource.getrusage(resource.RUSAGE_CHILDREN)
                    t = time.perf_counter()
                    row = {
                        "build": label,
                        "case": c,
                        "warmup": repetition == 0,
                        "repetition": repetition,
                        "command": command,
                        "error": None,
                    }
                    result["records"].append(row)
                    try:
                        proc = subprocess.run(
                            command,
                            capture_output=True,
                            text=True,
                            timeout=a.timeout,
                            preexec_fn=lambda: os.sched_setaffinity(0, affinity),
                        )
                        row.update(
                            process_seconds=time.perf_counter() - t,
                            returncode=proc.returncode,
                            stdout=proc.stdout,
                            stderr=proc.stderr,
                        )
                        after = resource.getrusage(resource.RUSAGE_CHILDREN)
                        row["cpu_seconds"] = (after.ru_utime + after.ru_stime) - (
                            before.ru_utime + before.ru_stime
                        )
                        if proc.returncode:
                            raise ValueError("child exit " + str(proc.returncode))
                        row["payload"] = validate(json.loads(proc.stdout), c)
                    except Exception as e:
                        row["error"] = repr(e)
                        raise
                    save()
        summary = []
        for c in cases:
            for label in builds:
                records = [
                    r
                    for r in result["records"]
                    if r["case"] == c and r["build"] == label and not r["warmup"]
                ]
                assert len(records) == a.repeats
                summary.append(
                    {
                        "build": label,
                        "case": c,
                        "samples": len(records),
                        "seconds": statistics.median(
                            r["payload"]["seconds"] for r in records
                        ),
                        "process_seconds": statistics.median(
                            r["process_seconds"] for r in records
                        ),
                        "cpu_seconds": statistics.median(
                            r["cpu_seconds"] for r in records
                        ),
                        "objective": records[0]["payload"]["objective"],
                        "pivots": records[0]["payload"]["pivots"],
                        "nodes": records[0]["payload"]["nodes"],
                    }
                )
        result["summary"] = summary
        result["status"] = "passed"
    except Exception as e:
        result["status"] = "failed"
        result["error"] = repr(e)
        save()
        raise
    save()
    print(
        json.dumps(
            {
                "output": str(a.output),
                "status": result["status"],
                "cases": len(cases),
                "records": len(result["records"]),
            }
        )
    )


if __name__ == "__main__":
    main()
