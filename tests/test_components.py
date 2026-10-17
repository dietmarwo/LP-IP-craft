import math
import pytest
from lp_ip_craft import NativeOptions,solve,lp_residuals
from test_native import make

def controls(**kwargs):return NativeOptions(decomposition_min_variables=1,**kwargs)

@pytest.mark.parametrize("sense",["minimize","maximize"])
@pytest.mark.parametrize("integer",[False,True])
def test_component_reassembly_offsets_and_duals(sense,integer):
    m=make(c=[3.,2.,5.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.],[1.,1.,1.,1.]],
           up=[1.]*4,ru=[1.5,1.5,math.inf],offset=7.25,sense=sense,kinds=["binary" if integer else "continuous"]*4)
    a=solve(m,backend="native",backend_options=controls())
    b=solve(m,backend="native",backend_options=controls(decompose=False))
    assert a.status==b.status=="optimal"
    assert a.evaluation.objective==pytest.approx(b.evaluation.objective)
    assert a.evaluation.feasible_candidate
    assert a.backend.effective_options["diagnostics.component_count"]=="2"
    if not integer:assert lp_residuals(m,a.candidate,a.duals).duality_gap<1e-7
    if a.best_bound is not None:
        assert (a.best_bound<=a.evaluation.objective+1e-8) if sense=="minimize" else (a.best_bound>=a.evaluation.objective-1e-8)

def test_component_global_node_budget_and_start():
    m=make(c=[3.,2.,5.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.]],up=[1.]*4,ru=[1.5,1.5],kinds=["binary"]*4)
    r=solve(m,backend="native",start=[0.]*4,backend_options=controls(node_limit=1))
    assert r.status=="node_limit" and r.nodes<=1
    assert r.candidate==[0.]*4 and r.evaluation.feasible_candidate

def test_component_status_composition():
    # The first block is unbounded; the second is infeasible. Together infeasible.
    m=make(c=[1.,1.,1.,1.],a=[[1.,-1.,0.,0.],[0.,0.,1.,1.]],lo=[0.]*4,up=[math.inf]*4,rl=[0.,-math.inf],ru=[0.,-1.])
    assert solve(m,backend="native",backend_options=controls()).status=="infeasible"
    m=make(c=[1.,1.,1.,1.],a=[[1.,-1.,0.,0.],[0.,0.,1.,1.]],lo=[0.]*4,up=[math.inf]*4,rl=[0.,-math.inf],ru=[0.,1.])
    assert solve(m,backend="native",backend_options=controls()).status=="unbounded"

def test_component_cap_is_per_factorization():
    m=make(c=[3.,2.,5.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.]],up=[1.]*4,ru=[1.5,1.5])
    assert solve(m,backend="native",backend_options=controls(max_factor_entries=1)).status=="optimal"
    with pytest.raises(NotImplementedError):solve(m,backend="native",backend_options=controls(max_factor_entries=1,decompose=False))

def test_isolated_contradictory_row_is_not_lost():
    m=make(c=[1.,1.,1.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.],[0.,0.,0.,0.]],up=[1.]*4,ru=[1.,1.,-1.])
    assert solve(m,backend="native",backend_options=controls()).status=="infeasible"

@pytest.mark.parametrize("workers",[1,2,4,16])
@pytest.mark.parametrize("domain",["lp","ip","mip"])
def test_parallel_matches_serial(workers,domain):
    kinds=["continuous"]*4 if domain=="lp" else ["binary"]*4 if domain=="ip" else ["binary","continuous","binary","continuous"]
    m=make(c=[3.,2.,5.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.]],up=[1.]*4,ru=[1.5,1.5],kinds=kinds,offset=7.25)
    a=solve(m,backend="native",backend_options=controls(component_threads=workers,component_parallel_min_entries=0))
    b=solve(m,backend="native",backend_options=controls(component_threads=1))
    assert a.status==b.status=="optimal" and a.evaluation.feasible_candidate
    assert a.evaluation.objective==pytest.approx(b.evaluation.objective)
    assert a.iterations==b.iterations and a.nodes==b.nodes
    assert int(a.backend.effective_options["diagnostics.component_workers"])==min(workers,2)

@pytest.mark.parametrize("limit",[0,1,2,3,5])
def test_parallel_global_node_and_pivot_limits(limit):
    m=make(c=[3.,2.,5.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.]],up=[1.]*4,ru=[1.5,1.5],kinds=["binary"]*4)
    for name in ["node_limit","iteration_limit"]:
        r=solve(m,backend="native",start=[0.]*4,backend_options=controls(component_threads=4,component_parallel_min_entries=0,**{name:limit}))
        assert (r.nodes if name=="node_limit" else r.iterations)<=limit
        assert r.status in ["node_limit","iteration_limit","optimal"]
        assert r.candidate is not None and len(r.candidate)==4 and r.evaluation.feasible_candidate

@pytest.mark.parametrize("workers",[0,17])
def test_parallel_invalid_worker_count(workers):
    with pytest.raises(ValueError,match="component_threads"):
        solve(make(),backend="native",backend_options=controls(component_threads=workers))


def test_parallel_child_error_is_propagated():
    from lp_ip_craft import Cancellation
    token=Cancellation()
    # The second block has two rows, so its basis needs two factor entries.
    m=make(c=[1.]*10,a=[[1.,1.]+[0.]*8,[0.,0.]+[1.]*8,[0.,0.,1.,1.]+[0.]*6],up=[1.]*10,ru=[1.5,3.5,1.5])
    with pytest.raises(NotImplementedError,match="max_factor_entries"):
        solve(m,backend="native",cancellation=token,backend_options=controls(component_threads=2,component_parallel_min_entries=0,max_factor_entries=1,presolve=False))
    assert not token.cancelled

def test_parallel_deadline_does_not_return_partial_vector():
    m=make(c=[3.,2.,5.,1.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.]],up=[1.]*4,ru=[1.5,1.5])
    r=solve(m,backend="native",backend_options=controls(component_threads=2,component_parallel_min_entries=0,time_limit_seconds=1e-9))
    assert r.status=="time_limit" and r.candidate is None

def test_parallel_cooperative_cancel():
    import threading,time
    from concurrent.futures import ThreadPoolExecutor
    from lp_ip_craft import Cancellation
    n=128;rows=[[1. if j//64==i//32 else 0. for j in range(n)] for i in range(64)]
    m=make(c=[1.]*n,a=rows,up=[1.]*n,ru=[31.5]*64)
    token=Cancellation();entered=threading.Event()
    def run():
        entered.set()
        return solve(m,backend="native",cancellation=token,backend_options=controls(component_threads=2,component_parallel_min_entries=0,time_limit_seconds=2.))
    with ThreadPoolExecutor(1) as pool:
        pending=pool.submit(run);assert entered.wait(2.);time.sleep(0.001);token.cancel();r=pending.result(timeout=10.)
    assert r.status in ["cancelled","optimal"]
    if r.candidate is not None:assert len(r.candidate)==n and r.evaluation.feasible_candidate


def test_parallel_free_and_upper_only_columns():
    m=make(c=[1.,-1.,2.,3.],a=[[1.,1.,0.,0.],[0.,0.,1.,1.]],lo=[-math.inf,-math.inf,0.,0.],up=[math.inf,2.,math.inf,math.inf],rl=[1.,1.],ru=[1.,1.],offset=7.25,sense="minimize")
    r=solve(m,backend="native",backend_options=controls(component_threads=2,component_parallel_min_entries=0))
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(6.25)
    assert lp_residuals(m,r.candidate,r.duals).duality_gap<1e-7

def test_explicit_zero_entries_do_not_connect_blocks():
    from lp_ip_craft import Model
    m=Model(objective=[3.,2.,5.,1.],sense="maximize",lower=[0.]*4,upper=[1.]*4,row_lower=[-math.inf]*2,row_upper=[1.5]*2,indptr=[0,3,6],indices=[0,1,2,0,2,3],values=[1.,1.,0.,0.,1.,1.])
    r=solve(m,backend="native",backend_options=controls(component_threads=2,component_parallel_min_entries=0))
    assert r.status=="optimal" and r.backend.effective_options["diagnostics.component_count"]=="2"

def test_all_sixteen_component_workers():
    blocks=16;n=blocks*4
    a=[[1. if j//4==i else 0. for j in range(n)] for i in range(blocks)]
    m=make(c=[4.,3.,2.,1.]*blocks,a=a,up=[1.]*n,ru=[1.5]*blocks)
    r=solve(m,backend="native",backend_options=controls(component_threads=16,component_parallel_min_entries=0))
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(88.)
    assert r.backend.effective_options["diagnostics.component_workers"]=="16"
