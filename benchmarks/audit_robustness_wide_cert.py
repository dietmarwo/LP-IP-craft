"""Audit 0.21 evidence without changing frozen inputs or adjudications."""
import hashlib,json,re
from pathlib import Path
root=Path(__file__).resolve().parent.parent
data=root/'docs/robustness-wide-cert/data'
old=root/'docs/performance-ten/data'
def read(p):return json.loads(p.read_text())
raw=(old/'oracle-inputs.json').read_bytes()
inputs={r['case_id']:r for r in json.loads(raw)['records']}
adjudications={r['case_id']:r for r in read(old/'oracle-adjudication.json')['records']}
out={'oracle_runs':[],'benchmarks':[]}
for name,count in [('default',6330),('ft',6330),('no-fix',6330),('refactor1',1200),('workers4',180)]:
 d=read(data/f'oracle-{name}.json')
 assert d['dataset_sha256']==hashlib.sha256(raw).hexdigest()
 assert len(d['records'])==len({r['case_id'] for r in d['records']})==count
 failures=[]
 for r in d['records']:
  original=inputs[r['case_id']];a=adjudications.get(r['case_id'],{})
  expected=a.get('adjudicated_status',original['expected_status']);ref=a.get('reference',original['reference'])
  issues=[s for s in r.get('problems',[]) if s!='status mismatch']
  if r['status']!=expected:issues.append('status mismatch')
  if r['status']=='optimal' and ref is not None and abs(r['objective']-ref)>1e-6*(1+abs(ref)):issues.append('objective mismatch')
  if issues:failures.append(dict(case_id=r['case_id'],issues=issues))
 out['oracle_runs'].append(dict(file=name,cases=count,raw_flags=sum(bool(r.get('problems')) for r in d['records']),failures=failures))
assert all(not r['failures'] for r in out['oracle_runs']),out['oracle_runs']
for suite in ['standard','extended','holdout','previous-fresh','new-holdout','public']:
 d=read(data/f'final-{suite}.json');failures=[];retained=accepted=0
 expected_records={'standard':372,'extended':342,'holdout':144,'previous-fresh':84,'new-holdout':84,'public':120}
 expected_summaries={'standard':93,'extended':57,'holdout':36,'previous-fresh':21,'new-holdout':21,'public':30}
 assert len(d['records'])==expected_records[suite],(suite,len(d['records']))
 assert len(d['summary'])==expected_summaries[suite],(suite,len(d['summary']))
 assert not d.get('objective_mismatches',[])
 for r in d['summary']:
  count=r.get('measured_solves',r.get('processes',0));retained+=count;accepted+=r['accepted']
  if r['accepted']!=count or r.get('process_errors',r.get('errors',0)):
   failures.append({k:r[k] for k in ['backend','fixture','size','accepted','statuses']})
 if suite!='public':assert not failures,(suite,failures)
 else:
  actual={(r['backend'],Path(r['fixture'][5:]).stem) for r in failures}
  expected={(b,n) for b in ['baseline','native'] for n in ['egout','flugpl','bell5']}|{('baseline','25fv47')}
  assert actual==expected,(actual,expected)
 out['benchmarks'].append(dict(suite=suite,retained=retained,accepted=accepted,failures=failures))
large=read(data/'final-large.json');assert all(r['accepted']==3 for r in large['summary'])
out['large_retained']=sum(r['accepted'] for r in large['summary'])
replay=read(data/'final-replay.json');assert len(replay['records'])==84
assert all(all(r['accepted'] for r in d['runs']) for d in replay['records'])
out['basis_replay_records']=len(replay['records']);out['worst_backward_error']=max(r['backward_error'] for r in replay['summary'])
assert out['worst_backward_error']<1e-9
out['unsafe_finite_bounds']=read(data/'bounds.json')['unsafe_finite_bounds'];assert out['unsafe_finite_bounds']==0
out['rust_checks']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(data/'final-rust-tests.txt').read_text())))
out['python_checks']=int(re.search(r'(\d+) passed',(data/'final-python-tests.txt').read_text()).group(1))
assert 'Finished' in (data/'final-clippy.txt').read_text() and 'error:' not in (data/'final-clippy.txt').read_text()
assert (data/'dependency-tree.txt').read_text().count('\n')==2
out['exact_prepared_bound_states']=256*20
manifest=read(root/'docs/packing-mip/corpus/manifest.json')
for r in manifest:
 p=root/'docs/packing-mip/corpus'/r['name']
 assert hashlib.sha256(p.with_suffix('.json').read_bytes()).hexdigest()==r['json_sha256']
 assert hashlib.sha256(p.with_suffix('.mps').read_bytes()).hexdigest()==r['source_sha256']
out['public_corpus_files']=len(manifest)
(data/'audit.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
