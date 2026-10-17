"""Identical Rust fixtures against current native and delivered HiGHS adapters."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

def configs():
    cases=[]
    legacy=['fixed','pivot','knapsack','partition','nodecopy','redundant','blocks']
    for name in legacy:
        for backend in ['native','highs']:
            cases.append((backend,name,0,True,1,3,'legacy'))
    for family,sizes in [('packing',[64,256,1024,2048,4096]),
                         ('network',[32,128,512,2048]),
                         ('irregular',[64,256,1024]),
                         ('selection',[12,20,32,64])]:
        for n in sizes:
            cases.append(('highs',family,n,True,1,1,'family'))
            for warm in ([True,False] if family=='selection' else [True]):
                cases.append(('native',family,n,warm,1,1,'family'))
    for name in ['box-lp','sparse-import-lp','transport-lp','partition-mip']:
        for backend in ['native','highs']:
            cases.append((backend,name,0,True,1,1,'import'))
    for name in ['blocks','partition-mip']:
        cases.append(('native',name,0,True,16,3 if name=='blocks' else 1,'parallel'))
    return cases + [('baseline',*c[1:]) for c in cases if c[0]=='native']

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--native',required=True)
    p.add_argument('--baseline',required=True)
    p.add_argument('--highs',required=True)
    p.add_argument('--output',required=True)
    p.add_argument('--rounds',type=int,default=5)
    a=p.parse_args()
    assert a.rounds > 0
    out=Path(a.output)
    binaries={'baseline':str(Path(a.baseline).resolve()),'native':str(Path(a.native).resolve()),'highs':str(Path(a.highs).resolve())}
    cpu=next((l.split(':',1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')),'unknown')
    result=dict(cpu=cpu,platform=platform.platform(),python=platform.python_version(),
                rounds=a.rounds,affinity=sorted(os.sched_getaffinity(0)),
                protocol='Fresh processes, rotated/reversed order, one discarded round. Each process warms one solve before 3 measured legacy solves or 1 measured family/import solve. One numerical thread; native component cap 1 except separately labelled parallel cases. Construction, fingerprint and external checks excluded. 30 s cooperative limit per solve. No exclusive-host guarantee.',
                binary_sha256={k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in binaries.items()},
                records=[])
    cases=configs()
    fingerprints={}
    for run in range(a.rounds+1):
        order=cases[run%len(cases):]+cases[:run%len(cases)]
        if run%2:order=list(reversed(order))
        for backend,name,size,warm,workers,repeats,suite in order:
            cmd=[binaries[backend],name,str(size),'warm' if warm else 'cold',str(workers),str(repeats)]
            done=subprocess.run(cmd,capture_output=True,text=True,timeout=100)
            if done.returncode:
                row=dict(backend=backend,fixture=name,size=size,warm=warm,worker_cap=workers,suite=suite,round=run,
                         process_error=done.stderr,returncode=done.returncode)
            else:
                row=json.loads(done.stdout)
                key=(name,size)
                fingerprint=row['model_fingerprint']
                if key in fingerprints:assert fingerprints[key]==fingerprint,(key,backend)
                fingerprints[key]=fingerprint
                row.update(backend=backend,suite=suite,round=run)
            result['records'].append(row)
            out.write_text(json.dumps(result,indent=2)+'\n')
            measured=row.get('runs',[{}])[-1]
            print(run,backend,name,size,'warm' if warm else 'cold',workers,measured.get('status','process_error'),round(1000*measured.get('seconds',0),3),flush=True)
        print('finished round',run,flush=True)
    result['summary']=[]
    for backend,name,size,warm,workers,repeats,suite in cases:
        records=[r for r in result['records'] if r['round']>0 and
                 (r['backend'],r['fixture'],r['size'],r['warm'],r['worker_cap'],r['suite'])==(backend,name,size,warm,workers,suite)]
        solves=[s for r in records for s in r.get('runs',[]) if s['run']>0]
        errors=[r for r in records if 'process_error' in r]
        statuses={s['status']:sum(t['status']==s['status'] for t in solves) for s in solves}
        summary=dict(backend=backend,fixture=name,size=size,warm=warm,workers=workers,suite=suite,
                     processes=len(records),measured_solves=len(solves),accepted=sum(s['accepted'] for s in solves),
                     process_errors=len(errors),statuses=statuses)
        if solves:
            summary.update(n=records[0]['n'],m=records[0]['m'],nnz=records[0]['nnz'],
                           median_ms=1000*statistics.median(s['seconds'] for s in solves),
                           range_ms=[1000*min(s['seconds'] for s in solves),1000*max(s['seconds'] for s in solves)],
                           median_rss_mib=statistics.median(r['peak_rss_kib'] for r in records)/1024,
                           nodes=statistics.median(s.get('nodes',0) for s in solves),
                           iterations=statistics.median(s.get('iterations',0) for s in solves),
                           objectives=sorted({s['checks']['objective'] for s in solves if s.get('checks')}),
                           phase_medians_ms={k:1000*statistics.median(s['phases'][k] for s in solves if 'phases' in s) for k in solves[0].get('phases',{})})
        result['summary'].append(summary)
    result['objective_mismatches']=[]
    for name,size in fingerprints:
        rows=[r for r in result['summary'] if r['fixture']==name and r['size']==size]
        objectives=[v for r in rows for v in r.get('objectives',[]) if r['accepted']==r['measured_solves'] and not r['process_errors']]
        if objectives and max(objectives)-min(objectives)>1e-6*(1+max(abs(x) for x in objectives)):
            result['objective_mismatches'].append(dict(fixture=name,size=size,objectives=objectives))
    out.write_text(json.dumps(result,indent=2)+'\n')

if __name__=='__main__':main()
