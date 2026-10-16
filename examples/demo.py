"""End-to-end use of the Python API with the dummy backend.

Small fixtures, not the GTOC benchmark: the production-planning LP of the
LP/IP tutorial (Example 1) and a two-variable binary assignment row.
"""

import math

from lp_ip_craft import Cancellation, Model, Options, evaluate, solve

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
print(production)

# Check a solution reported by another solver (Gurobi and HiGHS both report this one).
print("external optimum:", evaluate(production, [30.0, 10.0, 0.0]))

result = solve(
    production,
    start=[30.0, 10.0, 0.0],
    options=Options(feasibility_tolerance=1e-9),
    cancellation=Cancellation(),
)
print(result.status, "-", result.message)
print("evaluation:", result.evaluation)
assert result.status == "stub" and result.best_bound is None and result.gap is None

assignment = Model(
    objective=[2.0, 3.0],
    lower=[0.0, 0.0],
    upper=[1.0, 1.0],
    kinds=["binary", "binary"],
    indptr=[0, 2],
    indices=[0, 1],
    values=[1.0, 1.0],
    row_lower=[1.0],
    row_upper=[1.0],
)
fractional = solve(assignment, start=[0.5, 0.5])
print("fractional start, unrounded:", fractional.candidate, fractional.evaluation)
assert not fractional.evaluation.feasible_candidate
