"""Canonical LP1--LP9 experiments. LP10 stays with the rail model's owner."""

from __future__ import annotations
import argparse
from fractions import Fraction as F
import hashlib
import importlib.util
import itertools
import json
import math
from pathlib import Path
import sys
import time
import traceback
import numpy as np
from scipy.optimize import (
    Bounds,
    LinearConstraint,
    linear_sum_assignment,
    linprog,
    milp,
)
from linear_model import (
    LinearModel,
    exact_cover_bound,
    rational_prices,
    require,
    safe_bound,
    write,
)


def optimal(m, expected=None, relaxation=False, **kw):
    r, rec = m.run(relaxation=relaxation, **kw)
    require(r.status == "optimal", rec)
    if expected is not None:
        require(
            abs(r.evaluation.objective - expected) < 1e-5,
            (
                (
                    r.evaluation.objective,
                    expected,
                )
            ),
        )
    return r, rec


def allocation(preset):
    records = []
    for budget, expected in [(10, 16), (9, 15)]:
        m = LinearModel("maximize")
        x = [m.var(n, v, kind="binary") for n, v in zip("ABC", [9, 8, 7])]
        m.row("budget", dict(zip(x, [6, 5, 4])), upper=budget)
        ref = max(
            sum(v * z for v, z in zip([9, 8, 7], p))
            for p in itertools.product([0, 1], repeat=3)
            if sum(v * z for v, z in zip([6, 5, 4], p)) <= budget
        )
        require(ref == expected)
        lp, a = optimal(m, 16.5 if budget == 10 else 15, True)
        ip, b = optimal(m, expected)
        greedy = [0, 0, 0]
        remaining = budget
        for i in [2, 1, 0]:
            if [6, 5, 4][i] <= remaining:
                greedy[i] = 1
                remaining -= [6, 5, 4][i]
        _, warm = optimal(m, expected, start=greedy)
        fractional = m.check(lp.candidate)
        # At budget 10 the relaxation is fractional, so it must fail the binary check.
        require(fractional["feasible"] == (budget == 9), fractional)
        records.append(
            dict(
                budget=budget,
                model=m.artifact(),
                lp=a,
                mip=b,
                enumerated=ref,
                warm_start=warm,
                greedy=greedy,
                fractional_binary_check=fractional,
            )
        )
    m.row("force_all", {i: 1 for i in x}, lower=3)
    require(sum([6, 5, 4]) > 9, "the forced selection fits the budget")
    r, bad = m.run()
    require(r.status == "infeasible")
    extended = LinearModel("maximize")
    costs = [6, 5, 4, 3, 2, 1]
    staff = [3, 2, 2, 2, 1, 1]
    values = [9, 8, 7, 4, 3, 1]
    x = [extended.var(n, v, kind="binary") for n, v in zip("ABCDEF", values)]
    extended.row("cost", dict(zip(x, costs)), upper=10)
    extended.row("staff", dict(zip(x, staff)), upper=5)
    extended.row("prerequisite", {x[2]: 1, x[1]: -1}, upper=0)
    extended.row("exclusion", {x[0]: 1, x[3]: 1}, upper=1)
    oracle = max(
        sum(v * p for v, p in zip(values, z))
        for z in itertools.product([0, 1], repeat=6)
        if sum(a * b for a, b in zip(costs, z)) <= 10
        and sum(a * b for a, b in zip(staff, z)) <= 5
        and z[2] <= z[1]
        and z[0] + z[3] <= 1
    )
    _, ext = optimal(extended, oracle)
    require(oracle == 16)
    return dict(
        cases=records,
        infeasible=bad,
        extended=ext,
        independent_contradiction=f"forcing all projects costs {sum([6, 5, 4])}, above budget 9",
    )


def battery_model(
    load,
    solar,
    prices,
    eta=0.95,
    capacity=4.0,
    power=2.0,
    initial=2.0,
    exclusive=False,
    kappa=0.0,
):
    T = len(load)
    m = LinearModel()
    charge = []
    discharge = []
    grid = []
    curtail = []
    for t in range(T):
        grid.append(m.var(f"grid[{t}]", prices[t], upper=4))
        charge.append(m.var(f"charge[{t}]", kappa, upper=power))
        discharge.append(m.var(f"discharge[{t}]", kappa, upper=power))
        curtail.append(m.var(f"curtail[{t}]", upper=solar[t]))
    energy = [
        m.var(
            f"energy[{t}]",
            lower=initial if t in (0, T) else 0,
            upper=initial if t in (0, T) else capacity,
        )
        for t in range(T + 1)
    ]
    for t in range(T):
        m.row(
            f"power[{t}]",
            {grid[t]: 1, curtail[t]: -1, discharge[t]: 1, charge[t]: -1},
            lower=load[t] - solar[t],
            upper=load[t] - solar[t],
        )
        m.row(
            f"energy[{t}]",
            {energy[t + 1]: 1, energy[t]: -1, charge[t]: -eta, discharge[t]: 1 / eta},
            lower=0,
            upper=0,
        )
        if exclusive:
            z = m.var(f"mode[{t}]", kind="binary")
            m.row(f"charge_mode[{t}]", {charge[t]: 1, z: -power}, upper=0)
            m.row(f"discharge_mode[{t}]", {discharge[t]: 1, z: power}, upper=power)
    return m, dict(
        grid=grid, charge=charge, discharge=discharge, energy=energy, curtail=curtail
    )


def battery(preset):
    micro = []
    for eta, expected in [(1.0, 1.0), (0.9, 1.57)]:
        for exclusive in [False, True]:
            m, ix = battery_model([0, 1], [0, 0], [1, 3], eta, 1, 1, 0, exclusive)
            r, rec = optimal(m, expected)
            micro.append(dict(eta=eta, exclusive=exclusive, result=rec))
    for exclusive in [False, True]:
        m, ix = battery_model([0], [0], [-1], 0.9, 1, 1, 0, exclusive)
        r, rec = optimal(m, 0 if exclusive else -0.19)
        micro.append(dict(negative_price=True, exclusive=exclusive, result=rec))
    if preset == "study":
        h = np.arange(24)
        load = np.select(
            [h < 6, h < 9, h < 17, h < 22], [0.8, 2, 1.2, 2.8], default=1.0
        )
        solar = np.maximum(0, 3 * (1 - np.abs(h - 12) / 6))
        prices = np.select([h < 6, h < 17, h < 22], [0.1, 0.2, 0.4], default=0.15)
    else:
        load = [1, 1, 2, 2, 1, 1]
        solar = [0, 1, 3, 1, 0, 0]
        prices = [0.1, 0.1, 0.2, 0.4, 0.4, 0.15]
    daily = []
    for exclusive in [False, True]:
        m, ix = battery_model(load, solar, prices, exclusive=exclusive, kappa=0.01)
        r, rec = optimal(m)
        x = np.array(r.candidate)
        c, d, e, g = [x[ix[k]] for k in ["charge", "discharge", "energy", "grid"]]
        require(np.max(np.abs(np.diff(e) - 0.95 * c + d / 0.95)) < 1e-7)
        require(
            (
                np.max(
                    np.abs(
                        g
                        + np.asarray(solar)
                        - x[ix["curtail"]]
                        + d
                        - np.asarray(load)
                        - c
                    )
                )
                < 1e-7
            )
        )
        replay = []
        for lf, pf in [(1.15, 1), (1, 0.75), (1.15, 0.75)]:
            need = np.asarray(load) * lf + c - d - np.asarray(solar) * pf
            cur = np.maximum(-need, 0)
            grid = np.maximum(need, 0)
            replay.append(
                dict(
                    load_factor=lf,
                    solar_factor=pf,
                    feasible=bool(
                        np.all(grid <= 4 + 1e-7)
                        and np.all(cur <= np.asarray(solar) * pf + 1e-7)
                    ),
                    grid=grid,
                )
            )
        daily.append(
            dict(
                exclusive=exclusive,
                result=rec,
                dispatch={k: x[v] for k, v in ix.items()},
                replay=replay,
            )
        )
    baseline, ix = battery_model(load, solar, prices, capacity=0, power=0, initial=0)
    _, base = optimal(baseline)
    return dict(
        microcases=micro,
        load=load,
        solar=solar,
        prices=prices,
        daily=daily,
        no_battery=base,
    )


def count_sudoku(grid, limit=3):
    a = np.array(grid, int).copy()
    n = len(a)
    b = math.isqrt(n)
    solutions = []

    def visit():
        if len(solutions) >= limit:
            return
        best = None
        for r, c in zip(*np.where(a == 0)):
            used = (
                set(a[r])
                | set(a[:, c])
                | set(
                    a[r // b * b : r // b * b + b, c // b * b : c // b * b + b].ravel()
                )
            )
            opts = set(range(1, n + 1)) - used
            if not opts:
                return
            if best is None or len(opts) < len(best[2]):
                best = (r, c, opts)
        if best is None:
            solutions.append(a.copy())
            return
        r, c, opts = best
        for v in sorted(opts):
            a[r, c] = v
            visit()
            a[r, c] = 0

    # Reject conflicting clues before recursion.
    units = (
        [a[r, :] for r in range(n)]
        + [a[:, c] for c in range(n)]
        + [
            a[r : r + b, c : c + b].ravel()
            for r in range(0, n, b)
            for c in range(0, n, b)
        ]
    )
    if any(len(u[u > 0]) != len(set(u[u > 0])) for u in units):
        return []
    visit()
    return solutions


def sudoku_model(grid, exclude=None):
    n = len(grid)
    b = math.isqrt(n)
    m = LinearModel()
    x = np.empty((n, n, n), int)
    for r, c, v in itertools.product(range(n), repeat=3):
        clue = grid[r][c]
        x[r, c, v] = m.var(
            f"x[{r},{c},{v + 1}]", lower=int(clue == v + 1), kind="binary"
        )
    units = [x[r, c, :] for r, c in itertools.product(range(n), repeat=2)]
    units += [x[r, :, v] for r, v in itertools.product(range(n), repeat=2)]
    units += [x[:, c, v] for c, v in itertools.product(range(n), repeat=2)]
    units += [
        x[r : r + b, c : c + b, v].ravel()
        for r in range(0, n, b)
        for c in range(0, n, b)
        for v in range(n)
    ]
    for i, u in enumerate(units):
        m.row(f"unit{i}", {int(j): 1 for j in u}, lower=1, upper=1)
    if exclude is not None:
        free = [
            x[r, c, int(exclude[r, c]) - 1]
            for r, c in itertools.product(range(n), repeat=2)
            if grid[r][c] == 0
        ]
        m.row("no_good", {int(j): 1 for j in free}, upper=len(free) - 1)
    return m, x


def sudoku(preset):
    text = [
        "..1...7..",
        ".2.68...4",
        "487.....9",
        "...2.....",
        "..8.....3",
        ".4...1...",
        "3..8.2...",
        "6...3.9.1",
        ".7.....2.",
    ]
    grid = np.array([[0 if c == "." else int(c) for c in row] for row in text])
    records = []
    for two in [False, True]:
        g = grid.copy()
        if two:
            g[4, 2] = 0
        oracle = count_sudoku(g, 3)
        require(len(oracle) == (2 if two else 1))
        m, x = sudoku_model(g)
        r, a = optimal(m, 0)
        solution = np.argmax(np.array(r.candidate)[x], axis=2) + 1
        require(any(np.array_equal(solution, z) for z in oracle))
        m2, _ = sudoku_model(g, solution)
        rr, b = m2.run()
        require(rr.status == ("optimal" if two else "infeasible"), b)
        records.append(
            dict(
                two_solutions=two,
                puzzle=g,
                solution=solution,
                independent_count=len(oracle),
                first=a,
                after_cut=b,
            )
        )
    tiny = count_sudoku(np.zeros((4, 4), int), 1000)
    require(len(tiny) == 288)
    unique = tiny[0].copy()
    for r, c in itertools.product(range(4), repeat=2):
        test = unique.copy()
        test[r, c] = 0
        if len(count_sudoku(test, 2)) == 1:
            unique = test
    multiple = None
    for r, c in itertools.product(range(4), repeat=2):
        if unique[r, c]:
            test = unique.copy()
            test[r, c] = 0
            if len(count_sudoku(test, 3)) == 2:
                multiple = test
                break
    require(multiple is not None)
    for g, count in [(unique, 1), (multiple, 2)]:
        require(
            (
                sum(
                    all(
                        g[r, c] == 0 or g[r, c] == s[r, c]
                        for r, c in itertools.product(range(4), repeat=2)
                    )
                    for s in tiny
                )
                == count
            )
        )
        mm, xx = sudoku_model(g)
        rr, _ = optimal(mm, 0)
        found = np.argmax(np.array(rr.candidate)[xx], axis=2) + 1
        cut, _ = sudoku_model(g, found)
        after, _ = cut.run()
        # The no-good row must leave exactly the other completions.
        require(
            after.status == ("infeasible" if count == 1 else "optimal"), after.status
        )
    return dict(
        shidoku_count=len(tiny),
        unique_shidoku=unique,
        two_solution_shidoku=multiple,
        cases=records,
    )


def rostering(preset):
    demand = [17, 13, 15, 19, 14, 16, 11]
    A = [[int((d - j) % 7 < 5) for j in range(7)] for d in range(7)]

    def make(integer):
        m = LinearModel()
        x = [
            m.var(f"start{j}", 1, upper=19, kind="integer" if integer else "continuous")
            for j in range(7)
        ]
        for d in range(7):
            m.row(f"coverage{d}", dict(zip(x, A[d])), lower=demand[d])
        return m, x

    lp, _ = make(False)
    r, a = optimal(lp, 67 / 3)
    # The certificate is the solver's own duals, rationalized and then checked
    # exactly; a wrong or merely approximate dual vector is refused.
    prices = rational_prices(r.duals.row_duals)
    bound = exact_cover_bound(A, demand, prices)
    require(bound == F(67, 3), bound)
    safe = safe_bound(lp, r)
    m, x = make(True)
    r, b = optimal(m, 23)
    m.row("minimum_headcount", {j: 1 for j in x}, lower=23, upper=23)
    m.c = [0.0] * 7
    for j in x:
        m.c[j] = A[5][j] + A[6][j]
    r, c = optimal(m)
    # Independent reference for the second objective: SciPy's HiGHS MILP.
    weekend = np.array(m.c)
    ref = milp(
        weekend,
        constraints=[
            LinearConstraint(np.array(A), lb=demand),
            LinearConstraint(np.ones((1, 7)), lb=23, ub=23),
        ],
        integrality=np.ones(7),
        bounds=Bounds(0, 19),
    )
    require(ref.status == 0, ref.message)
    require(
        abs(r.evaluation.objective - ref.fun) < 1e-6, (r.evaluation.objective, ref.fun)
    )
    return dict(
        demand=demand,
        coverage_matrix=A,
        lp=a,
        mip=b,
        lexicographic=c,
        lexicographic_reference=float(ref.fun),
        native_dual_prices=[str(v) for v in prices],
        exact_bound=str(bound),
        safe_dual_bound=safe,
        integer_lower_bound=math.ceil(bound),
    )


COST = np.array([[8, 6, 10, 9], [9, 12, 13, 7], [14, 9, 16, 5]])


def transport_model(carrier=False, integer=False, extra=0):
    supply = [20, 30, 25 + extra]
    demand = [10, 25, 15, 25]
    m = LinearModel()
    x = np.empty((3, 4), int)
    for i, j in itertools.product(range(3), range(4)):
        x[i, j] = m.var(
            f"flow{i}{j}",
            int(COST[i, j]),
            upper=min(supply[i], demand[j]),
            kind="integer" if integer else "continuous",
        )
    for i in range(3):
        m.row(f"supply{i}", {int(k): 1 for k in x[i]}, upper=supply[i])
    for j in range(4):
        m.row(
            f"demand{j}", {int(k): 1 for k in x[:, j]}, lower=demand[j], upper=demand[j]
        )
    if carrier:
        m.row("carrier", {int(x[0, 1]): 1, int(x[2, 3]): 1}, upper=9)
    return m, x


def transportation(preset):
    base, x = transport_model()
    base_r, a = optimal(base, 585)
    weak, _ = transport_model(True)
    rr, b = optimal(weak, 617)
    discrete, _ = transport_model(True, True)
    rr, c = optimal(discrete, 619)
    whatif = []
    for extra in [0, 1, 5, 6]:
        m, _ = transport_model(extra=extra)
        r, rec = optimal(m, 585 - 2 * min(extra, 5))
        whatif.append(dict(extra=extra, result=rec))
    A = np.array([[9, 2, 7, 8], [6, 4, 3, 7], [5, 8, 1, 8], [7, 6, 9, 4]])
    row, col = linear_sum_assignment(A)
    enum = min(
        sum(A[i, p[i]] for i in range(4)) for p in itertools.permutations(range(4))
    )
    require(enum == sum(A[row, col]) == 13)
    # The assignment LP through the native solver: integral without integrality.
    am = LinearModel()
    z = [[am.var(f"assign{i}{j}", int(A[i, j])) for j in range(4)] for i in range(4)]
    for i in range(4):
        am.row(f"worker{i}", {z[i][j]: 1 for j in range(4)}, lower=1, upper=1)
        am.row(f"task{i}", {z[j][i]: 1 for j in range(4)}, lower=1, upper=1)
    ar, assignment_lp = optimal(am, 13, True)
    require(
        max(abs(v - round(v)) for v in ar.candidate) <= 1e-9,
        "the assignment LP vertex is not integral",
    )
    # Bound from the solver's own duals of the solved model, binary64-safe.
    base_bound = safe_bound(base, base_r)
    require(abs(base_bound["bound"] - 585) < 1e-6, base_bound)
    return dict(
        costs=COST,
        base=a,
        carrier_lp=b,
        carrier_mip=c,
        whatif=whatif,
        assignment=enum,
        assignment_lp=assignment_lp,
        native_duals=base_r.duals.row_duals,
        safe_dual_bound=base_bound,
    )


def facility(preset):
    sites = np.array([[0, 0], [4, 0], [8, 0], [2, 4], [6, 4]])
    customers = np.array(
        [[1, 1], [3, 1], [5, 1], [7, 1], [1, 3], [3, 3], [5, 3], [7, 3], [2, 5], [6, 5]]
    )
    fixed = np.array([30, 25, 30, 20, 22])
    weights = np.array([4, 3, 5, 2, 3, 4, 3, 5, 2, 4])
    cost = np.abs(sites[:, None] - customers).sum(axis=2) * weights
    enumeration = []
    for mask in itertools.product([0, 1], repeat=5):
        if any(mask):
            enumeration.append(
                (
                    float(fixed @ mask + cost[np.array(mask, bool)].min(axis=0).sum()),
                    mask,
                )
            )
    reference = min(enumeration)
    require(reference[0] == 134)
    results = []
    for strong in [False, True]:
        m = LinearModel()
        y = [m.var(f"open{i}", fixed[i], kind="binary") for i in range(5)]
        x = np.array(
            [[m.var(f"assign{i}{j}", cost[i, j]) for j in range(10)] for i in range(5)]
        )
        for j in range(10):
            m.row(f"customer{j}", {int(k): 1 for k in x[:, j]}, lower=1, upper=1)
        for i in range(5):
            if strong:
                for j in range(10):
                    m.row(f"link{i}{j}", {int(x[i, j]): 1, y[i]: -1}, upper=0)
            else:
                m.row(f"link{i}", {**{int(k): 1 for k in x[i]}, y[i]: -10}, upper=0)
        lp, a = optimal(m, 134 if strong else 87.6, True)
        ip, b = optimal(m, 134)
        require(
            (
                np.all(
                    np.array(ip.candidate)[x] <= np.array(ip.candidate)[y, None] + 1e-7
                )
            )
        )
        results.append(
            dict(strong=strong, lp=a, mip=b, sites=sites, customers=customers)
        )
    # Capacitated open-set enumeration: each subproblem is an independent LP.
    caps = np.array([12, 12, 12, 12, 12])
    best = np.inf
    for _, mask in enumeration:
        allowed = np.asarray(mask)
        Aeq = np.zeros((10, 50))
        Aub = np.zeros((5, 50))
        for i, j in itertools.product(range(5), range(10)):
            Aeq[j, 10 * i + j] = 1
            Aub[i, 10 * i + j] = weights[j]
        rr = linprog(
            cost.ravel(),
            A_ub=Aub,
            b_ub=caps * allowed,
            A_eq=Aeq,
            b_eq=np.ones(10),
            bounds=(0, 1),
            method="highs",
        )
        # HiGHS status 2 means this open set cannot serve all demand; any other
        # failure would leave the reference incomplete and is an error.
        require(rr.status in (0, 2), (mask, rr.status, rr.message))
        if rr.status == 0:
            best = min(best, float(rr.fun + fixed @ allowed))
    m = LinearModel()
    y = [m.var(f"open{i}", fixed[i], kind="binary") for i in range(5)]
    x = np.array(
        [[m.var(f"assign{i}{j}", cost[i, j]) for j in range(10)] for i in range(5)]
    )
    for j in range(10):
        m.row(f"customer{j}", {int(k): 1 for k in x[:, j]}, lower=1, upper=1)
    for i in range(5):
        m.row(
            f"capacity{i}",
            {**{int(x[i, j]): weights[j] for j in range(10)}, y[i]: -caps[i]},
            upper=0,
        )
        for j in range(10):
            m.row(f"link{i}.{j}", {int(x[i, j]): 1, y[i]: -1}, upper=0)
    _, cap = optimal(m, best)
    return dict(
        reference=reference, cases=results, capacitated=dict(reference=best, result=cap)
    )


def patterns(widths, capacity):
    return [
        p
        for p in itertools.product(*(range(capacity // w + 1) for w in widths))
        if any(p) and sum(w * n for w, n in zip(widths, p)) <= capacity
    ]


def cutting(preset):
    widths = [45, 36, 31, 14]
    demand = [97, 610, 395, 211]
    allp = patterns(widths, 100)
    columns = [
        tuple(100 // w if i == j else 0 for i in range(4)) for j, w in enumerate(widths)
    ]
    history = []
    for _ in range(40):
        m = LinearModel()
        for j in range(len(columns)):
            m.var(f"pattern{j}", 1, upper=sum(demand))
        for i in range(4):
            m.row(
                f"demand{i}", {j: p[i] for j, p in enumerate(columns)}, lower=demand[i]
            )
        r, rec = optimal(m)
        y = np.array(r.duals.row_duals)
        best = max(allp, key=lambda p: np.dot(p, y))
        price = float(np.dot(best, y))
        history.append(dict(columns=columns.copy(), result=rec, pricing_value=price))
        if price <= 1 + 1e-8:
            break
        require(best not in columns)
        columns.append(best)
    else:
        raise AssertionError("pricing did not terminate")
    require(abs(r.evaluation.objective - 452.25) < 1e-6)
    # Rationalize the solver's final master duals; the exact DP below decides.
    certificate = rational_prices(r.duals.row_duals, 64)
    require(all(v >= 0 for v in certificate), certificate)
    require(max(sum(n * v for n, v in zip(p, certificate)) for p in allp) == 1)
    dp = [F(0)] * 101
    for capacity in range(1, 101):
        dp[capacity] = max(
            [dp[capacity - 1]]
            + [
                dp[capacity - w] + v
                for w, v in zip(widths, certificate)
                if w <= capacity
            ]
        )
    require(dp[-1] == 1)
    bound = sum(F(d) * v for d, v in zip(demand, certificate))
    require(bound == F(1809, 4))
    m.kinds = ["integer"] * len(m.c)
    ip, integer = optimal(m, 453)
    return dict(
        widths=widths,
        demand=demand,
        history=history,
        integer=integer,
        exact_bound=str(bound),
        all_pattern_count=len(allp),
        exact_pricing_DP=str(dp[-1]),
        certificate=certificate,
    )


def stochastic(preset):
    demand = np.array([20, 40, 60, 80, 100])
    prob = np.array([0.3, 0.3, 0.2, 0.1, 0.1])
    m = LinearModel("maximize")
    q = m.var("order", -4, upper=100)
    sales = []
    for i, (d, p) in enumerate(zip(demand, prob)):
        s = m.var(f"sales{i}", 9 * p, upper=float(d))
        sales.append(s)
        m.row(f"availability{i}", {s: 1, q: -1}, upper=0)
    m.c[q] += 1
    r, rec = optimal(m, 198)
    require(abs(r.candidate[q] - 60) < 1e-6)

    def profit(q):
        return float(
            np.dot(prob, 10 * np.minimum(demand, q) + np.maximum(q - demand, 0) - 4 * q)
        )

    require(abs(profit(48) - 190.8) < 1e-9)
    ws = float(6 * np.dot(prob, demand))
    require(ws == 288)

    def application(S):
        rng = np.random.default_rng(42)
        draw = rng.integers([15, 10], [66, 56], size=(S, 2))
        m = LinearModel("maximize")
        initial = [m.var(f"order{j}", -cost, upper=80) for j, cost in enumerate([4, 5])]
        m.row("capacity", {i: 1 for i in initial}, upper=80)
        for s in range(S):
            for j in range(2):
                buy = m.var(f"recourse{s}.{j}", -[8, 10][j] / S, upper=int(draw[s, j]))
                sold = m.var(f"sale{s}.{j}", [10, 12][j] / S, upper=int(draw[s, j]))
                left = m.var(f"left{s}.{j}", 1 / S, upper=80 + int(draw[s, j]))
                m.row(
                    f"balance{s}.{j}",
                    {initial[j]: 1, buy: 1, sold: -1, left: -1},
                    lower=0,
                    upper=0,
                )
        rr, record = optimal(m)
        q = np.array(rr.candidate)[initial]

        # Independent reference: the sample-average profit is concave and
        # piecewise linear in the integer-valued orders, with breakpoints at the
        # integer demands, so an integer grid search attains its maximum.
        def sample_average(q0, q1):
            qq = np.array([q0, q1])
            return float(
                (
                    draw * np.array([10, 12])
                    - qq * np.array([4, 5])
                    - np.maximum(draw - qq, 0) * np.array([8, 10])
                    + np.maximum(qq - draw, 0)
                )
                .sum(axis=1)
                .mean()
            )

        grid = max(sample_average(a, b) for a in range(81) for b in range(81 - a))
        require(
            abs(rr.evaluation.objective - grid) < 1e-6,
            (rr.evaluation.objective, grid),
        )
        require(
            abs(sample_average(*q) - rr.evaluation.objective) < 1e-6,
            "the LP objective differs from the closed-form recourse at its own orders",
        )
        hold = np.random.default_rng(314159).integers([15, 10], [66, 56], size=(128, 2))
        profits = (
            hold * np.array([10, 12])
            - q * np.array([4, 5])
            - np.maximum(hold - q, 0) * np.array([8, 10])
            + np.maximum(q - hold, 0)
        ).sum(axis=1)
        return dict(
            samples=S,
            result=record,
            in_sample_reference=grid,
            order=q,
            holdout_mean=float(np.mean(profits)),
            holdout_profit=profits,
        )

    apps = [application(S) for S in ([8, 16, 32] if preset == "study" else [8])]
    return dict(
        scenarios=demand,
        probabilities=prob,
        result=rec,
        mean_plan_profit=profit(48),
        VSS=198 - profit(48),
        wait_and_see=ws,
        EVPI=ws - 198,
        critical_ratio="2/3",
        two_product=apps,
    )


def spoc(preset):
    root = Path(__file__).resolve().parents[2]
    spec = importlib.util.spec_from_file_location(
        "spoc_matching", root / "benchmarks/spoc4_matching.py"
    )
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    from types import SimpleNamespace

    path = (
        root
        / "benchmarks/spoc4-025/data"
        / ("matching-ii.txt" if preset == "study" else "matching-i.txt")
    )
    triples, milli, A, selected = mod.load(path)
    baseline = mod.candidate_check(triples, milli, mod.greedy(triples, milli))
    require(baseline["feasible"])
    prices = np.array(
        [
            np.max(milli[A.indices[A.indptr[i] : A.indptr[i + 1]]]) / 3
            for i in range(A.shape[0])
        ]
    )
    certificate, _ = mod.packing_upper_bound(A, milli, prices)
    records = []
    for seconds in [1.0, 3.0, 10.0] if preset == "study" else [1.0, 3.0]:
        options = SimpleNamespace(
            input=path,
            limit=0,
            encoding="milli",
            backend="native",
            mode="mip",
            policy="default",
            lp_algorithm="simplex",
            threads=1,
            seconds=seconds,
            start="greedy",
        )
        record = mod.solve(options)
        require(record["status"] != "exception", record.get("error"))
        candidate = record.get("candidate")
        require(candidate is not None, "the native solve returned no candidate")
        require(candidate["feasible"], "the native candidate is infeasible")
        require(
            abs(record["objective"] - candidate["objective"]) <= 1e-6,
            "native and independently checked objectives differ",
        )
        require(
            candidate["objective_milli"] <= certificate["objective_upper_milli"],
            "the candidate exceeds the independent upper bound",
        )
        record["improved_on_start"] = (
            candidate["objective_milli"] > baseline["objective_milli"]
        )
        records.append(record)
    return dict(
        full_table_edges=len(triples),
        baseline=baseline,
        records=records,
        independent_upper_bound=certificate,
        input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
        provenance="benchmarks/spoc4-025/DATA-SOURCES.md",
    )


CHAPTERS = {
    "LP1": allocation,
    "LP2": battery,
    "LP3": sudoku,
    "LP4": rostering,
    "LP5": transportation,
    "LP6": facility,
    "LP7": cutting,
    "LP8": stochastic,
    "LP9": spoc,
}


def main(chapter=None):
    p = argparse.ArgumentParser()
    p.add_argument("--chapter", choices=CHAPTERS, default=chapter)
    p.add_argument("--preset", choices=["smoke", "study"], default="smoke")
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    if not args.chapter:
        p.error("a chapter is required")
    if args.output.exists() and (
        not args.output.is_dir() or any(args.output.iterdir())
    ):
        raise FileExistsError("retain old results; choose a new output directory")
    args.output.mkdir(parents=True, exist_ok=True)
    start = time.time()
    try:
        result = CHAPTERS[args.chapter](args.preset)
        write(
            args.output / "summary.json",
            dict(
                chapter=args.chapter,
                preset=args.preset,
                seconds=time.time() - start,
                result=result,
                implementation="native Rust solver through Python API",
            ),
        )
        print(
            json.dumps(
                dict(chapter=args.chapter, status="passed", output=str(args.output))
            )
        )
    except Exception as e:
        write(
            args.output / "failure.json",
            dict(
                chapter=args.chapter,
                error=repr(e),
                traceback=traceback.format_exc(),
            ),
        )
        raise


if __name__ == "__main__":
    main()
