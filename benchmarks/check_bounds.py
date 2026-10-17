"""Same 400 bound-availability models and exact Lagrangian check per backend."""
import argparse
import importlib
import json
from pathlib import Path
import sys
import lp_ip_craft as lp

p=argparse.ArgumentParser();p.add_argument('repo');p.add_argument('backend');p.add_argument('output');a=p.parse_args()
sys.path.insert(0,str(Path(a.repo)/'examples'))
# Only dispatch and explicit backend settings change. No model/dual/bound
# formulas, arithmetic or acceptance checks in the original example change.
original=lp.solve
def dispatch(model,**kwargs):
    settings=lp.NativeOptions(time_limit_seconds=30.,component_threads=1) if a.backend=='native' else lp.HighsOptions(time_limit_seconds=30.)
    kwargs.update(backend=a.backend,backend_options=settings)
    return original(model,**kwargs)
lp.solve=dispatch
module=importlib.import_module('bound_availability')
rows,unsafe=module.run(100,1)
result=dict(backend=a.backend,seed=1,models=400,rows=rows,unsafe_finite_bounds=len(unsafe))
Path(a.output).write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
