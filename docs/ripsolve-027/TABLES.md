# Recomputed benchmark tables

Shift 0.01 s. Unproved/error cases are charged the full time cap, including early node limits. Successful times are capped at the time limit. Infeasible proofs count as solved. Single observations; no statistical significance claim.

## public

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| baseline/default | 7/9 | 0.43306 |
| native/presolve | 7/9 | 0.25393 |
| native/gmi | 7/9 | 0.48886 |
| native/mir | 7/9 | 0.25547 |
| native/combined | 8/9 | 0.40235 |
| native/heuristics | 8/9 | 0.44163 |
| native/branch-product | 6/9 | 0.46665 |
| native/branch-clamped | 7/9 | 0.42463 |
| ripsolve/default | 8/9 | 0.16419 |
| highs/default | 9/9 | 0.07747 |

| Case | baseline/default | native/presolve | native/gmi | native/mir | native/combined | native/heuristics | native/branch-product | native/branch-clamped | ripsolve/default | highs/default |
|---|---|---|---|---|---|---|---|---|---|---|
| avgas | 0.0003 | 0.0002 | 0.0001 | 0.0001 | 0.0001 | 0.0002 | 0.0002 | 0.0002 | 0.0001 | 0.0073 |
| bell5 | 3.3224 [node_limit] | 3.2283 [node_limit] | 4.5071 [node_limit] | 3.4385 [node_limit] | 4.7642 [node_limit] | 4.4301 [node_limit] | 3.3289 [node_limit] | 3.0894 [node_limit] | 10.0069 [timelimit] | 0.5377 |
| egout | 0.4074 | 0.0109 | 0.2020 | 0.0243 | 0.3416 | 0.3714 | 0.3618 | 0.4196 | 0.0428 | 0.0208 |
| flugpl | 0.2507 | 0.2539 | 0.7029 | 0.2528 | 0.5398 | 0.4545 | 0.2254 | 0.2519 | 0.1995 | 0.1053 |
| gt2 | 2.7298 | 1.2543 | 1.3652 | 0.2338 | 1.2392 | 1.2293 | 3.5600 [node_limit] | 1.8614 | 0.2085 | 0.0584 |
| lseu | 0.5666 | 0.2408 | 1.0096 | 0.4320 | 1.2517 | 1.2074 | 0.5662 | 0.7113 | 0.4765 | 0.2230 |
| p0548 | 10.0640 [time_limit] | 9.3200 [node_limit] | 10.0773 [time_limit] | 10.0204 [time_limit] | 1.7220 | 1.8284 | 10.0349 [time_limit] | 10.0461 [time_limit] | 0.0520 | 0.0853 |
| p01 | 0.0004 | 0.0004 | 0.0005 | 0.0016 | 0.0017 | 0.0174 | 0.0003 | 0.0003 | 0.0087 | 0.0118 |
| rgn | 0.3461 | 0.3232 | 0.8397 | 0.5641 | 0.5393 | 0.5592 | 0.2297 | 0.3372 | 1.0547 | 0.2587 |

## public-60s

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| baseline/default | 8/9 | 0.55682 |
| native/combined | 8/9 | 0.49510 |
| native/heuristics | 8/9 | 0.48886 |
| ripsolve/default | 8/9 | 0.19267 |
| highs/default | 9/9 | 0.08359 |

| Case | baseline/default | native/combined | native/heuristics | ripsolve/default | highs/default |
|---|---|---|---|---|---|
| avgas | 0.0002 | 0.0001 | 0.0001 | 0.0001 | 0.0089 |
| bell5 | 13.6803 | 43.7868 [node_limit] | 44.1483 [node_limit] | 60.0412 [timelimit] | 0.6098 |
| egout | 0.4088 | 0.3362 | 0.3429 | 0.0398 | 0.0261 |
| flugpl | 0.2319 | 0.5419 | 0.3854 | 0.2020 | 0.1036 |
| gt2 | 2.8619 | 1.2041 | 1.1997 | 0.1785 | 0.0631 |
| lseu | 0.5613 | 1.3203 | 1.2010 | 0.5815 | 0.2384 |
| p0548 | 60.1640 [time_limit] | 1.7022 | 1.7272 | 0.0549 | 0.0840 |
| p01 | 0.0004 | 0.0016 | 0.0057 | 0.0018 | 0.0159 |
| rgn | 0.4040 | 0.5632 | 0.5534 | 1.0469 | 0.2487 |

## heldout

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| baseline/default | 9/11 | 0.03333 |
| native/presolve | 9/11 | 0.03222 |
| native/mir | 9/11 | 0.04248 |
| native/combined | 9/11 | 0.07283 |
| ripsolve/default | 10/11 | 0.02416 |
| highs/default | 11/11 | 0.02582 |

Measured regressions over 1.5× (includes submillisecond cases):

| Case | Policy | Ratio | Extra seconds |
|---|---|---|---|
| issue-2290 | native/presolve | 2.07 | 0.00012 |
| 2171 | native/mir | 3.34 | 0.03441 |
| bound_implied | native/mir | 5.57 | 0.00183 |
| infeasible-mip0 | native/mir | 11.05 | 0.00720 |
| issue-2095 | native/mir | 3.83 | 0.00100 |
| issue-2204 | native/mir | 2.03 | 0.00042 |
| issue-2290 | native/mir | 6.60 | 0.00063 |
| issue-2402 | native/mir | 2.09 | 0.00080 |
| 2171 | native/combined | 13.24 | 0.17998 |
| bound_implied | native/combined | 2.96 | 0.00079 |
| infeasible-mip0 | native/combined | 205.28 | 0.14639 |
| infeasible-mip1 | native/combined | 7.89 | 0.02673 |
| issue-2095 | native/combined | 4.65 | 0.00129 |
| issue-2173 | native/combined | 2.10 | 0.25892 |
| issue-2204 | native/combined | 2.44 | 0.00059 |
| issue-2290 | native/combined | 15.57 | 0.00164 |
| issue-2402 | native/combined | 5.80 | 0.00353 |

## families

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| baseline/default | 10/10 | 0.00041 |
| native/presolve | 10/10 | 0.00036 |
| native/mir | 10/10 | 0.00489 |
| native/combined | 10/10 | 0.01867 |
| ripsolve/default | 10/10 | 0.00058 |
| highs/default | 10/10 | 0.01701 |

Measured regressions over 1.5× (includes submillisecond cases):

| Case | Policy | Ratio | Extra seconds |
|---|---|---|---|
| heldout-bigm-113 | native/presolve | 1.59 | 0.00019 |
| heldout-bigm-110 | native/mir | 2.90 | 0.00090 |
| heldout-bigm-112 | native/mir | 3.03 | 0.00073 |
| heldout-bigm-113 | native/mir | 2.91 | 0.00062 |
| heldout-bigm-114 | native/mir | 2.93 | 0.00070 |
| heldout-knapsack-110 | native/mir | 57.16 | 0.02072 |
| heldout-knapsack-111 | native/mir | 27.09 | 0.01594 |
| heldout-knapsack-112 | native/mir | 13.76 | 0.00351 |
| heldout-knapsack-113 | native/mir | 26.92 | 0.00846 |
| heldout-knapsack-114 | native/mir | 14.68 | 0.00516 |
| heldout-bigm-110 | native/combined | 18.02 | 0.00809 |
| heldout-bigm-111 | native/combined | 3.86 | 0.00191 |
| heldout-bigm-112 | native/combined | 2.90 | 0.00068 |
| heldout-bigm-113 | native/combined | 2.83 | 0.00060 |
| heldout-bigm-114 | native/combined | 235.91 | 0.08493 |
| heldout-knapsack-110 | native/combined | 81.10 | 0.02955 |
| heldout-knapsack-111 | native/combined | 829.58 | 0.50627 |
| heldout-knapsack-112 | native/combined | 17.12 | 0.00444 |
| heldout-knapsack-113 | native/combined | 47.85 | 0.01530 |
| heldout-knapsack-114 | native/combined | 21.17 | 0.00762 |

## branch-heldout

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| native/default | 9/11 | 0.03247 |
| native/branch-product | 9/11 | 0.03267 |
| native/branch-clamped | 9/11 | 0.03201 |

## branch-families

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| native/default | 10/10 | 0.00033 |
| native/branch-product | 10/10 | 0.00035 |
| native/branch-clamped | 10/10 | 0.00030 |

## ripsolve-serial

| Solver/policy | Proved optimal/infeasible | Shifted geometric mean (s) |
|---|---|---|
| ripsolve/serial | 8/9 | 0.19204 |

## Root LP objectives

These are relaxation objectives; checked conservative bounds are separate fields in raw records. — means no root MIP diagnostic (including the LP control avgas).

| Case | Original LP | Baseline root | W1 | W1+GMI | W1+MIR | Combined |
|---|---|---|---|---|---|---|
| avgas | -7.75 | — | — | — | — | — |
| bell5 | 8.60842e+06 | 8.60842e+06 | 8.60842e+06 | 8.67748e+06 | 8.60842e+06 | 8.681e+06 |
| egout | 149.589 | 367.085 | 511.618 | 566.457 | 528.256 | 568.101 |
| flugpl | 1.16719e+06 | 1.16719e+06 | 1.16719e+06 | 1.1723e+06 | 1.16719e+06 | 1.17227e+06 |
| gt2 | 13,460.2 | 13,460.2 | 13,460.2 | 19,497.1 | 20,522.9 | 20,006.8 |
| lseu | 834.682 | 834.682 | 944.754 | 1,015.63 | 1,023.23 | 1,043.08 |
| p0548 | 315.255 | 429.683 | 3,126.38 | 6,740.87 | 6,103.77 | 8,677.04 |
| p01 | 263 | 263 | 263 | 263 | 263 | 263 |
| rgn | 48.8 | 48.8 | 48.8 | 52.0804 | 68 | 68 |

## Work and candidate counters (10 s)

| Case | Policy | LP nodes | Pivots | GMI cuts | MIR cuts | Root heuristic candidates |
|---|---|---|---|---|---|---|
| avgas | baseline/default | 0 | 6 | — | — | — |
| avgas | native/mir | 0 | 6 | — | — | — |
| avgas | native/combined | 0 | 6 | — | — | — |
| avgas | native/heuristics | 0 | 6 | — | — | — |
| bell5 | baseline/default | 100000 | 105544 | — | — | — |
| bell5 | native/mir | 100000 | 109886 | 0 | 0 | 0 |
| bell5 | native/combined | 100000 | 123093 | 22 | 3 | 0 |
| bell5 | native/heuristics | 100000 | 123734 | 22 | 3 | 0 |
| egout | baseline/default | 12602 | 14718 | — | — | — |
| egout | native/mir | 165 | 272 | 0 | 9 | 0 |
| egout | native/combined | 26 | 219 | 16 | 9 | 0 |
| egout | native/heuristics | 32 | 839 | 16 | 9 | 0 |
| flugpl | baseline/default | 23947 | 15622 | — | — | — |
| flugpl | native/mir | 25082 | 16105 | 0 | 0 | 0 |
| flugpl | native/combined | 18578 | 12830 | 22 | 1 | 0 |
| flugpl | native/heuristics | 12552 | 8928 | 21 | 1 | 0 |
| gt2 | baseline/default | 72182 | 147116 | — | — | — |
| gt2 | native/mir | 1106 | 6950 | 0 | 62 | 0 |
| gt2 | native/combined | 6990 | 45984 | 13 | 63 | 0 |
| gt2 | native/heuristics | 7007 | 47527 | 13 | 63 | 0 |
| lseu | baseline/default | 19747 | 47935 | — | — | — |
| lseu | native/mir | 5267 | 19274 | 0 | 31 | 0 |
| lseu | native/combined | 1923 | 8892 | 11 | 52 | 0 |
| lseu | native/heuristics | 1940 | 9433 | 11 | 52 | 0 |
| p0548 | baseline/default | 59183 | 259423 | — | — | — |
| p0548 | native/mir | 28015 | 214151 | 0 | 256 | 0 |
| p0548 | native/combined | 271 | 1306 | 28 | 163 | 0 |
| p0548 | native/heuristics | 288 | 5208 | 28 | 163 | 0 |
| p01 | baseline/default | 1 | 25 | — | — | — |
| p01 | native/mir | 2 | 50 | 0 | 0 | 0 |
| p01 | native/combined | 2 | 50 | 0 | 0 | 0 |
| p01 | native/heuristics | 11 | 546 | 0 | 0 | 1 |
| rgn | baseline/default | 7249 | 33502 | — | — | — |
| rgn | native/mir | 5398 | 36908 | 0 | 46 | 0 |
| rgn | native/combined | 4407 | 26998 | 4 | 41 | 0 |
| rgn | native/heuristics | 4417 | 27175 | 4 | 41 | 1 |
