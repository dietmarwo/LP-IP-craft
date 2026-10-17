# Packing kernels, MIP search, and broader evidence

Baseline: 0.19.0, commit `0c31318a6abd6533af54675585421a0f2469786d`.
Production remains owned safe Rust, without HiGHS or numerical dependencies.
HiGHS is only an isolated benchmark reference. Plans are recorded before edits;
each implementation is tested and measured before proceeding. A proposed change
is retained only when its benefit justifies its measured overhead. Negative
results remain in the evidence. No promise of monotonic speed on every input.

## 1. Freeze and extend measurement

- Preserve immutable baseline and instrumented binaries. Keep existing packing,
  network, irregular, import, and MIP controls unchanged.
- Add deterministic packing variants with different dimensions, degree, row/
  column permutations and planted primal/dual optima, and harder integer-domain
  fixtures with independently computed exact optima.
- Compile identical extended harnesses for baseline, candidate, and old HiGHS.
- Use a discarded process round, one untimed warmup, rotated backend ordering,
  retained medians/ranges, identical model fingerprints and acceptance checks.
- Acceptance: unchanged old fixture fingerprints; new LP KKT checks and exact
  integer reference checks pass. Time/node limits and failures remain visible.

## 2. Forward basis solves

- Separate sparse and dense RHS behavior in the static and updated U solves.
  Investigate gather versus scatter traversal, avoiding repeated indirect writes
  and expensive symbolic work on dense RHS; preserve sparse reach where useful.
- Reuse work storage; avoid changing the public model or numerical tolerances.
- Test general RHS and unit RHS, both orientations, cancellation, FT/eta updates,
  changing dimensions, and frequent refactors. Compare instrumented kernel time
  separately from uninstrumented wall time.
- Gate: packing improvement without new numerical failures or material broad
  regressions. Revert or narrow strategies which fail the gate.

## 3. Pricing and pivot vector work

- Profile after step 2, then remove a remaining whole-vector bottleneck using
  sparse supports or an incremental infeasibility candidate set. Any cached
  state must be rebuilt after recomputation, restoration, or bound changes.
- Preserve steepest-edge selection, deterministic ties and full optimality
  verification. Keep a dense fallback when sparse bookkeeping costs more.
- Gate: forward/transpose residual checks, old oracle cases, packing size sweep,
  irregular/network/MIP controls; record per-step medians.

## 4. MIP propagation

- Replace the short-row-only, repeated full-scan propagator with exact integer
  row activities and a bounded incidence-driven work queue, supporting long
  knapsack rows. Retain conservative eligibility and overflow bounds.
- Round integral row bounds safely; propagate at the root as well as children;
  restore every implied variable bound on node exit. Check cancellation and
  bound work so propagation cannot monopolize a solve.
- Gate: exhaustive small binary/general-integer and mixed-continuous cases,
  negative coefficients, ranged/equality rows, contradictory bounds, long rows,
  restoration, and limits. Compare nodes as well as time.

## 5. MIP search decisions

- Measure weaknesses after propagation. Improve initial branch scoring with
  structural information and reliable pseudocost history; evaluate a tightly
  budgeted root probing strategy only if it pays for itself.
- Probes may guide selection but must never prune using unchecked numerical
  estimates. Keep the existing conservative incumbent/bound certification.
- Gate: exact optima and safe bounds unchanged, hard fixture node reductions,
  easy fixture overhead bounded. Document rejected alternatives.

## 6. Broaden and compare

- Add independent seeded packing/covering, multidimensional knapsack, graph
  selection, general-integer and mixed-variable cases. Reserve separate seeds
  as holdouts, and obtain a small public Netlib LP corpus when accessible.
- Public inputs retain provenance, checksums and the exact transformations.
  No invented finite bounds: unsupported MIPs are reported as unsupported.
- Compare final with 0.19 and isolated HiGHS using paired fresh processes;
  report time, nodes, iterations, peak RSS, acceptance and all limits/failures.
- Re-run large irregular and network controls to detect loss of prior gains.

## 7. Final validation and delivery

- Rust all-feature tests and strict Clippy; Python wrapper tests; frozen oracle
  inputs with the existing adjudications applied unchanged; exact bounds audit;
  captured-basis replay in both orientations.
- Document the actual wins, regressions, unsupported cases and coverage limits.
- Deliver source, plan, per-step results, raw evidence and reproducible commands
  in a versioned ZIP. Verify archive CRC/checksums and provide a download link.

## Execution log

Plan recorded before implementation. Step results will be appended here and
in RESULTS.md; raw measurements live under data/.

### Step 1 completed

Immutable baseline binaries saved. Added degree-nine and permuted packing,
long-row binary and general-integer knapsacks (independent capacity DP), and
multi-knapsack/graph selection (exhaustive enumeration). All 16 smoke cases,
both warmup and retained solves, passed original-model checks. Existing fixture
constructors are unchanged. Baseline raw data: `data/step1-baseline.json`.

### Step 2 implementation decision

Test a gather U solve for dense RHS: sum each pivot row into a scalar and write
its result once, rather than repeatedly updating indirect RHS locations through
U columns. Keep the existing reach/scatter path for sparse RHS. Apply to static
and FT factors; no changes to dense-tail arithmetic or eta transformations.

### Step 2 completed

Gather traversal rejected: packing slowed 9–24%. Pivot-order workspace was
mixed and not retained. Zero-aware division plus cached reciprocal diagonals
retained: five-round packing2048 63.41 -> 55.89 ms, packing4096 253.10 ->
223.52 ms; packing1024 unchanged (27.05 -> 27.28 ms). Network512 improved
5.97 -> 5.24 ms. Irregular1024 regressed 4.2% in this run; final controls will
revisit it. All 16 fixture families/configurations accepted all five retained
solves. Added scaled forward/transpose sparse/dense RHS regression coverage;
existing update/fallback tests pass. All rejected experiments retained as raw
JSON. Production changes: lu.rs and ft.rs only.

### Step 3 implementation decision

Cache the exact best steepest-edge row in blocks of 64 basis positions. Mark
blocks touched by primal or edge-weight changes; invalidate all on primal
recomputation, basis restoration/reset and bound changes. Bound flips use full
invalidation. Scan block winners with the original strict-greater comparison
and ascending order, preserving exact ties. Verify cached and uncached choices
directly under sparse changes, resets and bound changes before benchmarking.

### Step 3 completed

Exact block pricing retained with direct-scan fallback below 512 rows. In a
five-round comparison against step 2: packing4096 239.30 -> 222.25 ms (-7.1%),
packing1024 35.93 -> 28.28 ms, wide1024 449.11 -> 425.55 ms, network512
6.04 -> 5.79 ms. Every accepted trajectory retained identical pivot counts.
A shortcut invalidating all blocks for moderately dense transformed columns
lost gains and was removed. A direct cached-versus-full-scan test exercises
300 sparse primal/weight changes, repeated choices and resets. Earlier mixed
runs retained in data/; final paired measurement will determine release claims.

### Step 4 implementation decision

Use exact i128 min/max activities plus variable-to-row incidence. Synchronize
both the previous node's changed variables and the current path, including
restored implied bounds, then queue only affected rows. The root queues every
eligible row. Retain coefficient/bound eligibility limits; use a per-call work
cap and cancellation checks. No change to conservative incumbent pruning.

### Step 4 completed

Long-row incremental propagation and root fixpoint processing implemented.
80 signed-row instances x five sibling branches were checked against all 1024
binary assignments; no feasible integer point was removed. A reverse 40-variable
chain verifies implications beyond three sweeps and sibling restoration.
Unconditional long-row scans caused a 43% nodecopy regression in the first run;
a conservative root-width slack guard removed most unnecessary term scans.
Follow-up nodecopy 1.45 -> 1.37 ms; integer-knapsack .721 -> .701 ms. Long-row
and multi-knapsack cases still have 4%/15% overhead and unchanged nodes. This is
a capability improvement with measured costs, not a general speedup claim.

### Step 5 implementation decision

Evaluate root strong branching on two candidate variables (at most four probes,
32 pivots each), restricted to small/medium multi-constraint models with at least
two long rows. This avoids setup on easy single-row and graph fixtures. Probes
reuse and restore the parent basis; they only choose a branch and initialize
pseudocosts, never prune from unchecked numerical estimates. Count every probe
as a processed relaxation and charge all pivots/time to the global budget.
Expose a switch for ablation. Keep only if paired hard-MIP results justify it.
