"""Numerical reruns with immutable inputs/adjudications, including new policy gates."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,default=ROOT/'docs/mip-search-024/data');a=p.parse_args();out=a.output;out.mkdir(parents=True,exist_ok=True)
base=[('default',[]),('ft',['--variant','ft']),('no-fix',['--no-fixing']),('refactor1',['--variant','refactor1','--suite','exact']),('workers4',['--variant','workers4','--suite','components']),('devex',['--variant','devex']),('root-cuts',['--variant','root-cuts']),('factor-replay',['--variant','factor-replay']),('row-pricing',['--variant','row-pricing'])]
new=[('reliability',dict(reliability_branching=True)),('jump',dict(feasibility_jump=True)),('directed',dict(root_separation=True,cover_lifting=False)),('lifted',dict(root_separation=True)),('mixed',dict(mixed_propagation=True)),('combined',dict(reliability_branching=True,feasibility_jump=True,root_separation=True,mixed_propagation=True))]
for name,args in base+[(name,['--native-options',json.dumps(options)]) for name,options in new]:
 print('START',name,flush=True)
 with(out/f'oracle-{name}.log').open('w')as log:subprocess.run([sys.executable,str(ROOT/'benchmarks/oracle_worker.py'),str(ROOT/'docs/performance-ten/data/oracle-inputs.json'),'native',str(out/f'oracle-{name}.json'),*args],stdout=log,stderr=subprocess.STDOUT,check=True)
 print('DONE',name,flush=True)
with(out/'bounds.log').open('w')as log:subprocess.run([sys.executable,str(ROOT/'benchmarks/check_bounds.py'),str(ROOT),'native',str(out/'bounds.json')],stdout=log,stderr=subprocess.STDOUT,check=True)
