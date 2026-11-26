"""Run the frozen LocalMIP pilot, one fresh solver process per observation."""
import argparse
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
import fcntl
import hashlib
import json
import math
import os
import platform
from pathlib import Path
import subprocess

REPO = Path(__file__).resolve().parents[2]
SEEDS = (11, 29, 47)
LIMITS = {"smoke": (5, 1000, 50000), "campaign": (30, 5000, 250000),
          "root": (30, 5000, 250000), "history": (30, 100000, 1000000),
          "parity": (None, 5000, 250000), "context": (30, 5000, 250000)}
TOL = 1e-7


def digest(data):
    return hashlib.sha256(data).hexdigest()


def validate(model, result):
    """Check the returned point from the frozen original arrays, outside timing."""
    x = result.get("candidate")
    if x is None:
        return None
    if len(x) != len(model["objective"]) or not all(math.isfinite(v) for v in x):
        return {"accepted": False, "error": "invalid primal vector"}

    def violation(values, lower, upper):
        return max((max(0.0, 0.0 if lo is None else lo - v, 0.0 if hi is None else v - hi)
                    for v, lo, hi in zip(values, lower, upper, strict=True)), default=0.0)

    a = model["values"]
    inds = model["indices"]
    ptr = model["indptr"]
    activities = [math.fsum(a[k] * x[inds[k]] for k in range(lo, hi))
                  for lo, hi in zip(ptr, ptr[1:])]
    primal = max(violation(x, model["lower"], model["upper"]),
                 violation(activities, model["row_lower"], model["row_upper"]))
    integrality = max((abs(v - round(v)) for k, v in zip(model["kinds"], x, strict=True)
                       if k != "continuous"), default=0.0)
    objective = model.get("objective_offset", 0.) + math.fsum(v * c for v, c in zip(x, model["objective"], strict=True))
    return {"accepted": primal <= TOL and integrality <= TOL,
            "objective": objective, "primal_violation": primal, "integrality_violation": integrality}


def records(stdout):
    events = []
    for line in stdout.splitlines():
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            return events, "truncated or malformed JSON lines"
    return events, None


def verify_events(events, model, result):
    issues = []
    if any(a["seconds"] > b["seconds"] or a["nodes"] > b["nodes"] or a["pivots"] > b["pivots"]
           for a, b in zip(events, events[1:])):
        issues.append("non-monotone event clock or work counters")
    points = [e["objective"] for e in events if e["kind"] == "checked_incumbent"]
    sign = 1 if model["sense"] == "minimize" else -1
    if any(sign * a <= sign * b for a, b in zip(points, points[1:])):
        issues.append("non-improving trajectory point")
    if any(e["kind"] == "original_check_rejection" and e["source"] == "local_mip" for e in events):
        issues.append("original-model admission rejected LocalMIP")
    if points and result and result.get("candidate") is None:
        issues.append("checked trajectory has incumbent and final result has none")
    return issues


def observation(entry, arm, seed, mode, limits, solver_hash, core, memory):
    seconds, nodes, pivots = limits
    settings = {"arm": arm, "seed": seed, "mode": mode, "seconds": seconds,
                "nodes": nodes, "pivots": pivots, "memory_mib": memory}
    signature = digest(json.dumps(settings, sort_keys=True).encode())
    key = digest((solver_hash + entry["model_sha256"] + signature).encode())[:16]
    name = f"{entry['case']}-{arm}-{seed}-{key}.json"
    expected = {field: entry[field] for field in ("case", "split", "reference", "sense", "model_sha256")}
    expected.update(settings=settings, options_sha256=signature, core=core, binary_sha256=solver_hash)
    return name, expected


def read_record(path, expected):
    row = json.loads(path.read_text())
    if any(key not in row or row[key] != value for key, value in expected.items()):
        raise ValueError(f"run identity differs from the schedule: {path.name}")
    return row


def run_one(out, fixture, name, expected, binary):
    destination = out / name
    if destination.exists():
        return read_record(destination, expected), True
    settings = expected["settings"]
    arm, seed, mode = (settings[k] for k in ("arm", "seed", "mode"))
    seconds, nodes, pivots = (settings[k] for k in ("seconds", "nodes", "pivots"))
    core, memory = expected["core"], settings["memory_mib"]
    cmd = [str(binary), str(fixture), arm, str(seconds) if seconds is not None else "none",
           str(nodes), str(pivots), str(seed), mode]
    if core is not None:
        cmd = ["taskset", "-c", str(core), *cmd]
    if memory:
        cmd = ["prlimit", f"--as={memory * 1024**2}", "--", *cmd]
    failure = None
    try:
        process = subprocess.run(cmd, capture_output=True, timeout=(seconds or 30) + 10)
        stdout, stderr = process.stdout, process.stderr
        if process.returncode:
            failure = f"exit {process.returncode}"
    except subprocess.TimeoutExpired as error:
        stdout, stderr = error.stdout or b"", error.stderr or b""
        failure = "watchdog"
    events, malformed = records(stdout.decode(errors="replace"))
    result = events.pop() if events and events[-1].get("kind") == "result" else None
    failure = failure or malformed or ("missing result" if result is None else None)
    model = json.loads(fixture.read_text())
    model = model.get("model", model)
    audit = validate(model, result) if result else None
    errors = verify_events(events, model, result)
    if audit and (not audit["accepted"] or not result["checked"]
                  or not math.isclose(audit["objective"], result["objective"], rel_tol=1e-11, abs_tol=1e-8)):
        errors.append("final candidate or objective disagrees with independent check")
    if mode != "plain" and result and result.get("checked_objective") is not None:
        checked = [e["objective"] for e in events if e["kind"] == "checked_incumbent"]
        # A proof status may return a separate certified point. Record both
        # objectives, and require the observed best to be at least as good.
        sign = 1 if model["sense"] == "minimize" else -1
        if not checked or (sign * (checked[-1] - result["checked_objective"]) >
                           1e-8 * max(1., abs(result["checked_objective"]))):
            errors.append("trajectory missed a returned checked incumbent")
    if result and (result.get("nodes", 0) > nodes or result.get("pivots", 0) > pivots):
        errors.append("solver exceeded a deterministic budget")
    if result and result["status"] in {"error", "unsupported", "numerical_failure"}:
        errors.append(f"solver status {result['status']}")
    row = {**expected, "failure": failure,
           "issues": errors, "stderr": stderr.decode(errors="replace"), "audit": audit,
           "events": events, "result": result}
    staging = destination.with_suffix(".tmp")
    staging.write_text(json.dumps(row, allow_nan=False, separators=(",", ":")) + "\n")
    staging.replace(destination)
    return row, False


def schedule(phase, entry, seed, arms):
    if arms:
        return arms
    if phase == "history":
        return ["history"] if entry["case"] in {"p0548", "gt2", "lseu"} else []
    if phase == "context":
        return ["stock"] if entry["split"] != "control" else []
    if phase == "parity":
        return ["A"]
    if entry["split"] == "control":
        return ["A", "B"] if phase == "smoke" else []
    if phase == "smoke" and entry["split"] == "heldout":
        return []
    if phase == "root":
        return ["A", "B", "C", "D"]
    pair_flip = (int(digest(entry["case"].encode())[:8], 16) + SEEDS.index(seed)) % 2
    pairs = [["A", "B"]]
    if entry["split"] != "public":
        pairs.append(["C", "D"])
    return [arm for pair in pairs for arm in (pair[::-1] if pair_flip else pair)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--phase", choices=LIMITS, required=True)
    parser.add_argument("--cores", type=int, nargs="+", default=None)
    parser.add_argument("--cases", nargs="+")
    parser.add_argument("--arms", choices=("stock", "history", "A", "B", "C", "D"), nargs="+")
    parser.add_argument("--plain", action="store_true")
    parser.add_argument("--memory-mib", type=int, default=1536)
    args = parser.parse_args()
    if args.phase == "root" and not args.cases:
        parser.error("a root replay needs named cases")
    if args.plain and (args.phase == "root" or set(args.arms or ()) & {"C", "D"}):
        parser.error("root replay and matched arms require the observed solver")
    args.out.mkdir(parents=True, exist_ok=True)
    with (args.out / ".run.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        run(args, parser)


def run(args, parser):
    data, binary = args.data.resolve(), args.binary.resolve()
    manifest = json.loads((data / "manifest.json").read_text())
    solver_hash = digest(binary.read_bytes())
    cores = args.cores or [min(os.sched_getaffinity(0))]
    if not set(cores) <= os.sched_getaffinity(0):
        parser.error("requested CPU unavailable to this process")
    if len(set(cores)) != len(cores):
        parser.error("each worker needs a distinct CPU")
    batches = defaultdict(list)
    mode = "plain" if args.plain else "root" if args.phase == "root" else "trace"
    cases = [r for r in manifest["models"] if "excluded" not in r and (not args.cases or r["case"] in args.cases)]
    if args.cases and set(args.cases) != {r["case"] for r in cases}:
        parser.error("requested case is missing or excluded")
    if args.arms and len(set(args.arms)) != len(args.arms):
        parser.error("duplicate arm")
    planned = {}
    # Place complete case/seed pairs on the same CPU and balance anticipated
    # time limits by assigning larger cases first.
    cases.sort(key=lambda e: (-e["nnz"], e["case"]))
    seeds = SEEDS if args.phase == "campaign" else SEEDS[:1]
    group = 0
    for entry in cases:
        fixture = (data if entry["split"] in {"development", "heldout"} else REPO) / entry["fixture"]
        if digest(fixture.read_bytes()) != entry["model_sha256"]:
            raise ValueError(f"model hash differs for {entry['case']}")
        for seed in seeds:
            arms = schedule(args.phase, entry, seed, args.arms)
            if not arms:
                continue
            if args.plain and set(arms) & {"C", "D"}:
                parser.error("matched arms require the observed solver")
            core = cores[group % len(cores)]
            for arm in arms:
                name, expected = observation(entry, arm, seed, mode, LIMITS[args.phase], solver_hash,
                                             core, args.memory_mib)
                if name in planned:
                    raise ValueError(f"duplicate planned observation: {name}")
                planned[name] = expected
                batches[core].append((fixture, name, expected))
            group += 1
    if not planned:
        parser.error("selection schedules zero observations")
    host = {"platform": platform.platform(), "cpu": platform.processor(), "cores": cores,
            "limits": LIMITS[args.phase], "memory_mib": args.memory_mib,
            "phase": args.phase, "binary_sha256": solver_hash, "manifest_sha256": digest((data / "manifest.json").read_bytes())}
    host.update(runner_sha256=digest(Path(__file__).read_bytes()), observations=planned)
    config = args.out / "run-config.txt"
    if config.exists():
        if json.loads(config.read_text()) != host:
            raise ValueError("run configuration differs; choose a new output directory")
    else:
        if any(args.out.glob("*.json")):
            raise ValueError("output directory contains records without a schedule")
        staged = config.with_suffix(".tmp")
        staged.write_text(json.dumps(host, indent=2) + "\n")
        staged.replace(config)
    if {p.name for p in args.out.glob("*.json")} - planned.keys():
        raise ValueError("output directory contains unscheduled observations")

    def worker(tasks):
        for fixture, name, expected in tasks:
            row, cached = run_one(args.out, fixture, name, expected, binary)
            outcome = row["result"] or {}
            print(row["case"], expected["settings"]["seed"], expected["settings"]["arm"], outcome.get("status"), outcome.get("objective"),
                  round(outcome.get("seconds", 0.), 3), row["failure"] or row["issues"] or ("cached" if cached else ""), flush=True)

    with ThreadPoolExecutor(max_workers=len(cores)) as executor:
        tasks = [executor.submit(worker, batch) for batch in batches.values()]
        for task in tasks:
            task.result()


if __name__ == "__main__":
    main()
