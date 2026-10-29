"""Named linear models used by the structured tutorials (not a public DSL)."""

from __future__ import annotations
from dataclasses import asdict, is_dataclass
from fractions import Fraction
import json
import math
from pathlib import Path
import numpy as np
from lp_ip_craft import Model, NativeOptions, Options, solve


def plain(value):
    if is_dataclass(value):
        return plain(asdict(value))
    if isinstance(value, np.ndarray):
        return plain(value.tolist())
    if isinstance(value, np.generic):
        return plain(value.item())
    if isinstance(value, dict):
        return {str(k): plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, Fraction):
        return str(value)
    return value


def write(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(plain(value), indent=2, allow_nan=False) + "\n")


class LinearModel:
    def __init__(self, sense="minimize"):
        self.sense = sense
        self.names, self.c, self.lo, self.hi, self.kinds = [], [], [], [], []
        self.rows, self.row_names, self.rl, self.ru = [], [], [], []

    def var(self, name, cost=0.0, lower=0.0, upper=1.0, kind="continuous"):
        if name in self.names or not math.isfinite(cost) or lower > upper:
            raise ValueError("duplicate name or invalid variable")
        self.names.append(name)
        self.c.append(cost)
        self.lo.append(lower)
        self.hi.append(upper)
        self.kinds.append(kind)
        return len(self.names) - 1

    def row(self, name, coefficients, lower=-math.inf, upper=math.inf):
        if name in self.row_names or lower > upper:
            raise ValueError("duplicate name or invalid row")
        d = dict(coefficients)
        if any(i < 0 or i >= len(self.c) or not math.isfinite(v) for i, v in d.items()):
            raise ValueError("invalid row coefficient")
        self.rows.append(d)
        self.row_names.append(name)
        self.rl.append(lower)
        self.ru.append(upper)

    def arrays(self):
        ptr, ix, values = [0], [], []
        for row in self.rows:
            for i, v in sorted(row.items()):
                if v:
                    ix.append(i)
                    values.append(float(v))
            ptr.append(len(ix))
        return ptr, ix, values

    def model(self, relaxation=False):
        ptr, ix, values = self.arrays()
        return Model(
            objective=self.c,
            lower=self.lo,
            upper=self.hi,
            row_lower=self.rl,
            row_upper=self.ru,
            indptr=ptr,
            indices=ix,
            values=values,
            kinds=["continuous"] * len(self.c) if relaxation else self.kinds,
            sense=self.sense,
        )

    def artifact(self):
        ptr, ix, values = self.arrays()
        return plain(
            dict(
                schema_version="lp-ip.model/1",
                sense=self.sense,
                objective=self.c,
                objective_offset=0.0,
                variable_lower=self.lo,
                variable_upper=self.hi,
                kinds=self.kinds,
                matrix=dict(indptr=ptr, indices=ix, values=values),
                row_lower=self.rl,
                row_upper=self.ru,
            )
        )

    def check(self, x, relaxation=False, tol=1e-7):
        x = np.asarray(x, dtype=float)
        if len(x) != len(self.c) or not np.all(np.isfinite(x)):
            raise AssertionError("invalid point")
        activities = np.array(
            [sum(v * x[i] for i, v in row.items()) for row in self.rows]
        )
        violation = max(
            0.0,
            float(np.max(np.array(self.lo) - x)),
            float(np.max(x - np.array(self.hi))),
            max([0.0] + [l - a for l, a in zip(self.rl, activities)]),
            max([0.0] + [a - u for a, u in zip(activities, self.ru)]),
        )
        integer = max(
            [0.0]
            + [abs(v - round(v)) for v, k in zip(x, self.kinds) if k != "continuous"]
        )
        return dict(
            objective=float(np.dot(self.c, x)),
            primal_violation=violation,
            integrality_violation=integer,
            feasible=violation <= tol and (relaxation or integer <= tol),
            activities=activities.tolist(),
        )

    def run(self, relaxation=False, start=None, seconds=30, nodes=10000, presolve=True):
        r = solve(
            self.model(relaxation),
            backend="native",
            start=start,
            options=Options(threads=1),
            backend_options=NativeOptions(
                time_limit_seconds=seconds,
                node_limit=nodes,
                iteration_limit=200000,
                presolve=presolve,
                component_threads=1,
            ),
        )
        record = plain(r)
        record["independent"] = (
            self.check(r.candidate, relaxation) if r.candidate is not None else None
        )
        if r.status == "optimal":
            assert record["independent"] and record["independent"]["feasible"], record
            assert (
                abs(r.evaluation.objective - record["independent"]["objective"]) < 1e-6
            )
        return r, record


def exact_cover_bound(A, demand, prices):
    """Exact dual feasibility for min 1'x, Ax>=d, x>=0. Never rounds a bound."""
    y = [Fraction(v) for v in prices]
    if not len(A) or len(A) != len(demand) or len(y) != len(A):
        raise ValueError("certificate dimensions differ")
    if not len(A[0]) or any(len(row) != len(A[0]) for row in A):
        raise ValueError("invalid covering matrix")
    if any(v < 0 for v in y):
        raise ValueError("negative covering dual")
    for col in zip(*A):
        if sum(Fraction(a) * v for a, v in zip(col, y)) > 1:
            raise ValueError("dual infeasible")
    return sum(Fraction(d) * v for d, v in zip(demand, y))
