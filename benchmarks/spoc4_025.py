"""One isolated full matching run, or a serial suite of fresh processes."""
import argparse,hashlib,json,os,platform,subprocess,sys,time
from pathlib import Path
import numpy as np
import lp_ip_craft as lp
from spoc4_matching import load,greedy,candidate_check
ROOT=Path(__file__).resolve().parent/'spoc4-025'
p=argparse.ArgumentParser();p.add_argument('--instance',choices=['i','ii'],default='i');p.add_argument('--policy',choices=['default','root','lns1','lns4','cuts4','tree4'],default='root');p.add_argument('--seconds',type=float,default=10);p.add_argument('--suite',action='store_true');p.add_argument('--output',type=Path);a=p.parse_args()
if a.suite:
    for policy in ['default','root','lns1','lns4','cuts4','tree4']:
        for name in ['i','ii']:
            out=ROOT/'results'/f'{name}-{policy}.json';out.parent.mkdir(exist_ok=True)
            seconds=30 if policy=='default'else a.seconds
            print('START',name,policy,seconds,flush=True)
            with out.with_suffix('.log').open('w')as log:
                r=subprocess.run([sys.executable,__file__,'--instance',name,'--policy',policy,'--seconds',str(seconds),'--output',str(out)],stdout=log,stderr=subprocess.STDOUT,timeout=seconds+30)
            print('DONE',name,policy,'exit',r.returncode,flush=True)
            if r.returncode:raise SystemExit(r.returncode)
    raise SystemExit()
source=ROOT/'data'/f'matching-{a.instance}.txt';t=time.perf_counter();triples,w,A,selected=load(source);seed=greedy(triples,w);n=A.shape[1];m=A.shape[0]
model=lp.Model(objective=w.astype(float),lower=np.zeros(n),upper=np.ones(n),kinds=['binary']*n,sense='maximize',row_lower=np.full(m,-np.inf),row_upper=np.ones(m),indptr=A.indptr,indices=A.indices,values=A.data)
controls=dict(time_limit_seconds=a.seconds,node_limit=100000,iteration_limit=1000000,component_threads=1,mip_diagnostics=True,mip_trace_limit=32)
if a.policy!='default':controls.update(packing_root=True,scalable_branching=True)
if a.policy in ['lns1','lns4','cuts4','tree4']:controls.update(packing_neighborhoods=64,packing_neighborhood_size=192,packing_workers=1 if a.policy=='lns1'else 4)
if a.policy in ['cuts4','tree4']:controls.update(packing_cuts=True)
if a.policy=='tree4':controls.update(tree_threads=4,tree_split_depth=2)
build=time.perf_counter()-t;t=time.perf_counter()
r=lp.solve(model,backend='native',backend_options=lp.NativeOptions(**controls),start=seed if a.policy=='default'else None)
elapsed=time.perf_counter()-t
check=candidate_check(triples,w,r.candidate)
if check:assert check['feasible']
if check and r.best_bound is not None:assert r.best_bound>=check['objective_milli']-1e-6
record=dict(instance=a.instance,policy=a.policy,version=lp.__version__,baseline_commit='9bf392c1f0f1cbf3242a31d0b7d8edd0b3352149',input_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),n=n,m=m,nnz=A.nnz,controls=controls,status=r.status,seconds=elapsed,construction_seconds=build,nodes=r.nodes,pivots=r.iterations,objective=r.evaluation.objective/1000 if r.evaluation else None,bound=r.best_bound/1000 if r.best_bound is not None else None,greedy=candidate_check(triples,w,seed),candidate=check,metadata=r.backend.effective_options,platform=platform.platform(),cpu_affinity=len(os.sched_getaffinity(0)))
if a.output:
 a.output.parent.mkdir(exist_ok=True,parents=True);a.output.write_text(json.dumps(record,indent=2,allow_nan=False)+'\n')
 if r.candidate is not None:np.save(a.output.with_suffix('.solution.npy'),np.rint(r.candidate).astype(np.uint8))
print(json.dumps({k:v for k,v in record.items()if k!='metadata'},allow_nan=False),flush=True)
