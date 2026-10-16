"""Independent objectives for 180 separable LP/IP/MIP models."""
import json
import numpy as np
from scipy.optimize import linprog
from lp_ip_craft import Model,NativeOptions,solve,lp_residuals
from validate_native import exact_box_ip,exact_mixed

def main():
    import sys
    workers=int(sys.argv[1]) if len(sys.argv)>1 else 1
    rng=np.random.default_rng(946237);records=[]
    for domain in ["lp","ip","mip"]:
        for case in range(60):
            matrices=[];costs=[];lowers=[];uppers=[];reference=1.25
            for _ in range(2):
                a=rng.integers(-6,7,(4,5)).astype(float);c=rng.integers(-8,9,5).astype(float);w=rng.integers(0,4,5)
                activity=a@w;lo=activity-rng.integers(0,5,4);up=activity+rng.integers(0,5,4)
                if domain=="ip":value=exact_box_ip(c,a,lo,up)
                elif domain=="mip":value=exact_mixed(c,a,lo,up)
                else:
                    result=linprog(c,A_ub=np.vstack([a,-a]),b_ub=np.r_[up,-lo],bounds=[(0,3)]*5,method="highs");assert result.success;value=result.fun
                reference+=float(value);matrices.append(a);costs.extend(c);lowers.extend(lo);uppers.extend(up)
            matrix=np.zeros((8,10));matrix[:4,:5]=matrices[0];matrix[4:,5:]=matrices[1]
            kinds=(["continuous"]*5 if domain=="lp" else ["integer"]*5 if domain=="ip" else ["integer","continuous","integer","continuous","integer"])*2
            indptr=[0];indices=[];values=[]
            for row in matrix:
                for j,v in enumerate(row):
                    if v:indices.append(j);values.append(v)
                indptr.append(len(indices))
            model=Model(objective=costs,objective_offset=1.25,lower=[0.]*10,upper=[3.]*10,kinds=kinds,row_lower=lowers,row_upper=uppers,indptr=indptr,indices=indices,values=values)
            r=solve(model,backend="native",backend_options=NativeOptions(decomposition_min_variables=1,component_threads=workers,component_parallel_min_entries=0))
            assert r.status=="optimal" and r.evaluation.feasible_candidate,(domain,case,r)
            assert abs(r.evaluation.objective-reference)<1e-6,(domain,case,r,reference)
            if r.best_bound is not None:assert r.best_bound<=reference+1e-7
            if domain=="lp":assert lp_residuals(model,r.candidate,r.duals).duality_gap<1e-7
            records.append({"domain":domain,"case":case,"objective":r.evaluation.objective,"reference":reference,"components":int(r.backend.effective_options["diagnostics.component_count"])})
    print(json.dumps({"workers_requested":workers,"cases":len(records),"passed":len(records),"records":records},indent=2))

if __name__=="__main__":main()
