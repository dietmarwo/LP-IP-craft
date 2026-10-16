"""Real solves with the HiGHS backend; examples/demo.py shows the dummy.

A mixed binary/continuous model and the production-planning LP of the LP/IP
tutorial (Example 1), with an independent re-check of the LP solution.
"""

import math

from lp_ip_craft import HighsOptions, Model, evaluate, lp_residuals, solve

# minimize 2x + 3y  subject to  x + 2y >= 2.5,  0 <= x <= 3,  y binary
mixed = Model(
    objective=[2.0, 3.0],
    lower=[0.0, 0.0],
    upper=[3.0, 1.0],
    kinds=["continuous", "binary"],
    indptr=[0, 2],
    indices=[0, 1],
    values=[1.0, 2.0],
    row_lower=[2.5],
    row_upper=[math.inf],
)
result = solve(mixed, backend="highs", backend_options=HighsOptions(time_limit_seconds=10))
print(result.status, result.candidate, "bound", result.best_bound, result.gap)
print("backend:", result.backend.name, result.backend.version, result.backend.raw_status)
assert result.status == "optimal" and abs(result.evaluation.objective - 4.0) < 1e-8

production = Model(
    sense="maximize",
    objective=[45.0, 80.0, 110.0],  # chair, table, desk
    lower=[0.0] * 3,
    upper=[math.inf] * 3,
    indptr=[0, 3, 6, 9],
    indices=[0, 1, 2, 0, 1, 2, 0, 1, 2],
    values=[5.0, 20.0, 25.0, 10.0, 15.0, 20.0, 1.0, 3.0, 5.0],
    row_lower=[-math.inf] * 3,
    row_upper=[400.0, 450.0, 60.0],  # wood, labor, finishing
)
lp = solve(production, backend="highs")
print(lp.status, lp.candidate, "duals", lp.duals)
# The same checks work on any solver's solution:
print("evaluate:", evaluate(production, lp.candidate))
print("residuals:", lp_residuals(production, lp.candidate, lp.duals))
assert lp.status == "optimal" and abs(lp.evaluation.objective - 2150.0) < 1e-8
