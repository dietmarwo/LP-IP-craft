"""Frozen public MIPs and original SpOC4 checks in isolated serial child processes."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def child(args):
    import numpy as np
    import lp_ip_craft as lp

    started = time.perf_counter()
    problems = []
    expected = None
    if args.kind == "public":
        source = Path(args.name)
        payload = json.loads(source.read_text())
        spec = dict(payload["model"])
        expected = payload.get("expected")
        for k in ["lower", "upper", "row_lower", "row_upper"]:
            spec[k] = [
                (
                    (-math.inf if k in ["lower", "row_lower"] else math.inf)
                    if v is None
                    else v
                )
                for v in spec[k]
            ]
        model = lp.Model(**spec)
        start = None
        settings = lp.NativeOptions(
            decompose=False,
            component_threads=1,
            time_limit_seconds=args.seconds,
            node_limit=100000,
            iteration_limit=1000000,
        )
    else:
        sys.path.insert(0, str(ROOT / "benchmarks"))
        from spoc4_matching import load, greedy, candidate_check

        source = ROOT / "benchmarks/spoc4-025/data" / ("matching-" + args.name + ".txt")
        triples, w, mat, selected = load(source)
        seed = greedy(triples, w)
        seed[w == 0] = 0.0
        assert candidate_check(triples, w, seed)["feasible"]
        spec = dict(
            objective=w,
            lower=np.zeros(len(w)),
            upper=np.ones(len(w)),
            kinds=["binary"] * len(w),
            sense="maximize",
            row_lower=np.full(mat.shape[0], -np.inf),
            row_upper=np.ones(mat.shape[0]),
            indptr=mat.indptr,
            indices=mat.indices,
            values=mat.data,
        )
        model = lp.Model(**spec)
        start = seed
        settings = lp.NativeOptions(
            decompose=False,
            component_threads=1,
            time_limit_seconds=20.0,
            node_limit=1024,
            iteration_limit=1000000,
            packing_root=True,
            packing_root_iterations=64,
            packing_root_work=1000000000,
            packing_neighborhoods=16 if args.scenario == "repair" else 0,
            packing_rounds=1,
            packing_neighborhood_size=64,
            packing_workers=args.outer,
            packing_proof_seconds=0.0,
            scalable_branching=True,
        )
    construction = time.perf_counter() - started
    native = Path(lp.__file__).parent
    extension = next(native.glob("_native*.so"))
    t = time.perf_counter()
    try:
        r = lp.solve(
            model,
            backend="native",
            options=lp.Options(threads=args.inner),
            backend_options=settings,
            start=start,
        )
        seconds = time.perf_counter() - t
        outcome = dict(
            status=r.status,
            objective=r.evaluation.objective if r.evaluation else None,
            bound=r.best_bound,
            nodes=r.nodes,
            pivots=r.iterations,
            feasible=r.evaluation.feasible_candidate if r.evaluation else False,
            metadata=r.backend.effective_options,
        )
        point = list(r.candidate) if r.candidate is not None else None
        if point is not None:
            x = np.asarray(point)
            assert len(x) == model.n and np.isfinite(x).all()
            lo = np.asarray(spec["lower"])
            hi = np.asarray(spec["upper"])
            rl = np.asarray(spec["row_lower"])
            ru = np.asarray(spec["row_upper"])
            values = np.asarray(spec["values"])
            indices = np.asarray(spec["indices"], dtype=int)
            ptr = np.asarray(spec["indptr"], dtype=int)
            activity = np.array(
                [
                    np.dot(values[ptr[i] : ptr[i + 1]], x[indices[ptr[i] : ptr[i + 1]]])
                    for i in range(model.m)
                ]
            )
            primal = max(
                0.0,
                float(np.max(lo - x, initial=0)),
                float(np.max(x - hi, initial=0)),
                float(np.max(rl - activity, initial=0)),
                float(np.max(activity - ru, initial=0)),
            )
            integer = np.asarray(
                [k != "continuous" for k in spec.get("kinds", ["continuous"] * model.n)]
            )
            integral = float(
                np.max(np.abs(x[integer] - np.rint(x[integer])), initial=0)
            )
            objective = float(np.asarray(spec["objective"]) @ x) + spec.get(
                "objective_offset", 0.0
            )
            if outcome["feasible"] or r.status == "optimal":
                if primal > 1e-7 or integral > 1e-7:
                    problems.append("original NumPy primal/integrality")
                if outcome["objective"] is None or abs(
                    objective - outcome["objective"]
                ) > 1e-6 * (1 + abs(objective)):
                    problems.append("original objective mismatch")
            outcome.update(
                primal=primal,
                integrality=integral,
                checked_objective=objective,
                point_sha256=hashlib.sha256(
                    np.asarray(point, dtype="<f8").tobytes()
                ).hexdigest(),
                point=point,
            )
            if args.kind == "spoc":
                outcome["resource_check"] = candidate_check(triples, w, point)
                assert outcome["resource_check"]["feasible"]
                assert (
                    r.best_bound is None
                    or r.best_bound
                    >= outcome["resource_check"]["objective_milli"] - 1e-6
                )
        if r.status == "optimal" and point is None:
            problems.append("optimal without candidate")
        if expected is not None:
            tolerance = 1e-6 * (1 + abs(expected))
            sense = spec.get("sense", "minimize")
            if r.status == "optimal" and (
                outcome["objective"] is None
                or abs(outcome["objective"] - expected) > tolerance
            ):
                problems.append("wrong optimality")
            if r.best_bound is not None and (
                r.best_bound > expected + tolerance
                if sense == "minimize"
                else r.best_bound < expected - tolerance
            ):
                problems.append("bound on wrong side of reference")
        if args.kind == "spoc":
            assert (
                int(r.backend.effective_options["diagnostics.packing_root_iterations"])
                == 64
            ), "fixed root work did not finish"
            assert (
                int(
                    r.backend.effective_options[
                        "diagnostics.packing_neighborhood_errors"
                    ]
                )
                == 0
            )
    except Exception as e:
        if isinstance(e, AssertionError):
            raise
        seconds = time.perf_counter() - t
        outcome = dict(status="error", error=type(e).__name__ + ": " + str(e))
    record = dict(
        kind=args.kind,
        name=args.name,
        scenario=args.scenario,
        inner=args.inner,
        outer=args.outer,
        version=lp.__version__,
        extension_sha256=sha(extension),
        input_sha256=sha(source),
        construction_seconds=construction,
        seconds=seconds,
        workflow_seconds=time.perf_counter() - started,
        reference=expected,
        issues=problems,
        result=outcome,
    )
    print(json.dumps(record, allow_nan=False))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--baseline-python", type=Path)
    p.add_argument(
        "--output", type=Path, default=ROOT / "docs/parallel-028/data/applications.json"
    )
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--seconds", type=float, default=1.0)
    p.add_argument("--spoc-only", action="store_true")
    p.add_argument("--public-only", action="store_true")
    p.add_argument("--quick", action="store_true")
    p.add_argument("--child", action="store_true")
    p.add_argument("--kind", choices=["public", "spoc"])
    p.add_argument("--name")
    p.add_argument("--scenario", choices=["root", "repair"], default="root")
    p.add_argument("--inner", type=int, default=1)
    p.add_argument("--outer", type=int, default=1)
    a = p.parse_args()
    if a.inner * a.outer > 8 or min(a.inner, a.outer) < 1:
        p.error("eight-worker total budget exceeded")
    if a.child:
        child(a)
        return
    if a.baseline_python is None:
        p.error("--baseline-python required")
    cases = []
    if not a.spoc_only:
        names = [
            "avgas",
            "bell5",
            "egout",
            "flugpl",
            "gt2",
            "lseu",
            "p0548",
            "p01",
            "rgn",
        ]
        if a.quick:
            names = ["avgas", "egout", "gt2"]
        for name in names:
            source = next(ROOT.glob("docs/*/corpus/**/" + name + ".json"))
            cases.append(["public", str(source), "root"])
        if not a.quick:
            for source in sorted(
                (ROOT / "benchmarks/rust-solvers/heldout").glob("*.json")
            ):
                if source.name != "manifest.json":
                    cases.append(["public", str(source), "root"])
    if not a.public_only:
        for instance in ["i", "ii"]:
            for scenario in (["root"] if a.quick else ["root", "repair"]):
                cases.append(["spoc", instance, scenario])
    affinity = sorted(os.sched_getaffinity(0))[:8]
    configurations = []
    for case in cases:
        configurations.append((case, "baseline", 1, 1))
        configurations.append((case, "final", 1, 1))
        configurations.append((case, "final", 8, 1))
        if case[0] == "spoc" and case[2] == "repair":
            configurations.extend([(case, "baseline", 1, 8), (case, "final", 1, 8)])
    records = []
    result = dict(
        protocol="serial fresh processes; no compilation/stress during timing; one warmup; fixed root64 and fixed repair tasks; original-resource/oracle checks",
        status="running",
        repeats=a.repeats,
        affinity=affinity,
        cpu_max=Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
        configurations=configurations,
        records=records,
    )
    a.output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        a.output.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")

    save()
    try:
        for case in cases:
            configs = [c for c in configurations if c[0] == case]
            for repetition in range(a.repeats + 1):
                for config in (
                    configs[repetition % len(configs) :]
                    + configs[: repetition % len(configs)]
                ):
                    _, build, inner, outer = config
                    command = [
                        sys.executable,
                        __file__,
                        "--child",
                        "--kind",
                        case[0],
                        "--name",
                        case[1],
                        "--scenario",
                        case[2],
                        "--inner",
                        str(inner),
                        "--outer",
                        str(outer),
                        "--seconds",
                        str(a.seconds),
                    ]
                    env = dict(
                        os.environ, OPENBLAS_NUM_THREADS="1", OMP_NUM_THREADS="1"
                    )
                    if build == "baseline":
                        env["PYTHONPATH"] = str(a.baseline_python.resolve())
                    row = dict(
                        case=case,
                        build=build,
                        inner=inner,
                        outer=outer,
                        warmup=repetition == 0,
                        repetition=repetition,
                        command=command,
                        error=None,
                    )
                    records.append(row)
                    cpu = resource.getrusage(resource.RUSAGE_CHILDREN)
                    t = time.perf_counter()
                    proc = subprocess.run(
                        command,
                        cwd=ROOT,
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=60,
                        preexec_fn=lambda: os.sched_setaffinity(0, affinity),
                    )
                    after = resource.getrusage(resource.RUSAGE_CHILDREN)
                    row.update(
                        process_seconds=time.perf_counter() - t,
                        cpu_seconds=(after.ru_utime + after.ru_stime)
                        - (cpu.ru_utime + cpu.ru_stime),
                        returncode=proc.returncode,
                        stdout=proc.stdout,
                        stderr=proc.stderr,
                    )
                    if proc.returncode:
                        raise RuntimeError(
                            "application child failed: " + proc.stderr[-1000:]
                        )
                    row["payload"] = json.loads(proc.stdout)
                    assert not row["payload"]["issues"]
                    assert all(
                        math.isfinite(row["payload"][k])
                        for k in ["seconds", "workflow_seconds", "construction_seconds"]
                    )
                    save()
        summary = []
        for case, build, inner, outer in configurations:
            rows = [
                r
                for r in records
                if r["case"] == case
                and r["build"] == build
                and r["inner"] == inner
                and r["outer"] == outer
                and not r["warmup"]
            ]
            assert len(rows) == a.repeats
            summary.append(
                dict(
                    case=case,
                    build=build,
                    inner=inner,
                    outer=outer,
                    samples=len(rows),
                    seconds=statistics.median(r["payload"]["seconds"] for r in rows),
                    workflow_seconds=statistics.median(
                        r["payload"]["workflow_seconds"] for r in rows
                    ),
                    cpu_seconds=statistics.median(r["cpu_seconds"] for r in rows),
                    statuses=[r["payload"]["result"]["status"] for r in rows],
                    objectives=[r["payload"]["result"].get("objective") for r in rows],
                    bounds=[r["payload"]["result"].get("bound") for r in rows],
                    relaxation_seconds=statistics.median(
                        float(
                            r["payload"]["result"]
                            .get("metadata", {})
                            .get("diagnostics.packing_relaxation_seconds", 0)
                        )
                        for r in rows
                    ),
                )
            )
        result["summary"] = summary
        result["status"] = "passed"
    except Exception as e:
        result["status"] = "failed"
        result["error"] = repr(e)
        if records:
            records[-1]["error"] = repr(e)
        save()
        raise
    save()
    print(
        json.dumps(
            {
                "output": str(a.output),
                "status": "passed",
                "configurations": len(configurations),
                "records": len(records),
            }
        )
    )


if __name__ == "__main__":
    main()
