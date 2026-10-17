"""Matched fresh-process comparison; every failure remains in the output."""
import argparse,hashlib,json,statistics,subprocess,platform,os
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',action='append',required=True,help='label=/absolute/path')
p.add_argument('--output',required=True);p.add_argument('--rounds',type=int,default=5)
p.add_argument('--case',action='append',help='fixture:size');p.add_argument('--public-corpus',type=Path);p.add_argument('--holdout',action='store_true');a=p.parse_args()
bins=dict(s.split('=',1) for s in a.binary)
cases=[s.split(':') for s in (a.case or ['packing:1024','packing:2048','packing:4096','packing-wide:256','packing-wide:1024','packing-permuted:1024','network:512','irregular:256','irregular:1024','nodecopy:0','selection:20','selection:32','long-row-mip:64','integer-knapsack:32','multi-knapsack:18','graph-mip:18'])]
if a.public_corpus:
    cases=[['json='+str(f.resolve()),'0'] for f in sorted(a.public_corpus.glob('*.json')) if f.name!='manifest.json']
if a.holdout:
    cases=[['multi-knapsack',str(s)] for s in [1018,2018,3018,4018]]+[['graph-mip',str(s)] for s in [1018,2018,3018]]+[['mixed-overload',str(s)] for s in [1016,2016,3016]]+[['integer-knapsack','48'],['long-row-mip','96']]
assert a.rounds>0
out={'platform':platform.platform(),'affinity':sorted(os.sched_getaffinity(0)),'cpu':next((l.split(':',1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')),'unknown'),'protocol':'One discarded round; rotated/reversed fresh processes; one warmup and one measured solve; single thread; same 30s time, 100000 nodes and 1000000 simplex iterations. External checks excluded. No exclusive host.', 'binary_sha256':{k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in bins.items()},'records':[]}
order=[(k,n,int(s)) for n,s in cases for k in bins];fingerprints={}
for rnd in range(a.rounds+1):
    todo=order[rnd%len(order):]+order[:rnd%len(order)]
    if rnd%2:todo=todo[::-1]
    for backend,name,size in todo:
        try:
            done=subprocess.run([bins[backend],name,str(size),'warm','1','1'],capture_output=True,text=True,timeout=90)
            if done.returncode:raise RuntimeError(done.stderr)
            d=json.loads(done.stdout);key=(name,size)
            assert fingerprints.setdefault(key,d['model_fingerprint'])==d['model_fingerprint']
            row=dict(backend=backend,round=rnd,**d)
        except (subprocess.TimeoutExpired,RuntimeError,json.JSONDecodeError) as e:
            row=dict(backend=backend,round=rnd,fixture=name,size=size,error=str(e))
        out['records'].append(row);Path(a.output).write_text(json.dumps(out,indent=2)+'\n')
        r=row.get('runs',[{}])[-1];print(rnd,backend,name,size,r.get('status','error'),round(1000*r.get('seconds',0),3),r.get('nodes'),r.get('accepted'),flush=True)
out['summary']=[]
for backend,name,size in order:
    rows=[r for r in out['records'] if r['round']>0 and (r['backend'],r['fixture'],r['size'])==(backend,name,size)]
    runs=[r['runs'][1] for r in rows if 'runs' in r]
    d=dict(backend=backend,fixture=name,size=size,processes=len(rows),errors=sum('error' in r for r in rows),accepted=sum(r['accepted'] for r in runs),statuses=[r['status'] for r in runs])
    if runs:d.update(median_ms=statistics.median(r['seconds']*1000 for r in runs),range_ms=[min(r['seconds']*1000 for r in runs),max(r['seconds']*1000 for r in runs)],nodes=statistics.median(r['nodes'] for r in runs if 'nodes' in r) if any('nodes' in r for r in runs) else None,iterations=statistics.median(r['iterations'] for r in runs if 'iterations' in r) if any('iterations' in r for r in runs) else None,objectives=[r['checks'].get('objective') for r in runs if r.get('checks')],rss_mib=statistics.median(r['peak_rss_kib']/1024 for r in rows if 'runs' in r))
    out['summary'].append(d)
for name,size in cases:
    objs=[v for r in out['summary'] if r['fixture']==name and r['size']==int(size) and r['accepted']==a.rounds for v in r.get('objectives',[])]
    assert not objs or max(objs)-min(objs)<=1e-6*(1+max(map(abs,objs))), (name,size,objs)
Path(a.output).write_text(json.dumps(out,indent=2)+'\n')
