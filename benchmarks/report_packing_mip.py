"""Generate tables from retained raw runs; no manual timing edits."""
import json,statistics
from pathlib import Path
root=Path(__file__).resolve().parent.parent;data=root/'docs/packing-mip/data'
def read(n):return json.loads((data/n).read_text())
standard=read('final-standard.json')['summary'];extended=read('final-extended.json')['summary'];hold=read('final-holdout.json')['summary'];fresh=read('final-fresh-holdout.json')['summary'];public=read('final-public.json')['summary'];large=read('final-large.json')['summary'];audit=read('audit.json')
def rows_for(rows,name,n):
    return {r['backend']:r for r in rows if r['fixture']==name and r['size']==n and r.get('warm',True) and r.get('workers',1)==1}
def table(rows,cases):
    out=['| Workload | 0.19 ms | 0.20 ms | HiGHS ms | 0.20 / 0.19 | Native nodes before → after |','|---|---:|---:|---:|---:|---:|']
    for name,n in cases:
        d=rows_for(rows,name,n);b=d['baseline'];a=d['native'];h=d['highs'];label=Path(name[5:]).stem if name.startswith('json=') else f'{name} {n or ""}'.strip()
        valid=all(r['accepted']>0 for r in [b,a,h])
        if valid:out.append(f"| {label} | {b['median_ms']:.3f} | {a['median_ms']:.3f} | {h['median_ms']:.3f} | {a['median_ms']/b['median_ms']:.3f}× | {b['nodes']} → {a['nodes']} |")
        else:out.append(f"| {label} | {','.join(b['statuses'])} | {','.join(a['statuses'])} | {h.get('median_ms',0):.3f} | — | — |")
    return '\n'.join(out)
text='''# 0.20.0: packing solves, integer-domain fixing, and broader coverage

## Scope and protocol

Baseline 0.19.0: `0c31318a6abd6533af54675585421a0f2469786d`.
Owned safe Rust production kernel; HiGHS remains only an isolated reference.
Each timing batch runs sequentially, after compilation/validation, on a shared
AMD EPYC host without core exclusivity. One discarded rotated/reversed process
round, one process warmup, then five retained standard/extended/holdout rounds;
three retained public and large-irregular rounds. Construction and independent
checks excluded, internal conversion and checking included. Single numerical
thread; standard separately labelled component-parallel controls retained.
Same model fingerprints, limits and acceptance tolerances across backends.
Raw medians/ranges, peak RSS, nodes, pivots and effective options are saved.

## What changed

- Cached reciprocal LU/FT diagonals and zero-aware forward triangular arithmetic.
- Exact steepest-edge winner caching in blocks of 64, with full invalidation on
  recomputation/restoration/bound changes and direct scanning for small bases.
- Exact i128 integer-row activities and an incidence-driven bounded propagation
  queue, including long rows, root processing and sibling restoration.
- Smoothed pseudocost estimates and a balanced degradation score.
- Certified opposite-domain fixing: numerical reduced costs only shortlist;
  conservative core dual bounds against a certified incumbent justify each
  inherited integer fixing. The option `reduced_cost_fixing` defaults to true.
- Optional root probes, limited to four 32-pivot solves, charged to global
  budgets; `strong_branching` defaults to false after negative measurements.

## Standard controls

'''
text+=table(standard,[(n,s) for n,s in [('packing',1024),('packing',2048),('packing',4096),('network',2048),('irregular',256),('irregular',1024),('nodecopy',0),('selection',20),('selection',32)]])
text+='\n\n## Extended development controls\n\n'+table(extended,[(r['fixture'],r['size']) for r in extended if r['backend']=='native'])
text+='\n\n## Reserved seeded holdouts\n\nThese tags were not used to choose the original score or kernel parameters.\nThe first holdout results prompted the separately planned certified-fixing step;\nthey are therefore regression/coverage cases after that step, not a pristine\nstatistical holdout. All cases and pre-fixing results remain available.\n\n'+table(hold,[(r['fixture'],r['size']) for r in hold if r['backend']=='native'])
text+='\n\n## Fresh validation seeds after parameters were frozen\n\nThese seven configurations were run after implementation and parameter choices\nwere frozen; no changes were selected from their results.\n\n'+table(fresh,[(r['fixture'],r['size']) for r in fresh if r['backend']=='native'])
text+='\n\n## Original public inputs\n\n'+table(public,[(r['fixture'],r['size']) for r in public if r['backend']=='native'])
text+='''

The seven public LPs and three MIPs preserve original infinite bounds. Both
native releases fail numerically on 25fv47. Egout, flugpl and bell5 are rejected
because their MIPs include infinitely bounded variables. HiGHS solves all ten.
These are coverage failures, not speed wins. Six accepted public LPs do not
establish general Netlib/MIPLIB competitiveness. No real application capture
was available. Public objective constants and precision are documented in the
corpus README; references come from separately solved identical CSR models.

## Large irregular control

| Backend | Median seconds | Retained range seconds |
|---|---:|---:|
'''
for r in large:text+=f"| {r['backend']} | {r['median_seconds']:.3f} | {r['range_seconds'][0]:.3f}–{r['range_seconds'][1]:.3f} |\n"
text+='\n## Kernel profiles\n\nInstrumented times are separate from uninstrumented wall times above.\n\n| Case | Forward triangular ms before → after | Pricing ms before → after |\n|---|---:|---:|\n'
prof=read('final-profiles.json')['summary']
for name,n in [('packing',1024),('packing',4096),('irregular',1024),('network',2048)]:
    rs={r['backend']:r['exclusive_median_ms'] for r in prof if r['fixture']==name and r['size']==n};b=rs['before'];a=rs['after']
    text+=f"| {name} {n} | {b.get('ftran_triangular',0):.3f} → {a.get('ftran_triangular',0):.3f} | {b.get('pricing_choose',0):.3f} → {a.get('pricing_choose',0):.3f} |\n"
text+='''

## Correctness and limits

'''
text+=f"- {audit['rust_checks']} Rust/all-feature checks; {audit['python_checks']} Python checks; three MPS-parser contract checks; strict all-feature Clippy.\n"
text+=f"- {sum(r['cases'] for r in audit['oracle_runs'])} frozen oracle solves, unchanged prior adjudications, zero new discrepancies. Raw unresolved SciPy flags remain visible.\n"
text+=f"- 400 bound-availability models: {audit['unsafe_finite_bounds']} unsafe finite bounds under the exact Lagrangian checker.\n"
text+=f"- {audit['basis_replay_records']} captured-basis replay records, both solve orientations; worst normalized backward error {audit['worst_backward_error']:.3g}.\n"
text+=f"- {sum(r['retained'] for r in audit['benchmarks'])+audit['large_retained']} retained benchmark solves; {sum(r['accepted'] for r in audit['benchmarks'])+audit['large_retained']} accepted. All 24 expected public-native failures are reported above.\n"
text+='''

## Negative results and remaining weaknesses

Row-gather U solves slowed packing and were rejected. Pivot-order work vectors
were mixed and were rejected. Unconditional propagation scanning was too costly;
a row-slack guard now skips non-tightening work. Default root strong branching
was rejected after increasing nodecopy search. Branch-score changes alone gave
mixed holdouts; certified domain fixing produced the substantial MIP gain.
Per-step raw files preserve these experiments; PLAN.md records decisions.

The fresh long-row112 case regressed 2.08x despite reducing nodes 65 to 50;
proof and propagation overhead can outweigh search savings. Other regressions
include adlittle (+17%), mixed-overload1016 (+16%), wide1024 (+3.5%) and the
large irregular control (+1.7%). These costs are retained, not hidden by an
accepted-only average. `reduced_cost_fixing=false` permits workload-specific
ablation; disabling it does not disable the other kernel/search changes.

MIP search still lacks a mature cutting-plane system and parallel connected
search. Finite bounds on every MIP variable remain required. Dense kernels
remain single-threaded. Packing4096 and the failed public LP are still concrete
weaknesses. Timing ratios on sub-millisecond cases are sensitive to host noise;
use node counts and the retained ranges alongside elapsed time. Improvements
are not monotonic on every fixture; the tables report all controls and seeds.
'''
(root/'docs/packing-mip/RESULTS.md').write_text(text)
print(text[:6500])
