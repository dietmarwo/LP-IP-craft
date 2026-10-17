"""Proof, restoration, work-accounting and algorithmic regression checks for 0.24."""
import itertools
import json
import random
import pytest
import lp_ip_craft as lp


def knapsack(weights=(3., 3., 3., 4.), capacity=5.):
    n = len(weights)
    return lp.Model(objective=[1.] * n, lower=[0.] * n, upper=[1.] * n,
                    kinds=['binary'] * n, sense='maximize', row_lower=[-float('inf')],
                    row_upper=[capacity], indptr=[0, n], indices=list(range(n)), values=weights)


def run(m, **kwargs):
    settings = dict(decompose=False, mip_diagnostics=True, redundant_rows=False,
                    singleton_presolve=False, root_propagation=False)
    settings.update(kwargs)
    return lp.solve(m, backend='native', backend_options=lp.NativeOptions(**settings))


def test_rational_witness_for_nonrepresentable_boundary():
    m = lp.Model(objective=[0., 1.], objective_offset=2., lower=[0., 0.], upper=[1., 1.],
                 kinds=['binary', 'continuous'], indptr=[0, 1], indices=[1], values=[3.],
                 row_lower=[1.], row_upper=[1.])
    a, b = run(m, rational_certification=False), run(m)
    assert a.status == b.status == 'optimal'
    assert b.evaluation.objective == pytest.approx(2. + 1. / 3.)
    meta = b.backend.effective_options
    assert int(meta['diagnostics.rational_witness_successes']) > 0
    assert json.loads(meta['diagnostics.mip_search'])['first_certified_incumbent'] is not None
    assert json.loads(a.backend.effective_options['diagnostics.mip_search'])['first_certified_incumbent'] is None


@pytest.mark.parametrize('cap', [1, 2, 3, 7, 20])
def test_reliability_probes_share_limits_and_restore_parent(cap):
    m = knapsack()
    r = run(m, reliability_branching=True, node_limit=cap, reliability_max_probes=4,
            incumbent_heuristic=False)
    assert r.nodes <= cap
    assert int(r.backend.effective_options['diagnostics.strong_branch_probes']) <= 4
    if r.candidate is not None:
        assert lp.evaluate(m, r.candidate).feasible_candidate
    complete = run(m, reliability_branching=True, node_limit=100, reliability_max_probes=4)
    assert complete.status == 'optimal' and complete.evaluation.objective == 1.


def test_directed_lifting_strengthens_root_and_accounts_for_prepasses():
    m = knapsack()
    a, b = run(m), run(m, root_separation=True)
    assert a.status == b.status == 'optimal'
    assert b.evaluation.objective == a.evaluation.objective == 1.
    meta = b.backend.effective_options
    assert int(meta['diagnostics.directed_root_cuts']) > 0
    assert int(meta['diagnostics.lifted_root_cuts']) > 0
    d = json.loads(meta['diagnostics.mip_search'])
    assert d['root']['conservative_bound'] <= 1. + 1e-10
    assert b.iterations == d['terminal']['pivots'] + int(meta['diagnostics.root_separation_pivots'])
    assert b.nodes == d['terminal']['nodes'] + int(meta['diagnostics.root_separation_nodes'])


@pytest.mark.parametrize('caps', [dict(node_limit=1), dict(iteration_limit=1),
                                 dict(node_limit=0), dict(iteration_limit=0),
                                 dict(time_limit_seconds=0.)])
def test_root_separation_has_no_hidden_work(caps):
    r = run(knapsack(), root_separation=True, **caps)
    if 'node_limit' in caps:
        assert r.nodes <= caps['node_limit']
    if 'iteration_limit' in caps:
        assert r.iterations <= caps['iteration_limit']
    assert r.status in {'node_limit', 'iteration_limit', 'time_limit', 'optimal'}


def test_jump_repairs_an_infeasible_rounded_root_seed():
    m = knapsack((2., 2., 2.), 3.)
    r = run(m, feasibility_jump=True, incumbent_heuristic=False)
    assert r.status == 'optimal' and r.evaluation.objective == 1.
    meta = r.backend.effective_options
    assert int(meta['diagnostics.feasibility_jump_candidates']) > 0
    assert int(meta['diagnostics.feasibility_jump_work']) <= 2000 * 512


@pytest.mark.parametrize('bad', [dict(reliability_threshold=0), dict(reliability_threshold=17),
                                dict(reliability_max_probes=1025), dict(root_cut_rounds=5),
                                dict(feasibility_jump_steps=100001)])
def test_new_effort_limits_are_validated(bad):
    with pytest.raises((ValueError, lp.BackendError)):
        run(knapsack(), **bad)


def test_search_policies_match_small_exact_binary_enumeration():
    rng = random.Random(24017)
    for case in range(30):
        n, rows = 7, 3
        weights = [[rng.randrange(1, 9) for _ in range(n)] for _ in range(rows)]
        capacities = [float(sum(w) // 3) for w in weights]
        objective = [float(rng.randrange(-2, 10)) for _ in range(n)]
        m = lp.Model(objective=objective, lower=[0.] * n, upper=[1.] * n,
                     kinds=['binary'] * n, sense='maximize', row_lower=[-float('inf')] * rows,
                     row_upper=capacities, indptr=[i * n for i in range(rows + 1)],
                     indices=list(range(n)) * rows, values=sum(weights, []))
        optimum = max(sum(c * v for c, v in zip(objective, x))
                      for x in itertools.product([0., 1.], repeat=n)
                      if all(sum(c * v for c, v in zip(w, x)) <= b
                             for w, b in zip(weights, capacities)))
        r = run(m, reliability_branching=True, feasibility_jump=True,
                root_separation=True, mixed_propagation=True)
        assert r.status == 'optimal', (case, r)
        assert r.evaluation.objective == optimum
        assert r.best_bound >= optimum - 1e-7


def test_mixed_node_propagation_proves_tightenings_without_losing_optimum():
    m = lp.Model(objective=[3., 1., 2.], lower=[0., 0., 0.], upper=[4., 4., 4.],
                 kinds=['integer', 'integer', 'continuous'], sense='maximize',
                 row_lower=[-float('inf'), -float('inf')], row_upper=[2.5, 3.5],
                 indptr=[0, 3, 6], indices=[0, 1, 2, 0, 1, 2],
                 values=[1., 0.5, 0.75, 0.5, 1., -0.25])
    a, b = run(m), run(m, mixed_propagation=True)
    assert a.status == b.status == 'optimal'
    assert b.evaluation.objective == pytest.approx(a.evaluation.objective)
    assert lp.evaluate(m, b.candidate).feasible_candidate
    meta = b.backend.effective_options
    assert int(meta['diagnostics.mixed_propagation_rows']) == 2
    assert int(meta['diagnostics.mixed_propagation_changes']) > 0
