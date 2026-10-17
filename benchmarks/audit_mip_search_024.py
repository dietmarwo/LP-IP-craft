"""Audit fresh numerical observations against unchanged frozen adjudications."""
import hashlib
import json
from pathlib import Path
import re
ROOT=Path(__file__).resolve().parent.parent
DATA=ROOT/'docs/mip-search-024/data'
old=ROOT/'docs/performance-ten/data'

def read(p):return json.loads(p.read_text())
inputs_raw=(old/'oracle-inputs.json').read_bytes();adj_raw=(old/'oracle-adjudication.json').read_bytes()
assert hashlib.sha256(inputs_raw).hexdigest()=='7da1d445743d20c29c70e5bbe658c017f755fcf8769214c30a519401dbc21bb9'
assert hashlib.sha256(adj_raw).hexdigest()=='1ebc3c213212af2451801a965418520bfd402867f76db46b36a6b908a3cd1564'
inputs={r['case_id']:r for r in json.loads(inputs_raw)['records']};adj={r['case_id']:r for r in json.loads(adj_raw)['records']}
result=dict(input_sha256=hashlib.sha256(inputs_raw).hexdigest(),adjudication_sha256=hashlib.sha256(adj_raw).hexdigest(),oracle_runs=[])
counts={name:6330 for name in ['default','ft','no-fix','devex','root-cuts','factor-replay','row-pricing','reliability','jump','directed','lifted','mixed','combined']};counts.update(refactor1=1200,workers4=180)
for name,count in counts.items():
 d=read(DATA/f'oracle-{name}.json');assert d['wrapper_version']=='0.24.0';assert d['dataset_sha256']==result['input_sha256'];assert len(d['records'])==len({r['case_id']for r in d['records']})==count
 failures=[]
 for r in d['records']:
  original=inputs[r['case_id']];a=adj.get(r['case_id'],{});expected=a.get('adjudicated_status',original['expected_status']);reference=a.get('reference',original['reference']);issues=[v for v in r.get('problems',[])if v!='status mismatch']
  if r['status']!=expected:issues.append('status mismatch')
  if r['status']=='optimal'and reference is not None and abs(r['objective']-reference)>1e-6*(1+abs(reference)):issues.append('objective mismatch')
  if issues:failures.append(dict(case_id=r['case_id'],issues=issues))
 result['oracle_runs'].append(dict(policy=name,cases=count,raw_flags=sum(bool(r.get('problems'))for r in d['records']),failures=failures))
result['oracle_cases']=sum(r['cases']for r in result['oracle_runs'])
result['unsafe_finite_bounds']=read(DATA/'bounds.json')['unsafe_finite_bounds']
result['rust_checks']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(DATA/'final-rust-tests.log').read_text())))
result['native_unit_recheck_count']=int(re.search(r'test result: ok\. (\d+) passed',(DATA/'final-arithmetic-tests.log').read_text()).group(1))
result['python_checks']=int(re.search(r'(\d+) passed',(DATA/'final-python-tests.log').read_text()).group(1))
assert 'error:'not in(DATA/'final-clippy.log').read_text()
result['public_observations']=[]
for file in ['public-policies.json','public-repeat.json']:
 p=DATA/file
 if not p.exists():continue
 d=read(p)
 for row in d['records']:
  for r in row['result']['runs']:
   assert r['status']in['optimal','node_limit','iteration_limit','time_limit'],(file,row['case'],row['policy'],r['status'],r.get('message'))
   assert r['nodes']<=100000 and r['iterations']<=1000000
   if (r.get('checks') or {}).get('objective')is not None:assert r['checks']['feasible'],(file,row['case'],row['policy'])
   expected=row['result']['expected'];meta=dict(r['effective_options']);trace=json.loads(meta['diagnostics.mip_search'])
   assert r['iterations']==trace['terminal']['pivots']+int(meta.get('root_separation_pivots',0))
   assert r['nodes']==trace['terminal']['nodes']+int(meta.get('root_separation_nodes',0))
   if expected is not None:
    assert r['best_bound']is None or r['best_bound']<=expected+1e-6*(1+abs(expected)),(row['case'],row['policy'],'unsafe bound')
    proof=trace['terminal']['certified_incumbent_bound']
    assert proof is None or proof>=expected-1e-6*(1+abs(expected)),(row['case'],row['policy'],'unsafe incumbent')
  result['public_observations'].append(dict(file=file,case=row['case'],policy=row['policy'],round=row['round']))
lp=read(DATA/'lp-controls.json')
assert len(lp['summary'])==6
assert all(row['accepted']==5 and not row['errors']for row in lp['summary'])
result['lp_controls']=lp['summary']
result['public_process_count']=len(result['public_observations'])
result['public_solver_runs']=3*result['public_process_count']
result['legacy_work_equivalence']=[]
public_file=DATA/'public-policies.json'
if public_file.exists():
 rows=read(public_file)['records']
 for case in sorted({r['case']for r in rows}):
  sides={r['policy']:r['result']['runs'][-1]for r in rows if r['case']==case and r['policy']in['baseline-023','legacy-policy']}
  if len(sides)!=2:continue
  a,b=sides['baseline-023'],sides['legacy-policy'];limited='time_limit'in[a['status'],b['status']]
  same=all(a.get(k)==b.get(k)for k in ['status','nodes','iterations','best_bound'])and(a.get('checks')or{}).get('objective')==(b.get('checks')or{}).get('objective')
  result['legacy_work_equivalence'].append(dict(case=case,equal=same,wall_limited=limited))
  assert same or limited,(case,'legacy policy changed search work')
(DATA/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
assert not result['unsafe_finite_bounds']
assert all(not r['failures']for r in result['oracle_runs']),result['oracle_runs']
