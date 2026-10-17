"""Exploratory large irregular instance; one retained solve, not a median."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['baseline','native','highs','output']:p.add_argument('--'+key,required=True)
a=p.parse_args()
binaries={k:str(Path(getattr(a,k)).resolve()) for k in ['baseline','highs','native']}
out={'protocol':'One warmup and one retained irregular-4096 solve per backend. Exploratory only, same 30-second limit.','binary_sha256':{k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in binaries.items()},'records':[]}
for label,binary in binaries.items():
 d=json.loads(subprocess.check_output([binary,'irregular','4096','warm','1','1'],timeout=100))
 out['records'].append(dict(backend=label,**d));Path(a.output).write_text(json.dumps(out,indent=2))
 print(label,[(r['seconds'],r['status'],r['accepted']) for r in d['runs']],flush=True)
