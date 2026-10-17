import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys

p=argparse.ArgumentParser();p.add_argument('directory');p.add_argument('--mode',choices=['imports','oracles','ablations'],required=True)
p.add_argument('--native-python',default=sys.executable)
p.add_argument('--highs-python',default=sys.executable)
p.add_argument('--extra-pythonpath',default='')
a=p.parse_args()
base=Path(a.directory).resolve()
interpreters={'native':a.native_python,'highs':a.highs_python}
def environment(backend):
    env=dict(os.environ)
    paths=[]
    if (base/(backend+'-python')).is_dir():paths.append(str(base/(backend+'-python')))
    if a.extra_pythonpath:paths.append(a.extra_pythonpath)
    if paths:env['PYTHONPATH']=os.pathsep.join(paths)
    return env
raw=base/('python-'+a.mode);raw.mkdir(exist_ok=True)
records=[]
if a.mode=='imports':
    fixtures=base/'highs-0.4.0/LP-IP-craft/examples/benchmark_efficiency.py'
    cases=[(backend,name) for name in ['box-lp','sparse-import-lp','transport-lp','partition-mip'] for backend in ['native','highs']]
    for run in range(6):
        order=cases[run%len(cases):]+cases[:run%len(cases)]
        if run%2:order=list(reversed(order))
        for backend,name in order:
            dest=raw/f'{run}-{backend}-{name}.json'
            env=environment(backend)
            subprocess.run([interpreters[backend],str(base/'python_import_worker.py'),str(fixtures),backend,name,str(dest)],env=env,check=True,timeout=100)
            row=json.loads(dest.read_text());row['round']=run;records.append(row)
    summary=[]
    for backend,name in cases:
        group=[r for r in records if r['round']>0 and r['backend']==backend and r['case']==name]
        solves=[r['runs'][-1] for r in group]
        summary.append(dict(backend=backend,case=name,n=group[0]['n'],m=group[0]['m'],
                            measured_solves=len(solves),accepted=sum(s['accepted'] for s in solves),
                            wall_median_ms=1000*statistics.median(s['wall_seconds'] for s in solves),
                            wall_range_ms=[1000*min(s['wall_seconds'] for s in solves),1000*max(s['wall_seconds'] for s in solves)],
                            phase_median_ms={k:1000*statistics.median(s['phases'][k] for s in solves) for k in solves[0]['phases']}))
    for name in ['box-lp','sparse-import-lp','transport-lp','partition-mip']:
        assert len({r['model_sha256'] for r in records if r['case']==name})==1
    (base/'python-import-comparison.json').write_text(json.dumps(dict(method='same original Python fixture generator; fresh alternating processes, one discarded round, one in-process warmup plus one measurement, five retained rounds; model construction excluded; native component cap 1',records=records,summary=summary),indent=2)+'\n')
elif a.mode=='oracles':
    for run in range(3):
        for backend in (['native','highs'] if run%2==0 else ['highs','native']):
            dest=raw/f'{run}-{backend}.json'
            env=environment(backend)
            subprocess.run([interpreters[backend],str(base/'oracle_worker.py'),str(base/'oracle-inputs.json'),backend,str(dest)],env=env,check=True,timeout=600)
            r=json.loads(dest.read_text());records.append(dict(round=run,backend=backend,summary=r['summary'],failures=[s for s in r['records'] if s.get('problems')]))
    (base/'python-oracle-comparison.json').write_text(json.dumps(dict(method='three alternating complete passes of identical frozen inputs; Python solve wall time; construction, oracle and external checking excluded; one untimed batch warmup; native component cap 1',runs=records),indent=2)+'\n')
else:
    configurations=[('exact',None,'dantzig'),('exact',None,'refactor1'),('random',2,'refactor8'),('random',9,'dantzig'),('components',None,'workers4')]
    env=environment('native')
    for suite,seed,variant in configurations:
        dest=raw/f'{suite}-{seed}-{variant}.json'
        cmd=[interpreters['native'],str(base/'oracle_worker.py'),str(base/'oracle-inputs.json'),'native',str(dest),'--suite',suite,'--variant',variant]
        if seed is not None:cmd+=['--seed',str(seed)]
        subprocess.run(cmd,env=env,check=True,timeout=600)
        r=json.loads(dest.read_text());records.append(dict(suite=suite,seed=seed,variant=variant,summary=r['summary'],failures=[s for s in r['records'] if s.get('problems')]))
    (base/'python-ablations.json').write_text(json.dumps(records,indent=2)+'\n')
