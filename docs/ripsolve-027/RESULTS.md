# Native 0.27: ripsolve plan review, implementation and results

Implemented against dietmarwo/LP-IP-craft main `825ee42679c3892310d8658d2a71678b88cb6dd3` (native 0.26.0). All W0–W5 mechanisms in the reviewed plan are implemented and tested. Performance targets are mixed; the new controls remain opt-in and W6 remains deferred. This is a local source delivery and patch, with no upstream push or published release.

The main improvement is p0548: the combined presolve/GMI/MIR profile proves objective **8691 in 1.70–1.72 seconds**, against a baseline time limit at 60 seconds with incumbent 28397. Its checked root relaxation reaches **8677.0366**, closing about 99.8% of the original LP gap. This does not meet the original under-one-second target. Combining all cuts is not a generally faster policy: it regresses on bell5 and many easy holdouts.

## What changed

| Workstream | Implementation and review outcome |
|---|---|
| W0 measurement | Pinned common-array Rust reference harness, independent Python/HiGHS checks, raw diagnostics, 11 frozen public holdouts, disjoint seeded big-M/knapsack families, reproducible tables and reference ablations. avgas is a continuous LP control, so the development set contains eight MIPs plus one LP. |
| W1 presolve | Bounded exact/outward propagation, redundant-row removal, forcing observations, exact binary coefficient tightening and optional objective-directed singleton/empty-column fixes. The latter preserve an optimum rather than every feasible point. ripsolve's conflict-aware presolve is broader than this box-based implementation. |
| W2 GMI | Numerical BTRAN multipliers propose an exact reaggregation of original equations; small basic-column residuals are retained. Exact GMI construction and box-compensated binary64 conversion replace the original plan's insufficient floating-tableau rounding argument. Selected cuts extend the combinatorial basis and refactor the enlarged system. |
| W3 MIR | Exact MIR with rational variable-upper-bound substitution, valid shifts/complements, bounded divisor search and the shared safe conversion/selection path. Both families receive a work share when enabled together. |
| W4 heuristics | Checked root rounding, guided diving and genuine L1 feasibility-pump projection LPs, with parent restoration, cycles/backoff and shared whole-call budgets. Heuristic objectives never become pruning bounds. |
| W5 branching | Product-score and minimum-step pseudocost normalization experiments, bounded candidate/history traces and fixed-candidate scoring tests. Native already had directional global priors; the plan no longer claims to add them. Negative experiments are retained. |

See PLAN.md for the detailed contract, IMPLEMENTATION.md for every option/default and VALIDITY.md for the arithmetic arguments. No new production numerical dependency was introduced. Reference software is benchmark-only with pinned source and licences.

## Validation

| Check | Result |
|---|---|
| Full Rust workspace | 172 tests passed, including native unit/integration tests and doctests. |
| Full Python suite | 257 passed, including 65 independent exhaustive general-integer models under three new configurations (195 solves). |
| Frozen original oracle | 6330 cases under default and 6330 under combined experimental settings; zero failures after unchanged frozen adjudications. The raw worker's 19 pre-existing oracle disagreements remain visible. |
| Bound formula | Unchanged 400-model exact Fraction Lagrangian audit under each policy; zero unsafe finite bounds. Infinite/unavailable bounds are reported rather than replaced. |
| Cut/reduction fixtures | Exhaustive feasible-point checks for GMI, numerical BTRAN residuals, mixed VUB/MIR slices, negative bounds, tiny-term compensation and binary coefficient reductions; optimum comparisons for dual fixes. |
| Limits and restoration | Shared node/pivot limits, checked starts, heuristic parent restoration and bounded traces exercised. |
| Packing compatibility | Both full SpOC4 repairs use eight rounds, 64 tasks/round and four workers: 512 tasks each. Saved original-order vectors are byte-identical to 0.26; objectives 32847.565 and 70945.093, bounds 34153.324 and 76765.388. |

Default/all-feature Clippy, formatting and numerical all-target/all-feature tests are recorded with the final validation logs. The benchmark audit rechecks input fingerprints, identical model arrays, every returned candidate, objective consistency and all available reference bounds/statuses. Unsupported domains and time/node limits remain visible as unsuccessful results. These checks supplement native's documented numerical-search scope; this release does not emit a complete exact tree proof log.

## Root relaxation results

These are numerical root LP objectives from the final 10-second comparison, rather than the integer optimum or final global bound. Raw records retain separately checked conservative bounds and every resolved round.

| Case | W1 only | W1 + GMI | W1 + MIR | Combined | Original target |
|---|---:|---:|---:|---:|---|
| egout | 511.618 | 566.457 | 528.256 | 568.101 | Presolve ≥500 met |
| gt2 | 13460.233 | 19497.100 | 20522.928 | 20006.825 | Root ≥20000 met by MIR/combined; GMI alone misses |
| p0548 | 3126.383 | 6740.873 | 6103.766 | 8677.037 | Presolve ≥4500 missed; combined root ≥8500 met |
| bell5 | 8608417.947 | 8677482.757 | 8608417.947 | 8680995.160 | Root ≥8940000 missed |

p0548's combined profile adds 28 GMI and 163 MIR cuts, with 16 successful basis extensions. The root exact arithmetic is the dominant wall-time cost; the diagnostic LP times alone do not account for it. The bounded pool is retained without cut aging/deletion. These results use 16 cut rounds; the API default for an explicitly enabled family is eight.

## Timings and limits

At 60 seconds, native uses a 1,000,000-node / 10,000,000-pivot cap. Reference node caps are configured where supported; their APIs do not provide the same global pivot cap. All timed processes run serially after builds. The nine-case totals below include the LP control. Shifted geometric means use shift 0.01 seconds and charge any unproved/error result the full time cap, even when a node limit stops it early.

| Profile | Solved | Shifted geometric mean |
|---|---:|---:|
| Baseline 0.26 | 8/9 | 0.557 s |
| Native combined | 8/9 | 0.495 s |
| Native combined + root heuristics | 8/9 | 0.489 s |
| ripsolve default, B&B threads=1 | 8/9 | 0.193 s |
| ripsolve strictly serial, CDCL disabled | 8/9 | 0.192 s |
| HiGHS | 9/9 | 0.084 s |

ripsolve's default CDCL may add a background thread even with `threads=1`. A separate strict-serial profile disables CDCL; its measured results are in TABLES.md. Native and HiGHS use one solver thread. Rust timing excludes model parsing/building; the Python baseline includes wrapper work and HiGHS includes import/setup. These are single observations on the recorded host, not repeated statistical comparisons or isolated kernel timings.

gt2 improves from 2.730 seconds / 72182 LP nodes to **0.234 seconds / 1106 nodes with MIR-only** in the 10-second experiment. egout improves from 0.407 seconds / 12602 nodes to **0.011 seconds / 276 nodes with presolve-only**. The combined policy is slower than these simpler choices. lseu's combined tree shrinks from 19747 to 1923 nodes, but time increases from 0.567 to 1.252 seconds because of root work.

bell5 is a clear regression: at the larger cap the baseline proves optimality in 13.680 seconds / 374503 nodes. Combined cuts find the same optimal incumbent but hit 1000000 nodes after 43.787 seconds without proving it. ripsolve also remains unproved at its 60-second limit. The goal of 9/9 proofs in 60 seconds is therefore unmet for every tested single native policy. Neither the ripsolve timing target nor the stretch goal of within 3× of HiGHS is met.

## Held-out and negative results

The deterministic selection rule yields 11 eligible public MIPs, rather than the proposed 20; all exclusions and original MPS bytes are retained. At three seconds, baseline and each new profile prove 9/11 (including four infeasible instances). issue-2290 retains the baseline's unsupported native bound domain. sp150x300d remains unproved; combined improves its incumbent from 71 to 70 and bound from 37.5625 to 51.9318, while HiGHS proves 69.

The no-regression target is not met. For example, 2171 rises from 0.0147 seconds to 0.1947 seconds under combined cuts, and issue-2173 from 0.2349 to 0.4938. Exact cut generation is often wasted work on easy models. All ten additional held-out seeded family models solve correctly, but combined cuts can add several tenths of a second to a submillisecond baseline. TABLES.md lists every measured >1.5× regression with absolute overhead.

The root heuristic experiment does not produce a p0548 incumbent at the root, and adds no meaningful node reduction on gt2/lseu beyond the cuts. Their end-to-end node reductions come from relaxation strengthening, not demonstrated success of the new root heuristics. The triangle fixture does demonstrate root incumbent admission. The original heuristic performance goals remain unmet.

The 0.01 normalization floor improves gt2 to 1.861 seconds / 44004 nodes, but slows lseu to 0.711 seconds / 20771 nodes. Product scoring reaches the gt2 100000-node limit without proof. Held-out branching experiments retain all policies and outcomes; they do not justify a default change. Source differences include ripsolve's four-observation reliability threshold, eight candidates and 100-pivot probes versus native's separately configurable, default-disabled reliability probing, plus different LP bases, priors/smoothing and score. The missing historical `bare_bb` harness cannot be reconstructed exactly from source, and the new `bare` options profile is labelled accordingly. We do not claim a unique cause for the historical cross-solver node gap.

## Reference review and delivery

The four-case options/family ablations reproduce the importance of presolve and GMI/MIR on p0548 and gt2, and retain negative cut overhead on easy problems. Reference root fields belong to the returned restart pass; one ablation reports a p0548 root field of 8724 while returning optimum/bound 8691. Those internal fields are not treated as certified original-model bounds.

`restart_share=0` reproduces wrong Optimal statuses for lseu and bell5, and an open bound gap for rgn. Full outputs and an unsubmitted issue draft are included. Source inspection suggests a missing zero guard and a restart request that can leave an open frontier with an Optimal status; this is an explanation to investigate, not an upstream fix claim.

REPRODUCE.md and the benchmark README document exact profiles, commands, reference limitations and the corrected HiGHS LP adapter. TABLES.md is generated from the raw data by summarize.py; pilot observations are explicitly excluded. The delivery includes the full source, tests, frozen oracle inputs/adjudications, held-out provenance, validation logs, a baseline patch and file checksums. W6 restarts/CDCL/mod-2/clique/shared-tree development remains future work.
