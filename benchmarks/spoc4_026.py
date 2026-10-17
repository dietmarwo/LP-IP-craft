"""Fresh-process repair ablations with independent original-resource auditing."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time
import numpy as np
import lp_ip_craft as lp
from spoc4_matching import load, greedy, candidate_check

ROOT = Path(__file__).resolve().parent / 'spoc4-026'
p = argparse.ArgumentParser()
p.add_argument('--instance', choices=['i', 'ii'], default='i')
p.add_argument('--policy', choices=['legacy', 'merge', 'repeat', 'rejected', 'mixed', 'cliques', 'grown', 'proof'], default='mixed')
p.add_argument('--seconds', type=float, default=15)
p.add_argument('--workers', type=int, default=4)
p.add_argument('--tasks', type=int)
p.add_argument('--rounds', type=int)
p.add_argument('--size', type=int, default=192)
p.add_argument('--root-work', type=int, default=200_000_000)
p.add_argument('--root-iterations', type=int, default=1000)
p.add_argument('--proof-seconds', type=float)
p.add_argument('--seed-file', type=Path)
p.add_argument('--include-zero-start', action='store_true', help='Keep zero-weight selections in the adapter start')
p.add_argument('--cold', action='store_true', help='Use the native deterministic seed instead of passing the adapter seed')
p.add_argument('--suite', action='store_true')
p.add_argument('--output', type=Path)
a = p.parse_args()
if a.suite:
    for policy in ['legacy', 'merge', 'repeat', 'rejected', 'mixed', 'cliques', 'grown', 'proof']:
        for name in ['i', 'ii']:
            out = ROOT / 'results' / f'{name}-{policy}.json'
            out.parent.mkdir(parents=True, exist_ok=True)
            print('START', name, policy, flush=True)
            command = [sys.executable, __file__, '--instance', name, '--policy', policy,
                       '--seconds', str(a.seconds), '--output', str(out), '--workers', str(a.workers),
                       '--size', str(a.size), '--root-work', str(a.root_work),
                       '--root-iterations', str(a.root_iterations)]
            for flag, value in [('--tasks', a.tasks), ('--rounds', a.rounds),
                                ('--proof-seconds', a.proof_seconds)]:
                if value is not None: command += [flag, str(value)]
            if a.cold: command += ['--cold']
            if a.include_zero_start: command += ['--include-zero-start']
            if a.seed_file: raise ValueError('--seed-file requires a single-instance run')
            with out.with_suffix('.log').open('w') as log:
                r = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=a.seconds+60)
            print('DONE', name, policy, 'exit', r.returncode, flush=True)
            if r.returncode: raise SystemExit(r.returncode)
    raise SystemExit()
source = ROOT.parent / 'spoc4-025' / 'data' / f'matching-{a.instance}.txt'
t = time.perf_counter()
triples, w, A, selected = load(source)
seed = greedy(triples, w) if a.seed_file is None else np.load(a.seed_file).astype(float)
if a.seed_file is None and not a.include_zero_start:
    seed[w == 0] = 0.
assert candidate_check(triples, w, seed)['feasible']
n = A.shape[1]; m = A.shape[0]
model = lp.Model(objective=w.astype(float), lower=np.zeros(n), upper=np.ones(n), kinds=['binary']*n,
                 sense='maximize', row_lower=np.full(m, -np.inf), row_upper=np.ones(m),
                 indptr=A.indptr, indices=A.indices, values=A.data)
rounds = a.rounds if a.rounds is not None else (1 if a.policy in ['legacy', 'merge'] else 4)
controls = dict(time_limit_seconds=a.seconds, node_limit=100000, iteration_limit=1000000,
                packing_root=True, packing_root_iterations=a.root_iterations, packing_root_work=a.root_work,
                packing_neighborhoods=64 if a.tasks is None else a.tasks, packing_rounds=rounds,
                packing_neighborhood_size=a.size, packing_workers=a.workers,
                packing_combine_repairs=a.policy != 'legacy',
                packing_seed_strategy='incumbent' if a.policy in ['legacy','merge','repeat'] else ('rejected' if a.policy=='rejected' else 'mixed'),
                packing_residual_cliques=a.policy=='cliques',
                packing_neighborhood_growth=32 if a.policy=='grown' else 0,
                packing_proof_seconds=a.proof_seconds if a.proof_seconds is not None else (2. if a.policy=='proof' else 0.),
                scalable_branching=True, component_threads=1, decompose=False)
build = time.perf_counter()-t
t = time.perf_counter()
r = lp.solve(model, backend='native', backend_options=lp.NativeOptions(**controls), start=None if a.cold else seed)
elapsed = time.perf_counter()-t
check = candidate_check(triples, w, r.candidate)
assert check and check['feasible']
assert r.best_bound is None or r.best_bound >= check['objective_milli'] - 1e-6
assert r.nodes <= controls['node_limit'] and r.iterations <= controls['iteration_limit']
d = r.backend.effective_options
assert int(d['diagnostics.packing_neighborhood_errors']) == 0
assert int(d['diagnostics.packing_clique_work']) <= int(d['diagnostics.packing_neighborhood_tasks']) * 50000
record = dict(instance=a.instance, policy=a.policy, version=lp.__version__, baseline_commit='c2eb282',
              input_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
              start_sha256=hashlib.sha256(seed.astype(np.uint8).tobytes()).hexdigest(), start_kind='native' if a.cold else ('file' if a.seed_file else ('adapter-with-zeros' if a.include_zero_start else 'greedy-positive-only')),
              n=n, m=m, nnz=A.nnz, controls=controls, status=r.status, seconds=elapsed,
              construction_seconds=build, nodes=r.nodes, pivots=r.iterations,
              objective=r.evaluation.objective/1000, bound=r.best_bound/1000 if r.best_bound is not None else None,
              candidate=check, metadata=d, platform=platform.platform(), cpu_affinity=len(os.sched_getaffinity(0)))
if a.output:
    a.output.parent.mkdir(exist_ok=True, parents=True)
    a.output.write_text(json.dumps(record, indent=2, allow_nan=False)+'\n')
    np.save(a.output.with_suffix('.solution.npy'), np.rint(r.candidate).astype(np.uint8))
print(json.dumps({k:v for k,v in record.items() if k!='metadata'}, allow_nan=False), flush=True)
