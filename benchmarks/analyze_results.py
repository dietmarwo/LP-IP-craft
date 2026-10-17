"""Summarize raw data without changing any observation or acceptance threshold."""
import collections
import csv
import hashlib
import json
from pathlib import Path
import statistics

base=Path(__file__).resolve().parent.parent/"docs/performance-ten/data"
dataset=json.loads((base/'oracle-inputs.json').read_text())
inputs={r['case_id']:r for r in dataset['records']}
adjudication=json.loads((base/'oracle-adjudication.json').read_text())
adjusted={r['case_id']:r for r in adjudication['records'] if r.get('adjudicated_status')}
assert len(adjusted)==len(adjudication['records'])

def issues(row):
    item=inputs[row['case_id']]
    expected=adjusted.get(row['case_id'],{}).get('adjudicated_status',item['expected_status'])
    reference=adjusted.get(row['case_id'],{}).get('reference',item['reference'])
    bad=[p for p in row.get('problems',[]) if p!='status mismatch']
    if row['status']!=expected:bad.append('status mismatch')
    if row['status']=='optimal' and reference is not None:
        if abs(row['objective']-reference)>1e-6*(1+abs(reference)):bad.append('adjudicated reference mismatch')
        if row['best_bound'] is not None:
            gap=reference-row['best_bound'] if item['model']['sense']=='minimize' else row['best_bound']-reference
            if gap < -1e-7*(1+abs(reference)):bad.append('adjudicated bound mismatch')
    return bad

runs={backend:[json.loads((base/f'python-oracles/{i}-{backend}.json').read_text()) for i in range(3)] for backend in ['native','highs']}
dataset_hash=hashlib.sha256((base/'oracle-inputs.json').read_bytes()).hexdigest()
for passes in runs.values():
    for run in passes:assert run['dataset_sha256']==dataset_hash
summary=[]
failure_records={}
for backend,passes in runs.items():
    failure_records[backend]=[]
    for i,run in enumerate(passes):
        failures=[dict(round=i,case_id=r['case_id'],status=r['status'],problems=issues(r)) for r in run['records'] if issues(r)]
        failure_records[backend].extend(failures)
    for suite in ['random','exact','components']:
        for mip in [False,True]:
            groups=[[r for r in run['records'] if r['suite']==suite and r['mip']==mip] for run in passes]
            counts=[len(g) for g in groups]
            assert len(set(counts))==1
            if not counts[0]:continue
            bad=[sum(bool(issues(r)) for r in g) for g in groups]
            assert len(set(bad))==1
            summary.append(dict(backend=backend,suite=suite,mip=mip,cases=counts[0],failures=bad[0],
                                median_total_seconds=statistics.median(sum(r['seconds'] for r in g) for g in groups),
                                range_total_seconds=[min(sum(r['seconds'] for r in g) for g in groups),max(sum(r['seconds'] for r in g) for g in groups)],
                                nodes=statistics.median(sum(r.get('nodes',0) for r in g) for g in groups)))

common=[]
for suite in ['random','exact','components']:
    for mip in [False,True]:
        identifiers={r['case_id'] for r in runs['native'][0]['records'] if r['suite']==suite and r['mip']==mip}
        for passes in runs.values():
            for run in passes:
                identifiers &= {r['case_id'] for r in run['records'] if not issues(r)}
        if not identifiers:continue
        totals={backend:[sum(r['seconds'] for r in run['records'] if r['case_id'] in identifiers) for run in passes] for backend,passes in runs.items()}
        medians={k:statistics.median(v) for k,v in totals.items()}
        common.append(dict(suite=suite,mip=mip,common_successful_cases=len(identifiers),
                           median_total_seconds=medians,highs_over_native=medians['highs']/medians['native']))

ablations=[]
for path in sorted((base/'python-ablations').glob('*.json')):
    run=json.loads(path.read_text());failed=[dict(case_id=r['case_id'],problems=issues(r)) for r in run['records'] if issues(r)]
    ablations.append(dict(file=path.name,variant=run['variant'],cases=len(run['records']),failures=failed))

fixed=json.loads((base/'fixed-comparison.json').read_text())
assert not fixed['objective_mismatches']
assert all(s['accepted']==s['measured_solves'] and not s['process_errors'] for s in fixed['summary'])
python_import=json.loads((base/'python-import-comparison.json').read_text())
assert all(s['accepted']==s['measured_solves'] for s in python_import['summary'])
result=dict(unique_oracle_models=len(inputs),paired_passes=3,paired_solver_calls=len(inputs)*6,
            original_oracle_unresolved=sum(r['expected_status'].startswith('scipy_') for r in inputs.values()),
            adjudicated_cases=len(adjusted),dataset_sha256=dataset_hash,summary=summary,
            common_successful_timing=common,failures=failure_records,ablations=ablations,
            fixed_measured_solves=sum(s['measured_solves'] for s in fixed['summary']),
            fixed_accepted_solves=sum(s['accepted'] for s in fixed['summary']),
            python_import_measured_solves=sum(s['measured_solves'] for s in python_import['summary']))
(base/'comparison-summary.json').write_text(json.dumps(result,indent=2)+'\n')
with (base/'fixed-summary.csv').open('w',newline='') as stream:
    keys=['suite','fixture','size','n','m','nnz','backend','warm','workers','median_ms','median_rss_mib','nodes','iterations','accepted','measured_solves']
    writer=csv.DictWriter(stream,fieldnames=keys,extrasaction='ignore');writer.writeheader();writer.writerows(fixed['summary'])
print(json.dumps({k:v for k,v in result.items() if k not in ['failures','ablations']},indent=2))
