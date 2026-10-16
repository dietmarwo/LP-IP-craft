"""Exact rational oracles for the native bound, independent of HiGHS accuracy.

Fraction(float) is the exact stored binary64 value. Safety comparisons below
never use a floating-point tolerance. These tests also run in dummy-only builds.
"""
from concurrent.futures import ThreadPoolExecutor
from dataclasses import FrozenInstanceError
from fractions import Fraction as F
from itertools import combinations, product
import math
import pickle
import random
import sys

import pytest

from lp_ip_craft import BACKENDS, Model, SafeDualBound, safe_dual_bound, solve


def model(c, a, lo, hi, lower, upper, *, sense="minimize", offset=0.0, kinds=None):
    values, indices, indptr = [], [], [0]
    for row in a:
        for j, v in enumerate(row):
            if v != 0:
                values.append(v)
                indices.append(j)
        indptr.append(len(values))
    return Model(objective=c, objective_offset=offset, lower=lower, upper=upper,
                 row_lower=lo, row_upper=hi, indptr=indptr, indices=indices,
                 values=values, sense=sense, kinds=kinds)


def matrix(m):
    a = [[F(0) for _ in range(m.n)] for _ in range(m.m)]
    ptr, idx, val = m.indptr, m.indices, m.values
    for i in range(m.m):
        for k in range(ptr[i], ptr[i + 1]):
            a[i][idx[k]] = F(val[k])
    return a


def exact_bound(m, y):
    """Exact real Lagrangian bound; no interval or native rounding code reused."""
    sign = 1 if m.sense == "minimize" else -1
    y = [sign * F(v) for v in y]
    a = matrix(m)
    total = sign * F(m.objective_offset)
    for v, lo, hi in zip(y, m.row_lower, m.row_upper):
        if not v:
            continue
        side = lo if v > 0 else hi
        if not math.isfinite(side):
            return None
        total += v * F(side)
    for j, (c, lo, hi) in enumerate(zip(m.objective, m.lower, m.upper)):
        r = sign * F(c) - sum((a[i][j] * y[i] for i in range(m.m)), F(0))
        if not r:
            continue
        side = lo if r > 0 else hi
        if not math.isfinite(side):
            return None
        total += r * F(side)
    return sign * total


def assert_safe(m, y, *, require_finite=True):
    result = safe_dual_bound(m, y)
    if require_finite:
        assert result.status == "finite", (m, y, result)
    if result.status == "finite":
        exact = exact_bound(m, y)
        assert exact is not None and result.bound is not None
        assert math.isfinite(result.bound) and result.index is None
        if m.sense == "minimize":
            assert F(result.bound) <= exact, (m, y, result, exact)
        else:
            assert F(result.bound) >= exact, (m, y, result, exact)
    else:
        assert result.bound is None
    return result


def inequalities(m):
    """Exact A*x <= b representation, including the variable box."""
    a = matrix(m)
    rows = a + [[F(int(i == j)) for j in range(m.n)] for i in range(m.n)]
    out = []
    for row, lo, hi in zip(rows, m.row_lower + m.lower, m.row_upper + m.upper):
        if math.isfinite(hi):
            out.append((row, F(hi)))
        if math.isfinite(lo):
            out.append(([-v for v in row], -F(lo)))
    return out


def exact_vertices(m):
    """Enumerate all intersections of boundary lines of a bounded 2-D LP."""
    assert m.n == 2
    rows = inequalities(m)
    vertices = []
    for (a, b), (c, d) in combinations(rows, 2):
        det = a[0] * c[1] - a[1] * c[0]
        if det == 0:
            continue
        x = ((b * c[1] - a[1] * d) / det, (a[0] * d - b * c[0]) / det)
        if all(sum((v * z for v, z in zip(row, x)), F(0)) <= rhs for row, rhs in rows):
            vertices.append(x)
    assert vertices
    return vertices


def check_optimum(m, y, points):
    scores = [F(m.objective_offset) + sum((F(c) * x for c, x in zip(m.objective, p)), F(0))
              for p in points]
    optimum = min(scores) if m.sense == "minimize" else max(scores)
    result = assert_safe(m, y)
    if m.sense == "minimize":
        assert F(result.bound) <= optimum
    else:
        assert F(result.bound) >= optimum


@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_random_exact_lp_vertices(sense):
    rng = random.Random(812)
    for _ in range(80):
        c = [rng.randint(-9, 9) / 4 for _ in range(2)]
        a = [[rng.randint(-7, 7) / 4 for _ in range(2)] for _ in range(3)]
        m = model(c, a, [-2.0] * 3, [3.0] * 3, [-2.0, -2.0], [3.0, 3.0],
                  sense=sense, offset=rng.randint(-5, 5) / 8)
        y = [rng.uniform(-3, 3) for _ in range(3)]
        check_optimum(m, y, exact_vertices(m))


@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_random_exact_integer_optima(sense):
    rng = random.Random(1904)
    for _ in range(30):
        c = [rng.randint(-9, 9) / 8 for _ in range(4)]
        a = [[rng.randint(-3, 3) for _ in range(4)] for _ in range(2)]
        m = model(c, a, [-5.0] * 2, [6.0] * 2, [0.0] * 4, [3.0] * 4,
                  sense=sense, offset=0.1, kinds=["integer"] * 4)
        rows = inequalities(m)
        points = [p for p in product(range(4), repeat=4)
                  if all(sum(v * x for v, x in zip(row, p)) <= b for row, b in rows)]
        check_optimum(m, [rng.uniform(-2, 2) for _ in a], points)


@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_fraction_oracle_over_wide_exponents(sense):
    rng = random.Random(773)
    finite = 0
    for _ in range(500):
        def number():
            return math.ldexp(rng.uniform(-1, 1), rng.randint(-1070, 1000))
        c = [number() for _ in range(3)]
        a = [[number() for _ in range(3)] for _ in range(2)]
        box = [sorted((number(), number())) for _ in range(3)]
        rows = [sorted((number(), number())) for _ in range(2)]
        m = model(c, a, [v[0] for v in rows], [v[1] for v in rows],
                  [v[0] for v in box], [v[1] for v in box], sense=sense, offset=number())
        result = assert_safe(m, [number(), number()], require_finite=False)
        assert result.status in ("finite", "arithmetic_overflow")
        finite += result.status == "finite"
    assert finite >= 50  # Ensure overflow handling has not made the test vacuous.


@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_adversarial_cancellation_subnormals_and_offsets(sense):
    tiny = math.ulp(0.0)
    fixtures = [
        # Massive cancellation in A^T*y must not hide the small exact residual.
        ([1.0], [[1e16], [1.0], [-1e16]], [1.0, 1.0, 1.0], -3.0, 7.0, 0.1),
        # A nonzero exact product rounds to zero in ordinary arithmetic.
        ([0.0], [[tiny]], [0.5], -1.0, 1.0, tiny),
        # Objective/box products and offset cancellation.
        ([1e300], [], [], -1e-300, 2e-300, -0.1),
        ([tiny], [], [], 0.5, 1.5, 0.0),
        # Signed zero and wholly fixed variables.
        ([-0.0], [[-0.0]], [-0.0], -0.0, 0.0, -0.0),
    ]
    for c, a, y, lo, hi, offset in fixtures:
        m = model(c, a, [-10.0] * len(a), [10.0] * len(a), [lo], [hi],
                  sense=sense, offset=offset)
        assert_safe(m, y)


@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_infinite_boxes_rows_and_exact_zero(sense):
    s = 1 if sense == "minimize" else -1
    # Exact cancellation uses multiplication by 1, so even a free variable works.
    m = model([s * 2.0], [[1.0]], [3.0], [3.0], [-math.inf], [math.inf], sense=sense)
    assert_safe(m, [s * 2.0])
    failed = safe_dual_bound(m, [s * (2.0 + 1e-12)])
    assert failed == SafeDualBound(None, "unbounded_variable", 0)
    # One-sided variable bounds are sufficient when the residual sign is safe.
    m = model([s * 2.0], [], [], [], [3.0], [math.inf], sense=sense)
    assert_safe(m, [])
    m = model([-s * 2.0], [], [], [], [-math.inf], [-3.0], sense=sense)
    assert_safe(m, [])
    # An interval crossing zero cannot be treated as zero on an unbounded box.
    m = model([s * 0.25], [[0.5]], [1.0], [1.0], [-math.inf], [math.inf], sense=sense)
    assert exact_bound(m, [s * 0.5]) is not None
    assert safe_dual_bound(m, [s * 0.5]).status == "unbounded_variable"
    # A free row with zero multiplier is skipped; nonzero is explicitly unavailable.
    m = model([1.0], [[1.0]], [-math.inf], [math.inf], [0.0], [1.0], sense=sense)
    assert_safe(m, [-0.0])
    assert safe_dual_bound(m, [s * 1.0]) == SafeDualBound(None, "unbounded_row", 0)


def test_overflow_is_typed_and_never_a_finite_bound():
    m = model([1.0], [[2.0]], [2.0], [2.0], [0.0], [1.0])
    assert safe_dual_bound(m, [sys.float_info.max]) == SafeDualBound(None, "arithmetic_overflow", None)
    # Summation overflow, including two exactly represented individual products.
    m = model([sys.float_info.max] * 2, [], [], [], [1.0] * 2, [1.0] * 2)
    assert safe_dual_bound(m, []).status == "arithmetic_overflow"


@pytest.mark.parametrize("y", [[], [0.0, 0.0], [math.nan], [math.inf], [-math.inf]])
def test_invalid_multipliers(y):
    m = model([1.0], [[1.0]], [0.0], [1.0], [0.0], [1.0])
    with pytest.raises(ValueError, match="row duals|row_duals"):
        safe_dual_bound(m, y)


def test_iterables_numpy_frozen_pickle_and_threads():
    import numpy as np
    m = model([2.0, 3.0], [[1.0, 1.0]], [1.0], [1.0], [0.0, 0.0], [1.0, 1.0])
    expected = safe_dual_bound(m, [2.0])
    assert safe_dual_bound(m, (x for x in [2.0])) == expected
    assert safe_dual_bound(m, np.array([2.0])) == expected
    assert pickle.loads(pickle.dumps(expected)) == expected
    with pytest.raises(FrozenInstanceError):
        expected.bound = 42.0
    with ThreadPoolExecutor(max_workers=4) as pool:
        assert list(pool.map(lambda _: safe_dual_bound(m, [2.0]), range(32))) == [expected] * 32


@pytest.mark.skipif("highs" not in BACKENDS, reason="native HiGHS is not compiled in")
@pytest.mark.parametrize("sense", ["minimize", "maximize"])
def test_highs_duals_and_perturbations_against_exact_vertices(sense):
    rng = random.Random(117)
    for _ in range(20):
        m = model([rng.uniform(-4, 4) for _ in range(2)],
                  [[rng.uniform(-2, 2) for _ in range(2)] for _ in range(3)],
                  [-2.0] * 3, [3.0] * 3, [-2.0] * 2, [3.0] * 2, sense=sense, offset=0.1)
        result = solve(m, backend="highs")
        assert result.status == "optimal" and result.duals is not None
        # Separate API: the adapter's best_bound contract has not changed.
        assert result.best_bound is None and result.gap is None
        points = exact_vertices(m)
        for perturbation in [0.0, -1e-7, 1e-7]:
            y = [v + perturbation for v in result.duals.row_duals]
            check_optimum(m, y, points)
