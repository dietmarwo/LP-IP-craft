"""Replay identical frozen bases, rotated fresh processes, untimed residual checks."""
import argparse,hashlib,json,statistics,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',action='append',required=True,help='label=path');p.add_argument('--corpus',required=True);p.add_argument('--output',required=True);p.add_argument('--rounds',type=int,default=3);p.add_argument('--pivot-search',type=int,default=4);p.add_argument('--ordering',default='natural');a=p.parse_args();assert a.rounds>0
bins={k:str(Path(v).resolve()) for k,v in (s.split('=',1) for s in a.binary)}
files=sorted(Path(a.corpus).rglob('*.bin'));assert files
out={'protocol':'One discarded rotated round; fresh process with one warmup and one measured factorization/RHS set. Exclusive kernel instrumentation. Input loading and residual checks excluded.','binary_sha256':{k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in bins.items()},'input_sha256':{str(f.relative_to(a.corpus)):hashlib.sha256(f.read_bytes()).hexdigest() for f in files},'records':[]}
cases=[(k,f) for f in files for k in bins]
for round in range(a.rounds+1):
 order=cases[round%len(cases):]+cases[:round%len(cases)]
 if round%2:order=order[::-1]
 for label,f in order:
  d=json.loads(subprocess.check_output([bins[label],str(f),'1',str(a.pivot_search),a.ordering],timeout=100))
  assert all(r['accepted'] for r in d['runs']),(label,str(f),d)
  if round:out['records'].append(dict(backend=label,case=str(f.relative_to(a.corpus)),round=round,**d))
 Path(a.output).write_text(json.dumps(out,indent=2))
out['summary']=[]
for label,f in cases:
 key=str(f.relative_to(a.corpus));runs=[d['runs'][1] for d in out['records'] if d['backend']==label and d['case']==key]
 out['summary'].append(dict(backend=label,case=key,factor_ms=statistics.median(r['factor_seconds']*1000 for r in runs),solve_ms=statistics.median(r['solve_seconds']*1000 for r in runs),entries=runs[0]['entries'],dense_dimension=runs[0]['dense_dimension'],pivot_fingerprint=runs[0]['pivot_fingerprint'],backward_error=max(r['backward_error'] for r in runs)))
Path(a.output).write_text(json.dumps(out,indent=2));print(json.dumps(out['summary'],indent=2))
