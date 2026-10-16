"""Independent bounds from HiGHS LP duals, including unsafe naive dual sums.

Run: python examples/safe_bound.py. This is also runnable without HiGHS, using
the analytic duals of this tiny model. It makes no pruning decision.
"""
from lp_ip_craft import BACKENDS, Model, safe_dual_bound, solve


def main() -> None:
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


if __name__ == "__main__":
    main()
