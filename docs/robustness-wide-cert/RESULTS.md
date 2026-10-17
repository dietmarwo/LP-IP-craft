# 0.21.0: numerical robustness, wide packing and certification reuse

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


## Extended controls

| Workload | 0.20 ms | 0.21 ms | HiGHS ms | 0.21 / 0.20 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| packing 1024 | 26.879 | 29.405 | 22.274 | 1.094× | 0 → 0 |
| packing 2048 | 54.557 | 57.823 | 62.553 | 1.060× | 0 → 0 |
| packing 4096 | 225.086 | 233.311 | 176.930 | 1.037× | 0 → 0 |
| packing-wide 256 | 13.653 | 12.357 | 10.676 | 0.905× | 0 → 0 |
| packing-wide 1024 | 392.178 | 320.208 | 205.343 | 0.816× | 0 → 0 |
| packing-wide 1536 | 876.611 | 767.806 | 472.924 | 0.876× | 0 → 0 |
| packing-permuted 1024 | 13.115 | 13.524 | 13.850 | 1.031× | 0 → 0 |
| network 512 | 4.985 | 5.123 | 8.683 | 1.028× | 0 → 0 |
| irregular 256 | 6.392 | 6.164 | 6.487 | 0.964× | 0 → 0 |
| irregular 1024 | 149.293 | 146.462 | 196.157 | 0.981× | 0 → 0 |
| nodecopy | 0.785 | 0.694 | 47.185 | 0.884× | 42 → 42 |
| selection 20 | 0.203 | 0.225 | 4.871 | 1.113× | 11 → 11 |
| selection 32 | 0.294 | 0.262 | 5.858 | 0.891× | 10 → 10 |
| long-row-mip 64 | 0.499 | 0.375 | 4.055 | 0.751× | 16 → 16 |
| long-row-mip 112 | 1.915 | 1.158 | 20.973 | 0.605× | 50 → 50 |
| integer-knapsack 32 | 0.570 | 0.543 | 19.416 | 0.951× | 53 → 53 |
| integer-knapsack 49 | 0.709 | 0.610 | 11.640 | 0.860× | 41 → 41 |
| multi-knapsack 18 | 0.292 | 0.281 | 5.658 | 0.963× | 22 → 22 |
| graph-mip 18 | 0.224 | 0.277 | 5.186 | 1.238× | 6 → 6 |

## Previous seeded holdouts

| Workload | 0.20 ms | 0.21 ms | HiGHS ms | 0.21 / 0.20 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| multi-knapsack 1018 | 0.610 | 0.707 | 22.215 | 1.159× | 39 → 39 |
| multi-knapsack 2018 | 0.489 | 0.473 | 23.211 | 0.968× | 37 → 37 |
| multi-knapsack 3018 | 0.096 | 0.124 | 3.737 | 1.286× | 6 → 6 |
| multi-knapsack 4018 | 0.235 | 0.181 | 18.882 | 0.771× | 12 → 12 |
| graph-mip 1018 | 0.080 | 0.080 | 5.044 | 0.995× | 1 → 1 |
| graph-mip 2018 | 0.348 | 0.344 | 13.173 | 0.988× | 12 → 12 |
| graph-mip 3018 | 0.166 | 0.186 | 5.029 | 1.117× | 5 → 5 |
| mixed-overload 1016 | 0.176 | 0.178 | 19.283 | 1.007× | 25 → 25 |
| mixed-overload 2016 | 0.218 | 0.155 | 5.119 | 0.710× | 23 → 23 |
| mixed-overload 3016 | 0.065 | 0.066 | 4.287 | 1.009× | 5 → 5 |
| integer-knapsack 48 | 0.201 | 0.155 | 3.700 | 0.770× | 9 → 9 |
| long-row-mip 96 | 0.083 | 0.084 | 4.863 | 1.014× | 1 → 1 |

## Previous fresh-seed controls

| Workload | 0.20 ms | 0.21 ms | HiGHS ms | 0.21 / 0.20 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| multi-knapsack 5018 | 0.256 | 0.278 | 4.225 | 1.087× | 24 → 24 |
| multi-knapsack 6018 | 0.599 | 0.442 | 34.035 | 0.738× | 32 → 32 |
| multi-knapsack 7018 | 0.537 | 0.346 | 26.266 | 0.646× | 30 → 30 |
| graph-mip 4018 | 0.082 | 0.082 | 5.659 | 0.992× | 1 → 1 |
| mixed-overload 4016 | 0.241 | 0.179 | 11.062 | 0.743× | 27 → 27 |
| integer-knapsack 49 | 0.713 | 0.556 | 11.583 | 0.780× | 41 → 41 |
| long-row-mip 112 | 1.672 | 1.148 | 19.895 | 0.687× | 50 → 50 |

## New holdouts after implementation freeze

| Workload | 0.20 ms | 0.21 ms | HiGHS ms | 0.21 / 0.20 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| multi-knapsack 8018 | 0.100 | 0.096 | 10.533 | 0.957× | 4 → 4 |
| multi-knapsack 9018 | 0.139 | 0.132 | 3.962 | 0.952× | 9 → 9 |
| graph-mip 5018 | 0.308 | 0.353 | 6.127 | 1.146× | 11 → 11 |
| mixed-overload 5016 | 0.052 | 0.052 | 11.024 | 1.011× | 3 → 3 |
| integer-knapsack 57 | 1.520 | 1.255 | 12.574 | 0.826× | 88 → 88 |
| long-row-mip 128 | 5.480 | 4.489 | 16.930 | 0.819× | 250 → 250 |
| packing-wide 2048 | 1898.773 | 1584.938 | 736.664 | 0.835× | 0 → 0 |

## Public original models

| Workload | 0.20 ms | 0.21 ms | HiGHS ms | 0.21 / 0.20 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| 25fv47 | numerical_failure | 418.843 | 131.174 | — | 0 → 0 |
| adlittle | 0.565 | 0.594 | 1.354 | 1.051× | 0 → 0 |
| afiro | 0.062 | 0.063 | 0.489 | 1.020× | 0 → 0 |
| avgas | 0.034 | 0.034 | 0.241 | 1.012× | 0 → 0 |
| bell5 | error | error | 623.650 | — | — |
| e226 | 6.696 | 6.229 | 6.164 | 0.930× | 0 → 0 |
| egout | error | error | 15.253 | — | — |
| etamacro | 9.768 | 10.022 | 8.026 | 1.026× | 0 → 0 |
| flugpl | error | error | 116.319 | — | — |
| israel | 2.856 | 2.870 | 3.024 | 1.005× | 0 → 0 |

## All earlier standard performance checks

Includes legacy models, packing/network/irregular/selection sizes, cold MIP
solves, imports, transport, and separately labelled component parallelism.

| Workload / configuration | 0.20 ms | 0.21 ms | HiGHS ms |
|---|---:|---:|---:|
| fixed | 0.094 | 0.098 | 1.645 |
| pivot | 0.503 | 0.471 | 2.520 |
| knapsack | 0.063 | 0.060 | 4.286 |
| partition | 0.020 | 0.020 | 0.253 |
| nodecopy | 0.759 | 0.668 | 45.882 |
| redundant | 0.999 | 0.960 | 6.923 |
| blocks | 2.013 | 1.950 | 7.275 |
| packing 64 | 0.212 | 0.210 | 0.670 |
| packing 256 | 1.456 | 1.440 | 3.205 |
| packing 1024 | 28.167 | 30.205 | 22.358 |
| packing 2048 | 56.626 | 60.457 | 64.528 |
| packing 4096 | 221.970 | 243.180 | 162.978 |
| network 32 | 0.074 | 0.074 | 0.605 |
| network 128 | 0.433 | 0.508 | 1.360 |
| network 512 | 5.143 | 4.907 | 8.517 |
| network 2048 | 77.106 | 82.302 | 99.459 |
| irregular 64 | 0.515 | 0.460 | 0.823 |
| irregular 256 | 6.652 | 6.311 | 6.155 |
| irregular 1024 | 153.458 | 123.808 | 187.916 |
| selection 12 | 0.035 | 0.034 | 6.855 |
| selection 12 cold | 0.035 | 0.035 | — |
| selection 20 | 0.197 | 0.218 | 4.622 |
| selection 20 cold | 0.208 | 0.203 | — |
| selection 32 | 0.294 | 0.306 | 5.383 |
| selection 32 cold | 0.394 | 0.392 | — |
| selection 64 | 0.125 | 0.128 | 4.935 |
| selection 64 cold | 0.128 | 0.121 | — |
| box-lp | 4.604 | 5.217 | 11.974 |
| sparse-import-lp | 23.489 | 22.228 | 110.365 |
| transport-lp | 2.311 | 2.602 | 13.267 |
| partition-mip | 39.901 | 39.190 | 69.979 |
| blocks workers=16 | 1.385 | 1.283 | — |
| partition-mip workers=16 | 38.948 | 40.891 | — |

## Large irregular control

| Backend | Median seconds | Range seconds | Accepted |
|---|---:|---|---:|
| baseline | 7.477 | 6.696–7.866 | 3/3 |
| native | 5.717 | 5.487–6.325 | 3/3 |
| highs | 10.407 | 9.605–10.816 | 3/3 |

## Validation

- 115 Rust checks, 169 Python tests; strict all-target/all-feature Clippy and formatting pass.
- 20,370 frozen oracle cases: default, forced FT, fixing disabled, refactor-every-pivot exact cases and four-worker components. No failures after the unchanged prior adjudications. Raw flags remain explicit (19 per main pass); no new adjudication was invented.
- 5,120 exact i128 dyadic Lagrangian states validate both fresh and prepared enclosures; bound updates, invalid inputs, unbounded terms, overflow recovery, immutable row failures and both senses are covered.
- 400 independent exact-rational bound checks: 0 unsafe finite bounds.
- 84 frozen-basis replay records; worst backward error 2.17e-12 < 1e-9. Original 10 public corpus source/JSON hashes are unchanged.

## Remaining deficiencies

All seven public LPs now pass, including 25fv47, but 25fv47 still needs many
more pivots and remains slower than HiGHS. Wide LPs retain dense-tail and sparse
factorization costs and can still trail HiGHS. MIP search still lacks mature
cutting planes and requires finite bounds on every variable; egout, flugpl and
bell5 remain unsupported while HiGHS solves them. Small-workload regressions
and timing variability are visible in the complete tables; improvements are
not uniform. The real application instance is still unavailable.

Reproduction: build the current harness, restore the verified 0.20 archive
and pinned HiGHS reference, then use `compare_three.py`,
`compare_packing_mip.py`, `compare_large.py`, `replay_compare.py`,
`oracle_worker.py`, `check_bounds.py`, and `audit_robustness_wide_cert.py`.
Binary/source hashes and all raw results are included.

