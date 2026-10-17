"""Independent-job throughput; construction and independent checks excluded."""
import argparse,json,time,statistics,platform,os,hashlib
from pathlib import Path
import lp_ip_craft as lp
p=argparse.ArgumentParser();p.add_argument('--output',required=True);p.add_argument('--size',type=int,default=256);p.add_argument('--jobs',type=int,default=32);a=p.parse_args()
n=a.size;rows=[{}for _ in range(n)];cost=[0.]*n
for j in range(n):
 for offset in [0,1,7,19,43,71,103,139,181]:
  i=(j+offset)%n;v=1.+((j*13+i*7)%5);rows[i][j]=v;cost[j]+=v
ptr=[0];ind=[];val=[];rhs=[]
for row in rows:
 for j,v in sorted(row.items()):ind.append(j);val.append(v)
 ptr.append(len(ind));rhs.append(sum(row.values()))
spec=dict(objective=cost,sense='maximize',lower=[0.]*n,upper=[2.]*n,row_lower=[-float('inf')]*n,row_upper=rhs,indptr=ptr,indices=ind,values=val)
m=lp.Model(**spec);settings=lp.NativeOptions(decompose=False,time_limit_seconds=30.)
out=dict(version=lp.__version__,protocol='32 independent jobs, one discarded warmup per worker count, 3 rotated measured rounds; one numerical thread/private state, shared host. Wall time includes pool and result serialization, excludes model construction and external validation.',platform=platform.platform(),affinity=sorted(os.sched_getaffinity(0)),model_sha256=hashlib.sha256(json.dumps(spec,sort_keys=True).encode()).hexdigest(),jobs=a.jobs,records=[])
for round in range(4):
 counts=[1,2,4,8,16];counts=counts[round:]+counts[:round]
 for workers in counts:
  t=time.perf_counter();r=lp.solve_batch([m]*a.jobs,workers=workers,backend_options=settings);elapsed=time.perf_counter()-t
  assert all(isinstance(s,lp.SolveResult) and s.status=='optimal' for s in r)
  for result in r:
   assert abs(result.evaluation.objective-sum(cost))<1e-6
   k=lp.lp_residuals(m,result.candidate,result.duals)
   assert k.primal<1e-7 and k.stationarity<1e-6 and k.dual_sign<1e-6 and k.duality_gap<1e-7
  if round:out['records'].append(dict(round=round,workers=workers,seconds=elapsed,iterations=[s.iterations for s in r]))
  print(round,workers,elapsed,flush=True)
out['summary']=[dict(workers=w,median_seconds=statistics.median(r['seconds'] for r in out['records'] if r['workers']==w)) for w in [1,2,4,8,16]]
base=out['summary'][0]['median_seconds']
for r in out['summary']:r['speedup']=base/r['median_seconds'];r['jobs_per_second']=a.jobs/r['median_seconds']
Path(a.output).write_text(json.dumps(out,indent=2)+'\n')
