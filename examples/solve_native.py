"""A native Rust LP/MIP solve through the Python wrapper."""
from lp_ip_craft import Model, NativeOptions, solve

for integer in [False, True]:
    m = Model(objective=[7., 11., 5., 6.], sense="maximize",
              lower=[0.]*4, upper=[1.]*4,
              kinds=["binary" if integer else "continuous"]*4,
              row_lower=[float("-inf")], row_upper=[8.5],
              indptr=[0,4], indices=[0,1,2,3], values=[3.,5.,2.,4.])
    r = solve(m, backend="native", backend_options=NativeOptions())
    assert r.status == "optimal" and r.evaluation.feasible_candidate
    print("MIP" if integer else "LP", r.evaluation.objective, r.candidate,
          "nodes", r.nodes, "pivots", r.iterations, "bound", r.best_bound)
