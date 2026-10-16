"""Native engine acceptance, independent checks and explicit limits."""
import math
from concurrent.futures import ThreadPoolExecutor
import pytest
from lp_ip_craft import BACKENDS, Model, NativeOptions, Options, Cancellation, Duals, solve, lp_residuals

def make(c=(3.,2.), a=((1.,1.),(2.,1.)), lo=None, up=None, rl=None, ru=(4.,5.), kinds=None, sense="maximize", offset=0.):
    n=len(c); a=list(a); indptr=[0];indices=[];values=[]
    for row in a:
        for j,v in enumerate(row):
            if v!=0:indices.append(j);values.append(v)
        indptr.append(len(indices))
    return Model(objective=c,objective_offset=offset,lower=[0.]*n if lo is None else lo,upper=[math.inf]*n if up is None else up,
                 row_lower=[-math.inf]*len(a) if rl is None else rl,row_upper=ru,indptr=indptr,indices=indices,values=values,kinds=kinds,sense=sense)

def test_backend_inventory():
    assert BACKENDS == ("dummy","native")
    with pytest.raises(ValueError):solve(make(),backend="highs")

@pytest.mark.parametrize("sense,c,expected",[("maximize",(3.,2.),9.),("minimize",(-3.,-2.),-9.)])
def test_lp_duals_and_offsets(sense,c,expected):
    m=make(c=c,sense=sense,offset=7.)
    r=solve(m,backend="native")
    assert r.status=="optimal" and r.evaluation.feasible_candidate
    assert r.evaluation.objective==pytest.approx(expected+7.)
    k=lp_residuals(m,r.candidate,r.duals)
    assert max(k.primal,k.stationarity,k.dual_sign,k.duality_gap)<1e-7

@pytest.mark.parametrize("opts,status",[(NativeOptions(time_limit_seconds=0.),"time_limit"),(NativeOptions(iteration_limit=0),"iteration_limit")])
def test_limits(opts,status):assert solve(make(),backend="native",backend_options=opts).status==status

def test_cancel_and_validation():
    c=Cancellation();c.cancel()
    assert solve(make(),backend="native",cancellation=c).status=="cancelled"
    with pytest.raises(ValueError):solve(make(),backend="native",cancellation=c,start=[])
    with pytest.raises(ValueError):solve(make(),backend="native",backend_options=NativeOptions(pivot_tolerance=math.nan))

def test_mip_start_node_limit():
    m=make(up=[1.,1.],kinds=["binary"]*2)
    r=solve(m,backend="native",start=[1.,0.],backend_options=NativeOptions(node_limit=0))
    assert r.status=="node_limit" and r.candidate==[1.,0.] and r.best_bound is None

def test_independent_instances():
    m=make()
    with ThreadPoolExecutor(4) as pool:results=list(pool.map(lambda _:solve(m,backend="native"),range(12)))
    assert all(r.status=="optimal" and r.evaluation.objective==pytest.approx(9.) for r in results)

def test_infeasible_unbounded_and_general_bounds():
    assert solve(make(c=[1.],a=[[1.],[1.]],lo=[-math.inf],rl=[2.,-math.inf],ru=[math.inf,1.]),backend="native").status=="infeasible"
    assert solve(make(c=[1.],a=[],ru=[]),backend="native").status=="unbounded"
    r=solve(make(c=[1.],a=[],lo=[-math.inf],up=[3.],ru=[]),backend="native")
    assert r.status=="optimal" and r.candidate==[3.]

def test_small_coefficients():
    r=solve(make(c=[1.],a=[[1e-11]],rl=[2e-11],ru=[2e-11],up=[10.],sense="minimize"),backend="native")
    assert r.status=="optimal" and r.candidate==pytest.approx([2.])

def test_binary_exhaustive():
    import random,itertools
    rng=random.Random(8923)
    for _ in range(60):
        c=[rng.randint(-9,13) for _ in range(7)]; a=[[rng.randint(-3,9) for _ in c] for _ in range(3)];b=[rng.randint(3,15) for _ in a]
        feasible=[sum(cj*xj for cj,xj in zip(c,x)) for x in itertools.product([0.,1.],repeat=7) if all(sum(v*t for v,t in zip(row,x))<=rhs for row,rhs in zip(a,b))]
        r=solve(make(c=c,a=a,ru=b,up=[1.]*7,kinds=["binary"]*7),backend="native")
        assert r.status=="optimal" and r.evaluation.objective==pytest.approx(max(feasible))
        if r.best_bound is not None:assert r.best_bound>=max(feasible)-1e-8

@pytest.mark.parametrize("settings",[
    NativeOptions(presolve=False),NativeOptions(fast_pivots=False),
    NativeOptions(dantzig_pricing=False),NativeOptions(node_reoptimization=False),
    NativeOptions(incumbent_heuristic=False,objective_lattice=False)])
def test_optimization_switches(settings):
    m=make(c=[7.,11.,5.,6.],a=[[3.,5.,2.,4.]],ru=[8.5],up=[1.]*4,kinds=["binary"]*4)
    r=solve(m,backend="native",backend_options=settings)
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(18.)

@pytest.mark.parametrize("c,kinds,offset,expected_lattice",[
    ([2.,3.],["integer","integer"],0.,"true"),
    ([2.5,3.],["integer","integer"],0.,"false"),
    ([2.,3.],["integer","continuous"],0.,"false"),
    ([2.,3.],["integer","integer"],0.5,"false")])
def test_lattice_scope(c,kinds,offset,expected_lattice):
    m=make(c=c,a=[[1.,1.]],ru=[1.5],up=[2.,2.],kinds=kinds,offset=offset)
    r=solve(m,backend="native")
    assert r.status=="optimal"
    assert r.backend.effective_options["diagnostics.objective_lattice"]==expected_lattice

def test_uncertified_start_cannot_prune():
    m=make(c=[1.],a=[[1.]],ru=[0.99999995],up=[1.],kinds=["integer"])
    r=solve(m,backend="native",start=[1.])
    # The start passes tolerance acceptance, but violates the exact stored row.
    # It must not turn the conservative bound into an exact-search certificate.
    assert r.status=="optimal" and r.best_bound>=0.
    assert r.candidate==[0.] and r.evaluation.objective==0.

def test_lp_primal_start_is_explicitly_rejected():
    r=solve(make(),backend="native",start=[0.,0.])
    assert r.status=="optimal" and r.backend.start_status=="rejected"


def test_mixed_bound_against_exact_optimum():
    m=make(c=[7.,-6.,0.,0.,2.],a=[[0.,-4.,5.,-1.,-1.],[-6.,1.,5.,4.,-6.],[1.,3.,6.,-3.,2.],[-4.,-1.,4.,4.,-5.]],
           lo=[0.]*5,up=[3.]*5,rl=[-12.,-13.,9.,-12.],ru=[-9.,-10.,12.,-8.],kinds=["integer","continuous","integer","continuous","integer"],sense="minimize")
    r=solve(m,backend="native")
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(-8.)
    assert r.best_bound<=-8.+1e-10

@pytest.mark.parametrize("enabled",[False,True])
def test_redundant_rows(enabled):
    m=make(c=[3.,2.],a=[[1.,1.],[1.,-1.],[0.,0.]],up=[1.,1.],rl=[0.,-1.,0.],ru=[2.,1.,0.])
    r=solve(m,backend="native",backend_options=NativeOptions(redundant_rows=enabled))
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(5.)
    if enabled:assert int(r.backend.effective_options["diagnostics.presolve_removed_row_instances"])==3


def test_infeasible_root_cannot_certify_tolerance_start_as_optimal():
    m=make(c=[1.],a=[[1.],[1.]],up=[1.],rl=[-math.inf,1.],ru=[0.99999995,math.inf],kinds=["integer"])
    r=solve(m,backend="native",start=[1.])
    assert r.status=="numerical_failure" and r.candidate==[1.]
