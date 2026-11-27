"""Run isolated solver processes on a frozen roster and check original points."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
import os
from pathlib import Path
import resource
from queue import Queue
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.append(str(ROOT / "benchmarks/rust-solvers"))
TOL = 1e-7


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2, allow_nan=False) + "\n")


def corrected_reference(case, input_sha256, expected, corrections):
    correction = corrections.get(input_sha256)
    if correction is None:
        return expected
    if correction["case"] != case or correction["original_expected"] != expected:
        raise ValueError("reference correction identity disagrees with the frozen model")
    return correction["expected"]


CORRECTIONS = Path(__file__).with_name("reference-corrections.json")


def checked_point(spec, point):
    if point is None:
        return None
    obj, lo, up = spec["objective"], spec["lower"], spec["upper"]
    if len(obj) != len(point) or not all(map(math.isfinite, point)):
        raise ValueError("candidate has wrong length or nonfinite values")
    violation = max(0., max((a-x for a, x in zip(lo, point) if a is not None), default=0.),
                    max((x-b for b, x in zip(up, point) if b is not None), default=0.))
    integrality = max((abs(x-round(x)) for x, k in zip(point, spec["kinds"])
                       if k != "continuous"), default=0.)
    ptr, ind, val = spec["indptr"], spec["indices"], spec["values"]
    for i, (lower, upper) in enumerate(zip(spec["row_lower"], spec["row_upper"])):
        activity = math.fsum(val[k] * point[ind[k]] for k in range(ptr[i], ptr[i + 1]))
        if lower is not None:
            violation = max(violation, lower - activity)
        if upper is not None:
            violation = max(violation, activity - upper)
    objective = math.fsum([spec.get("objective_offset", 0.), *(c*x for c, x in zip(obj, point))])
    return dict(objective=objective, primal_violation=violation, integrality_violation=integrality,
                feasible=violation <= TOL and integrality <= TOL)


def validate(spec, result, expected, expected_status):
    issues = []
    point = result.get("candidate")
    checked = checked_point(spec, point)
    value = checked and checked["objective"]
    if checked is not None:
        if not checked["feasible"]:
            issues.append("candidate exceeds original feasibility tolerance")
        obj = result.get("objective")
        if obj is None or not math.isclose(value, obj, abs_tol=1e-7, rel_tol=1e-7):
            issues.append("reported objective disagrees with original model")
    status, bound = result["status"], result.get("bound")
    if status == "optimal":
        if checked is None or not checked["feasible"]:
            issues.append("optimal without checked feasible point")
        if expected is not None and (value is None or not math.isclose(value, expected, abs_tol=1e-6, rel_tol=1e-7)):
            issues.append("optimal objective differs from reference")
        if bound is not None and value is not None and not math.isclose(bound, value, abs_tol=1e-6, rel_tol=1e-7):
            issues.append("optimal result has open gap")
        if bound is None and any(k != "continuous" for k in spec["kinds"]):
            issues.append("optimal MIP without bound")
    if status in ("infeasible", "unbounded", "infeasible_or_unbounded") and expected_status == "optimal":
        issues.append("terminal status contradicts feasible bounded reference")
    direction = 1 if spec["sense"] == "minimize" else -1
    for label, tested in (("final", bound), *((e["kind"], e.get("bound")) for e in result.get("events", [])
                             if e["kind"] in ("frontier", "frontier_sample", "root_bound", "root_node"))):
        ref = expected if expected is not None else value
        if tested is not None and ref is not None and direction * (tested - ref) > 1e-6 + abs(ref) * 1e-7:
            issues.append(f"{label} bound lies beyond the reference or incumbent")
    for event in result.get("events", []):
        if "candidate" in event:
            e = checked_point(spec, event["candidate"])
            event["checked"] = e
            if not e["feasible"]:
                issues.append("callback candidate exceeds original feasibility tolerance")
    return checked, sorted(set(issues))


def profile_options(profile):
    options = {}
    for token in profile.split("+"):
        if token == "default":
            continue
        if token == "legacy":
            options.update(mip_presolve=False, coefficient_tightening=False, decompose=False)
        elif token == "serial":
            options["decompose"] = False
        elif token == "dual":
            options["dual_presolve"] = True
        elif token in ("gmi", "mir"):
            options[token + "_cuts"] = True
            options.setdefault("mip_cut_rounds", 16)
        elif token.startswith("r") and token[1:].isdigit():
            options["mip_cut_rounds"] = int(token[1:])
        elif token == "adaptive":
            options["adaptive_root"] = True
        elif token == "no_obj_lattice":
            options["objective_lattice"] = False
        elif token == "lattice":
            options["integer_equality_presolve"] = True
        elif token == "reliable":
            options["reliability_branching"] = True
        elif token == "scalable":
            options["scalable_branching"] = True
        elif token == "heuristics":
            options["root_heuristics"] = True
        elif token == "mixed":
            options["mixed_propagation"] = True
        elif token == "presolve0":
            options["presolve"] = False
        else:
            raise ValueError(f"unknown native profile token {token!r}")
    return options


def native(spec, settings, profile, start):
    import lp_ip_craft as lp
    from lp_ip_craft import _native

    original = dict(spec)
    for name in ("lower", "upper", "row_lower", "row_upper"):
        inf = -math.inf if "lower" in name else math.inf
        original[name] = [v if v is not None else inf for v in spec[name]]
    model = lp.Model(**original)
    opts = lp.NativeOptions(time_limit_seconds=settings["seconds"],
                            node_limit=settings["nodes"], iteration_limit=settings["pivots"],
                            component_threads=1, mip_diagnostics=settings["observe"],
                            **profile_options(profile))
    provenance = dict(version=lp.__version__, extension_sha256=digest(Path(_native.__file__)),
                      package_sha256=digest(Path(lp.__file__)))
    began = time.perf_counter()
    try:
        answer = lp.solve(model, backend="native", backend_options=opts, start=start)
    except (ValueError, RuntimeError, NotImplementedError) as exc:
        return dict(seconds=time.perf_counter()-began,
                    result=dict(status="unsupported" if isinstance(exc, NotImplementedError) else "error",
                                error=str(exc), **provenance))
    elapsed = time.perf_counter() - began
    meta = dict(answer.backend.effective_options)
    events = []
    search = json.loads(meta.get("diagnostics.mip_search", "{}"))
    if search:
        pre = float(meta.get("diagnostics.mip_search.preprocessing_seconds", 0.))
        for stage in json.loads(meta.get("diagnostics.root_stages", "[]")):
            if "elapsed_seconds" in stage and stage["bound"] is not None:
                events.append(dict(kind="root_bound", seconds=stage["elapsed_seconds"],
                                   bound=stage["bound"], lp_objective=stage["objective"]))
        for e in search.get("clock", []):
            events.append(dict(kind="frontier_sample", seconds=pre+e["elapsed_seconds"],
                               bound=e["bound"], objective=e["objective"], nodes=e["nodes"], pivots=e["pivots"]))
        for e in search["events"]:
            kind = {"progress": "frontier", "root_relaxation": "root_bound",
                    "accepted_candidate": "checked_incumbent",
                    "certified_incumbent": "certified_incumbent"}.get(e["kind"])
            if kind:
                events.append(dict(kind=kind, seconds=pre+e["elapsed_seconds"], nodes=e["nodes"],
                                   bound=e["global_bound"], objective=e["objective"]))
    events.sort(key=lambda e: e["seconds"])
    return dict(seconds=elapsed, result=dict(
        status=answer.status, objective=answer.evaluation.objective if answer.evaluation else None,
        bound=answer.best_bound, candidate=answer.candidate, nodes=answer.nodes, pivots=answer.iterations,
        message=answer.message, metadata=meta, events=events, **provenance))


def worker(request):
    payload = read(Path(request["case_path"]))
    spec = payload["model"]
    start = request.get("start")
    if start is not None and not checked_point(spec, start)["feasible"]:
        raise ValueError("optimal-start input failed original feasibility checks")
    settings = request["settings"]
    solver = request["solver"]
    if solver == "gurobi":
        from gurobi_reference import solve
        try:
            observation = solve(spec, settings, start=start, observed=settings["observe"],
                                switches=request["switches"], presolve=request["presolve"])
        except ImportError:
            observation = dict(seconds=0, result=dict(status="unavailable", error="install optional gurobipy"))
    else:
        observation = native(spec, settings, request["profile"], start)
    result = observation["result"]
    expected = corrected_reference(request["case"], request["input_sha256"],
                                   payload.get("expected"), read(CORRECTIONS))
    checked, issues = validate(spec, result, expected, payload.get("expected_status"))
    observation.update(checked=checked, issues=issues, peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    return observation


def run_one(job):
    request, package, cpu = job
    env = os.environ.copy()
    for key in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS"):
        env[key] = "1"
    env.pop("PYTHONPATH", None)
    if package:
        env["PYTHONPATH"] = str(package)
    command = [sys.executable, str(Path(__file__).resolve()), "--worker"]
    if cpu is not None:
        command = ["taskset", "-c", str(cpu), *command]
    with tempfile.TemporaryFile(mode="w+", encoding="utf-8") as inp:
        json.dump(request, inp)
        inp.seek(0)
        try:
            p = subprocess.run(command, stdin=inp, capture_output=True, text=True, env=env,
                               timeout=request["settings"]["seconds"] + 15)
            if p.returncode:
                out = dict(seconds=None, result=dict(status="worker_error", error=p.stderr[-4000:]))
            else:
                out = json.loads(p.stdout)
        except subprocess.TimeoutExpired:
            out = dict(seconds=None, result=dict(status="watchdog"))
    req = {k: v for k, v in request.items() if k != "start"}
    req["start_sha256"] = (hashlib.sha256(json.dumps(request["start"]).encode()).hexdigest()
                            if request["start"] is not None else None)
    return dict(request=req, cpu=cpu, **out)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--group", nargs="+", default=["development"])
    p.add_argument("--cases", nargs="+")
    p.add_argument("--arms", nargs="+", default=["stock:default", "gurobi:default"])
    p.add_argument("--stock", type=Path)
    p.add_argument("--candidate", type=Path)
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--seconds", type=float, default=10.)
    p.add_argument("--nodes", type=int)
    p.add_argument("--pivots", type=int)
    p.add_argument("--cpus", type=int, nargs="+", help="distinct CPUs available for this runner")
    p.add_argument("--jobs", type=int, default=1)
    p.add_argument("--observe", action="store_true")
    p.add_argument("--presolve-report", action="store_true")
    p.add_argument("--gurobi-set", action="append", default=[])
    p.add_argument("--starts", type=Path, help="JSON mapping of case name to checked candidate")
    a = p.parse_args()
    settings = dict(seconds=a.seconds, nodes=a.nodes, pivots=a.pivots, observe=a.observe)
    switches = dict((key, float(v) if "." in v else int(v))
                    for key, v in (x.split("=", 1) for x in a.gurobi_set))
    starts = read(a.starts) if a.starts else {}
    group = set(a.group)
    manifest = read(a.manifest)
    roster = [r for r in manifest["roster"] if r["group"] in group and (a.cases is None or r["name"] in a.cases)]
    if not roster:
        p.error("selection is empty")
    if a.jobs < 1 or a.repeats < 1:
        p.error("jobs and repeats must be positive")
    if a.cpus and (len(a.cpus) != a.jobs or len(set(a.cpus)) != a.jobs
                   or not set(a.cpus).issubset(os.sched_getaffinity(0))):
        p.error("specify one distinct available CPU per job")
    targets = {"wheel": None, "stock": a.stock, "candidate": a.candidate, "gurobi": None}
    arms = [tuple(arm.split(":", 1)) for arm in a.arms]
    for solver, profile in arms:
        if solver not in targets or (solver in ("stock", "candidate") and targets[solver] is None):
            p.error(f"package path required for {solver}")
        if solver != "gurobi":
            profile_options(profile)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    cpu_pool = Queue()
    for cpu in a.cpus or [None] * a.jobs:
        cpu_pool.put(cpu)
    jobs = []
    for i, record in enumerate(roster):
        path = (a.manifest.parent / record["file"]).resolve()
        if digest(path) != record["sha256"]:
            raise ValueError(f"input hash mismatch for {path}")
        for rep in range(a.repeats):
            order = (i + rep) % len(arms)
            for solver, profile in arms[order:] + arms[:order]:
                req = dict(case=record["name"], group=record["group"], family=record["family"],
                           case_path=str(path), input_sha256=record["sha256"], repetition=rep,
                           solver=solver, profile=profile, settings=settings, start=starts.get(record["name"]),
                           switches=switches, presolve=a.presolve_report)
                jobs.append((req, targets[solver]))

    def on_cpu(job):
        cpu = cpu_pool.get()
        try:
            return run_one((*job, cpu))
        finally:
            cpu_pool.put(cpu)

    with a.output.open("x") as out, ThreadPoolExecutor(a.jobs) as pool:
        for r in pool.map(on_cpu, jobs):
            out.write(json.dumps(r, allow_nan=False) + "\n")
            out.flush()
            req, res = r["request"], r["result"]
            print(req["case"], req["solver"], req["profile"], req["repetition"], res["status"],
                  round(r["seconds"] or 0, 5), res.get("nodes"), r.get("issues", []), flush=True)


if __name__ == "__main__":
    if sys.argv[1:] == ["--worker"]:
        print(json.dumps(worker(json.load(sys.stdin)), allow_nan=False))
    else:
        main()
