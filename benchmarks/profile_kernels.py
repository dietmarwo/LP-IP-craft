"""Paired exclusive kernel profiles; instrumented binaries, one thread."""
import argparse, hashlib, json, statistics, subprocess
from pathlib import Path

p=argparse.ArgumentParser()
p.add_argument('--before', required=True);p.add_argument('--after', required=True)
p.add_argument('--output', required=True);p.add_argument('--rounds',type=int,default=5)
a=p.parse_args();assert a.rounds>0
binaries={k:str(Path(getattr(a,k)).resolve()) for k in ['before','after']}
cases=[(label,name,n) for name,n in [('packing',1024),('packing',4096),('irregular',1024),('network',2048)] for label in binaries]
out={'protocol':'One discarded round, rotated/reversed fresh processes, one warmup plus one retained solve per process. Exclusive instrumented times; single thread.','binary_sha256':{k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in binaries.items()},'records':[]}
hashes={}
for round in range(a.rounds+1):
 order=cases[round%len(cases):]+cases[:round%len(cases)]
 if round%2:order=order[::-1]
 for label,name,n in order:
  d=json.loads(subprocess.check_output([binaries[label],name,str(n),'warm','1','1'],timeout=100))
  key=(name,n);assert hashes.setdefault(key,d['model_fingerprint'])==d['model_fingerprint']
  assert all(r['accepted'] for r in d['runs']), (label,name,n,d)
  assert all('kernel_profile' in r for r in d['runs']), 'build both binaries with kernel-profile'
  if round:out['records'].append(dict(backend=label,round=round,**d))
 Path(a.output).write_text(json.dumps(out,indent=2))
out['summary']=[]
for label,name,n in cases:
 runs=[d['runs'][1] for d in out['records'] if (d['backend'],d['fixture'],d['size'])==(label,name,n)]
 keys=sorted({k for r in runs for k,_,_ in r['kernel_profile']})
 med={key:statistics.median(next((t for k,t,c in r['kernel_profile'] if k==key),0)/1e6 for r in runs) for key in keys}
 out['summary'].append(dict(backend=label,fixture=name,size=n,median_ms=statistics.median(r['seconds']*1000 for r in runs),exclusive_median_ms=med))
Path(a.output).write_text(json.dumps(out,indent=2));print(json.dumps(out['summary'],indent=2))
