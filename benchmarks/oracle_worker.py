"""One backend and pass over pre-generated common inputs, external checks untimed."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import time
import lp_ip_craft as lp

p=argparse.ArgumentParser();p.add_argument('dataset');p.add_argument('backend',choices=['native','highs']);p.add_argument('output')
p.add_argument('--variant',choices=['default','dantzig','refactor1','refactor8','workers4','ft','eta','ordering','fixed128','fixed256'],default='default')
p.add_argument('--no-fixing',action='store_true');p.add_argument('--basis-update',choices=['auto','eta','forrest_tomlin']);p.add_argument('--suite',choices=['random','exact','components']);p.add_argument('--seed',type=int);a=p.parse_args()
data=Path(a.dataset).read_bytes();dataset=json.loads(data)
records=[]
for entry in dataset['records']:
    if a.suite and entry['suite']!=a.suite:continue
    if a.seed and entry['seed']!=a.seed:continue
    model=lp.Model(**entry['model'])
    mip=any(k!='continuous' for k in entry['model']['kinds'])
    if a.backend=='native':
        settings=lp.NativeOptions(reduced_cost_fixing=not a.no_fixing,basis_update=a.basis_update or ("forrest_tomlin" if a.variant=="ft" else "eta" if a.variant=="eta" else "auto"),
                                  factor_ordering="min_degree" if a.variant=="ordering" else "auto",
                                  adaptive_refactor=a.variant not in ["fixed128","fixed256"],time_limit_seconds=30.,component_threads=4 if a.variant=='workers4' else 1,
                                  steepest_edge=a.variant!='dantzig',refactor_interval=1 if a.variant=='refactor1' else 8 if a.variant=='refactor8' else 128 if a.variant=="fixed128" else 256 if a.variant=="fixed256" else lp.NativeOptions().refactor_interval,
                                  decomposition_min_variables=1 if entry['suite']=='components' else 32,
                                  component_parallel_min_entries=0 if entry['suite']=='components' else 2048)
    else:settings=lp.HighsOptions(time_limit_seconds=30.,node_limit=100_000 if mip else None,iteration_limit=None if mip else 1_000_000)
    if not records:
        lp.solve(model,backend=a.backend,backend_options=settings)  # one batch warmup
    started=time.perf_counter()
    try:
        r=lp.solve(model,backend=a.backend,backend_options=settings)
        seconds=time.perf_counter()-started
    except Exception as exc:
        records.append(dict(case_id=entry['case_id'],suite=entry['suite'],seed=entry['seed'],mip=mip,
                            status='exception',seconds=time.perf_counter()-started,problems=[repr(exc)]))
        continue
    expected=entry['expected_status'];reference=entry['reference']
    row=dict(case_id=entry['case_id'],suite=entry['suite'],seed=entry['seed'],mip=mip,n=model.n,m=model.m,
             status=r.status,expected_status=expected,seconds=seconds,nodes=r.nodes,iterations=r.iterations,
             backend_version=r.backend.version,best_bound=r.best_bound,reference=reference,
             phases={k:getattr(r.backend,k) for k in ['total_seconds','conversion_seconds','solve_seconds','check_seconds']})
    problems=[]
    if r.status!=expected:problems.append('status mismatch')
    if expected.startswith('scipy_'):
        row['oracle_unresolved']=True
    if r.status=='optimal':
        e=lp.evaluate(model,r.candidate)
        row['objective']=e.objective;row['primal']=e.primal_violation;row['integrality']=e.integrality_violation
        if not e.feasible_candidate:problems.append('independent candidate rejection')
        if reference is not None and abs(e.objective-reference)>1e-6*(1+abs(reference)):problems.append('reference objective mismatch')
        if r.best_bound is not None and reference is not None:
            gap=(reference-r.best_bound) if model.sense=='minimize' else (r.best_bound-reference)
            if gap < -1e-7*(1+abs(reference)):problems.append('bound on wrong side')
        if not mip:
            if r.duals is None:problems.append('missing LP duals')
            else:
                k=lp.lp_residuals(model,r.candidate,r.duals)
                row['residuals']=vars(k)
                scale=1+max(map(abs,entry['model']['objective']))
                if k.primal>1e-7 or k.stationarity>1e-7*scale or k.dual_sign>1e-7*scale or k.duality_gap>1e-7:problems.append('independent LP residual rejection')
    if problems:row['problems']=problems;row['message']=r.message
    records.append(row)
summary=[]
for suite in sorted({r['suite'] for r in records}):
    for mip in [False,True]:
        group=[r for r in records if r['suite']==suite and r['mip']==mip]
        if not group:continue
        summary.append(dict(suite=suite,mip=mip,cases=len(group),failures=sum(bool(r.get('problems')) for r in group),
                            seconds=sum(r['seconds'] for r in group),nodes=sum(r.get('nodes',0) for r in group),
                            iterations=sum(r.get('iterations',0) for r in group),statuses={s:sum(r['status']==s for r in group) for s in sorted({r['status'] for r in group})}))
result=dict(backend=a.backend,variant=a.variant,reduced_cost_fixing=not a.no_fixing,wrapper_version=lp.__version__,python=platform.python_version(),
            dataset_sha256=hashlib.sha256(data).hexdigest(),numpy=dataset['numpy'],summary=summary,records=records)
Path(a.output).write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(dict(backend=a.backend,variant=a.variant,cases=len(records),failures=sum(bool(r.get('problems')) for r in records),summary=summary)),flush=True)
