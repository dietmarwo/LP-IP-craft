"""Bounded fresh-process gap campaign, with identical default records checked separately.
Run with this checkout's Python; --python selects the baseline interpreter.
"""

from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
import pathlib
import platform
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load(path):
    d = json.loads(path.read_text())
    m = d["model"]
    for key in ["lower", "row_lower"]:
        m[key] = [-math.inf if v is None else v for v in m[key]]
    for key in ["upper", "row_upper"]:
        m[key] = [math.inf if v is None else v for v in m[key]]
    return m, d.get("expected")


def worker(path, variant, limit, node_limit=None):
    import lp_ip_craft as lp

    m, expected = load(path)
    model = lp.Model(**m)
    options = dict(time_limit_seconds=limit, component_threads=1)
    if node_limit is not None:
        options.update(node_limit=node_limit, time_limit_seconds=None)
    if variant != "full":
        options["mip_relative_gap"] = float(variant)
    began = time.perf_counter()
    r = lp.solve(model, backend="native", backend_options=lp.NativeOptions(**options))
    elapsed = time.perf_counter() - began
    check = lp.evaluate(model, r.candidate) if r.candidate is not None else None
    g = r.gap
    return dict(
        event="result",
        case=str(path.relative_to(ROOT)),
        variant=variant,
        version=lp.__version__,
        input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
        status=r.status,
        nodes=r.nodes,
        iterations=r.iterations,
        objective=check.objective if check else None,
        expected=expected,
        feasible=check.feasible_candidate if check else None,
        candidate=r.candidate,
        best_bound=r.best_bound,
        absolute_gap=g.absolute if g and math.isfinite(g.absolute) else None,
        relative_gap=g.relative if g and math.isfinite(g.relative) else None,
        seconds=elapsed,
        message=r.message,
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--worker", type=pathlib.Path)
    p.add_argument("--node-limit", type=int)
    p.add_argument("--variant", default="full")
    p.add_argument("--limit", type=float, default=10.0)
    p.add_argument("--python", default=sys.executable)
    p.add_argument("--output", type=pathlib.Path)
    p.add_argument("--variants", default="full,0.01,0.0001")
    a = p.parse_args()
    if a.worker:
        print(json.dumps(worker(a.worker, a.variant, a.limit, a.node_limit), allow_nan=False))
        return
    paths = [ROOT / "docs/packing-mip/corpus" / f"{n}.json" for n in ["bell5", "egout", "flugpl"]]
    paths += [
        ROOT / "docs/packing-ftran-mip-diagnostics/corpus/public" / f"{n}.json"
        for n in ["gt2", "lseu", "p0548", "rgn", "p01"]
    ]
    paths += [
        ROOT
        / "docs/packing-ftran-mip-diagnostics/corpus/fresh-packing"
        / f"packing-{d}-1024-seed20261017.json"
        for d in ["d3", "d9"]
    ]
    paths += [
        ROOT / "docs/packing-mip/corpus" / f"{n}.json" for n in ["afiro", "adlittle", "israel"]
    ]
    paths += sorted((ROOT / "docs/benchmarks/gap-limits/corpus").glob("binary-packing-n*.json"))
    a.output.parent.mkdir(parents=True, exist_ok=True)
    with a.output.open("w") as out:
        out.write(
            json.dumps(
                dict(
                    event="environment",
                    python=a.python,
                    host=platform.platform(),
                    cpus=os.cpu_count(),
                    limit=a.limit,
                    watchdog=a.limit + 20,
                    started=time.strftime("%Y-%m-%dT%H:%M:%S%z"),
                )
            )
            + "\n"
        )
        out.flush()
        for path in paths:
            for variant in a.variants.split(","):
                began = time.perf_counter()
                try:
                    proc = subprocess.run(
                        [
                            a.python,
                            str(pathlib.Path(__file__).resolve()),
                            "--worker",
                            str(path),
                            "--variant",
                            variant,
                            "--limit",
                            str(a.limit),
                        ]
                        + (["--node-limit", str(a.node_limit)] if a.node_limit is not None else []),
                        capture_output=True,
                        text=True,
                        timeout=a.limit + 20,
                    )
                    if proc.returncode:
                        raise RuntimeError(proc.stderr[-2000:])
                    result = json.loads(proc.stdout)
                except Exception as e:
                    result = dict(
                        event="result",
                        case=str(path.relative_to(ROOT)),
                        variant=variant,
                        status="harness_error",
                        message=str(e),
                        seconds=time.perf_counter() - began,
                    )
                out.write(json.dumps(result, allow_nan=False) + "\n")
                out.flush()
                print(
                    result["case"],
                    variant,
                    result["status"],
                    result.get("nodes"),
                    round(result["seconds"], 3),
                    flush=True,
                )


if __name__ == "__main__":
    main()
