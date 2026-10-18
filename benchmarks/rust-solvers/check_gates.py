"""Audit unchanged frozen adjudications and the original bound formula."""
import argparse,hashlib,importlib,json,sys
from pathlib import Path
import lp_ip_craft as lp
ROOT=Path(__file__).resolve().parents[2];DATA=ROOT/'docs/ripsolve-027/data'
p=argparse.ArgumentParser();p.add_argument('--bounds',choices=['default','combined']);a=p.parse_args()
if a.bounds:
    sys.path.insert(0,str(ROOT/'examples'))
    original=lp.solve
    def dispatch(model,**kwargs):
        options=dict(time_limit_seconds=30.,component_threads=1)
        if a.bounds=='combined':options.update(mip_presolve=True,coefficient_tightening=True,dual_presolve=True,gmi_cuts=True,mir_cuts=True,mip_cut_rounds=4,root_heuristics=True,heuristic_lp_calls=8,branching_score='product',pseudocost_min_step=.01)
        kwargs.update(backend='native',backend_options=lp.NativeOptions(**options))
        return original(model,**kwargs)
    lp.solve=dispatch
    module=importlib.import_module('bound_availability')
    rows,unsafe=module.run(100,1)
    result=dict(policy=a.bounds,models=400,rows=rows,unsafe_finite_bounds=len(unsafe),formula='unchanged examples/bound_availability.py exact Fraction Lagrangian')
    (DATA/f'bounds-{a.bounds}.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result));assert not unsafe
else:
    original=ROOT/'docs/performance-ten/data'
    raw=(original/'oracle-inputs.json').read_bytes();adjraw=(original/'oracle-adjudication.json').read_bytes()
    assert hashlib.sha256(raw).hexdigest()=='7da1d445743d20c29c70e5bbe658c017f755fcf8769214c30a519401dbc21bb9'
    assert hashlib.sha256(adjraw).hexdigest()=='1ebc3c213212af2451801a965418520bfd402867f76db46b36a6b908a3cd1564'
    inputs={r['case_id']:r for r in json.loads(raw)['records']};adj={r['case_id']:r for r in json.loads(adjraw)['records']};results=[]
    for policy in ['default','combined']:
        data=json.loads((DATA/f'oracle-{policy}.json').read_text());assert data['dataset_sha256']==hashlib.sha256(raw).hexdigest();assert len(data['records'])==6330
        failures=[]
        for r in data['records']:
            original=inputs[r['case_id']];correction=adj.get(r['case_id'],{});expected=correction.get('adjudicated_status',original['expected_status']);reference=correction.get('reference',original['reference'])
            problems=[p for p in r.get('problems',[])if p!='status mismatch']
            if r['status']!=expected:problems.append('adjudicated status mismatch')
            if reference is not None and r['status']=='optimal'and abs(r['objective']-reference)>1e-6*(1+abs(reference)):problems.append('objective mismatch')
            if problems:failures.append(dict(case=r['case_id'],status=r['status'],expected=expected,problems=problems))
        results.append(dict(policy=policy,cases=6330,failures=failures))
    (DATA/'oracle-audit.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results,indent=2));assert not any(r['failures']for r in results)
