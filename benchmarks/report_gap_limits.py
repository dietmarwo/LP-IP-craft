"""Validate gap evidence and generate node/time tables without removing failures."""
import json, math
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]/'docs/benchmarks/gap-limits'
def records(file):return [r for r in map(json.loads,(ROOT/file).open())if r.get('event')=='result']
rows=records('results.jsonl');lookup={(r['case'],r['variant']):r for r in rows}
errors=[]
for r in rows:
 if r['status']=='harness_error':errors.append((r['case'],r['variant'],'harness error'));continue
 if r['status']=='gap_limit':
  if r.get('feasible') is not True or not (0<=r['relative_gap']<=float(r['variant'])):errors.append((r['case'],r['variant'],'invalid gap stop'))
 if r.get('expected') is not None and r['status']=='optimal':
  if not math.isclose(r['objective'],r['expected'],abs_tol=1e-6,rel_tol=1e-8):errors.append((r['case'],r['variant'],'false optimum'))
def numerical(r):return {k:v for k,v in r.items()if k not in ['seconds','version']}
a=records('baseline-fixed-nodes.jsonl');b=records('default-fixed-nodes.jsonl')
default_equal=len(a)==len(b)==16 and list(map(numerical,a))==list(map(numerical,b))
summary=dict(records=len(rows),cases=len(rows)//3,errors=errors,default_fixed_node_cases=len(a),default_fixed_node_identical=default_equal,gap_stops={v:sum(r['variant']==v and r['status']=='gap_limit'for r in rows)for v in ['0.01','0.0001']},wall_budget_seconds=10,node_comparison_budget=8192)
(ROOT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
lines=['# MIP gap measurements','','Each variant uses the same model and a fresh process, with a 10 s solver limit and a 30 s process watchdog. `full` leaves gap stopping off. Time/node limits are retained; they are not proofs of an optimum. Timings are observations from a shared QA host.','','| Case | Full status / nodes / ms | 1% status / nodes / ms | 0.01% status / nodes / ms |','| --- | --- | --- | --- |']
for case in dict.fromkeys(r['case']for r in rows):
 def cell(v):
  r=lookup[case,v];return f"{r['status']} / {r.get('nodes','?')} / {r['seconds']*1000:.2f}"
 lines.append(f"| {Path(case).stem} | {cell('full')} | {cell('0.01')} | {cell('0.0001')} |")
lines+=['',f"Validation: {len(rows)} records, {len(errors)} invalid findings. Default numerical records match 0.29.0 on all {len(a)} cases with a fixed 8,192-node budget and no cooperative wall limit (timings and version labels excluded). The 10-second p0548 runs are wall-censored and have different node counts, as expected for time-limited runs.",'']
(ROOT/'tables.md').write_text('\n'.join(lines))
assert not errors and default_equal,summary
print(json.dumps(summary,indent=2))
