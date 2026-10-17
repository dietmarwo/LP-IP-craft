import pytest
import lp_ip_craft as lp

def box(integral=False, free=False):
    return lp.Model(objective=[1.],lower=[-float('inf') if free else 0.],upper=[float('inf') if free else 7.],row_lower=[],row_upper=[],indptr=[0],indices=[],values=[],kinds=['integer' if integral else 'continuous'],sense='maximize')

@pytest.mark.parametrize('workers',[1,2,4,8,16])
def test_ordered_batch_with_job_error(workers):
    good,bad=box(),box(True,True)
    result=lp.solve_batch([good,bad,good]*6,workers=workers)
    assert len(result)==18
    for i,r in enumerate(result):
        if i%3==1: assert isinstance(r,NotImplementedError)
        else:
            assert r.status=='optimal'
            assert r.evaluation.objective==7.
            assert r.backend.effective_options['component_threads']=='1'

def test_empty_invalid_and_cancelled_batch():
    assert lp.solve_batch([],workers=16)==[]
    for n in [0,-1,17]:
        with pytest.raises(ValueError): lp.solve_batch([],workers=n)
    for n in [True,1.5]:
        with pytest.raises(TypeError): lp.solve_batch([],workers=n)
    token=lp.Cancellation();token.cancel()
    assert all(r.status=='cancelled' for r in lp.solve_batch([box()]*20,workers=4,cancellation=token))
