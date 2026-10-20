"""Smoke-test an installed lp-ip-craft wheel or source distribution."""
import os
import tempfile
from importlib.metadata import version


def main() -> None:
    # Run outside the checkout, so the test cannot import the source tree.
    os.chdir(tempfile.mkdtemp(prefix="lp-ip-craft-smoke-"))
    import lp_ip_craft

    installed = version("lp-ip-craft")
    expected = os.environ.get("EXPECTED_VERSION")
    if expected is not None:
        assert installed == expected, (installed, expected)
    assert lp_ip_craft.__version__ == installed, (lp_ip_craft.__version__, installed)

    from lp_ip_craft import Model, NativeOptions, solve

    model = Model(objective=[3.0, 2.0], sense="maximize",
                  lower=[0.0, 0.0], upper=[10.0, 10.0],
                  row_lower=[float("-inf"), float("-inf")], row_upper=[4.0, 5.0],
                  indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[1.0, 1.0, 2.0, 1.0])
    result = solve(model, backend="native", backend_options=NativeOptions())
    assert result.status == "optimal", result.status
    assert abs(result.evaluation.objective - 9.0) < 1e-7, result.evaluation.objective
    summary = f"objective={result.evaluation.objective}"
    print(f"lp-ip-craft {installed}: {summary}")


if __name__ == "__main__":
    main()
