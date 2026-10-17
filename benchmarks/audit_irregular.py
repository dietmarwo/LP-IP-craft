"""Re-audit saved oracle/replay/benchmark evidence without rerunning solvers."""
import hashlib,json,re
from pathlib import Path
root=Path(__file__).resolve().parent.parent;data=root/'docs/irregular-lu/data';old=root/'docs/performance-ten/data'
raw=(old/'oracle-inputs.json').read_bytes();inputs={r['case_id']:r for r in json.loads(raw)['records']}
adj={r['case_id']:r for r in json.loads((old/'oracle-adjudication.json').read_text())['records']}
out={'oracle_runs':[]}
for p in sorted(data.glob('oracle-*.json')):
 if p.name=='oracle-audit.json':continue
 d=json.loads(p.read_text());assert d['dataset_sha256']==hashlib.sha256(raw).hexdigest();bad=[]
 for r in d['records']:
  e=inputs[r['case_id']];a=adj.get(r['case_id'],{});expected=a.get('adjudicated_status',e['expected_status']);ref=a.get('reference',e['reference'])
  issues=[s for s in r.get('problems',[]) if s!='status mismatch']
  if r['status']!=expected:issues.append('status mismatch')
  if ref is not None and r['status']=='optimal' and abs(r['objective']-ref)>1e-6*(1+abs(ref)):issues.append('objective mismatch')
  if issues:bad.append(dict(case_id=r['case_id'],issues=issues))
 out['oracle_runs'].append(dict(file=p.name,cases=len(d['records']),raw_flags=sum(bool(r.get('problems')) for r in d['records']),failures=bad))
out['rust_checks']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(data/'final-rust-tests.txt').read_text())))
out['unsafe_finite_bounds']=json.loads((data/'bounds.json').read_text())['unsafe_finite_bounds']
out['benchmark_retained_solves']=sum(s['accepted'] for s in json.loads((data/'final-comparison.json').read_text())['summary'])
out['large_benchmark_accepted']=sum(s['accepted'] for s in json.loads((data/'final-large.json').read_text())['summary'])
(data/'audit.json').write_text(json.dumps(out,indent=2));print(json.dumps(out,indent=2))
assert all(not r['failures'] for r in out['oracle_runs']) and out['unsafe_finite_bounds']==0
