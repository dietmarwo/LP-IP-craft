"""Fresh rotated comparison of irregular-4096; one discarded round."""
import argparse,hashlib,json,platform,statistics,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['baseline','native','highs','output']:p.add_argument('--'+name,required=True)
p.add_argument('--rounds',type=int,default=3);a=p.parse_args();assert a.rounds>0
bins={k:str(Path(getattr(a,k)).resolve()) for k in ['baseline','native','highs']}
out={'protocol':'One discarded round, then rotated/reversed fresh processes; each warms once and measures one solve. Single thread, same original model, 30-second solve limit. Uninstrumented binaries. Shared host.','rounds':a.rounds,'platform':platform.platform(),'binary_sha256':{k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in bins.items()},'records':[]};fingerprint=None
for round in range(a.rounds+1):
 labels=list(bins);labels=labels[round%3:]+labels[:round%3]
 if round%2:labels=labels[::-1]
 for label in labels:
  d=json.loads(subprocess.check_output([bins[label],'irregular','4096','warm','1','1'],timeout=100))
  if fingerprint is None:fingerprint=d['model_fingerprint']
  assert fingerprint==d['model_fingerprint']
  if round:out['records'].append(dict(backend=label,round=round,**d))
  Path(a.output).write_text(json.dumps(out,indent=2))
  print(round,label,[(r['seconds'],r['status'],r['accepted']) for r in d['runs'][1:]],flush=True)
out['summary']=[]
for label in bins:
 runs=[d['runs'][1] for d in out['records'] if d['backend']==label]
 out['summary'].append(dict(backend=label,accepted=sum(r['accepted'] for r in runs),median_seconds=statistics.median(r['seconds'] for r in runs),range_seconds=[min(r['seconds'] for r in runs),max(r['seconds'] for r in runs)],iterations=sorted({r['iterations'] for r in runs})))
Path(a.output).write_text(json.dumps(out,indent=2));print(out['summary'])
