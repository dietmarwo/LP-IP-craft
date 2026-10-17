# LP-IP-craft 0.22: implementation of the HiGHS lessons

Baseline: 0.21.0, revision 5e5665e2bda9d0e6ca71b68d4dccf059cd925320.
The 17 October HiGHS lessons report is the design input. Production remains
owned, safe, dependency-free Rust numerics; HiGHS is an isolated reference.

## Execution rules

Execute the ten packages below in sequence. Record the specific implementation
choice before editing each numerical subsystem, test it, correct failures, and
commit accepted changes. Preserve negative experiments and measured regressions.
A rejected optimization may remain explicitly selectable when useful; it must
not silently become the default. No relaxed acceptance tolerances, invented
finite bounds, unsafe pruning, or altered oracle adjudications.

This release builds bounded first implementations, rather than attempting a
complete HiGHS clone: reversible singleton reductions, infinity-aware root
inference, exact binary cover cuts before factorization, and an independent-job
worker pool. General substitutions, arbitrary MIR/GMI, shared-tree MIP and
SIP/PAMI remain separate future extensions. Every package below is executed.

## 1. Attribute the performance gap

Add matched benchmark controls for presolve/decomposition and explicit HiGHS
serial dual simplex, plus actual engine iteration diagnostics. Add truly wide
and tall planted LPs (the old packing-wide fixture is square). Build an immutable
0.21 reference against the same expanded fixture generator. Compare default and
presolve-off 25fv47 and packing, retaining failures and dimensions/fingerprints.
Test known objectives and original-model KKT checks.

## 2. Preserve sparse support

Carry the FTRAN result support out of triangular/eta solves. Clear/copy touched
work entries on sparse paths and avoid a separate full-vector index discovery.
Keep dense paths and exact support supersets, including cancellation. Test
indexed versus dense results through repeated calls, mixed supports and updates;
measure packing, networks, irregular and basis replay.

## 3. Broaden update coverage

Experiment with FT eligibility beyond the very sparse-L rule, using existing
forced FT and eta as controls. Retain a work/fill/stability-based bounded choice
only where it wins. Dense-tail materialization must be measured, not assumed
beneficial. Test general-L/dense-tail update sequences and fallback; compare
profiled update/solve costs and total time. Document rejected policies.

## 4. Reuse refactor decisions

Add a bounded numerical pivot-order replay for compatible rebuilt matrices,
with per-pivot threshold checks and fresh Markowitz fallback. Start with exact
basis signatures; do not assume related node bases are interchangeable. Test
same basis, coefficient changes, singularity and cap handling. Record hit rate
and total cost, selecting an opt-in default if real solves provide no benefit.

## 5. Adaptive pricing

Compare row traversal work with column-dot work, keeping the existing sparse
row path and adding contiguous column pricing for dense BTRAN support. Preserve
logical columns and nonbasic states. Test both paths on square/wide/tall models,
near ties and public cases. Keep a measured crossover and explicit control.

## 6. Improve pivot efficiency

Instrument phase-one/two, degeneracy, cleanup and DSE work. Test a bounded Devex
alternative or weight-maintenance policy against current DSE, especially on
25fv47. Keep only demonstrated defaults; expose a tested optional policy if
workload tradeoffs prevent a universal choice. Verify transformed 25fv47,
infeasible/unbounded models, iteration/time/cancellation budgets and oracles.

## 7. Reversible presolve

Implement a typed reduction history for singleton-row bound tightening with
primal/dual reconstruction. Preserve original-model dual conventions by moving
active tightened-bound reduced costs back to the responsible original rows.
Use conservative enclosures; reject unsupported reductions. Test both senses,
negative coefficients, fixed/free columns, conflicts and postsolve residuals.
Keep stronger algebraic substitutions out until their integer/dual maps exist.

## 8. Broader sound bound inference

Add bounded root propagation over mixed rows with outward arithmetic and
explicit infinite-side handling, before the finite-MIP-bound capability check.
Use exact integer rounding when valid, retain the existing exact integer node
propagator, and record inferred domains. Never replace infinity by arbitrary M.
Test exact rational cases, overflow, free/one-sided inputs, feasibility starts,
rollback contracts, and public MIPs; unresolved infinities stay explicit.

## 9. Restricted proof-checked root cuts

Generate a bounded number of binary positive-integer knapsack cover cuts, with
exact i128 witnesses checked independently before row insertion. Cuts are global
and inserted at the root before any basis/cache exists; the matrix then stays
immutable during search. This avoids pretending to implement dynamic local cut
lifecycles. Validate candidates against the original model; derive pruning
bounds on the justified augmented relaxation. Test exhaustive small models,
cut scope/deduplication/caps, original starts and cuts-off comparisons. Enable
by default only when target and holdout results justify the overhead.

## 10. Parallelize independent jobs

Add a bounded safe-Rust batch worker API: shared immutable models, private
solver states, dynamic job assignment, deterministic result order and shared
cancellation. Avoid nested component oversubscription. Test 1/2/4/8/16 workers,
invalid jobs, cancellation and result equivalence; measure throughput on batches
large enough to amortize scheduling. This is not shared-tree or parallel simplex.

## Final gates and delivery

Run Rust all-feature tests, strict Clippy and formatting, Python tests, frozen
20370-case oracle variants under unchanged adjudications, exact bound audits,
existing and changed-basis replay, old performance suites, new aspect ratios,
public originals and fresh MIP seeds. Baseline/candidate/HiGHS binaries use the
same compiler, fixtures, budgets and independent acceptance. Timing runs are
sequential after builds/tests, fresh rotated processes with warmups/discards;
report medians/ranges, pivots, nodes, memory and every failure.

Deliver full source 0.22, this plan and execution log, results, raw evidence and
reproduction instructions in a checked downloadable ZIP. No GitHub push is
required or implied.

### Stage 1 result and stage 2 implementation decision

Matched explicit serial dual simplex, original fingerprints and original-unit
KKT checks pass for square packing and true 4:1 / 1:4 aspect-ratio fixtures, with
presolve on/off. A useful new finding: monolithic 25fv47 takes 3093 pivots with
presolve, versus the prior decomposed 6662. HiGHS uses 2583. This makes the
near-monolithic component decision a stage-six candidate, not proof that a new
pricing method is needed. Attribution observations are single-process probes;
final retained timing batches will provide the comparison.

Stage 2 will carry a safe output-support superset through sparse U and eta
solves, use it to clear/copy touched entries, and avoid a separate result scan.
Dense/FT paths retain bounded fallbacks. Work-vector metadata is invalidated
when BTRAN or dense paths reuse that storage. Tests must alternate orientations
and supports so stale work entries cannot survive.

### Stage 2 result and stage 3 decision

All native all-feature checks pass; the added regression alternates sparse and
dense RHS, cancellation, replacements and BTRAN storage reuse. Three-process
medians: network512 5.016 -> 4.810ms, square degree-nine packing1024
313.477 -> 301.180ms, irregular1024 119.702 -> 118.261ms. Ordinary packing4096
225.594 -> 232.238ms remains a retained regression. Sparse U support now survives
eta updates and supplies the entering-column index without rediscovery.

Stage 3 will allow bounded general sparse L (up to four entries per row on
average and bounded total fill) while retaining the no-dense-tail auto rule.
The existing observed update-work criterion still decides whether FT starts.
Forced dense-tail materialization is compared explicitly; it will not be enabled
by default merely to increase FT coverage.

### Stage 3 outcome and stage 4 implementation decision

General sparse-L eligibility and both-orientation solve tests pass, while
retained dense tails stay outside auto FT. Explicit auto/eta/FT measurements
are retained, including dense-tail materialization controls. Broader eligibility
remains gated by the existing measured eta-work comparison; it is not forced.

Stage 4 adds an explicit factor_replay control, initially off. It hashes the
assembled basis (including ordering/search settings), tries cached row/position
pivots only for an identical signature, validates each numerical threshold,
and otherwise searches normally. Even a signature collision cannot bypass the
numerical pivot checks. Replays count actual accepted pivots. Changed matrices,
singular cases and caps are tested; no factorization is reused without numeric
elimination. Same-basis microbenefits must not be confused with node-tree gains.

### Stage 4 outcome and stage 5 decision

Identical-basis replay, changed coefficients, singularity and memory-cap tests
pass. Real-solve timing and replay-hit diagnostics are retained. Replay remains
explicitly opt-in: avoiding pivot search is not a universal end-to-end gain,
and the implementation does not claim reuse across arbitrary changed bases.
Python and Rust expose factor_replay; production defaults preserve fresh search.

Stage 5 estimates row traversal from the current BTRAN support and compares it
with the static column-file work. Only dense-support cases use column dots;
sparse pricing remains the fallback. Both paths apply the identical DROP rule
to rho, preserve logical-column signs and nonbasic filtering, and are checked
against each other on a deliberately dense transformed basis.

### Stage 5 outcome and stage 6 decision

Column and row tableau prices agree on a dense transformed basis including
logical columns. All native all-feature tests and aspect/square controls pass.
Both pricing counters are exposed. Timing/pivot tradeoffs are retained rather
than assuming a column path always wins.

Stage 6 tests an optional Devex-style approximate edge-weight recurrence (no
extra DSE forward solve), with periodic/nonfinite resets. Exact DSE remains the
initial default. It also skips decomposition for large models with at least 90%
of variables in one component; stage-one evidence identified this as a pivot
count problem on 25fv47. Both policies get explicit controls. Phase-one,
degeneracy and weight-reset diagnostics are added. Perturbed objective cleanup,
original-model acceptance and global budgets remain unchanged.

### Stage 6 outcome and stage 7 decision

The original stage-six policy timing file was misconfigured: both labels used
DSE because the optional Devex environment override was missing. It is retained
as rejected-harness evidence and replaced by an asserted effective-option run.
The replay override was already wired; replay is rerun with an assertion too. Exact DSE stays the default pending broad final oracle/holdout gates.
Large near-monolithic decomposition can be explicitly forced or avoided.
Phase-one, degeneracy and weight resets are now separately reported.

Stage 7 implements exact-quotient singleton reductions only. A binary64
significand/exponent product check proves a*q equals the original row bound;
inexact divisions leave the row intact. A typed bound-owner history restores
removed-row multipliers from the reduced costs, then recomputes reduced costs
against the original matrix. MIP starts accepted only within tolerance can
bypass strengthening if the reduced model would reject them. Zero budgets and
cancellation bypass preprocessing; postsolve checks remain original-model checks.
This deliberately starts with reversible rows, not general column substitutions.

### Stage 7 result and stage 8 plan

Exact singleton quotient checks and typed bound-owner postsolve are implemented;
negative coefficients, competing owners, both objective senses, subnormals,
inexact quotient retention and zero-budget bypass have dedicated tests. Original
LP KKT is checked after dual reconstruction. No arbitrary column substitution.

Stage 8 adds bounded, MIP-only root activity propagation before the existing
finite-domain admission check. Outward intervals, explicit infinity counts and
directed quotient rounding must enclose the exact real implication. Integral
rounding happens only after that enclosure. Unresolved infinite domains remain
unsupported. Root transformations precede all caches; original candidates and
whole-call budgets are preserved. LP general-bound postsolve is intentionally
excluded because its proof history is more complex than singleton postsolve.

### Stage 8 result and stage 9 plan

MIP root inference now handles mixed continuous/integer rows and infinities
before finite-domain admission. Dedicated tests cover inferred finite domains,
both senses, unresolved free domains, overflow, and exhaustive dyadic feasible
points. All native checks pass before the next change. This expands admission
only when real bounds follow from constraints; it never invents a big-M.

Stage 9 adds up to eight exact binary cover cuts at the root, before any LP
basis or bound cache exists. Positive integral row coefficients, binary domains,
exact integer capacity sums and independent witness validation are required.
Cuts are immutable and global for the entire search. No local-cut deletion,
MIR/GMI or conflict-pool claims. The initial policy is opt-in; compare nodes,
root bounds and total time before considering a default change.

### Stage 9 result and stage 10 plan

Exact cover witnesses are independently rescanned before insertion. Exhaustive
binary enumeration checks every added row against every feasible point and
both-sense optimal objectives. Root cuts are opt-in; their matrix is fixed
before basis creation. The full native checks pass after this stage.

Stage 10 implements a bounded 1–16 worker pool for independent models. Models
are shared immutably, each worker owns a private solver, component threading is
disabled inside a batch, and results/errors retain input order. Limits apply
per job; cancellation is shared. Python releases the interpreter while the
Rust pool runs. Test empty/invalid batches, mixed errors, cancellation and
sequential equivalence at 1/2/4/8/16 workers; measure throughput separately from
single-instance latency.

### Stage 10 implementation and final gates

The Rust and Python ordered batch APIs are implemented, including per-job error
values and shared cancellation. The pricing work estimate is now behind cheap
eligibility checks: sparse BTRAN calls should not pay an extra support traversal
for a column path they cannot use. This changes no pricing decisions.

Final gates: build/install the wheel; run all Rust/Python checks and strict
Clippy; replay the unchanged 20,370 frozen variants plus optional-policy passes;
run the exact dual-bound audit; compare 0.21/current/HiGHS over standard,
extended, holdout, aspect-ratio and public fixtures; replay frozen basis systems;
measure worker throughput. Keep unsupported/public failures and timing
regressions visible. Select defaults from the retained evidence, rerun gates if
code changes, and package code, plan and evidence in a downloadable 0.22 ZIP.

### Retained final-gate decisions

The frozen oracle inputs and adjudications are unchanged. All 45,690 runs pass
after the same 19 pre-existing oracle disagreements are adjudicated on each
full pass; raw flags remain visible. The 400 exact-bound models have zero unsafe
finite results. Rust checks: 129; Python: 175; strict Clippy passes at MSRV 1.85
(the directed-rounding helpers use bit operations, avoiding APIs added in 1.86).
Unproductive root preprocessing now also consumes the whole-call time budget.

The first matched batch finds a degree-nine packing1024 gain (363 -> 311ms),
with an ordinary packing4096 regression (235 -> 246ms). Both are retained.
Replay was already wired; the missing optional override affected Devex and the
other new policy controls. The corrected trials assert effective options.
The standard timing raw records completed but lacked the final summary, so
`summarize_standard.py` reconstructs the 93 summaries from all 372 raw records,
without rerunning or changing any observations. No objective mismatches.

Additional coverage isolates the new capabilities: exact singleton LPs with
planted x=1, and mixed blocks whose initially free integer/continuous domains
become finite by row inference, with an independently known optimum 5 per
block. Original 0.21 numerical sources are rebuilt against those same fixtures;
its unsupported inferred-domain cases are retained as capability failures.

Root inference admits all three previously unsupported public MIPs. First
retained outcomes: egout and flugpl now solve; bell5 reaches the unchanged
100,000-node cap. This exposes a search-strength gap rather than hiding it
behind the former finite-box rejection. No cap or acceptance threshold is
relaxed to turn that failure into a pass.

### Final profile-driven correction plan

The instrumented packing4096 comparison attributes column setup 3.54 -> 11.70ms
and triangular FTRAN 114.81 -> 125.57ms. Carrying an indexed result is expensive
when its support is broad: random gathers, mark maintenance and copying can
cost more than one contiguous scan. Before release, retain carried support only
when its superset is at most m/8; otherwise use the proven dense index scan.
This changes traversal order, not the solved system or drop thresholds. Test
alternating support/cancellation/update regressions, then compare ordinary and
degree-nine packing, irregular and aspect controls. Accept only with retained
measurements and rerun the numerical/performance gates for final code.

The first density experiment reduced ordinary packing4096 248.6 -> 233.8ms,
but irregular/aspect controls regressed. Retain that rejected broad policy as
`density-fix-comparison.json`. Narrow the candidate to FT output above 50%
density only; sparse-U output keeps its carried index. Recheck all four controls
before accepting the narrower policy.

### Density experiment disposition and release freeze

The narrowed FT-only experiment also failed the packing target: ordinary
packing4096 247.9 -> 274.7ms, while degree-nine packing improved 372.5 -> 323.1ms;
irregular was unchanged and tall improved modestly. Neither candidate gives a
consistent target win. Both are reverted. The final accepted numerical code is
therefore exactly the implementation already covered by all final 45,690 oracle
runs and matched benchmarks. Negative experiments remain in the archive. Do
not claim their transient gains as release improvements.
