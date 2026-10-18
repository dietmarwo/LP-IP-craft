"""Pinned ripsolve option/cut-family ablations and wrong-status reproductions."""
import argparse,json
from pathlib import Path
from run import ROOT,BASE,run_one
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--seconds',type=float,default=5);p.add_argument('--output',type=Path,required=True);p.add_argument('--cases',nargs='+',default=['p0548','egout','gt2','lseu']);p.add_argument('--bug-only',action='store_true');a=p.parse_args()
records=[]
for name in (['lseu','bell5','rgn']if a.bug_only else a.cases):
 path=next(ROOT.glob('docs/*/corpus/**/'+name+'.json'))
 for policy,off in ([('no-restart',None)]if a.bug_only else [('default',None),('no-presolve',None),('no-cuts',None),('bare',None),('default','gomory'),('default','mir')]):
  r=run_one('ripsolve',path,a.seconds,policy,a.binary,off=off);records.append(r)
  a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(dict(reference='ripsolve 0.4.0 published source + documented env-only family exits',seconds=a.seconds,records=records),indent=2)+'\n')
  print(name,policy,off,r.get('result',{}).get('status'),r.get('result',{}).get('objective'),r.get('issues'),flush=True)
