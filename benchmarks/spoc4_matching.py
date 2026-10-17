"""Pinned ESA matching instances -> identical binary set-packing models.

One worker per fresh process; conversion and solve time are recorded separately.
Direct input validation uses original resource identifiers and integer thousandths of the published weight.
"""
from __future__ import annotations
import argparse
from collections import deque
from decimal import Decimal
import hashlib
import json
import math
from pathlib import Path
import platform
import time
import numpy as np
from scipy.sparse import coo_matrix

ROOT=Path(__file__).resolve().parent

def load(path:Path,limit:int=0):
    lines=[line.split()for line in path.read_text().splitlines()if line.strip()]
    triples=np.array([[int(v)for v in line[:3]]for line in lines],dtype=np.int64)
    milli=np.array([int(Decimal(line[3])*1000)for line in lines],dtype=np.int64)
    assert all(Decimal(line[3])*1000==int(v) for line,v in zip(lines,milli))
    selected=np.arange(len(lines))
    if limit and limit<len(lines):
        # Deterministic connected edge-BFS, preserving original edge identities.
        incident={}
        for j,t in enumerate(triples):
            for part,node in enumerate(t):incident.setdefault((part,int(node)),[]).append(j)
        seen={0};queue=deque([0]);order=[]
        while queue and len(order)<limit:
            j=queue.popleft();order.append(j)
            for part,node in enumerate(triples[j]):
                for k in incident[(part,int(node))]:
                    if k not in seen:seen.add(k);queue.append(k)
        assert len(order)==limit
        selected=np.array(sorted(order));triples=triples[selected];milli=milli[selected]
    maps=[np.unique(triples[:,k],return_inverse=True)for k in range(3)]
    offsets=np.cumsum([0]+[len(v[0])for v in maps]);n=len(milli)
    row=np.concatenate([v[1]+offsets[k]for k,v in enumerate(maps)])
    col=np.tile(np.arange(n),3)
    A=coo_matrix((np.ones(len(row)),(row,col)),shape=(offsets[-1],n)).tocsr()
    # Degree-one resource rows exactly duplicate a binary column's upper bound.
    keep=np.diff(A.indptr)>1
    A=A[keep].tocsr()
    return triples,milli,A,selected

def candidate_check(triples,milli,x):
    if x is None:return None
    x=np.asarray(x);rounded=np.rint(x);ints=float(np.max(np.abs(x-rounded),initial=0.));chosen=np.flatnonzero(rounded>0.5)
    duplicates=[len(chosen)-len(np.unique(triples[chosen,k]))for k in range(3)]
    return dict(feasible=bool(ints<=1e-7 and np.all(x>=-1e-7)and np.all(x<=1+1e-7)and not any(duplicates)),integrality=ints,selected=len(chosen),duplicate_resources=duplicates,objective_milli=int(milli[chosen].sum()),objective=float(milli[chosen].sum()/1000))

def greedy(triples,milli):
    used=[set(),set(),set()];x=np.zeros(len(milli));order=np.lexsort((np.arange(len(milli)),-milli))
    for j in order:
        if all(int(triples[j,k])not in used[k]for k in range(3)):
            x[j]=1
            for k in range(3):used[k].add(int(triples[j,k]))
    return x

def packing_upper_bound(A,milli,row_dual,denominator=2**20):
    """Exact dyadic dual bound, using upper bounds x<=1 for residual costs.

    Any nonnegative y is valid: U = sum(y)+sum(max(0,c-A.T@y)).
    All operations below are integer; the returned floor also uses the integer
    objective lattice. This is independent of the solver's optimality status.
    """
    values=np.asarray(row_dual,dtype=float)
    if not np.all(np.isfinite(values)):return None,None
    y=np.rint(np.maximum(0.,values)*denominator)
    if np.max(y,initial=0)>2**50:raise ValueError('dual magnitude too large')
    y=y.astype(np.int64)
    activity=np.zeros(A.shape[1],dtype=np.int64)
    np.add.at(activity,A.indices,np.repeat(y,np.diff(A.indptr)))
    residual=np.maximum(milli*denominator-activity,0)
    # Python integer summation avoids overflow of the total bound.
    numerator=sum(map(int,y))+sum(map(int,residual))
    return dict(numerator=numerator,denominator=denominator,unit='published weight thousandths',objective_upper_milli=numerator//denominator,objective_upper=(numerator//denominator)/1000),y

def solve(a):
    started=time.perf_counter();triples,milli,A,selected=load(a.input,a.limit);n=A.shape[1];m=A.shape[0]
    weights=milli.astype(float)if a.encoding=='milli'else milli.astype(float)/1000
    scale=1000 if a.encoding=='milli'else 1
    seed=greedy(triples,milli);build=time.perf_counter()-started
    r=dict(input=a.input.name,input_sha256=hashlib.sha256(a.input.read_bytes()).hexdigest(),subset='edge-bfs'if a.limit else'full',edge_limit=a.limit,selected_edge_sha256=hashlib.sha256(selected.tobytes()).hexdigest(),n=n,m=m,nnz=A.nnz,encoding=a.encoding,backend=a.backend,mode=a.mode,policy=a.policy,lp_algorithm=a.lp_algorithm,lp_negate=getattr(a,'lp_negate',False),threads=a.threads,time_limit=a.seconds,seed_kind=a.start,greedy=candidate_check(triples,milli,seed),construction_seconds=build)
    if a.backend=='greedy':r.update(status='feasible_heuristic',candidate=r['greedy']);return r
    if a.backend=='native':
        import lp_ip_craft as lp
        model=lp.Model(objective=weights,lower=np.zeros(n),upper=np.ones(n),kinds=['continuous'if a.mode=='lp'else'binary']*n,sense='maximize',row_lower=np.full(m,-np.inf),row_upper=np.ones(m),indptr=A.indptr,indices=A.indices,values=A.data)
        controls=dict(time_limit_seconds=a.seconds,node_limit=100000,iteration_limit=1000000,component_threads=a.threads,mip_diagnostics=True,mip_trace_limit=32)
        if a.policy=='reliability':controls['reliability_branching']=True
        if a.policy=='jump':controls['feasibility_jump']=True
        if a.policy=='covers':controls.update(root_separation=True)
        if a.policy=='mixed':controls['mixed_propagation']=True
        if a.policy=='combined':controls.update(reliability_branching=True,feasibility_jump=True,root_separation=True,mixed_propagation=True)
        if a.policy=='factor16m':controls['max_factor_entries']=16000000
        t=time.perf_counter()
        try:
            result=lp.solve(model,backend='native',backend_options=lp.NativeOptions(**controls),start=seed if a.start=='greedy'and a.mode=='mip'else None)
            r.update(status=result.status,solve_seconds=time.perf_counter()-t,nodes=result.nodes,pivots=result.iterations,objective=result.evaluation.objective/scale if result.evaluation else None,best_bound=result.best_bound/scale if result.best_bound is not None else None,metadata=result.backend.effective_options,message=result.message,version=lp.__version__)
            if a.mode=='mip':r['candidate']=candidate_check(triples,milli,result.candidate)
            elif result.candidate is not None:r['lp_candidate']=dict(row_violation=float(np.max(A@result.candidate-1,initial=0.)),fractional=int(np.sum(np.abs(np.asarray(result.candidate)-np.rint(result.candidate))>1e-7)))
        except Exception as e:r.update(status='exception',solve_seconds=time.perf_counter()-t,error=str(e),version=lp.__version__)
    elif a.backend=='highs':
        import highspy as hs
        h=hs.Highs();h.setOptionValue('output_flag',True);h.setOptionValue('log_to_console',False);h.setOptionValue('log_file',str(a.output.with_suffix('.solver.log')));h.setOptionValue('threads',a.threads);h.setOptionValue('time_limit',a.seconds);h.setOptionValue('random_seed',0)
        if a.mode=='lp':
            h.setOptionValue('solver',a.lp_algorithm)
            if a.lp_algorithm=='simplex':h.setOptionValue('simplex_strategy',1)
            else:h.setOptionValue('run_crossover','off')
        direction=-1 if a.mode=='lp'and getattr(a,'lp_negate',False)else 1
        h.addVars(n,np.zeros(n),np.ones(n));h.changeColsCost(n,np.arange(n,dtype=np.int32),direction*weights)
        if direction==-1:h.setMinimize()
        else:h.setMaximize()
        h.addRows(m,np.full(m,-hs.kHighsInf),np.ones(m),A.nnz,A.indptr,A.indices,A.data)
        if a.mode=='mip':
            h.changeColsIntegrality(n,np.arange(n,dtype=np.int32),np.ones(n,dtype=np.uint8))
            h.setOptionValue('mip_max_nodes',100000)
            if a.start=='greedy':h.setSolution(n,np.arange(n,dtype=np.int32),seed)
        t=time.perf_counter();h.run();info=h.getInfo();sol=h.getSolution();status=h.getModelStatus()
        r.update(status=h.modelStatusToString(status),solve_seconds=time.perf_counter()-t,nodes=info.mip_node_count,pivots=info.simplex_iteration_count,objective=direction*h.getObjectiveValue()/scale if sol.value_valid else None,best_bound=info.mip_dual_bound/scale if a.mode=='mip'and math.isfinite(info.mip_dual_bound)else None,gap=info.mip_gap if a.mode=='mip'and math.isfinite(info.mip_gap)else None,version=h.version())
        if sol.value_valid:
            if a.mode=='mip':r['candidate']=candidate_check(triples,milli,sol.col_value)
            else:
                x=np.asarray(sol.col_value);r['lp_candidate']=dict(row_violation=float(np.max(A@x-1,initial=0.)),fractional=int(np.sum(np.abs(x-np.rint(x))>1e-7)));np.save(a.output.with_suffix('.lp.npy'),x)
                if sol.dual_valid:
                    cert,y=packing_upper_bound(A,milli,direction*np.asarray(sol.row_dual)*(1000/scale))
                    r['exact_packing_upper_bound']=cert
                    if y is not None:np.save(a.output.with_suffix('.dual.npy'),y)
                r['lp_info']=dict(primal_solution_status=info.primal_solution_status,dual_solution_status=info.dual_solution_status,max_primal_infeasibility=info.max_primal_infeasibility,max_dual_infeasibility=info.max_dual_infeasibility,ipm_iterations=info.ipm_iteration_count)
    elif a.backend=='gurobi':
        import gurobipy as gp
        env=gp.Env(empty=True);env.setParam('OutputFlag',0);env.start();model=gp.Model(env=env);model.Params.Threads=a.threads;model.Params.TimeLimit=a.seconds;model.Params.Seed=0;model.Params.NodeLimit=100000
        x=model.addMVar(n,vtype=gp.GRB.CONTINUOUS if a.mode=='lp'else gp.GRB.BINARY,lb=0.,ub=1.);model.setObjective(weights@x,gp.GRB.MAXIMIZE);model.addMConstr(A,x,'<',np.ones(m))
        if a.start=='greedy'and a.mode=='mip':x.Start=seed
        t=time.perf_counter()
        try:
            model.optimize();r.update(status=int(model.Status),solve_seconds=time.perf_counter()-t,nodes=model.NodeCount,pivots=model.IterCount,version='.'.join(map(str,gp.gurobi.version())))
            if model.SolCount:
                r.update(objective=model.ObjVal/scale,best_bound=(model.ObjBound if a.mode=='mip'else model.ObjVal)/scale)
                if a.mode=='mip':r['candidate']=candidate_check(triples,milli,x.X)
        except gp.GurobiError as e:r.update(status='license_or_solver_error',error_code=e.errno,error=str(e),solve_seconds=time.perf_counter()-t)
    r['total_seconds']=time.perf_counter()-started
    return r

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--input',type=Path,required=True);p.add_argument('--backend',choices=['native','highs','gurobi','greedy'],required=True);p.add_argument('--mode',choices=['lp','mip'],default='mip');p.add_argument('--encoding',choices=['decimal','milli'],default='milli');p.add_argument('--lp-negate',action='store_true');p.add_argument('--lp-algorithm',choices=['simplex','ipm'],default='simplex');p.add_argument('--policy',default='default',choices=['default','reliability','jump','covers','mixed','combined','factor16m']);p.add_argument('--limit',type=int,default=0);p.add_argument('--seconds',type=float,default=30.);p.add_argument('--threads',type=int,default=1);p.add_argument('--start',choices=['none','greedy'],default='none');p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.parent.mkdir(parents=True,exist_ok=True)
    r=solve(a);a.output.write_text(json.dumps(r,indent=2,allow_nan=False)+'\n');print(json.dumps({k:v for k,v in r.items()if k not in['metadata']},allow_nan=False),flush=True)
