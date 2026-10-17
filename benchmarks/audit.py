import json,hashlib,re
from pathlib import Path
b=Path(__file__).resolve().parent.parent/"docs/performance-ten/data"
inputs={x['case_id']:x for x in json.loads((b/'oracle-inputs.json').read_text())['records']}
adj={x['case_id']:x for x in json.loads((b/'oracle-adjudication.json').read_text())['records']}
def issues(r):
 e=inputs[r['case_id']];a=adj.get(r['case_id'],{});expected=a.get('adjudicated_status',e['expected_status']);ref=a.get('reference',e['reference'])
 bad=[x for x in r.get('problems',[]) if x!='status mismatch']
 if r['status']!=expected:bad.append('status mismatch')
 if ref is not None and r['status']=='optimal':
  if abs(r['objective']-ref)>1e-6*(1+abs(ref)):bad.append('objective mismatch')
 return bad
out={'runs':[],'ablations':[]}
for p in sorted((b/'python-oracles').glob('*.json')):
 r=json.loads(p.read_text());bad=[{'id':s['case_id'],'issues':issues(s)} for s in r['records'] if issues(s)]
 out['runs'].append({'file':p.name,'cases':len(r['records']),'failures':bad})
for p in sorted((b/'python-ablations').glob('*.json')):
 r=json.loads(p.read_text());bad=[{'id':s['case_id'],'issues':issues(s)} for s in r['records'] if issues(s)]
 out['ablations'].append({'file':p.name,'cases':len(r['records']),'failures':bad})
out['rust_tests']=sum(int(x) for x in re.findall(r'test result: ok\. (\d+) passed',(b/'validation-logs/iteration-10-tests.log').read_text()))
(b/'audit.json').write_text(json.dumps(out,indent=2))
print({k: ([{**x,'failures':len(x['failures'])} for x in v] if isinstance(v,list) else v) for k,v in out.items()})
