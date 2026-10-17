"""Reconstruct summaries solely from retained matched standard raw records."""
import argparse,json,statistics
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('file');a=p.parse_args();path=Path(a.file);d=json.loads(path.read_text());groups={}
for r in d['records']:
 if r['round']>0:groups.setdefault((r['backend'],r['fixture'],r['size'],r['warm'],r['worker_cap'],r['suite']),[]).append(r)
d['summary']=[]
for key,records in groups.items():
 backend,fixture,size,warm,workers,suite=key;runs=[s for r in records for s in r.get('runs',[])if s['run']>0];errors=sum('process_error'in r for r in records)
 row=dict(backend=backend,fixture=fixture,size=size,warm=warm,workers=workers,suite=suite,processes=len(records),measured_solves=len(runs),accepted=sum(s['accepted']for s in runs),process_errors=errors,statuses={s['status']:sum(t['status']==s['status']for t in runs)for s in runs})
 if runs:row.update(median_ms=1000*statistics.median(s['seconds']for s in runs),range_ms=[1000*min(s['seconds']for s in runs),1000*max(s['seconds']for s in runs)],iterations=statistics.median(s.get('iterations',0)for s in runs),nodes=statistics.median(s.get('nodes',0)for s in runs),objectives=sorted({s['checks']['objective']for s in runs if s.get('checks')}))
 d['summary'].append(row)
d['objective_mismatches']=[]
for fixture,size in {(r['fixture'],r['size'])for r in d['summary']}:
 objs=[v for r in d['summary']if r['fixture']==fixture and r['size']==size and r['accepted']==r['measured_solves']and not r['process_errors']for v in r.get('objectives',[])]
 if objs and max(objs)-min(objs)>1e-6*(1+max(map(abs,objs))):d['objective_mismatches'].append(dict(fixture=fixture,size=size,objectives=objs))
path.write_text(json.dumps(d,indent=2)+'\n');print(len(d['summary']),'summaries',len(d['objective_mismatches']),'objective mismatches')
