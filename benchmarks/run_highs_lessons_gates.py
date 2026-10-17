"""Retained 0.22 numerical gates; never edits frozen inputs/adjudications."""
import subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parent.parent
out=root/'docs/highs-lessons-implementation/data'
inputs=root/'docs/performance-ten/data/oracle-inputs.json'
cases=[('default',[]),('ft',['--variant','ft']),('no-fix',['--no-fixing']),('refactor1',['--variant','refactor1','--suite','exact']),('workers4',['--variant','workers4','--suite','components']),('devex',['--variant','devex']),('root-cuts',['--variant','root-cuts']),('factor-replay',['--variant','factor-replay']),('row-pricing',['--variant','row-pricing'])]
for name,args in cases:
 print('START',name,flush=True)
 subprocess.run([sys.executable,str(root/'benchmarks/oracle_worker.py'),str(inputs),'native',str(out/f'oracle-{name}.json'),*args],check=True,stdout=(out/f'oracle-{name}.log').open('w'))
 print('DONE',name,flush=True)
subprocess.run([sys.executable,str(root/'benchmarks/check_bounds.py'),str(root),'native',str(out/'bounds.json')],check=True,stdout=(out/'bounds.log').open('w'))
