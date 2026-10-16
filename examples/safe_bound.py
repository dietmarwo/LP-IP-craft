"""Independent bounds from HiGHS LP duals, including unsafe naive dual sums.

Run: python examples/safe_bound.py. This is also runnable without HiGHS, using
the analytic duals of these tiny models. It makes no pruning decision.

Part 1 corrects perturbed duals on a bounded box. Part 2 shows why columns
with an infinite bound need duals requested with a margin (``margin_duals``).
"""
import math

from lp_ip_craft import BACKENDS, Model, safe_dual_bound, solve


def margin_duals(model: Model, margin: float = 1e-6) -> "list[float] | None":
    """Row duals of the LP relaxation, solved with the cost of every one-sided
    column shifted by ``margin * max(1, |c_j|)`` in the safe direction.

    A column with ``x_j >= l_j`` and no upper bound contributes a finite term
    only if its exact residual ``c_j - a_j^T y`` is nonnegative (minimizing).
    Solver duals meet that only to the dual feasibility tolerance (1e-7 in
    HiGHS), so about half the basic columns get a negative exact residual.
    Lowering their costs by more than that tolerance makes the residuals of
    the original costs positive, with room for rounding. Any multipliers give
    a valid bound for the original model; this only makes a finite one
    likely, weakened by about ``margin * sum |c_j x_j|``. Free columns need an
    exactly zero residual; a shift cannot help them, a finite bound can.
    """
    sign = 1.0 if model.sense == "minimize" else -1.0
    objective = []
    for c, lower, upper in zip(model.objective, model.lower, model.upper):
        shift = margin * max(1.0, abs(c))
        if upper == math.inf and lower > -math.inf:
            c -= sign * shift
        elif lower == -math.inf and upper < math.inf:
            c += sign * shift
        objective.append(c)
    relaxation = Model(
        objective=objective, objective_offset=model.objective_offset, sense=model.sense,
        lower=model.lower, upper=model.upper,  # kinds default to continuous
        row_lower=model.row_lower, row_upper=model.row_upper,
        indptr=model.indptr, indices=model.indices, values=model.values,
    )
    result = solve(relaxation, backend="highs")
    if result.status != "optimal" or result.duals is None:
        return None
    return result.duals.row_duals


def bounded_box() -> None:
    for sense, optimal_y, optimum in [("minimize", 2.0, 7.0), ("maximize", 3.0, 8.0)]:
        model = Model(
            objective=[2.0, 3.0], objective_offset=5.0, sense=sense,
            lower=[0.0, 0.0], upper=[1.0, 1.0],
            indptr=[0, 2], indices=[0, 1], values=[1.0, 1.0],
            row_lower=[1.0], row_upper=[1.0],
        )
        y = [optimal_y]
        source = "analytic"
        if "highs" in BACKENDS:
            result = solve(model, backend="highs")
            assert result.status == "optimal" and result.duals is not None
            y = result.duals.row_duals
            source = "HiGHS"
        certified = safe_dual_bound(model, y)
        # HiGHS may choose another optimal dual because bound duals also enter
        # its certificate. Use the stated analytic dual for a reproducible
        # near-optimum counterexample; the first bound still uses HiGHS duals.
        perturbed_y = optimal_y + (1e-7 if sense == "minimize" else -1e-7)
        corrected = safe_dual_bound(model, [perturbed_y])
        assert certified.bound is not None and corrected.bound is not None
        if sense == "minimize":
            assert certified.bound <= optimum and corrected.bound <= optimum
            assert 5.0 + perturbed_y > optimum  # A naive dual sum is unsafe.
        else:
            assert certified.bound >= optimum and corrected.bound >= optimum
            assert 5.0 + perturbed_y < optimum
        print(f"{sense}: source={source}, exact optimum={optimum}, "
              f"bound={certified.bound:.17g}, naive={5 + perturbed_y:.17g}, "
              f"corrected={corrected.bound:.17g}")


def one_sided_box() -> None:
    # minimize 0.3 x0 + 0.7 x1  subject to  0.2 x0 + 0.3 x1 >= 2,  x >= 0.
    # The stored binary64 coefficients are not 0.2 and 0.3, so the textbook
    # dual 1.5 leaves the exact residual 0.3 - 0.2 * 1.5 = -2.8e-17 < 0 on a
    # column without an upper bound: its exact Lagrangian bound is -inf.
    # The exact optimum of the stored model is 2.99999999999999961...
    model = Model(
        objective=[0.3, 0.7], lower=[0.0, 0.0], upper=[math.inf, math.inf],
        indptr=[0, 2], indices=[0, 1], values=[0.2, 0.3],
        row_lower=[2.0], row_upper=[math.inf],
    )
    textbook = safe_dual_bound(model, [1.5])
    assert textbook.status == "unbounded_variable" and textbook.index == 0
    if "highs" in BACKENDS:
        plain = safe_dual_bound(model, solve(model, backend="highs").duals.row_duals)
        y = margin_duals(model)
        assert y is not None
        source = f"HiGHS duals: {plain.status}; margin duals"
    else:
        y = [1.5 * (1 - 1e-6)]  # what margin_duals returns, up to HiGHS's rounding
        source = "analytic margin dual"
    with_margin = safe_dual_bound(model, y)
    assert with_margin.bound is not None and with_margin.bound <= 2.9999999999999996
    print(f"one-sided box: textbook dual 1.5: {textbook.status}; {source}: "
          f"bound={with_margin.bound:.17g}")


if __name__ == "__main__":
    bounded_box()
    one_sided_box()
