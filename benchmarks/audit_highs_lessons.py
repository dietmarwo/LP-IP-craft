"""Validate 0.22 evidence against unchanged frozen adjudications."""
import hashlib,json,re
from pathlib import Path
root=Path(__file__).resolve().parent.parent
data=root/'docs/highs-lessons-implementation/data';old=root/'docs/performance-ten/data'
def read(p):return json.loads(p.read_text())
raw=(old/'oracle-inputs.json').read_bytes();inputs={r['case_id']:r for r in json.loads(raw)['records']};adj={r['case_id']:r for r in read(old/'oracle-adjudication.json')['records']}
out={'dataset_sha256':hashlib.sha256(raw).hexdigest(),'adjudication_sha256':hashlib.sha256((old/'oracle-adjudication.json').read_bytes()).hexdigest(),'oracle_runs':[]}
for name,count in [('default',6330),('ft',6330),('no-fix',6330),('refactor1',1200),('workers4',180),('devex',6330),('root-cuts',6330),('factor-replay',6330),('row-pricing',6330)]:
 d=read(data/f'oracle-{name}.json');assert d['dataset_sha256']==out['dataset_sha256'];assert len(d['records'])==len({r['case_id']for r in d['records']})==count
 failures=[]
 for r in d['records']:
  original=inputs[r['case_id']];a=adj.get(r['case_id'],{});expected=a.get('adjudicated_status',original['expected_status']);ref=a.get('reference',original['reference'])
  issues=[s for s in r.get('problems',[]) if s!='status mismatch']
  if r['status']!=expected:issues.append('status mismatch')
  if r['status']=='optimal' and ref is not None and abs(r['objective']-ref)>1e-6*(1+abs(ref)):issues.append('objective mismatch')
  if issues:failures.append(dict(case_id=r['case_id'],issues=issues))
 out['oracle_runs'].append(dict(file=name,cases=count,raw_flags=sum(bool(r.get('problems'))for r in d['records']),failures=failures))
out['unsafe_finite_bounds']=read(data/'bounds.json')['unsafe_finite_bounds'];assert out['unsafe_finite_bounds']==0
out['rust_checks']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(data/'final-rust-tests.txt').read_text())))
out['python_checks']=int(re.search(r'(\d+) passed',(data/'final-python-tests.txt').read_text()).group(1))
assert 'error:' not in (data/'final-clippy.txt').read_text() and 'Finished' in (data/'final-clippy.txt').read_text()
assert all(not r['failures']for r in out['oracle_runs']),out['oracle_runs']
out['oracle_cases']=sum(r['cases']for r in out['oracle_runs']);out['benchmarks']=[]
expected={'standard':(372,93),'extended':(192,48),'holdout':(144,36),'aspect':(48,12),'large':(9,3),'replay':(84,42),'batch':(15,5),'public':(120,30),'fresh':(84,21),'weight-policies':(32,8),'cut-policies':(40,10),'root-coverage':(48,12),'profiles':(24,8)}
for name,(records,summaries) in expected.items():
 p=data/f'final-{name}.json'
 if p.exists():
  d=read(p);assert len(d['records'])==records and len(d['summary'])==summaries,(name,len(d['records']),len(d.get('summary',[])))
for p in sorted(data.glob('final-*.json')):
 if p.stem in ['final-batch','final-replay','final-profiles']:continue
 d=read(p)
 if 'summary' not in d:continue
 failures=[{k:r.get(k)for k in ['backend','fixture','size','accepted','statuses','processes','errors']}for r in d['summary']if r.get('accepted')!=r.get('measured_solves',r.get('processes',3)) or r.get('process_errors',r.get('errors',0))]
 out['benchmarks'].append(dict(file=p.name,summaries=len(d['summary']),failures=failures))
 if p.name=='final-public.json':
  actual={(r['backend'],Path(r['fixture'][5:]).stem)for r in failures}
  assert actual=={('baseline','egout'),('baseline','flugpl'),('baseline','bell5'),('native','bell5')},actual
 elif p.name=='final-root-coverage.json':assert all(r['backend']=='baseline'and r['fixture']=='inferred-mip'for r in failures),failures
 elif p.name not in ['final-public.json','final-profiles.json']:assert not failures,(p.name,failures)
if (data/'final-replay.json').exists():
 r=read(data/'final-replay.json');assert len(r['records'])==84;assert all(all(run['accepted']for run in row['runs'])for row in r['records']);out['basis_replay_records']=len(r['records']);out['max_backward_error']=max(row['backward_error']for row in r['summary']);assert out['max_backward_error']<1e-9
(data/'audit.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2))
