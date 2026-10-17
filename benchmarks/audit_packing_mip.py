"""Recheck frozen evidence; known public limitations remain explicit failures."""
import hashlib,json,re
from pathlib import Path
root=Path(__file__).resolve().parent.parent;data=root/'docs/packing-mip/data';old=root/'docs/performance-ten/data'
def read(p):return json.loads(p.read_text())
raw=(old/'oracle-inputs.json').read_bytes();inputs={r['case_id']:r for r in json.loads(raw)['records']}
adj={r['case_id']:r for r in read(old/'oracle-adjudication.json')['records']}
out={'oracle_runs':[],'benchmarks':[]}
for p in sorted(data.glob('oracle-*.json')):
    d=read(p);assert d['dataset_sha256']==hashlib.sha256(raw).hexdigest();bad=[]
    expected_count=1200 if d['variant']=='refactor1' else 180 if d['variant']=='workers4' else 6330
    assert len(d['records'])==expected_count
    assert len({r['case_id'] for r in d['records']})==expected_count
    for r in d['records']:
        original=inputs[r['case_id']];a=adj.get(r['case_id'],{});status=a.get('adjudicated_status',original['expected_status']);ref=a.get('reference',original['reference'])
        issues=[s for s in r.get('problems',[]) if s!='status mismatch']
        if r['status']!=status:issues.append('status mismatch')
        if ref is not None and r['status']=='optimal' and abs(r['objective']-ref)>1e-6*(1+abs(ref)):issues.append('objective mismatch')
        if issues:bad.append({'case_id':r['case_id'],'issues':issues})
    out['oracle_runs'].append({'file':p.name,'cases':len(d['records']),'raw_flags':sum(bool(r.get('problems')) for r in d['records']),'failures':bad})
assert len(out['oracle_runs'])==5
assert all(not r['failures'] for r in out['oracle_runs']),out['oracle_runs']
for name in ['standard','extended','holdout','fresh-holdout','public']:
    d=read(data/f'final-{name}.json');fail=[];total=accepted=0
    if name=='standard':assert not d['objective_mismatches']
    for r in d['summary']:
        count=r.get('measured_solves',r.get('processes',0));total+=count;accepted+=r['accepted']
        if r['accepted']!=count or r.get('process_errors',r.get('errors',0)):
            fail.append({k:r[k] for k in ['backend','fixture','size','accepted','statuses']})
    if name!='public':assert not fail,(name,fail)
    else:
        assert len(fail)==8,fail
        for r in fail:
            assert r['backend'] in ['native','baseline']
            assert Path(r['fixture'][5:]).stem in ['25fv47','egout','flugpl','bell5']
    out['benchmarks'].append({'suite':name,'retained':total,'accepted':accepted,'failures':fail})
large=read(data/'final-large.json');assert all(r['accepted']==3 for r in large['summary'])
out['large_retained']=sum(r['accepted'] for r in large['summary'])
replay=read(data/'final-replay.json');assert len(replay['records'])==84
assert all(all(r['accepted'] for r in d['runs']) for d in replay['records'])
out['basis_replay_records']=len(replay['records']);out['worst_backward_error']=max(r['backward_error'] for r in replay['summary'])
assert out['worst_backward_error']<1e-9
out['unsafe_finite_bounds']=read(data/'bounds.json')['unsafe_finite_bounds'];assert out['unsafe_finite_bounds']==0
out['rust_checks']=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(data/'final-rust-tests.txt').read_text())))
out['python_checks']=int(re.search(r'(\d+) passed',(data/'final-python-tests.txt').read_text()).group(1))
manifest=read(root/'docs/packing-mip/corpus/manifest.json')
for r in manifest:
    p=root/'docs/packing-mip/corpus'/r['name']
    assert hashlib.sha256(p.with_suffix('.json').read_bytes()).hexdigest()==r['json_sha256']
    assert hashlib.sha256(p.with_suffix('.mps').read_bytes()).hexdigest()==r['source_sha256']
out['public_corpus_files']=len(manifest)
(data/'audit.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2))
