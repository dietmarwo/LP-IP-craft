"""Iterative repair controls and independent integer-oracle contracts."""
from itertools import product
import math
import random
import pytest
from lp_ip_craft import Cancellation, NativeOptions, solve, safe_dual_bound
from test_spoc4_025 import model, run


@pytest.mark.parametrize('field,value', [
    ('packing_rounds', 0), ('packing_rounds', 33),
    ('packing_neighborhood_growth', 1025), ('packing_clique_work', 1_000_001),
    ('packing_seed_strategy', 'unknown'), ('packing_proof_seconds', -1),
    ('packing_repair_seconds', math.inf), ('packing_proof_seconds', math.nan)])
def test_new_controls_validate_before_cancel(field, value):
    token = Cancellation(); token.cancel()
    with pytest.raises((ValueError, OverflowError)):
        solve(model([1.], []), backend='native', cancellation=token,
              backend_options=NativeOptions(**{field: value}))


def test_round_product_cap():
    with pytest.raises(ValueError):
        run(model([1.], []), packing_neighborhoods=4096, packing_rounds=2)


def test_combination_rounds_and_zero_proof_budget():
    m = model([6., 5., 5., 6., 5., 5.], [[0, 1], [0, 2], [3, 4], [3, 5]])
    for combine, rounds, expected in [(False, 1, 16), (True, 1, 20), (False, 2, 20)]:
        vectors = []
        for workers in [1, 4, 16]:
            r = run(m, packing_neighborhoods=16, packing_neighborhood_size=4,
                    packing_rounds=rounds, packing_combine_repairs=combine,
                    packing_workers=workers, packing_proof_seconds=0, decompose=False)
            d = r.backend.effective_options
            assert r.status == 'time_limit'
            assert r.evaluation.feasible_candidate and r.evaluation.objective == expected
            assert int(d['diagnostics.packing_proof_nodes']) == 0
            assert int(d['diagnostics.packing_proof_pivots']) == 0
            assert int(d['diagnostics.packing_rounds']) == rounds
            assert r.best_bound >= expected
            trajectory = d['diagnostics.packing_round_trajectory'].split(';')
            scores = [float(row.split(',')[1]) for row in trajectory]
            assert scores == sorted(scores)
            vectors.append((list(r.candidate), r.nodes, r.iterations))
        assert vectors[0] == vectors[1] == vectors[2]


def test_price_diagnostics_match_safe_bound_and_default_is_quiet():
    m = model([6., 5., 5.], [[0, 1], [0, 2]])
    for expose in [False, True]:
        r = run(m, packing_root=True, packing_root_iterations=64,
                packing_price_diagnostics=expose, packing_proof_seconds=0)
        d = r.backend.effective_options
        assert ('diagnostics.packing_root_prices' in d) == expose
        if expose:
            prices = list(map(float, d['diagnostics.packing_root_prices'].split(',')))
            assert len(prices) == 2 and all(math.isfinite(y) and y >= 0 for y in prices)
            # Lattice rounding may strengthen the stored bound.
            assert r.best_bound <= safe_dual_bound(m, prices).bound + 1e-7
            assert r.best_bound >= 10


def test_all_rounds_share_node_pivot_caps_and_phase_zero_is_respected():
    m = model([9.] * 15, [[j, (j+1) % 15] for j in range(15)])
    for workers in [1, 4]:
        for repair_seconds in [None, 0.]:
            r = run(m, packing_neighborhoods=16, packing_rounds=4,
                    packing_seed_strategy='mixed', packing_combine_repairs=True,
                    packing_residual_cliques=True, packing_workers=workers,
                    packing_repair_seconds=repair_seconds,
                    packing_neighborhood_growth=500, packing_proof_seconds=.01,
                    node_limit=96, iteration_limit=120, time_limit_seconds=.1)
            d = r.backend.effective_options
            assert r.nodes <= 96 and r.iterations <= 120
            assert r.evaluation.feasible_candidate
            assert r.best_bound >= 63
            if repair_seconds == 0:
                assert int(d['diagnostics.packing_neighborhood_tasks']) == 0
            assert float(d['diagnostics.packing_proof_seconds']) < .2
            assert int(d['diagnostics.packing_max_neighborhood_size']) <= 1024


def test_new_repair_modes_against_exhaustive_oracles():
    rng = random.Random(261017)
    for case in range(80):
        n = 6 + case % 4
        c = [float(rng.randrange(1, 21)) for _ in range(n)]
        rows = [[j for j in range(n) if rng.random() < .3] for _ in range(n+2)]
        lower = [0.] * n; upper = [1.] * n
        if case % 4 == 0: upper[case % n] = 0.
        if case % 9 == 0: lower[(case+1) % n] = 1.
        feasible = [sum(a*v for a,v in zip(c,x)) for x in product([0,1],repeat=n)
                    if all(l <= v <= u for l,v,u in zip(lower,x,upper))
                    and all(sum(x[j] for j in row) <= 1 for row in rows)]
        optimum = max(feasible, default=None)
        m = model(c, rows, lower, upper)
        for strategy in ['incumbent', 'rejected', 'mixed']:
            r = run(m, packing_neighborhoods=8, packing_rounds=3,
                    packing_neighborhood_size=6, packing_neighborhood_growth=2,
                    packing_combine_repairs=True, packing_seed_strategy=strategy,
                    packing_residual_cliques=True, packing_clique_work=1000,
                    packing_workers=4, decompose=False)
            if optimum is None:
                assert r.status == 'infeasible'; continue
            assert r.status == 'optimal', (case, strategy, r)
            assert r.evaluation.feasible_candidate
            assert abs(r.evaluation.objective - optimum) < 1e-7
            assert r.best_bound is None or r.best_bound >= optimum - 1e-7
            d = r.backend.effective_options
            assert int(d['diagnostics.packing_neighborhood_errors']) == 0


def test_local_clique_cuts_execute_inside_residual_repairs():
    m = model([1.] * 4, [[0,1],[2,3],[0,2],[1,3],[0,3],[1,2]])
    r = run(m, packing_neighborhoods=4, packing_neighborhood_size=4,
            packing_residual_cliques=True, packing_proof_seconds=0)
    d = r.backend.effective_options
    assert int(d['diagnostics.packing_residual_cliques']) > 0
    assert int(d['diagnostics.packing_triangles']) == 0
    assert r.evaluation.feasible_candidate and r.evaluation.objective == 1
    assert int(d['diagnostics.packing_proof_nodes']) == 0


def test_cancellation_during_iterative_repair_retains_original_checked_start():
    import threading
    import time
    n = 30001
    m = model([10.] * n, [[j, (j+1) % n] for j in range(n)])
    token = Cancellation()
    def stop():
        time.sleep(.005)
        token.cancel()
    worker = threading.Thread(target=stop); worker.start()
    try:
        r = solve(m, backend='native', start=[0.] * n, cancellation=token,
                  backend_options=NativeOptions(packing_neighborhoods=64, packing_rounds=4,
                      packing_seed_strategy='mixed', packing_combine_repairs=True,
                      packing_residual_cliques=True, packing_workers=4, time_limit_seconds=2))
    finally:
        worker.join()
    assert r.status == 'cancelled'
    assert r.evaluation.feasible_candidate and r.evaluation.objective >= 0
