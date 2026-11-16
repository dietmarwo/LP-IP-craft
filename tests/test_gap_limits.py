import math
import pytest
from lp_ip_craft import Model, NativeOptions, solve

def model(offset=0.0, sense='maximize', zero=False):
    # A fractional root and a known feasible start below it.
    return Model(objective=[10.0,9.0,8.0,7.0], objective_offset=offset,
        lower=[0.0]*4,upper=[1.0]*4,kinds=['binary']*4,sense=sense,
        indptr=[0,4],indices=[0,1,2,3],values=[1.0]*4,row_lower=[-math.inf],row_upper=[2.5])

@pytest.mark.parametrize('key',['mip_relative_gap','mip_absolute_gap'])
@pytest.mark.parametrize('bad',[-1,math.nan,math.inf])
def test_invalid_gap_options(key,bad):
    with pytest.raises(ValueError,match=key):
        solve(model(),backend='native',backend_options=NativeOptions(**{key:bad}))

def test_relative_and_absolute_gap_have_a_feasible_status():
    for option in [dict(mip_relative_gap=0.3),dict(mip_absolute_gap=10.0)]:
        r=solve(model(),backend='native',start=[1,1,0,0],backend_options=NativeOptions(
            objective_lattice=False,**option))
        assert r.status=='gap_limit'
        assert r.evaluation.feasible_candidate
        assert r.gap.absolute>0
        assert r.gap.relative<=option.get('mip_relative_gap',math.inf)
        assert r.gap.absolute<=option.get('mip_absolute_gap',math.inf)
        assert r.backend.effective_options['gap_limit_scope']=='whole_model'

def test_zero_limits_keep_full_solution_and_specialized_paths_reject():
    a=solve(model(),backend='native')
    b=solve(model(),backend='native',backend_options=NativeOptions(mip_relative_gap=0,mip_absolute_gap=0))
    assert a.status==b.status=='optimal' and a.candidate==b.candidate and a.nodes==b.nodes
    for path in [dict(tree_threads=2),dict(packing_root=True)]:
        with pytest.raises(NotImplementedError,match='gap limits'):
            solve(model(),backend='native',backend_options=NativeOptions(mip_relative_gap=.01,**path))

def test_zero_objective_uses_only_absolute_gap_and_separable_scope_is_global():
    m=Model(objective=[-1.,-1.,-1.,-1.],objective_offset=2.,lower=[0.]*4,upper=[1.]*4,
        kinds=['binary']*4,sense='minimize',indptr=[0,2,4],indices=[0,1,2,3],values=[1.]*4,
        row_lower=[-math.inf]*2,row_upper=[1.5]*2)
    settings=dict(decomposition_min_variables=1, objective_lattice=False)
    r=solve(m,backend='native',start=[1,0,1,0],backend_options=NativeOptions(mip_relative_gap=0.9,**settings))
    assert r.status=='optimal' and r.evaluation.objective==0
    r=solve(m,backend='native',start=[1,0,1,0],backend_options=NativeOptions(mip_absolute_gap=1.1,**settings))
    assert r.status=='gap_limit' and r.evaluation.objective==0
    assert math.isinf(r.gap.relative) and 0<r.gap.absolute<=1.1
    assert r.backend.effective_options['decompose']=='false'
