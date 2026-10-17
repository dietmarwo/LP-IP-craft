"""Application-mode contracts, independent integer oracles and budget checks."""
from itertools import product
import random
import pytest
from lp_ip_craft import Model,NativeOptions,Cancellation,solve

def model(c,rows,lower=None,upper=None,kinds=None):
    indices=[];indptr=[0]
    for row in rows:indices.extend(sorted(row));indptr.append(len(indices))
    return Model(objective=c,lower=lower or [0.]*len(c),upper=upper or [1.]*len(c),kinds=kinds or ['binary']*len(c),sense='maximize',row_lower=[-float('inf')]*len(rows),row_upper=[1.]*len(rows),indptr=indptr,indices=indices,values=[1.]*len(indices))

def run(m,**kw):return solve(m,backend='native',backend_options=NativeOptions(**kw))

@pytest.mark.parametrize('field,value',[
 ('packing_workers',0),('packing_workers',17),('tree_threads',0),('tree_threads',17),
 ('packing_neighborhood_size',3),('packing_neighborhood_size',1025),('packing_root_iterations',100001),
 ('packing_root_work',10_000_000_001),('packing_neighborhoods',4097),('tree_split_depth',9),
 ('branch_probe_candidates',17),('branch_probe_pivots',1025),('branch_probe_work',10_000_000_001),
 ('packing_cut_work',100_000_001)])
def test_controls_reject_invalid_values_even_when_cancelled(field,value):
    token=Cancellation();token.cancel()
    with pytest.raises((ValueError,OverflowError)):
        solve(model([1.],[]),backend='native',backend_options=NativeOptions(**{field:value}),cancellation=token)

def test_new_flags_preserve_lp_fallback_and_valid_user_start():
    m=model([6.,5.,5.],[[0,1],[0,2]])
    r=solve(m,backend='native',start=[0,1,1],backend_options=NativeOptions(packing_root=True,packing_root_iterations=1,packing_neighborhoods=8,packing_workers=4,iteration_limit=1))
    assert r.evaluation.feasible_candidate and r.evaluation.objective==10
    lp=model([1.,1.,1.],[[0,1],[1,2],[0,2]],kinds=['continuous']*3)
    r=run(lp,packing_root=True,packing_cuts=True,tree_threads=4)
    assert r.status=='optimal' and abs(r.evaluation.objective-1.5)<1e-7

def test_tiny_cut_work_and_global_lns_tree_budget_are_honoured():
    m=model([10.]*9,[[j,(j+1)%9]for j in range(9)])
    for workers in [1,4]:
        r=run(m,packing_root=True,packing_root_iterations=10,packing_neighborhoods=32,packing_workers=workers,
              packing_cuts=True,packing_cut_work=0,tree_threads=4,node_limit=12,iteration_limit=16)
        assert r.nodes<=12 and r.iterations<=16
        assert r.evaluation.feasible_candidate
        assert r.best_bound is not None and r.best_bound>=40
        assert r.backend.effective_options['diagnostics.packing_cut_work']=='0'

def test_random_oracle_all_new_modes():
    rng=random.Random(25017)
    for case in range(100):
        n=6+case%4;c=[float(rng.randrange(1,21))for _ in range(n)]
        rows=[[j for j in range(n)if rng.random()<.3]for _ in range(n+2)]
        lower=[0.]*n;upper=[1.]*n
        if case%4==0:upper[case%n]=0.
        if case%9==0:lower[(case+1)%n]=1.
        feasible=[]
        for x in product([0,1],repeat=n):
            if all(l<=v<=u for l,v,u in zip(lower,x,upper))and all(sum(x[j]for j in row)<=1 for row in rows):feasible.append(sum(a*v for a,v in zip(c,x)))
        optimum=max(feasible,default=None)
        m=model(c,rows,lower,upper)
        for extra in [dict(packing_root=True),dict(packing_root=True,packing_cuts=True),
                      dict(packing_root=True,packing_neighborhoods=8,packing_workers=4),
                      dict(packing_root=True,packing_cuts=True,scalable_branching=True,tree_threads=4)]:
            r=run(m,**extra,packing_root_iterations=64,packing_cut_work=5000,decompose=False)
            if optimum is None:assert r.status=='infeasible';continue
            assert r.status=='optimal',(case,extra,r)
            assert r.evaluation.feasible_candidate
            assert abs(r.evaluation.objective-optimum)<1e-7
            if r.best_bound is not None:assert r.best_bound>=optimum-1e-7

def test_inflight_parallel_cancellation_retains_checked_start():
    import threading,time
    n=20001
    m=model([10.]*n,[[j,(j+1)%n]for j in range(n)])
    token=Cancellation()
    def stop():
        time.sleep(.002)
        token.cancel()
    worker=threading.Thread(target=stop);worker.start()
    try:
        r=solve(m,backend='native',start=[0.]*n,cancellation=token,
                backend_options=NativeOptions(tree_threads=4,tree_split_depth=2,time_limit_seconds=2))
    finally:worker.join()
    assert r.status=='cancelled'
    assert r.evaluation.feasible_candidate
    assert r.evaluation.objective>=0

def test_independent_batch_disables_nested_worker_pools():
    from lp_ip_craft import solve_batch
    m=model([6.,5.,5.],[[0,1],[0,2]])
    out=solve_batch([m]*4,workers=2,backend_options=NativeOptions(packing_root=True,packing_neighborhoods=8,packing_workers=4,tree_threads=4))
    for r in out:
        assert not isinstance(r,Exception)
        assert r.evaluation.feasible_candidate and r.evaluation.objective==10
        assert r.backend.effective_options['packing_workers']=='1'
        assert r.backend.effective_options['tree_threads']=='1'
