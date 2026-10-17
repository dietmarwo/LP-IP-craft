"""Generate reviewable 0.21 report from audited raw evidence."""
import json,statistics
from pathlib import Path
root=Path(__file__).resolve().parent.parent
folder=root/'docs/robustness-wide-cert';data=folder/'data'
def read(name):return json.loads((data/name).read_text())
audit=read('audit.json')
text=['''# 0.21.0: numerical robustness, wide packing and certification reuse

Baseline is the verified 0.20.0 archive, original source revision
`bf5bcb0565d9411cfa812b668c0a917975af019c`. All three stages have a plan,
implementation, tests and retained evidence in [PLAN.md](PLAN.md). The final
production numerical crates remain dependency-free safe Rust. HiGHS 1.15.0
is built only in an isolated copy of the original 0.4 adapter for comparison.

## What changed

1. **25fv47:** bound consecutive restarts without pivot progress, instead of all
   restarts. Clean perturbed costs before classifying one-sided dual infeasibility;
   a perturbed auxiliary optimum cannot establish that the original objective is
   unbounded. Global budgets, pivot guard and original-model acceptance tolerances
   remain in force. The guard-only attempt incorrectly returned unbounded and was
   rejected; its trace and result are retained. Four equivalent original/reversed/
   power-of-two-scaled forms and limited solves are permanent Rust regressions.
2. **Wide packing:** four independent accumulators shorten the dense-tail FTRAN
   dot-product dependency chain. Short rows retain scalar arithmetic. No fast-math,
   unsafe code, external BLAS or acceptance relaxation. Stage profiles roughly
   halve the dense FTRAN kernel (wide1024: 170 -> 84ms), while pivot counts stay
   unchanged. This targets a measured bottleneck in the sparse basis solver's
   retained dense tail.
3. **Certification:** `PreparedDualBound` caches the immutable row contribution
   and residual intervals, with an outward-rounded balanced sum tree for variable
   contributions. Preparation is O(nnz+n+m), one changed bound O(log n), reads O(1).
   Native opposite-domain fixing uses it for multiple trials and rolls back failed
   trials. The incumbent requirement, shortlist and trial cap stay unchanged.
   Only conservative bounds justify removal. Stage long-row112 profile: 310 fresh
   proofs 0.800ms -> cached preparation plus trials 0.134ms; still 50 search nodes.

## Protocol

Same compiler and host for baseline/candidate/reference. Sequential fresh
processes, rotated/reversed order, one discarded round, then one warmup per
process. Three retained standard/holdout/public/large rounds and five extended
rounds. Single numerical thread; component-parallel controls are separately
labelled. Internal conversion/checking is included; fixture construction and
external checks are excluded. Same model fingerprints, 30-second cooperative
solve limit, 100000 MIP nodes and 1000000 LP iterations. No exclusive host.
Absolute times of failures are not speed wins. Medians are milliseconds; full
ranges, effective options, pivot/node counts and RSS are in the JSON files.
Public failures stay in the denominator. These synthetic and public models do
not establish performance on an unseen application formulation.
''']
for suite,label in [('extended','Extended controls'),('holdout','Previous seeded holdouts'),('previous-fresh','Previous fresh-seed controls'),('new-holdout','New holdouts after implementation freeze'),('public','Public original models')]:
 d=read('final-'+suite+'.json');rows=d['summary'];keys=list(dict.fromkeys((r['fixture'],r['size']) for r in rows))
 text+=['\n## '+label+'\n','| Workload | 0.20 ms | 0.21 ms | HiGHS ms | 0.21 / 0.20 | Native nodes before → after |','|---|---:|---:|---:|---:|---:|']
 for name,size in keys:
  rr={r['backend']:r for r in rows if (r['fixture'],r['size'])==(name,size)}
  def number(r):
   if r['accepted']!=r['processes'] or r.get('errors'):return ', '.join(sorted(set(r['statuses'])))
   return f"{r['median_ms']:.3f}"
  before,after,highs=(rr[b] for b in ['baseline','native','highs'])
  ratio=f"{after['median_ms']/before['median_ms']:.3f}×" if all(r['accepted']==r['processes'] for r in [before,after]) else '—'
  label=Path(name[5:]).stem if name.startswith('json=') else name+(f' {size}' if size else '')
  nodes=f"{before.get('nodes','—'):g} → {after.get('nodes','—'):g}" if all(r.get('nodes') is not None for r in [before,after]) else '—'
  text.append(f"| {label} | {number(before)} | {number(after)} | {number(highs)} | {ratio} | {nodes} |")
text+=['\n## All earlier standard performance checks\n','Includes legacy models, packing/network/irregular/selection sizes, cold MIP\nsolves, imports, transport, and separately labelled component parallelism.\n','| Workload / configuration | 0.20 ms | 0.21 ms | HiGHS ms |','|---|---:|---:|---:|']
rows=read('final-standard.json')['summary'];keys=list(dict.fromkeys((r['fixture'],r['size'],r['warm'],r['workers'],r['suite']) for r in rows))
for name,size,warm,workers,suite in keys:
 rr={r['backend']:r for r in rows if (r['fixture'],r['size'],r['warm'],r['workers'],r['suite'])==(name,size,warm,workers,suite)}
 name=name+(f' {size}' if size else '')+(' cold' if not warm else '')+(f' workers={workers}' if workers>1 else '')
 cells=[f"{rr[b]['median_ms']:.3f}" if b in rr and rr[b]['accepted']==rr[b]['measured_solves'] else '—' for b in ['baseline','native','highs']]
 text.append('| '+name+' | '+' | '.join(cells)+' |')
text+=['\n## Large irregular control\n','| Backend | Median seconds | Range seconds | Accepted |','|---|---:|---|---:|']
for r in read('final-large.json')['summary']:
 text.append(f"| {r['backend']} | {r['median_seconds']:.3f} | {r['range_seconds'][0]:.3f}–{r['range_seconds'][1]:.3f} | {r['accepted']}/3 |")
text+=['\n## Validation\n',f"- {audit['rust_checks']} Rust checks, {audit['python_checks']} Python tests; strict all-target/all-feature Clippy and formatting pass.",f"- {sum(r['cases'] for r in audit['oracle_runs']):,} frozen oracle cases: default, forced FT, fixing disabled, refactor-every-pivot exact cases and four-worker components. No failures after the unchanged prior adjudications. Raw flags remain explicit (19 per main pass); no new adjudication was invented.",f"- {audit['exact_prepared_bound_states']:,} exact i128 dyadic Lagrangian states validate both fresh and prepared enclosures; bound updates, invalid inputs, unbounded terms, overflow recovery, immutable row failures and both senses are covered.",f"- 400 independent exact-rational bound checks: {audit['unsafe_finite_bounds']} unsafe finite bounds.",f"- {audit['basis_replay_records']} frozen-basis replay records; worst backward error {audit['worst_backward_error']:.3g} < 1e-9. Original 10 public corpus source/JSON hashes are unchanged."]
text+=['\n## Remaining deficiencies\n','All seven public LPs now pass, including 25fv47, but 25fv47 still needs many\nmore pivots and remains slower than HiGHS. Wide LPs retain dense-tail and sparse\nfactorization costs and can still trail HiGHS. MIP search still lacks mature\ncutting planes and requires finite bounds on every variable; egout, flugpl and\nbell5 remain unsupported while HiGHS solves them. Small-workload regressions\nand timing variability are visible in the complete tables; improvements are\nnot uniform. The real application instance is still unavailable.\n', 'Reproduction: build the current harness, restore the verified 0.20 archive\nand pinned HiGHS reference, then use `compare_three.py`,\n`compare_packing_mip.py`, `compare_large.py`, `replay_compare.py`,\n`oracle_worker.py`, `check_bounds.py`, and `audit_robustness_wide_cert.py`.\nBinary/source hashes and all raw results are included.\n']
(folder/'RESULTS.md').write_text('\n'.join(text)+'\n')
