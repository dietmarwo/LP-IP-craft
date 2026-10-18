"""One baseline/HiGHS reference process. No solver inputs are rewritten."""
import sys,json,time,os
from pathlib import Path
import numpy as np
import lp_ip_craft as lp
payload=json.loads(Path(sys.argv[1]).read_text());spec=payload['model'];seconds=float(sys.argv[2]);solver=sys.argv[3]
clean=dict(spec)
for key in ['lower','upper','row_lower','row_upper']:
 clean[key]=[(-float('inf')if key in ['lower','row_lower']else float('inf'))if v is None else v for v in spec[key]]
m=lp.Model(**clean)
began=time.perf_counter()
if solver=='baseline':
 try:
  r=lp.solve(m,backend='native',backend_options=lp.NativeOptions(time_limit_seconds=seconds,node_limit=int(os.environ.get("LPIP_COMPARE_NODES","100000")),iteration_limit=int(os.environ.get("LPIP_COMPARE_PIVOTS","1000000")),decompose=False,mip_diagnostics=True,mip_trace_limit=8))
  out=dict(status=r.status,objective=r.evaluation.objective if r.evaluation else None,bound=r.best_bound,candidate=list(r.candidate)if r.candidate is not None else None,nodes=r.nodes,pivots=r.iterations,metadata=r.backend.effective_options)
 except Exception as e:out=dict(status='error',error=str(e))
else:
 import highspy
 h=highspy.Highs();h.setOptionValue('output_flag',False);h.setOptionValue('threads',1);h.setOptionValue('time_limit',seconds);h.setOptionValue('mip_rel_gap',0.);h.setOptionValue('mip_abs_gap',0.)
 c=np.array(spec['objective']);lo=np.array([-highspy.kHighsInf if v is None else v for v in spec['lower']]);up=np.array([highspy.kHighsInf if v is None else v for v in spec['upper']]);rl=np.array([-highspy.kHighsInf if v is None else v for v in spec['row_lower']]);ru=np.array([highspy.kHighsInf if v is None else v for v in spec['row_upper']]);n=len(c)
 h.addVars(n,lo,up);h.changeColsCost(n,np.arange(n,dtype=np.int32),c);h.addRows(len(rl),rl,ru,len(spec['values']),np.array(spec['indptr'],dtype=np.int32),np.array(spec['indices'],dtype=np.int32),np.array(spec['values']))
 if spec['sense']=='maximize':h.setMaximize()
 kinds=np.array([0 if k=='continuous'else 1 for k in spec['kinds']],dtype=np.uint8)
 if any(kinds):h.changeColsIntegrality(n,np.arange(n,dtype=np.int32),kinds)
 if 'LPIP_COMPARE_NODES' in os.environ:h.setOptionValue('mip_max_nodes',int(os.environ['LPIP_COMPARE_NODES']))
 h.run();info=h.getInfo();x=h.getSolution();bound=info.mip_dual_bound
 if not any(kinds):
  bound=lp.safe_dual_bound(m,x.row_dual).bound if x.dual_valid else None
  if bound is None:bound=float('nan')
 # safe_dual_bound already includes the original offset; HiGHS' MIP field does not.
 reported_bound=bound+(spec.get('objective_offset',0.)if any(kinds)else 0.)if np.isfinite(bound)else None
 out=dict(status=h.modelStatusToString(h.getModelStatus()),objective=h.getObjectiveValue()+spec.get('objective_offset',0.)if x.value_valid else None,bound=reported_bound,candidate=x.col_value if x.value_valid else None,nodes=info.mip_node_count,pivots=info.simplex_iteration_count,version=h.version())
print(json.dumps(dict(solver=solver,policy='default',seconds=time.perf_counter()-began,result=out),allow_nan=False))
