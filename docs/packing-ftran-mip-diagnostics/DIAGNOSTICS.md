# MIP observations in 0.23

Enable `NativeOptions(mip_diagnostics=True, mip_trace_limit=256)` in Python,
or the corresponding Rust fields. Decode the existing backend metadata:

```python
import json
from lp_ip_craft import NativeOptions, solve

result = solve(model, backend="native", backend_options=NativeOptions(
    mip_diagnostics=True, mip_trace_limit=256, decompose=False))
trace = json.loads(result.backend.effective_options["diagnostics.mip_search"])
print(trace["root"], trace["first_certified_incumbent"], trace["terminal"])
```

No new top-level result format is introduced. `schema_version=1` describes
this optional observation payload. Normal result status, candidate quality,
best bound and gap retain their existing meanings. An unsupported solve raises
the existing error and cannot supply a result trace; the benchmark retains it.

## Scope, time and work

- `scope=serial_search` covers one tree after reversible root preprocessing.
  `original_rows/columns` and `search_rows/columns` identify its model scope.
- `elapsed_seconds` starts at the internal search call, including its setup and
  strengthening. Add backend metadata
  `diagnostics.mip_search.preprocessing_seconds` (default zero when absent)
  for an approximate whole-call position; root postsolve and trace encoding
  are later. The result's total time includes all this work.
- `scope=component_aggregate` explicitly supplies no global tree trace. Use
  `decompose=False` for serial diagnosis. This option does not silently disable
  decomposition. Batch jobs keep independent per-model observations.
- `nodes` is the existing relaxation counter, which also includes strong
  branching probes. `tree_nodes_visited` counts tree visits including nodes
  pruned before solving an LP. Pivots include phase one, retries, probes and
  certification. Work-limited trajectories should match with tracing on/off;
  elapsed times need not. Enabled overhead can change a wall-limit exit.

## Values and certificates

All objectives/bounds use the original objective sense and include the offset.
`root.lp_objective` is the accepted relaxation objective, not a pruning proof.
It refers to the root after existing preprocessing and integer-row
strengthening, not necessarily the original model's unstrengthened LP.
`root.conservative_bound` is independently enclosed and can also use the
existing verified objective lattice. It can be stronger than the raw LP value
for integer optimization. A missing bound remains JSON null.

An `accepted_candidate` passes independent evaluation of the user's original
model. This is tolerance feasibility, not an exact certificate. A
`certified_incumbent` additionally records an improved outward objective bound
from the solver's existing incumbent certification on the equivalent search
model. `objective` and `certified_objective_bound` may differ. The first two
milestones and total improvement counts survive event truncation.

Global bound snapshots take the minimum of known open/current subtree bounds
and retained closed-region bounds in internally minimized units. The existing
heap exposes an unknown bound first. If a required bound is missing, the
snapshot is null; no LP objective or pseudocost is substituted. Snapshots are
observational and never feed pruning. Their sequence can fluctuate numerically.

Trace gaps use the certified incumbent bound and global conservative bound,
with the existing `Gap` sense/sign convention; the result's normal gap uses its
returned tolerance-feasible candidate. Undefined/nonfinite JSON numbers are
null (including a relative gap with zero objective and nonzero absolute gap).
The trace terminal describes the internal search before root postsolve. The
returned result is authoritative; root-postsolve original acceptance, when
applicable, is additionally recorded as
`diagnostics.mip_search.final_original_check`.

## Coverage and limits

`propagation` measures the existing exact integer-row node propagator. Eligible
rows meet its integer/coefficient/domain restrictions. Mixed or otherwise
unsupported rows count as ineligible; root mixed-bound inference remains a
separate feature (`root_inferred_bounds` metadata). No new deduction is added.
Queue/term work, slack skips, bound changes and work-cap exits are observational.
When `enabled=false`, coverage is unassessed/unused; zero eligible rows does
not by itself claim that every row is structurally unsupported. A bound change
counts one variable-domain update, even if both endpoints change together.

`node_lp_pivots` counts actual node LP attempts including retries;
`other_lp_pivots` covers probes/certification. Existing backend counters report
their individual counts, reoptimization/fallbacks and reduced-cost fixings.
Separate inherited-bound and solved-bound prune counts explain tree closure.

Events record candidate/certificate improvements, root relaxation, frontier
changes and power-of-two work checkpoints. `depth` is the most recently visited
tree node's depth at that observation. `mip_trace_limit` accepts 0..4096;
zero retains summaries only. `events_dropped` and `truncated` disclose loss.
First milestones and terminal summary are stored independently of the cap.
The disabled path allocates no trace and does not recheck original candidates.
Enabled/disabled overhead is measured separately in `data/mip-diagnostics.json`.

The profile `entries.ftran_output` and `entries.ftran_workspace_clear` counters
currently cover FT forward copies/clears; they are not whole-solver totals.
Their corresponding exclusive timers also include instrumented non-FT paths.
