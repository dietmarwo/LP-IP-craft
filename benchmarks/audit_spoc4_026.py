"""Independent original-ID solution and exact dyadic reference audit."""
import hashlib
import json
from pathlib import Path
import numpy as np
from spoc4_matching import load, greedy, candidate_check

BASE = Path(__file__).resolve().parent
ROOT = BASE / 'spoc4-026'
data = {name: load(BASE / 'spoc4-025' / 'data' / f'matching-{name}.txt') for name in ['i','ii']}
checks = []
for directory in ['results', 'results-adapter']:
    for p in sorted((ROOT / directory).glob('*.json')):
        r = json.loads(p.read_text()); c = r['controls']; d = r['metadata']
        triples, weights, A, _ = data[r['instance']]
        source = BASE / 'spoc4-025' / 'data' / f"matching-{r['instance']}.txt"
        assert r['input_sha256'] == hashlib.sha256(source.read_bytes()).hexdigest()
        x = np.load(p.with_suffix('.solution.npy'))
        assert x.dtype == np.uint8 and len(x) == len(weights)
        checked = candidate_check(triples, weights, x)
        assert checked['feasible'] and checked == r['candidate'], p
        assert checked['objective_milli'] == round(1000*r['objective'])
        assert r['bound'] is None or r['bound']*1000 >= checked['objective_milli']-1e-6
        assert r['nodes'] <= c['node_limit'] and r['pivots'] <= c['iteration_limit']
        assert int(d['diagnostics.packing_root_work']) <= c['packing_root_work']
        assert int(d['diagnostics.packing_neighborhood_tasks']) <= c['packing_neighborhoods']*c['packing_rounds']
        assert int(d['diagnostics.packing_rounds']) <= c['packing_rounds']
        assert int(d['diagnostics.packing_neighborhood_errors']) == 0
        assert int(d['diagnostics.packing_clique_work']) <= int(d['diagnostics.packing_neighborhood_tasks'])*50000
        if c['packing_proof_seconds'] == 0:
            assert int(d['diagnostics.packing_proof_nodes']) == int(d['diagnostics.packing_proof_pivots']) == 0
        trajectory = [line.split(',') for line in d['diagnostics.packing_round_trajectory'].split(';') if line]
        values = [float(line[1]) for line in trajectory]
        assert values == sorted(values)
        if values:
            assert values[-1] == float(d['diagnostics.packing_neighborhood_objective'])
            assert checked['objective_milli'] >= values[-1]
        checks.append(dict(file=f'{directory}/{p.name}', objective_milli=checked['objective_milli'],
                           selected=checked['selected'], solution_sha256=hashlib.sha256(x.tobytes()).hexdigest()))
references = []
for p in sorted((ROOT / 'reference').glob('*.json')):
    r = json.loads(p.read_text())
    cert = r.get('exact_packing_upper_bound')
    if cert is None:
        references.append(dict(file=p.name, status=r['status'], checked_bound=None))
        continue
    _, weights, A, _ = data['ii']
    assert r['input_sha256'] == hashlib.sha256((BASE/'spoc4-025/data/matching-ii.txt').read_bytes()).hexdigest()
    prices = np.load(p.with_suffix('.dual.npy'))
    assert len(prices) == A.shape[0] and prices.dtype == np.int64 and np.all(prices >= 0)
    activity = [0]*len(weights)
    for i in range(A.shape[0]):
        for k in range(A.indptr[i], A.indptr[i+1]):
            activity[A.indices[k]] += int(prices[i])
    denom = cert['denominator']
    numerator = sum(map(int,prices)) + sum(max(0, int(w)*denom-v) for w,v in zip(weights,activity))
    assert numerator == cert['numerator']
    assert numerator//denom == cert['objective_upper_milli']
    references.append(dict(file=p.name, status=r['status'], checked_bound=cert['objective_upper']))
parallel = []
for name in ['i','ii']:
    a = ROOT/f'results/{name}-rejected.json'; b = ROOT/f'results/{name}-rejected-1w.json'
    if a.exists() and b.exists():
        x,y = json.loads(a.read_text()),json.loads(b.read_text())
        assert np.array_equal(np.load(a.with_suffix('.solution.npy')),np.load(b.with_suffix('.solution.npy')))
        assert (x['nodes'],x['pivots']) == (y['nodes'],y['pivots'])
        parallel.append(dict(instance=name, workers=[1,4], same_vector_and_work=True))
a = ROOT/'results/i-legacy.json'; b = ROOT/'results/i-legacy-cold.json'
if a.exists() and b.exists():
    x,y = json.loads(a.read_text()),json.loads(b.read_text())
    assert np.array_equal(np.load(a.with_suffix('.solution.npy')),np.load(b.with_suffix('.solution.npy')))
    assert (x['nodes'],x['pivots']) == (y['nodes'],y['pivots'])
result = dict(solutions=len(checks), failures=0, records=checks, references=references, parallel=parallel)
(ROOT/'AUDIT.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='records'},indent=2))
