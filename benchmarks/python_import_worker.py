"""Re-run the original four efficiency fixtures through both Python wrappers."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import time
import lp_ip_craft as lp

p=argparse.ArgumentParser();p.add_argument('fixtures');p.add_argument('backend');p.add_argument('case');p.add_argument('output');a=p.parse_args()
spec=importlib.util.spec_from_file_location('efficiency_fixtures',a.fixtures)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
name,data,expected=next(item for item in module.cases() if item[0]==a.case)
began=time.perf_counter();model=lp.Model(**data);construction=time.perf_counter()-began
settings=lp.NativeOptions(time_limit_seconds=30.,component_threads=1) if a.backend=='native' else lp.HighsOptions(time_limit_seconds=30.)
runs=[]
for run in range(2):
    began=time.perf_counter();r=lp.solve(model,backend=a.backend,backend_options=settings);wall=time.perf_counter()-began
    e=lp.evaluate(model,r.candidate) if r.candidate is not None else None
    accepted=r.status=='optimal' and e is not None and e.feasible_candidate
    reference=16980. if name=='transport-lp' else expected
    if accepted and reference is not None:accepted=abs(e.objective-reference)<=1e-7*(1+abs(reference))
    residuals=None
    if r.duals is not None:
        k=lp.lp_residuals(model,r.candidate,r.duals);residuals=vars(k)
        scale=1+max(map(abs,data['objective']))
        accepted=accepted and k.primal<=1e-7 and k.stationarity<=1e-7*scale and k.dual_sign<=1e-7*scale and k.duality_gap<=1e-7
    runs.append(dict(run=run,status=r.status,accepted=bool(accepted),wall_seconds=wall,
                     objective=e.objective if e else None,residuals=residuals,nodes=r.nodes,iterations=r.iterations,
                     backend_version=r.backend.version,phases={k:getattr(r.backend,k) for k in ['total_seconds','conversion_seconds','solve_seconds','check_seconds']}))
Path(a.output).write_text(json.dumps(dict(case=name,backend=a.backend,n=model.n,m=model.m,nnz=len(data['values']),
                                        model_sha256=hashlib.sha256(json.dumps(data,sort_keys=True).encode()).hexdigest(),
                                        construction_seconds=construction,python=platform.python_version(),
                                        wrapper_version=lp.__version__,runs=runs),indent=2)+'\n')
print(a.backend,name,runs[-1]['status'],runs[-1]['wall_seconds'],flush=True)
