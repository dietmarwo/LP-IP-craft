"""Optional independent SciPy LP/MIP oracle; not a runtime dependency."""
import json
import itertools
from fractions import Fraction
import numpy as np
from scipy.optimize import linprog
from lp_ip_craft import Model, NativeOptions, solve, lp_residuals

def exact_box_ip(c,a,rl,ru):
    return min(sum(int(cj)*xj for cj,xj in zip(c,x)) for x in itertools.product(range(4),repeat=5)
               if all(int(l)<=sum(int(v)*t for v,t in zip(row,x))<=int(u) for row,l,u in zip(a,rl,ru)))

def exact_mixed(c,a,rl,ru):
    # Three integer columns in [0,3], two continuous columns. Enumerate the
    # integer assignments and all continuous vertices using integer numerators.
    rows=[(int(row[1]),int(row[3])) for row in a]
    coeff=rows+[(-u,-v) for u,v in rows]+[(1,0),(-1,0),(0,1),(0,-1)]
    pairs=[(i,j,ai*bj-aj*bi) for i,(ai,bi) in enumerate(coeff) for j,(aj,bj) in enumerate(coeff[:i]) if ai*bj-aj*bi]
    best=None
    for x0,x2,x4 in itertools.product(range(4),repeat=3):
        shift=[int(row[0])*x0+int(row[2])*x2+int(row[4])*x4 for row in a]
        rhs=[int(u)-v for u,v in zip(ru,shift)]+[v-int(l) for v,l in zip(shift,rl)]+[3,0,3,0]
        constant=int(c[0])*x0+int(c[2])*x2+int(c[4])*x4
        for i,j,det in pairs:
            ai,bi=coeff[i];aj,bj=coeff[j]
            nx=rhs[i]*bj-rhs[j]*bi;ny=ai*rhs[j]-aj*rhs[i];den=det
            if den<0:nx,ny,den=-nx,-ny,-den
            if all(u*nx+v*ny<=b*den for (u,v),b in zip(coeff,rhs)):
                value=constant*den+int(c[1])*nx+int(c[3])*ny
                if best is None or value*best[1]<best[0]*den:best=(value,den)
    assert best is not None
    return Fraction(*best)

def main():
    import argparse
    parser=argparse.ArgumentParser()
    parser.add_argument("--dantzig", action="store_true", help="largest-infeasibility pricing")
    parser.add_argument("--refactor-interval", type=int, default=64)
    parser.add_argument("--seed", type=int, default=59103)
    parser.add_argument("--cases", type=int, default=200)
    args=parser.parse_args()
    rng=np.random.default_rng(args.seed);records=[]
    for domain in ["lp","ip","mip"]:
        integer=domain!="lp"
        integrality=np.array([1,1,1,1,1] if domain=="ip" else [1,0,1,0,1])
        for case in range(args.cases):
            n=5;rows=4;a=rng.integers(-6,7,(rows,n)).astype(float);c=rng.integers(-8,9,n).astype(float)
            lower=np.zeros(n);upper=np.full(n,3.);witness=rng.integers(0,4,n).astype(float)
            activity=a@witness;rl=activity-rng.integers(0,5,rows);ru=activity+rng.integers(0,5,rows)
            indptr=[0];indices=[];values=[]
            for row in a:
                for j,v in enumerate(row):
                    if v:indices.append(j);values.append(v)
                indptr.append(len(indices))
            m=Model(objective=c,lower=lower,upper=upper,row_lower=rl,row_upper=ru,indptr=indptr,indices=indices,values=values,kinds=["integer" if integer and integrality[j] else "continuous" for j in range(n)])
            if domain=="ip":reference=exact_box_ip(c,a,rl,ru)
            elif domain=="mip":reference=exact_mixed(c,a,rl,ru)
            else:
                oracle=linprog(c,A_ub=np.vstack([a,-a]),b_ub=np.r_[ru,-rl],bounds=list(zip(lower,upper)),method="highs")
                assert oracle.success
                reference=oracle.fun
            r=solve(m,backend="native",backend_options=NativeOptions(steepest_edge=not args.dantzig,refactor_interval=args.refactor_interval))
            assert r.status=="optimal",(domain,case,r,reference)
            assert abs(r.evaluation.objective-float(reference))<1e-5,(domain,case,r,reference)
            assert r.evaluation.feasible_candidate
            if r.best_bound is not None:assert r.best_bound<=float(reference)+1e-7,(domain,case,r,reference,dict(c=c.tolist(),a=a.tolist(),rl=rl.tolist(),ru=ru.tolist()))
            if not integer:
                k=lp_residuals(m,r.candidate,r.duals);assert k.duality_gap<1e-7
            records.append({"domain":domain,"case":case,"objective":r.evaluation.objective,"oracle":float(reference),"nodes":r.nodes,"pivots":r.iterations})
    print(json.dumps({"steepest_edge":not args.dantzig,"refactor_interval":args.refactor_interval,"seed":args.seed,"cases":len(records),"passed":len(records),"results":records},indent=2))
if __name__=="__main__":main()
