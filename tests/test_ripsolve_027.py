"""Independent integration/oracle checks for opt-in root improvements."""
from itertools import product
import json,random,math
import pytest
from lp_ip_craft import Model,NativeOptions,Cancellation,solve

def model(c,A,lo,up,rl,ru,kinds,sense='maximize'):
    indptr=[0];indices=[];values=[]
    for row in A:
        for j,v in enumerate(row):
            if v: indices.append(j);values.append(float(v))
        indptr.append(len(indices))
    return Model(objective=c,lower=lo,upper=up,row_lower=rl,row_upper=ru,kinds=kinds,sense=sense,indptr=indptr,indices=indices,values=values)

def run(m,**kw):return solve(m,backend='native',backend_options=NativeOptions(decompose=False,**kw))

@pytest.mark.parametrize('field,value',[('mip_presolve_rounds',33),('mip_cut_rounds',51),('mip_cut_limit',0),('mip_cut_limit',65),('cut_exact_work',100_000_001),('heuristic_lp_calls',129),('branching_score','invalid'),('branch_trace_limit',257),('pseudocost_min_step',-.1),('pseudocost_min_step',math.nan)])
def test_control_validation_before_cancellation(field,value):
    m=model([1.],[],[0.],[1.],[],[],['binary']);c=Cancellation();c.cancel()
    with pytest.raises(ValueError):solve(m,backend='native',cancellation=c,backend_options=NativeOptions(**{field:value}))

def test_gmi_root_strengthens_both_senses_and_preserves_user_start():
    for sense,sign in [('maximize',1),('minimize',-1)]:
        m=model([sign,sign],[[2.,2.]],[0.,0.],[2.,2.],[-math.inf],[3.],['integer']*2,sense)
        r=solve(m,backend='native',start=[1.,0.],backend_options=NativeOptions(gmi_cuts=True,mip_cut_rounds=4,decompose=False))
        assert r.status=='optimal' and abs(r.evaluation.objective-sign)<1e-7
        d=r.backend.effective_options;stages=json.loads(d['diagnostics.root_stages'])
        assert int(d['diagnostics.gmi_cuts'])>0 and int(d['diagnostics.root_basis_reuses'])>0
        assert abs(stages[0]['objective']-1.5*sign)<1e-7
        assert abs(stages[-1]['objective']-sign)<1e-7

def test_combined_mode_shared_limits_keep_safe_bounds_and_checked_start():
    m=model([1.,1.],[[2.,2.]],[0.,0.],[2.,2.],[-math.inf],[3.],['integer']*2)
    for cap in [1,2,5]:
        r=solve(m,backend='native',start=[1.,0.],backend_options=NativeOptions(gmi_cuts=True,mir_cuts=True,node_limit=cap,iteration_limit=20))
        assert r.nodes<=cap and r.iterations<=20
        assert r.evaluation.feasible_candidate and r.evaluation.objective>=1
        assert r.best_bound is None or r.best_bound>=1-1e-7
        if cap==1: assert r.status=='node_limit'

def test_root_heuristics_do_not_change_proof_or_drop_feasible_candidates():
    m=model([1.,1.,1.],[[1.,1.,0.],[0.,1.,1.],[1.,0.,1.]],[0.]*3,[1.]*3,[-math.inf]*3,[1.]*3,['binary']*3)
    r=run(m,root_heuristics=True,heuristic_lp_calls=8,node_limit=20)
    assert r.status=='optimal' and r.evaluation.feasible_candidate and r.evaluation.objective==1
    d=r.backend.effective_options
    assert int(d['diagnostics.root_heuristic_lp_calls'])<=8
    assert int(d['diagnostics.root_heuristic_candidates'])>0

def test_random_general_integer_modes_against_exhaustive_independent_oracle():
    rng=random.Random(27018)
    for case in range(65):
        n=4+case%3;m=3+case%2
        A=[[rng.randrange(-4,5)for _ in range(n)]for _ in range(m)]
        c=[float(rng.randrange(-5,6))for _ in range(n)]
        lo=[-1.]*n;up=[1.+case%2]*n
        witness=[rng.randrange(-1,int(up[j])+1)for j in range(n)]
        activity=[sum(a*v for a,v in zip(row,witness))for row in A]
        rl=[v-rng.randrange(4)for v in activity];ru=[v+rng.randrange(4)+.5 for v in activity]
        sense='minimize'if case%2 else'maximize'
        feasible=[sum(a*v for a,v in zip(c,x))for x in product(range(-1,int(up[0])+1),repeat=n)if all(l<=sum(a*v for a,v in zip(row,x))<=u for row,l,u in zip(A,rl,ru))]
        optimum=(min if sense=='minimize'else max)(feasible)
        mod=model(c,A,lo,up,rl,ru,['integer']*n,sense)
        for controls in [dict(mip_presolve=True,coefficient_tightening=True,dual_presolve=True),dict(gmi_cuts=True,mir_cuts=True,mip_cut_rounds=3),dict(mip_presolve=True,coefficient_tightening=True,gmi_cuts=True,mir_cuts=True,root_heuristics=True,mip_cut_rounds=3,branching_score='product',pseudocost_min_step=.01,branch_trace_limit=4)]:
            r=run(mod,**controls)
            assert r.status=='optimal',(case,controls,r.status,r.message)
            assert r.evaluation.feasible_candidate and abs(r.evaluation.objective-optimum)<1e-6,(case,controls,r.evaluation.objective,optimum)
            if r.best_bound is not None:
                assert r.best_bound<=optimum+1e-6 if sense=='minimize'else r.best_bound>=optimum-1e-6
            trace=r.backend.effective_options.get('diagnostics.branch_trace')
            if trace:assert len(json.loads(trace))<=4

def test_rens_incumbents_are_feasible_budgeted_and_keep_the_exhaustive_optimum():
    rng=random.Random(28005);found=0
    for case in range(40):
        n=10+case%4;m=3
        A=[[rng.randrange(1,15)for _ in range(n)]for _ in range(m)]
        c=[float(rng.randrange(1,20))for _ in range(n)]
        ru=[sum(row)*.45 for row in A]
        optimum=max(sum(a*v for a,v in zip(c,x))for x in product((0,1),repeat=n)if all(sum(a*v for a,v in zip(row,x))<=u for row,u in zip(A,ru)))
        mod=model(c,A,[0.]*n,[1.]*n,[-math.inf]*m,ru,['binary']*n)
        for limit in [None,40]:
            r=run(mod,gmi_cuts=True,mir_cuts=True,root_heuristics=True,heuristic_lp_calls=4,node_limit=limit)
            d=r.backend.effective_options
            assert int(d['diagnostics.root_rens_nodes'])<=50*4
            if limit:assert r.nodes<=limit
            assert r.evaluation.feasible_candidate
            if r.status=='optimal':assert abs(r.evaluation.objective-optimum)<1e-6,(case,r.evaluation.objective,optimum)
            else:assert r.evaluation.objective<=optimum+1e-6 and (r.best_bound is None or r.best_bound>=optimum-1e-6)
            found+=int(d['diagnostics.root_rens_candidates'])
        assert run(mod).evaluation.objective==pytest.approx(optimum)
    assert found>0
