"""Three-way frozen-generator oracle audit; all actual solver calls are journaled."""

import argparse
import contextlib
import hashlib
import importlib.util
import io
import json
import math
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def hash_source(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def worker(args):
    import numpy as np
    import lp_ip_craft as lp

    module = load_module("frozen_random", ROOT / "examples/validate_random.py")
    original = lp.solve
    journal = []
    checks = []
    context = {}
    native = Path(lp.__file__).parent
    extension = next(native.glob("_native*.so"))

    def wrapped(model, *a, **kw):
        kw["options"] = lp.Options(threads=args.threads)
        actual = hashlib.sha256()
        actual.update(model.sense.encode())
        for key in ["objective", "lower", "upper", "row_lower", "row_upper", "values"]:
            actual.update(np.asarray(getattr(model, key), dtype="<f8").tobytes())
        actual.update(np.asarray([model.objective_offset], dtype="<f8").tobytes())
        for key in ["indptr", "indices"]:
            actual.update(np.asarray(getattr(model, key), dtype="<u8").tobytes())
        actual.update("|".join(model.kinds).encode())
        r = original(model, *a, **kw)
        row = dict(
            key=context["key"],
            input_sha256=context.get("input_sha256"),
            model_sha256=actual.hexdigest(),
            threads=args.threads,
            status=r.status,
            objective=r.evaluation.objective if r.evaluation else None,
            feasible=r.evaluation.feasible_candidate if r.evaluation else False,
            bound=r.best_bound,
            nodes=r.nodes,
            pivots=r.iterations,
            point=list(r.candidate) if r.candidate is not None else None,
            duals=(
                {"row": list(r.duals.row_duals), "reduced": list(r.duals.reduced_costs)}
                if r.duals
                else None
            ),
        )
        data = context.get("data")
        if data is None:
            mat = np.zeros((model.m, model.n))
            ptr = model.indptr
            indices = model.indices
            values = model.values
            for i in range(model.m):
                mat[i, np.asarray(indices[ptr[i] : ptr[i + 1]], dtype=int)] = values[
                    ptr[i] : ptr[i + 1]
                ]
            data = (
                mat,
                np.asarray(model.objective),
                np.asarray(model.lower),
                np.asarray(model.upper),
                np.asarray(model.row_lower),
                np.asarray(model.row_upper),
                np.asarray([k != "continuous" for k in model.kinds]),
                model.sense,
            )
        row["independent_problems"] = []
        if data is not None and r.candidate is not None:
            mat, c, lo, hi, rl, ru, integer, sense = data
            x = np.asarray(r.candidate)
            activity = mat @ x
            violation = max(
                0.0,
                float(np.max(lo - x, initial=0)),
                float(np.max(x - hi, initial=0)),
                float(np.max(rl - activity, initial=0)),
                float(np.max(activity - ru, initial=0)),
            )
            integral = float(
                np.max(np.abs(x[integer] - np.rint(x[integer])), initial=0)
            )
            objective = float(c @ x)
            row["numpy_primal"] = violation
            row["numpy_integrality"] = integral
            row["numpy_objective"] = objective
            if row["feasible"] or r.status == "optimal":
                if violation > 1e-7 or integral > 1e-7:
                    row["independent_problems"].append(
                        "original NumPy primal/integrality"
                    )
                if r.evaluation is None or abs(
                    objective - r.evaluation.objective
                ) > 1e-7 * (1 + abs(objective)):
                    row["independent_problems"].append("original NumPy objective")
            if r.duals:
                stationarity = float(
                    np.max(
                        np.abs(
                            c
                            - mat.T @ np.asarray(r.duals.row_duals)
                            - np.asarray(r.duals.reduced_costs)
                        ),
                        initial=0,
                    )
                )
                row["numpy_stationarity"] = stationarity
                if r.status == "optimal" and stationarity > 1e-6 * (
                    1 + float(np.max(np.abs(c), initial=0))
                ):
                    row["independent_problems"].append("original NumPy stationarity")
        journal.append(row)
        return r

    module.solve = wrapped
    seeds = [1, 17] if args.quick else [1, 17, 2026]
    policies = [
        ("default", {}),
        ("legacy", {"mip_presolve": False, "coefficient_tightening": False}),
    ]
    if not args.quick:
        policies.append(
            (
                "eta-dantzig",
                {
                    "basis_update": "eta",
                    "steepest_edge": False,
                    "refactor_interval": 16,
                },
            )
        )
    lp_cases, mip_cases = (80, 30) if args.quick else (400, 150)
    for seed in seeds:
        for policy, changes in policies:
            rng = np.random.default_rng(seed)
            settings = lp.NativeOptions(decompose=False, component_threads=1, **changes)
            for domain, count in [("lp", lp_cases), ("mip", mip_cases)]:
                for i in range(count):
                    data = module.draw(rng, domain == "mip")
                    h = hashlib.sha256()
                    for value in data[:-1]:
                        h.update(np.asarray(value).tobytes())
                    h.update(data[-1].encode())
                    key = f"{seed}/{policy}/{domain}/{i}"
                    context.update(key=key, input_sha256=h.hexdigest(), data=data)
                    q = module.check(key, data, settings)
                    checks.append(q)
    # Exact integer/mixed rational and small SciPy LP oracle, unchanged generator.
    exact = load_module("frozen_exact", ROOT / "examples/validate_native.py")
    exact.solve = wrapped
    real = wrapped

    def exact_solve(model, *a, **kw):
        context.clear()
        context["key"] = "exact/" + str(len(journal))
        return real(model, *a, **kw)

    exact.solve = exact_solve
    oldargv = sys.argv
    try:
        sys.argv = [
            "validate_native.py",
            "--cases",
            str(10 if args.quick else 50),
            "--seed",
            "59103",
        ]
        with contextlib.redirect_stdout(io.StringIO()) as output:
            exact.main()
        exact_report = json.loads(output.getvalue())
    finally:
        sys.argv = oldargv
    # Larger full LPs are separately certified by a planted primal/dual pair.
    # These supplement (and do not replace) the unchanged original corpus.
    for n in ([193] if args.quick else [193, 512, 1024]):
        i, j = np.indices((n, n))
        mat = ((i * 37 + j * 19) % 17 + 1) / 16.0
        mat[i == j] = 2 * n
        dual = 1 + np.arange(n) % 5
        c = dual @ mat
        rhs = mat.sum(axis=1)
        lo = np.zeros(n)
        hi = np.full(n, 2.0)
        rl = np.full(n, -np.inf)
        integer = np.zeros(n, dtype=bool)
        data = (mat, c, lo, hi, rl, rhs, integer, "maximize")
        h = hashlib.sha256(mat.tobytes() + c.tobytes()).hexdigest()
        context.update(key=f"large/{n}", input_sha256=h, data=data)
        m = lp.Model(
            objective=c,
            lower=lo,
            upper=hi,
            sense="maximize",
            row_lower=rl,
            row_upper=rhs,
            indptr=np.arange(0, n * n + 1, n),
            indices=np.tile(np.arange(n), n),
            values=mat.ravel(),
        )
        r = wrapped(
            m,
            backend="native",
            backend_options=lp.NativeOptions(decompose=False, component_threads=1),
        )
        assert r.status == "optimal" and abs(
            r.evaluation.objective - c.sum()
        ) <= 1e-6 * (1 + abs(c.sum()))
        residual = lp.lp_residuals(m, r.candidate, r.duals)
        assert residual.primal <= 1e-7 and residual.duality_gap <= 1e-7
    expected_calls = (
        sum("skipped" not in q for q in checks)
        + exact_report["cases"]
        + (1 if args.quick else 3)
    )
    assert len(journal) == expected_calls and len(checks) == len(seeds) * len(
        policies
    ) * (lp_cases + mip_cases)
    result = {
        "expected_calls": expected_calls,
        "mode": args.mode,
        "threads": args.threads,
        "version": lp.__version__,
        "extension_sha256": hash_source(extension),
        "frozen_sources": {
            str(p.relative_to(ROOT)): hash_source(p)
            for p in [
                ROOT / "examples/validate_random.py",
                ROOT / "examples/validate_native.py",
            ]
        },
        "generated_checks": checks,
        "exact_report": exact_report,
        "calls": journal,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(
        json.dumps(
            {
                "mode": args.mode,
                "calls": len(journal),
                "oracle_issues": sum("problems" in c for c in checks),
                "independent_issues": sum(
                    bool(c["independent_problems"]) for c in journal
                ),
            }
        )
    )


def audit(paths, output):
    data = {k: json.loads(p.read_text()) for k, p in paths.items()}
    reference = data["baseline"]
    keys = [r["key"] for r in reference["calls"]]
    assert len(keys) == len(set(keys)), "duplicate baseline call"
    issues = []
    max_obj = 0.0
    point_changes = 0
    status_changes = []
    iteration_changes = 0
    counts = {}
    for name, d in data.items():
        assert (
            d["frozen_sources"] == reference["frozen_sources"]
        ), "generator source changed"
        rows = d["calls"]
        assert [r["key"] for r in rows] == keys, "missing/reordered actual calls"
        assert len(rows) == d["expected_calls"], "incomplete observed calls"
        assert len(d["extension_sha256"]) == 64 and d["threads"] == (
            8 if name == "parallel" else 1
        )
        counts[name] = sum(r["status"] == "optimal" for r in rows)
        for base, row in zip(reference["calls"], rows):
            assert (
                base["input_sha256"] == row["input_sha256"]
                and base["model_sha256"] == row["model_sha256"]
            ), "mathematical input changed"
            if row["independent_problems"]:
                issues.append(
                    {
                        "mode": name,
                        "key": row["key"],
                        "problems": row["independent_problems"],
                    }
                )
            if row["status"] != base["status"] or row["feasible"] != base["feasible"]:
                status_changes.append(
                    {
                        "mode": name,
                        "key": row["key"],
                        "before": base["status"],
                        "after": row["status"],
                    }
                )
            if name != "baseline":
                iteration_changes += int(
                    row["pivots"] != base["pivots"] or row["nodes"] != base["nodes"]
                )
                point_changes += int(row["point"] != base["point"])
                if row["objective"] is not None and base["objective"] is not None:
                    delta = abs(row["objective"] - base["objective"])
                    max_obj = max(max_obj, delta)
                    if delta > 1e-6 * (1 + abs(base["objective"])):
                        issues.append(
                            {
                                "mode": name,
                                "key": row["key"],
                                "problems": ["objective changed"],
                            }
                        )
        oracle_issues = [r for r in d["generated_checks"] if r.get("problems")]
        if oracle_issues:
            issues.extend(
                {"mode": name, "key": r["case"], "problems": r["problems"]}
                for r in oracle_issues
            )
    assert (
        data["serial"]["version"] == data["parallel"]["version"]
        and data["baseline"]["version"] == "0.27.0"
    )
    assert (
        data["serial"]["extension_sha256"] == data["parallel"]["extension_sha256"]
    ), "different current extensions"
    result = {
        "total_solver_calls": sum(len(d["calls"]) for d in data.values()),
        "calls_per_mode": len(keys),
        "paired_comparisons": len(keys) * 2,
        "optimal_counts": counts,
        "issues": issues,
        "status_or_acceptance_changes": status_changes,
        "node_or_pivot_changes": iteration_changes,
        "point_changes": point_changes,
        "max_objective_difference": max_obj,
        "versions": {k: d["version"] for k, d in data.items()},
        "extension_sha256": {k: d["extension_sha256"] for k, d in data.items()},
        "journal_sha256": {k: hash_source(p) for k, p in paths.items()},
        "note": "Calls include configurations; optimality is numerical, not an exact tree certificate. Existing original corpus is unchanged; larger planted LPs are a separate supplement.",
    }
    output.write_text(json.dumps(result, indent=2) + "\n")
    assert not issues and not status_changes, result
    print(json.dumps(result))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--baseline-python", type=Path)
    p.add_argument(
        "--output", type=Path, default=ROOT / "docs/parallel-028/data/audit-full"
    )
    p.add_argument("--quick", action="store_true")
    p.add_argument("--child", action="store_true")
    p.add_argument("--mode", choices=["baseline", "serial", "parallel"])
    p.add_argument("--threads", type=int, default=1)
    a = p.parse_args()
    if a.child:
        worker(a)
        return
    if a.baseline_python is None:
        p.error("--baseline-python is required")
    a.output.mkdir(parents=True, exist_ok=True)
    paths = {}
    for name, threads in [("baseline", 1), ("serial", 1), ("parallel", 8)]:
        paths[name] = a.output / (name + ".json")
        env = dict(os.environ, OPENBLAS_NUM_THREADS="1", OMP_NUM_THREADS="1")
        if name == "baseline":
            env["PYTHONPATH"] = str(a.baseline_python.resolve())
        command = [
            sys.executable,
            __file__,
            "--child",
            "--mode",
            name,
            "--threads",
            str(threads),
            "--output",
            str(paths[name]),
        ]
        if a.quick:
            command.append("--quick")
        with (a.output / (name + ".log")).open("w") as log:
            subprocess.run(
                command,
                cwd=ROOT,
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
            )
    audit(paths, a.output / "audit.json")


if __name__ == "__main__":
    main()
