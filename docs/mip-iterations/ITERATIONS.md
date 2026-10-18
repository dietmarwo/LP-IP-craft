# Twenty iterations on the open 0.27 issues

Starting point: main `3f928fa` (0.27 with review fixes, PR #7). The open
issues are listed in [../ripsolve-027/REVIEW.md](../ripsolve-027/REVIEW.md):

- combined cuts slower than the better single family;
- rgn slower with cuts than without;
- milliseconds of root work on small models;
- bell5's root target;
- the root incumbent on p0548, and incumbent finding on gt2;
- issue-2290's unsupported domain;
- sp150x300d unsolved;
- defaults unchanged.

## Protocol

Fixed before the first iteration:

- **Evaluation.** `benchmarks/rust-solvers/run.py`, native only, policies
  default, gmi, mir, combined and heuristics, on four sets:
  - the 9 public development cases, 10 s cap;
  - the new synthetic development set (24 models), 5 s;
  - the 11 held-out public MIPs, 3 s;
  - the 10 held-out seeded families, 3 s.
  Every run is a single fresh process, serial, on a 2-core host. The
  summary is the shifted geometric mean (shift 0.01 s, unproved charged the
  cap).
- **Synthetic set.** `benchmarks/rust-solvers/synthetic.py` makes six
  standard classes × 4 seeds: correlated multidimensional knapsack, set
  cover, capacitated facility location (VUB rows), generalized assignment,
  capacitated lot sizing and general-integer covering. A second group of
  24 (`test`, seeds 101–104) is evaluated only at the end. HiGHS 1.15.1
  proved all 48 optimal; those optima are frozen in the inputs, so a wrong
  optimality claim is flagged.
- **Acceptance.** An iteration is accepted only if all of these hold:
  - no solved case is lost;
  - no set's mean gets more than 3% worse for any policy (the noise level
    seen with unchanged code);
  - the targeted measure improves;
  - default results are unchanged, unless the iteration is about defaults.
  Rejected iterations are reverted and recorded here.
- **Correctness.** Unit tests in each iteration. For accepted iterations,
  also the frozen 6,330-case oracle under default and combined settings.
  The full gates run again at the end.

Iteration 0 (main) on this host:

| Set | default | gmi | mir | combined | heuristics |
|---|---|---|---|---|---|
| public | 7/9, 0.522 | 9/9, 0.166 | 7/9, 0.241 | 9/9, 0.178 | 9/9, 0.199 |
| synthetic | 24/24, 0.0449 | 24/24, 0.0399 | 24/24, 0.0320 | 24/24, 0.0469 | 24/24, 0.0527 |
| held-out | 9/11, 0.0334 | 9/11, 0.0361 | 9/11, 0.0353 | 9/11, 0.0380 | 9/11, 0.0432 |
| families | 10/10, 0.0004 | 10/10, 0.0011 | 10/10, 0.0011 | 10/10, 0.0025 | 10/10, 0.0025 |

## Iterations

### 1. In-place exact accumulation for GMI — accepted

- **Plan.** Callgrind on synthetic gap-2 (combined) put 80% of
  instructions in GMI. Almost half was its exact accumulator, whose every
  addition realigned and renormalized heap integers. Sum the aggregation
  and back-substitution products in place instead, as two's-complement
  fixed-point integers of 1536 bits at the smallest exponent involved
  (`dyadic::Wide`). An addend that does not fit makes that sum repeat in
  the existing dyadic code. Both are exact, so cuts must not change.
- **Test.** A new unit test checks in-place sums against dyadic sums on
  random multipliers up to 300 bits and binary64 values over a 1500-bit
  exponent spread, including the fallback.
- **Result.** All 270 records are search-identical to iteration 0 (status,
  objective, nodes, pivots). Means improve by up to 13% (synthetic GMI
  0.0399 → 0.0349 s, public MIR 0.241 → 0.224 s). Examples: gap-2
  combined 0.256 → 0.131 s, lseu GMI 0.422 → 0.274 s. Held-out and family
  means moved within noise (default moved by as much).

### 2. Stop separating a family that stops contributing — accepted

- **Plan.** On facility location and lot sizing, combined selected exactly
  GMI-only's cuts (zero MIR cuts in any round), yet ran MIR separation on
  every row each round. That tripled root time: cfl-1 took 0.025 s,
  against 0.007 s for GMI alone. Rule: a family with no selected cut in a
  round is not separated again; the remaining family gets the whole work
  budget, as it would alone. Combined only; single-family policies cannot
  change.
- **Result.** Combined means improve on all four sets:

  | Set | Before | After |
  |---|---:|---:|
  | public | 0.173 s | 0.163 s |
  | synthetic | 0.0420 s | 0.0375 s |
  | held-out | 0.0392 s | 0.0362 s |
  | families | 0.0026 s | 0.0014 s |

  bell5 drops from 4.40 to 3.58 s and rgn from 0.70 to 0.57 s. Nothing is
  lost or slower beyond the flag threshold, and only combined/heuristics
  records change. Oracle gates: 0 failures, default and combined.
