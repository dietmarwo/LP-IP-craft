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
