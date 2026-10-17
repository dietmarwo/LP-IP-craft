# 0.21 plan: numerical recovery, wide packing, certification cost

Baseline: verified 0.20.0 archive, original revision
bf5bcb0565d9411cfa812b668c0a917975af019c. Rebuild baseline and candidate with
the same compiler/host; historical timings are context, not the comparison.
Production stays safe owned Rust without external numerical dependencies.

## 1. Numerical robustness: 25fv47

Before implementation, reproduce the failure and instrument the exact exit,
phase, pivots, primal/dual residuals and refactorization state. Capture a failing
basis if useful. Fix the underlying defect or add a bounded, justified numerical
recovery. Preserve original-model acceptance tolerances and time/iteration
budgets; no invented bounds. Test original and equivalent row/column permutations,
other public LPs, infeasible/unbounded cases and the frozen oracle dataset.
Acceptance: 25fv47 solves to independently checked primal/dual residuals and its
reference objective; no new false statuses or unchecked optimum declarations.

## 2. Wide packing

Profile degree-nine packing at 256/1024 and larger held-out sizes. Separate
factor search/elimination/dense tail, FTRAN/BTRAN, eta/FT and pricing. Use frozen
bases to isolate kernels where useful. Implement the dominant improvement first,
with bounded fallback and reusable storage. Measure pivot counts and memory as
well as elapsed time; retain rejected experiments. Gate against ordinary and
permuted packing, networks, irregular LPs, public cases and numerical tests.

## 3. Certification overhead

Profile conservative dual bounds and candidate-domain checks. Separate matrix/
multiplier work from bound-dependent work, cache reusable exact enclosures, and
avoid repeating whole-matrix work for every domain trial. Keep interval rounding,
infinite-bound/overflow handling, incumbent requirements and pruning proof
semantics. Bound work when proof cost exceeds likely LP savings. Test cached
versus fresh enclosures against exact rational Lagrangian calculations, changed
bounds, both objective senses and all arithmetic failure paths. Compare fixing
on/off, node counts and time on long-row112, general-integer/multi-knapsack,
graph and mixed-variable cases, plus fresh independent seeds.

## Final gate and delivery

Run Rust/all-feature tests, strict Clippy, Python tests, unchanged frozen oracles
with unchanged prior adjudications, exact bound audits and captured-basis replay.
Use rotated fresh processes, a discarded round, warmups and retained medians/
ranges. Report every public failure and all regressions, not an accepted-only
average. Deliver source, plans, raw evidence, results and a downloadable ZIP.

## Execution log

Plan recorded before implementation. Restored archive SHA256:
90c5a69996ccc4be8ce66a74ca6d0b1899f7519f5f945c0355e71a875b54da1f.

### Step 1 implementation and test

The initial 953-pivot exit was the fixed 12-restart guard. A progress-aware
replacement exposed a second defect: phase one removed perturbations and then
used an optimum for the perturbed objective to classify the original objective
as dual infeasible. The rejected guard-only experiment is retained in the trace
history. Cleanup now retries the original costs before making that decision;
perturbation drift on a one-sided column first triggers cost cleanup, not a new
phase-one classification. Acceptance tolerances are unchanged.

The full Rust all-feature suite passes, including a new lossless 25fv47 fixture
with original/reversed columns, reversed and power-of-two-scaled rows, both
component and monolithic solves, and capped runs. Original optimum:
5501.84588828677; primal violation 2.88e-11, relative gap 2.48e-16, 6662 pivots.

### Step 2 measured implementation plan

Exclusive profiles: wide1024 FTRAN dense 167.1/439.6ms, wide1536
336.7/1051.6ms. Replace the serial dense triangular dot accumulation with four
independent partial sums (safe Rust, no fast-math); retain scalar short rows.
Measure paired ordinary builds and exclusive profiles, then run dense
certificate tests, public LPs, equivalent 25fv47 forms and frozen basis replays.
Reject the change if numerical gates fail or matched timing does not improve.

### Step 2 result

Three paired retained process medians: wide256 14.378 -> 12.270ms,
wide1024 384.037 -> 343.300ms, wide1536 877.272 -> 739.098ms.
Pivot counts identical. Ordinary packing4096 214.176 -> 223.239ms and
permuted1024 13.053 -> 13.465ms are retained regressions/noise to remeasure.
Irregular1024 141.384 -> 122.439ms. Rust all-feature gates pass; 21 captured
basis cases pass, worst backward error below 1e-9. No tolerances were relaxed.

### Step 3 implementation plan

Keep the existing fresh conservative-bound API and arithmetic order. Factor
its matrix/multiplier and per-variable work into shared helpers. Introduce an
owned prepared bound with cached residual intervals, immutable row constant,
and an outward-rounded balanced sum tree. Each changed bound recomputes a
single variable rectangle infimum and O(log n) ancestors. Failure nodes retain
unbounded/overflow status and can recover when that bound changes. Constructor
and update validation, arithmetic probes, and sign reversal remain explicit.
Tree order can change last-bit enclosures, never their conservative direction.

Use the cache for two or more shortlisted opposite-domain trials, retaining the
fresh path for a single trial. Accepted fixed domains stay in the cache; rejected
trials restore their prior interval. Add exact integer-rational random tests in
both senses, mutation/rollback, free/infinite sides, overflow recovery, zero
columns, invalid input, plus end-to-end MIP oracle and fixing on/off gates.
Profile preparation versus per-trial work; benchmark long-row112 and independent
seeded MIPs. No numerical reduced cost alone may justify pruning.

### Step 3 result

256 dyadic models x 20 initial/updated states = 5120 exact i128 Lagrangian
comparisons, in both objective senses, pass for fresh and cached calculations.
Infinite terms, overflow at leaves/internal nodes, rollback, unchanged state on
invalid input, empty rows, exact zero residuals and immutable row failures pass.
Rust all-feature and strict workspace Clippy checks pass.

Five retained paired process medians: long-row112 2.573 -> 1.196ms,
long-row64 0.509 -> 0.338ms, integer49 0.758 -> 0.660ms; respective nodes
remain 50, 16 and 41. Nodecopy 0.727 -> 0.758ms and mixed4016
0.179 -> 0.182ms remain explicit small regressions/noise. At long-row112,
310 fresh proofs cost 0.800ms; cached preparation + trials cost 0.134ms.
Original shortlist, trial cap, incumbent requirement and proof-only pruning stay
unchanged. Final validation freezes this implementation before new seed holdouts.

### Final gate

0.21.0 is frozen at the step-three implementation before new holdouts. The
full audit passes: 115 Rust checks, 169 Python tests, strict Clippy/formatting,
20370 frozen oracle cases under unchanged adjudications, 5120 exact prepared
bound states, 400 exact rational bound audits with no unsafe finite result,
and 84 captured-basis replay records (worst backward error 2.17e-12).

All earlier standard performance cases, extended controls, earlier/fresh seeds,
new holdouts, original public cases and irregular4096 were rerun against the
same rebuilt 0.20 baseline and matched pinned HiGHS harness. One public output
ended at 119/120 records without a summary; it was rejected as incomplete and
rerun in full. Both process logs are retained; the audited replacement has
120 records including the discarded round. No performance failure was omitted.

Final extended medians: wide1024 392.178 -> 320.208ms, wide1536
876.611 -> 767.806ms, long-row112 1.915 -> 1.158ms. Fresh wide2048
1898.773 -> 1584.938ms; irregular4096 7.477 -> 5.717 seconds.
Ordinary packing4096 regresses 225.086 -> 233.311ms; several small MIPs also
regress. All tables and ranges remain available. Seven public LPs now pass;
the three infinitely bounded MIPs remain unsupported. No application model
was supplied, so workload readiness remains unverified.
