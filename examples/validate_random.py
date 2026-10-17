"""Randomized LP/MIP differential check of the native backend against SciPy.

Optional and external: SciPy's ``linprog``/``milp`` (HiGHS) serve only as an
independent oracle here; the library never imports them. Each case draws a
sparse model with mixed column types (boxed, fixed, one-sided, free), row
types (equality, ranged, one-sided, free) and row scales spanning six orders
of magnitude, built around a witness point. Some LPs are made infeasible or
unbounded on purpose. The native result must match the oracle's status and
objective, pass the LP residual checks, and report a bound on the valid side.

    python examples/validate_random.py --seed 1 > random-oracle.json
"""
import argparse
import json
import math
import os
import sys
import time

import numpy as np
from scipy.optimize import Bounds as ScipyBounds
from scipy.optimize import LinearConstraint, linprog, milp

from lp_ip_craft import Model, NativeOptions, lp_residuals, solve


def column_bounds(rng, n, integer, finite):
    lower, upper = np.zeros(n), np.zeros(n)
    for j in range(n):
        if integer[j]:
            lower[j] = rng.integers(-2, 1)
            upper[j] = lower[j] + rng.choice([1, 2, 3, 5])
            continue
        kind = rng.choice(["boxed", "fixed"] if finite
                          else ["boxed", "boxed", "lower", "upper", "free", "fixed"])
        lo = float(rng.integers(-5, 3))
        hi = lo + float(rng.integers(1, 8))
        lower[j], upper[j] = {
            "boxed": (lo, hi),
            "lower": (lo, math.inf),
            "upper": (-math.inf, hi),
            "free": (-math.inf, math.inf),
            "fixed": (lo, lo),
        }[kind]
    return lower, upper


def witness(rng, lower, upper, integer):
    x = np.empty(len(lower))
    for j, (lo, hi) in enumerate(zip(lower, upper)):
        a = lo if math.isfinite(lo) else (hi - 4 if math.isfinite(hi) else -2.0)
        b = hi if math.isfinite(hi) else a + 4
        x[j] = rng.integers(a, b + 1) if integer[j] else rng.uniform(a, b)
    return x


def draw(rng, mip):
    n = int(rng.integers(8, 30) if mip else rng.integers(5, 90))
    m = int(rng.integers(3, 16) if mip else rng.integers(3, 70))
    integer = rng.random(n) < (0.6 if mip else 0.0)
    density = rng.uniform(0.08, 0.4)
    a = np.where(rng.random((m, n)) < density, rng.integers(-6, 7, (m, n)), 0).astype(float)
    for i in range(m):  # no empty rows by construction
        if not a[i].any():
            a[i, rng.integers(n)] = 1.0
    if not mip:
        a *= 10.0 ** rng.integers(-3, 4, (m, 1))  # badly scaled rows
    lower, upper = column_bounds(rng, n, integer, finite=mip)  # native MIPs need boxes
    x = witness(rng, lower, upper, integer)
    activity = a @ x
    rl, ru = np.empty(m), np.empty(m)
    for i in range(m):
        kind = rng.choice(["eq", "range", "le", "ge", "le", "ge", "free"] if not mip
                          else ["range", "le", "ge", "le", "ge"])
        slack = abs(activity[i]) * rng.uniform(0, 0.3) + rng.uniform(0, 2)
        rl[i], ru[i] = {
            "eq": (activity[i], activity[i]),
            "range": (activity[i] - slack, activity[i] + slack),
            "le": (-math.inf, activity[i] + slack),
            "ge": (activity[i] - slack, math.inf),
            "free": (-math.inf, math.inf),
        }[kind]
    restricted = np.flatnonzero(np.isfinite(ru))
    if not mip and rng.random() < 0.08 and len(restricted):  # contradictory rows
        i = int(rng.choice(restricted))
        a = np.vstack([a, a[i]])
        rl = np.r_[rl, ru[i] + 1.0]
        ru = np.r_[ru, math.inf]
    c = rng.integers(-9, 10, n).astype(float)
    sense = rng.choice(["minimize", "maximize"])
    return a, c, lower, upper, rl, ru, integer, str(sense)


def model(a, c, lower, upper, rl, ru, integer, sense):
    indptr, indices, values = [0], [], []
    for row in a:
        nz = np.flatnonzero(row)
        indices.extend(int(j) for j in nz)
        values.extend(float(row[j]) for j in nz)
        indptr.append(len(indices))
    return Model(objective=c, lower=lower, upper=upper, row_lower=rl, row_upper=ru,
                 indptr=indptr, indices=indices, values=values, sense=sense,
                 kinds=["integer" if k else "continuous" for k in integer])


class quiet_stdout:
    """HiGHS may print diagnostics on the C-level stdout; keep the JSON clean."""

    def __enter__(self):
        sys.stdout.flush()
        self.saved = os.dup(1)
        self.null = os.open(os.devnull, os.O_WRONLY)
        os.dup2(self.null, 1)

    def __exit__(self, *exc):
        os.dup2(self.saved, 1)
        os.close(self.saved)
        os.close(self.null)


def oracle(a, c, lower, upper, rl, ru, integer, sense):
    s = 1.0 if sense == "minimize" else -1.0
    codes = {0: "optimal", 2: "infeasible", 3: "unbounded"}
    with quiet_stdout():
        if integer.any():
            r = milp(s * c, integrality=integer.astype(int), bounds=ScipyBounds(lower, upper),
                     constraints=LinearConstraint(a, rl, ru))
            status = codes.get(r.status, f"scipy_{r.status}")
            return status, (s * r.fun if status == "optimal" else None)
        ub_a, ub_b = [a[np.isfinite(ru)]], [ru[np.isfinite(ru)]]
        ub_a.append(-a[np.isfinite(rl)])
        ub_b.append(-rl[np.isfinite(rl)])
        bounds = [(lo if math.isfinite(lo) else None, hi if math.isfinite(hi) else None)
                  for lo, hi in zip(lower, upper)]
        lp = dict(A_ub=np.vstack(ub_a), b_ub=np.concatenate(ub_b), bounds=bounds,
                  method="highs", options={"presolve": False})
        # Feasibility first: HiGHS presolve can report unbounded LPs as infeasible.
        feasible = linprog(np.zeros_like(c), **lp)
        if feasible.status != 0:
            return codes.get(feasible.status, f"scipy_{feasible.status}"), None
        r = linprog(s * c, **lp)
    status = codes.get(r.status, f"scipy_{r.status}")
    if status == "infeasible":
        status = "unbounded"  # feasible above; HiGHS reports some unbounded LPs so
    return status, (s * r.fun if status == "optimal" else None)


def check(case, data, settings):
    a, c, lower, upper, rl, ru, integer, sense = data
    m = model(*data)
    expected, reference = oracle(*data)
    if expected.startswith("scipy_"):
        return {"case": case, "skipped": expected}
    started = time.perf_counter()
    r = solve(m, backend="native", backend_options=settings)
    seconds = time.perf_counter() - started
    record = {"case": case, "n": m.n, "m": m.m, "mip": bool(integer.any()), "status": r.status,
              "oracle": expected, "nodes": r.nodes, "iterations": r.iterations,
              "seconds": seconds}
    problems = []
    if r.status != expected:
        problems.append(f"status {r.status} != {expected}")
    elif expected == "optimal":
        error = abs(r.evaluation.objective - reference)
        record["error"] = error
        if error > 1e-6 * (1 + abs(reference)):
            problems.append(f"objective {r.evaluation.objective} != {reference}")
        if not r.evaluation.feasible_candidate:
            problems.append("candidate not feasible")
        if r.best_bound is not None:
            gap = (reference - r.best_bound) if sense == "minimize" else (r.best_bound - reference)
            if gap < -1e-7 * (1 + abs(reference)):
                problems.append(f"bound {r.best_bound} on the wrong side of {reference}")
        if not integer.any():
            k = lp_residuals(m, r.candidate, r.duals)
            if k.primal > 1e-7 or k.duality_gap > 1e-7 or k.stationarity > 1e-6 * (1 + abs(c).max()):
                problems.append(f"residuals {k}")
    if problems:
        record["problems"] = problems
    return record


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--lp-cases", type=int, default=400)
    parser.add_argument("--mip-cases", type=int, default=150)
    parser.add_argument("--dantzig", action="store_true", help="largest-infeasibility pricing")
    parser.add_argument("--refactor-interval", type=int, default=64)
    args = parser.parse_args()
    settings = NativeOptions(steepest_edge=not args.dantzig,
                             refactor_interval=args.refactor_interval)
    rng = np.random.default_rng(args.seed)
    records = [check(f"lp-{k}", draw(rng, False), settings) for k in range(args.lp_cases)]
    records += [check(f"mip-{k}", draw(rng, True), settings) for k in range(args.mip_cases)]
    failures = [r for r in records if "problems" in r]
    statuses = {}
    for r in records:
        key = r.get("oracle", r.get("skipped"))
        statuses[key] = statuses.get(key, 0) + 1
    print(json.dumps({
        "seed": args.seed, "steepest_edge": not args.dantzig,
        "refactor_interval": args.refactor_interval, "cases": len(records),
        "oracle_statuses": statuses, "failures": len(failures),
        "max_objective_error": max((r.get("error", 0.0) for r in records), default=0.0),
        "native_seconds": sum(r.get("seconds", 0.0) for r in records),
        "lp_seconds": sum(r.get("seconds", 0.0) for r in records if not r.get("mip", True)),
        "mip_seconds": sum(r.get("seconds", 0.0) for r in records if r.get("mip")),
        "mip_nodes": sum(r.get("nodes", 0) for r in records if r.get("mip")),
        "iterations": sum(r.get("iterations", 0) for r in records),
        "failed": failures,
    }, indent=2))
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
