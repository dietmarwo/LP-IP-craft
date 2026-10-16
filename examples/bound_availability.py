"""How often safe_dual_bound is finite with native Rust duals, and how tight it is.

    python examples/bound_availability.py [--count N] [--seed S] [--table]

Random feasible, bounded LPs (n = 3..24) in four families of variable boxes,
plus MIPs bounded through their LP relaxation. Each model is bounded twice:
with the row duals native Rust returns, and with ``margin_duals`` from
safe_bound.py (costs of one-sided columns shifted by 1e-6 before solving).
Looseness is ``(optimum - bound) / max(1, |optimum|)`` against the native Rust
objective (for MIPs, its MIP objective), oriented so a valid bound gives a
nonnegative value. Requires the native Rust backend.

Every finite bound is also checked, without tolerance, against the exact
Lagrangian bound of its multipliers computed in rational arithmetic; the
script exits nonzero if one is on the wrong side. For unavailable results it
counts how many have an infinite exact Lagrangian bound, which no arithmetic
could avoid, as opposed to failing on the interval width.
"""
from __future__ import annotations

import argparse
import collections
import math
import pathlib
import statistics
import sys
from fractions import Fraction

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from safe_bound import margin_duals  # noqa: E402

from lp_ip_craft import BACKENDS, Model, safe_dual_bound, solve  # noqa: E402

INF = math.inf
FAMILIES = ["box [0, 3]", "x >= 0, no upper bound", "half the columns free",
            "MIP: all variables in [0, 4]"]


def csr(a):
    indptr, indices, values = [0], [], []
    for row in a:
        for j, v in enumerate(row):
            if v != 0:
                indices.append(j)
                values.append(float(v))
        indptr.append(len(indices))
    return indptr, indices, values


def problem(family: str, rng) -> Model:
    n = int(rng.integers(3, 25))
    m = int(rng.integers(1, n + 1))
    a = rng.normal(size=(m, n)) * (rng.random((m, n)) < 0.5)
    hidden = rng.integers(0, 3, n).astype(float)  # feasible, integral
    activity = a @ hidden
    equality = rng.random(m) < 0.4
    row_lower = [float(v) if e else -INF for v, e in zip(activity, equality)]
    row_upper = [float(v) if e else float(v + rng.uniform(0, 1))
                 for v, e in zip(activity, equality)]
    lower, upper, kinds = [0.0] * n, [3.0] * n, None
    if family != "box [0, 3]":
        # Keep the LP bounded with explicit rows instead of variable bounds.
        free = rng.random(n) < (0.5 if family == "half the columns free" else 0.0)
        lower = [-INF if f else 0.0 for f in free]
        upper = [INF] * n
        a = np.vstack([a, np.ones(n), np.eye(n)[free]])
        row_lower += [-INF] + [-10.0] * int(free.sum())
        row_upper += [float(hidden.sum() + 5)] + [INF] * int(free.sum())
    if family.startswith("MIP"):
        integer = rng.random(n) < 0.5
        upper = [4.0 if i else INF for i in integer]
        kinds = ["integer" if i else "continuous" for i in integer]
    if any(k == "integer" for k in (kinds or [])):
        upper = [4.0] * n
    indptr, indices, values = csr(a)
    return Model(objective=[float(v) for v in rng.normal(size=n)],
                 sense="minimize" if rng.random() < 0.5 else "maximize",
                 lower=lower, upper=upper, row_lower=row_lower, row_upper=row_upper,
                 indptr=indptr, indices=indices, values=values, kinds=kinds)


def exact_lagrangian(model: Model, y) -> Fraction | None:
    """The exact bound of multipliers y for the stored binary64 data."""
    s = 1 if model.sense == "minimize" else -1
    total = s * Fraction(model.objective_offset)
    residual = [s * Fraction(c) for c in model.objective]
    for i in range(model.m):
        v = s * Fraction(y[i])
        if v:
            side = model.row_lower[i] if v > 0 else model.row_upper[i]
            if math.isinf(side):
                return None
            total += v * Fraction(side)
        for k in range(model.indptr[i], model.indptr[i + 1]):
            residual[model.indices[k]] -= Fraction(model.values[k]) * v
    for j, r in enumerate(residual):
        if r:
            side = model.lower[j] if r > 0 else model.upper[j]
            if math.isinf(side):
                return None
            total += r * Fraction(side)
    return s * total


def relaxation(model: Model) -> Model:
    return Model(objective=model.objective, objective_offset=model.objective_offset,
                 sense=model.sense, lower=model.lower, upper=model.upper,
                 row_lower=model.row_lower, row_upper=model.row_upper,
                 indptr=model.indptr, indices=model.indices, values=model.values)


def run(count: int, seed: int):
    rng = np.random.default_rng(seed)
    rows, unsafe = [], []
    for family in FAMILIES:
        tally = {"native Rust duals": collections.Counter(), "margin duals": collections.Counter()}
        loose = {key: [] for key in tally}
        inherent = collections.Counter()
        for _ in range(count):
            model = problem(family, rng)
            reference = solve(model, backend="native")
            lp = solve(relaxation(model), backend="native") if model.kinds.count("integer") else reference
            if reference.status != "optimal" or lp.status != "optimal":
                continue
            sources = {"native Rust duals": lp.duals.row_duals, "margin duals": margin_duals(model)}
            for key, y in sources.items():
                if y is None:
                    tally[key]["no duals"] += 1
                    continue
                b = safe_dual_bound(model, y)
                tally[key][b.status] += 1
                exact = exact_lagrangian(model, y)
                if b.bound is None:
                    # Inherent (the exact bound of y is -inf) or interval width?
                    inherent[key] += exact is None
                    continue
                minimize = model.sense == "minimize"
                if exact is None or (Fraction(b.bound) > exact if minimize else Fraction(b.bound) < exact):
                    unsafe.append((family, key, model, y, b))
                optimum = reference.evaluation.objective
                gap = (optimum - b.bound) if minimize else (b.bound - optimum)
                loose[key].append(gap / max(1.0, abs(optimum)))
        for key in tally:
            solved = sum(tally[key].values())
            other = ", ".join(f"{s} {k}" for s, k in sorted(tally[key].items()) if s != "finite")
            if other:
                other += f" (exact bound of these duals infinite: {inherent[key]})"
            median = f"{statistics.median(loose[key]):.1e}" if loose[key] else "-"
            rows.append((family, key, f"{tally[key]['finite']} / {solved}", median, other or "-"))
    return rows, unsafe


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--count", type=int, default=100)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--table", action="store_true", help="print a Markdown table")
    args = parser.parse_args()
    if "native" not in BACKENDS:
        sys.exit("this measurement needs the native Rust backend")
    rows, unsafe = run(args.count, args.seed)
    if args.table:
        print(f"Seed {args.seed}, {args.count} models per family.\n")
        print("| Variable boxes | Multipliers | Finite bounds | Median looseness | Otherwise |")
        print("| --- | --- | ---: | ---: | --- |")
        for r in rows:
            print("| " + " | ".join(r) + " |")
    else:
        for r in rows:
            print(f"{r[0]:44s} {r[1]:13s} {r[2]:>9s}  {r[3]:>8s}  {r[4]}")
    if unsafe:
        sys.exit(f"{len(unsafe)} finite bounds on the wrong side of the exact Lagrangian: {unsafe[:3]}")


if __name__ == "__main__":
    main()
