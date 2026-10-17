"""Separate unresolved SciPy oracle outcomes from backend disagreements.

These are numerical checks, not exact infeasibility/unboundedness certificates.
They never alter the timed inputs, raw outcomes or timings.
"""
import json
from pathlib import Path
import numpy as np
from scipy.optimize import Bounds, LinearConstraint, linprog, milp
import lp_ip_craft as lp

base=Path(__file__).resolve().parent.parent/"docs/performance-ten/data"
dataset=json.loads((base/'oracle-inputs.json').read_text())
inputs={r['case_id']:r for r in dataset['records']}
native=json.loads((base/'python-oracles/0-native.json').read_text())
highs=json.loads((base/'python-oracles/0-highs.json').read_text())
interesting={r['case_id'] for raw in [native,highs] for r in raw['records'] if r.get('problems') and r['status']!='exception'}
records=[]
for case_id in sorted(interesting):
    item=inputs[case_id];d=item['model'];n=len(d['objective']);m=len(d['row_lower'])
    matrix=np.zeros((m,n))
    for i in range(m):
        for k in range(d['indptr'][i],d['indptr'][i+1]):matrix[i,d['indices'][k]]=d['values'][k]
    lower=np.array(d['lower']);upper=np.array(d['upper']);rl=np.array(d['row_lower']);ru=np.array(d['row_upper'])
    c=np.array(d['objective'])*(1 if d['sense']=='minimize' else -1)
    integer=np.array([k!='continuous' for k in d['kinds']])
    row=dict(case_id=case_id,original_oracle_status=item['expected_status'])
    if integer.any():
        result=milp(c,integrality=integer.astype(int),bounds=Bounds(lower,upper),constraints=LinearConstraint(matrix,rl,ru),options={'presolve':False})
        row.update(scipy_no_presolve_status=int(result.status),scipy_no_presolve_message=result.message)
        if result.status==0:
            model=lp.Model(**d);evaluation=lp.evaluate(model,result.x)
            row.update(adjudicated_status='optimal',reference=float(result.fun)*(1 if d['sense']=='minimize' else -1)+d['objective_offset'],
                       candidate_feasible=evaluation.feasible_candidate,primal=evaluation.primal_violation,integrality=evaluation.integrality_violation)
    else:
        # The added finite box is used only to find an independently checked
        # feasible point of the original LP; a separate homogeneous ray proves
        # numerical unboundedness without imposing that box on the ray.
        rows_upper=np.isfinite(ru);rows_lower=np.isfinite(rl)
        aub=np.vstack([matrix[rows_upper],-matrix[rows_lower]])
        bub=np.r_[ru[rows_upper],-rl[rows_lower]]
        bounds=list(zip(np.where(np.isfinite(lower),lower,-10.),np.where(np.isfinite(upper),upper,10.)))
        feasible=linprog(np.zeros(n),A_ub=aub,b_ub=bub,bounds=bounds,method='highs',options={'presolve':False,'primal_feasibility_tolerance':1e-9,'dual_feasibility_tolerance':1e-9})
        dbounds=[]
        for lo,hi in zip(lower,upper):
            dbounds.append((0.,0.) if np.isfinite(lo) and np.isfinite(hi) else
                           (0.,1.) if np.isfinite(lo) else (-1.,0.) if np.isfinite(hi) else (-1.,1.))
        ray=linprog(c,A_ub=aub,b_ub=np.zeros(len(bub)),bounds=dbounds,method='highs',options={'presolve':False})
        row.update(boxed_feasibility_status=int(feasible.status),ray_status=int(ray.status))
        if feasible.status==0 and ray.status==0:
            model=lp.Model(**d);evaluation=lp.evaluate(model,feasible.x)
            violation=max(0.,float(np.max(aub@ray.x,initial=0.)))
            direction_lower=np.array([x[0] for x in dbounds]);direction_upper=np.array([x[1] for x in dbounds])
            bound_violation=max(0.,float(np.max(direction_lower-ray.x,initial=0.)),float(np.max(ray.x-direction_upper,initial=0.)))
            improvement=float(c@ray.x)
            row.update(feasible=evaluation.feasible_candidate,primal=evaluation.primal_violation,
                       ray_violation=violation,ray_bound_violation=bound_violation,ray_oriented_cost=improvement)
            if evaluation.feasible_candidate and violation<=1e-7 and bound_violation<=1e-7 and improvement < -1e-7:
                row['adjudicated_status']='unbounded'
    records.append(row)
(base/'oracle-adjudication.json').write_text(json.dumps(dict(method='separate untimed SciPy no-presolve MIP; independent original-model point and homogeneous recession-ray checks for LPs. Numerical evidence, not exact certification.',records=records),indent=2)+'\n')
print(json.dumps(records,indent=2))
