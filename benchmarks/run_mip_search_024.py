"""Serial fresh-process policy comparisons; no solver timing overlaps builds/tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
POLICIES = {
    'baseline-023': {},
    'legacy-policy': {'LPIP_RATIONAL_CERTIFICATION': 'off'},
    'certification': {},
    'reliability': {'LPIP_RELIABILITY': '1'},
    'jump': {'LPIP_FEASIBILITY_JUMP': '1'},
    'directed-covers': {'LPIP_ROOT_SEPARATION': '1', 'LPIP_COVER_LIFTING': 'off'},
    'lifted-covers': {'LPIP_ROOT_SEPARATION': '1'},
    'mixed-propagation': {'LPIP_MIXED_PROPAGATION': '1'},
    'combined': {'LPIP_RELIABILITY': '1', 'LPIP_FEASIBILITY_JUMP': '1',
                 'LPIP_ROOT_SEPARATION': '1', 'LPIP_MIXED_PROPAGATION': '1'},
}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--native', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--rounds', type=int, default=1)
    p.add_argument('--resume', action='store_true')
    p.add_argument('--cases', nargs='+', default=['bell5', 'egout', 'flugpl', 'gt2', 'lseu', 'p0548', 'rgn', 'p01'])
    p.add_argument('--policies', nargs='+', default=list(POLICIES))
    p.add_argument('--time-limit', type=float, default=30.)
    p.add_argument('--jump-steps', type=int)
    a = p.parse_args()
    records = json.loads(a.output.read_text())['records'] if a.resume and a.output.exists() else []
    fingerprints = {r['case']: r['result']['model_fingerprint'] for r in records}
    completed = {(r['case'],r['policy'],r['round']) for r in records}
    hashes = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k,v in [('baseline',a.baseline),('native',a.native)]}
    for round_index in range(a.rounds):
        for case in a.cases:
            fixture = next(ROOT.glob('docs/*/corpus/**/' + case + '.json'))
            policies = a.policies[round_index % len(a.policies):] + a.policies[:round_index % len(a.policies)]
            for policy in policies:
                if (case,policy,round_index+1) in completed: continue
                binary = a.baseline if policy == 'baseline-023' else a.native
                env = {k:v for k,v in os.environ.items() if not k.startswith('LPIP_')}
                env.update(LPIP_BENCH_NO_DECOMPOSE='1', LPIP_MIP_DIAGNOSTICS='1',
                           LPIP_MIP_TRACE_LIMIT='32', LPIP_TIME_LIMIT=str(a.time_limit),
                           LPIP_NODE_LIMIT='100000', LPIP_ITERATION_LIMIT='1000000')
                env.update(POLICIES[policy])
                if a.jump_steps is not None and policy in ['jump','combined']:
                    env['LPIP_JUMP_STEPS'] = str(a.jump_steps)
                # One warmup and two measured solves use distinct solver instances in one fresh process.
                cmd = [str(binary), 'json=' + str(fixture), '0', 'warm', '1', '2']
                began = time.time()
                proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=2*a.time_limit+30)
                if proc.returncode:
                    raise RuntimeError((case,policy,proc.stderr[-2000:]))
                result = json.loads(proc.stdout)
                old = fingerprints.setdefault(case, result['model_fingerprint'])
                assert old == result['model_fingerprint']
                record = dict(case=case, policy=policy, round=round_index+1, started=began,
                              input_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(), result=result)
                records.append(record)
                a.output.parent.mkdir(parents=True, exist_ok=True)
                a.output.write_text(json.dumps(dict(protocol='serial fresh processes, first solve warmup, next two measured; fixed caps; policy order rotated across rounds',binary_sha256=hashes,time_limit=a.time_limit,jump_steps=a.jump_steps,records=records), indent=2)+'\n')
                r = result['runs'][-1]
                print(json.dumps(dict(case=case,policy=policy,round=round_index+1,status=r['status'],nodes=r.get('nodes'),pivots=r.get('iterations'),objective=(r.get('checks') or {}).get('objective'),ms=round(r['seconds']*1000,3))), flush=True)


if __name__ == '__main__': main()
