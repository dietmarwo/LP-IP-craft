"""Real mixed-integer solve; use examples/demo.py for the retained dummy."""
from lp_ip_craft import Model, HighsOptions, solve
model=Model(objective=[2.,3.],lower=[0.,0.],upper=[3.,1.],kinds=["continuous","binary"],indptr=[0,2],indices=[0,1],values=[1.,2.],row_lower=[2.5],row_upper=[float("inf")])
result=solve(model,backend="highs",backend_options=HighsOptions(time_limit_seconds=10))
print(result)
assert result.status=="optimal" and abs(result.evaluation.objective-4)<1e-8
