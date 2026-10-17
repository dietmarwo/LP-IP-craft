"""Fresh process comparison of auto, product-form eta and forced FT."""
import argparse, hashlib, json, statistics, subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',required=True);p.add_argument('--rounds',type=int,default=5);a=p.parse_args()
assert a.rounds>0
binary=str(Path(a.binary).resolve())
cases=[(mode,name,n) for name,n in [('packing',1024),('packing',4096),('irregular',1024),('network',2048)] for mode in ['auto','eta','ft']]
out={'protocol':'One discarded round, rotated/reversed fresh processes, one warmup plus one retained solve. Uninstrumented binary, one thread.','binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'records':[]};hashes={}
for round in range(a.rounds+1):
 order=cases[round%len(cases):]+cases[:round%len(cases)]
 if round%2:order=order[::-1]
 for mode,name,n in order:
  d=json.loads(subprocess.check_output([binary,name,str(n),'warm','1','1',mode],timeout=100))
  assert hashes.setdefault((name,n),d['model_fingerprint'])==d['model_fingerprint']
  if round:out['records'].append(dict(mode=mode,round=round,**d))
 Path(a.output).write_text(json.dumps(out,indent=2))
out['summary']=[]
for mode,name,n in cases:
 runs=[d['runs'][1] for d in out['records'] if (d['mode'],d['fixture'],d['size'])==(mode,name,n)]
 out['summary'].append(dict(mode=mode,fixture=name,size=n,accepted=sum(r['accepted'] for r in runs),median_ms=statistics.median(r['seconds']*1000 for r in runs),range_ms=[min(r['seconds']*1000 for r in runs),max(r['seconds']*1000 for r in runs)]))
Path(a.output).write_text(json.dumps(out,indent=2));print(json.dumps(out['summary'],indent=2))
