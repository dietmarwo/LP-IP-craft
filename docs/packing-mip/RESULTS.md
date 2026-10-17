# 0.20.0: packing solves, integer-domain fixing, and broader coverage

## Scope and protocol

Baseline 0.19.0: `0c31318a6abd6533af54675585421a0f2469786d`.
Owned safe Rust production kernel; HiGHS remains only an isolated reference.
Each timing batch runs sequentially, after compilation/validation, on a shared
AMD EPYC host without core exclusivity. One discarded rotated/reversed process
round, one process warmup, then five retained standard/extended/holdout rounds;
three retained public and large-irregular rounds. Construction and independent
checks excluded, internal conversion and checking included. Single numerical
thread; standard separately labelled component-parallel controls retained.
Same model fingerprints, limits and acceptance tolerances across backends.
Raw medians/ranges, peak RSS, nodes, pivots and effective options are saved.

## What changed

- Cached reciprocal LU/FT diagonals and zero-aware forward triangular arithmetic.
- Exact steepest-edge winner caching in blocks of 64, with full invalidation on
  recomputation/restoration/bound changes and direct scanning for small bases.
- Exact i128 integer-row activities and an incidence-driven bounded propagation
  queue, including long rows, root processing and sibling restoration.
- Smoothed pseudocost estimates and a balanced degradation score.
- Certified opposite-domain fixing: numerical reduced costs only shortlist;
  conservative core dual bounds against a certified incumbent justify each
  inherited integer fixing. The option `reduced_cost_fixing` defaults to true.
- Optional root probes, limited to four 32-pivot solves, charged to global
  budgets; `strong_branching` defaults to false after negative measurements.

## Standard controls

| Workload | 0.19 ms | 0.20 ms | HiGHS ms | 0.20 / 0.19 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| packing 1024 | 28.135 | 27.171 | 22.373 | 0.966× | 0 → 0 |
| packing 2048 | 66.096 | 53.951 | 66.414 | 0.816× | 0 → 0 |
| packing 4096 | 263.673 | 216.343 | 159.737 | 0.820× | 0 → 0 |
| network 2048 | 88.733 | 73.055 | 96.636 | 0.823× | 0 → 0 |
| irregular 256 | 6.255 | 6.393 | 6.145 | 1.022× | 0 → 0 |
| irregular 1024 | 156.848 | 147.215 | 197.092 | 0.939× | 0 → 0 |
| nodecopy | 1.246 | 0.708 | 44.527 | 0.568× | 105 → 42 |
| selection 20 | 0.283 | 0.225 | 4.318 | 0.795× | 15 → 11 |
| selection 32 | 0.274 | 0.287 | 5.345 | 1.047× | 10 → 10 |

## Extended development controls

| Workload | 0.19 ms | 0.20 ms | HiGHS ms | 0.20 / 0.19 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| packing 1024 | 30.394 | 27.193 | 23.349 | 0.895× | 0 → 0 |
| packing 2048 | 67.084 | 54.571 | 61.731 | 0.813× | 0 → 0 |
| packing 4096 | 250.648 | 218.895 | 163.269 | 0.873× | 0 → 0 |
| packing-wide 256 | 13.527 | 14.234 | 11.229 | 1.052× | 0 → 0 |
| packing-wide 1024 | 406.801 | 421.229 | 217.167 | 1.035× | 0 → 0 |
| packing-permuted 1024 | 14.145 | 13.534 | 14.394 | 0.957× | 0 → 0 |
| network 512 | 6.414 | 5.621 | 8.541 | 0.876× | 0 → 0 |
| irregular 256 | 6.040 | 6.020 | 6.398 | 0.997× | 0 → 0 |
| irregular 1024 | 149.186 | 151.568 | 184.471 | 1.016× | 0 → 0 |
| nodecopy | 1.318 | 0.721 | 44.699 | 0.547× | 105 → 42 |
| selection 20 | 0.285 | 0.217 | 4.227 | 0.761× | 15 → 11 |
| selection 32 | 0.262 | 0.270 | 5.314 | 1.030× | 10 → 10 |
| long-row-mip 64 | 0.889 | 0.469 | 3.724 | 0.527× | 93 → 16 |
| integer-knapsack 32 | 0.681 | 0.573 | 21.264 | 0.840× | 108 → 53 |
| multi-knapsack 18 | 0.517 | 0.299 | 6.145 | 0.579× | 51 → 22 |
| graph-mip 18 | 0.246 | 0.226 | 5.544 | 0.917× | 6 → 6 |

## Reserved seeded holdouts

These tags were not used to choose the original score or kernel parameters.
The first holdout results prompted the separately planned certified-fixing step;
they are therefore regression/coverage cases after that step, not a pristine
statistical holdout. All cases and pre-fixing results remain available.

| Workload | 0.19 ms | 0.20 ms | HiGHS ms | 0.20 / 0.19 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| multi-knapsack 1018 | 0.702 | 0.719 | 22.517 | 1.024× | 63 → 39 |
| multi-knapsack 2018 | 0.530 | 0.487 | 20.106 | 0.918× | 56 → 37 |
| multi-knapsack 3018 | 0.200 | 0.097 | 3.403 | 0.487× | 25 → 6 |
| multi-knapsack 4018 | 0.248 | 0.213 | 18.451 | 0.860× | 25 → 12 |
| graph-mip 1018 | 0.076 | 0.077 | 4.986 | 1.022× | 1 → 1 |
| graph-mip 2018 | 0.389 | 0.323 | 13.070 | 0.831× | 12 → 12 |
| graph-mip 3018 | 0.166 | 0.177 | 5.438 | 1.062× | 5 → 5 |
| mixed-overload 1016 | 0.151 | 0.175 | 18.736 | 1.156× | 20 → 25 |
| mixed-overload 2016 | 0.155 | 0.152 | 5.101 | 0.981× | 25 → 23 |
| mixed-overload 3016 | 0.064 | 0.065 | 3.817 | 1.023× | 5 → 5 |
| integer-knapsack 48 | 0.226 | 0.197 | 3.943 | 0.869× | 21 → 9 |
| long-row-mip 96 | 0.076 | 0.082 | 5.381 | 1.088× | 1 → 1 |

## Fresh validation seeds after parameters were frozen

These seven configurations were run after implementation and parameter choices
were frozen; no changes were selected from their results.

| Workload | 0.19 ms | 0.20 ms | HiGHS ms | 0.20 / 0.19 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| multi-knapsack 5018 | 0.252 | 0.261 | 4.244 | 1.035× | 30 → 24 |
| multi-knapsack 6018 | 0.628 | 0.407 | 30.715 | 0.648× | 73 → 32 |
| multi-knapsack 7018 | 0.337 | 0.351 | 24.228 | 1.043× | 39 → 30 |
| graph-mip 4018 | 0.077 | 0.082 | 5.498 | 1.077× | 1 → 1 |
| mixed-overload 4016 | 0.188 | 0.177 | 10.611 | 0.940× | 27 → 27 |
| integer-knapsack 49 | 1.163 | 0.655 | 11.896 | 0.563× | 149 → 41 |
| long-row-mip 112 | 0.871 | 1.809 | 20.333 | 2.076× | 65 → 50 |

## Original public inputs

| Workload | 0.19 ms | 0.20 ms | HiGHS ms | 0.20 / 0.19 | Native nodes before → after |
|---|---:|---:|---:|---:|---:|
| 25fv47 | numerical_failure,numerical_failure,numerical_failure | numerical_failure,numerical_failure,numerical_failure | 119.414 | — | — |
| adlittle | 0.494 | 0.580 | 1.223 | 1.172× | 0 → 0 |
| afiro | 0.063 | 0.063 | 0.460 | 1.000× | 0 → 0 |
| avgas | 0.031 | 0.033 | 0.240 | 1.074× | 0 → 0 |
| bell5 | error,error,error | error,error,error | 525.657 | — | — |
| e226 | 5.936 | 5.726 | 6.253 | 0.965× | 0 → 0 |
| egout | error,error,error | error,error,error | 13.444 | — | — |
| etamacro | 9.433 | 9.652 | 7.729 | 1.023× | 0 → 0 |
| flugpl | error,error,error | error,error,error | 101.679 | — | — |
| israel | 2.576 | 2.696 | 2.838 | 1.047× | 0 → 0 |

The seven public LPs and three MIPs preserve original infinite bounds. Both
native releases fail numerically on 25fv47. Egout, flugpl and bell5 are rejected
because their MIPs include infinitely bounded variables. HiGHS solves all ten.
These are coverage failures, not speed wins. Six accepted public LPs do not
establish general Netlib/MIPLIB competitiveness. No real application capture
was available. Public objective constants and precision are documented in the
corpus README; references come from separately solved identical CSR models.

## Large irregular control

| Backend | Median seconds | Retained range seconds |
|---|---:|---:|
| baseline | 6.832 | 6.741–6.928 |
| native | 6.947 | 6.667–6.995 |
| highs | 9.405 | 9.305–10.126 |

## Kernel profiles

Instrumented times are separate from uninstrumented wall times above.

| Case | Forward triangular ms before → after | Pricing ms before → after |
|---|---:|---:|
| packing 1024 | 9.826 → 9.900 | 1.730 → 1.481 |
| packing 4096 | 174.285 → 120.635 | 31.178 → 17.377 |
| irregular 1024 | 5.442 → 4.897 | 2.644 → 2.302 |
| network 2048 | 25.853 → 18.245 | 5.301 → 0.626 |


## Correctness and limits

- 111 Rust/all-feature checks; 169 Python checks; three MPS-parser contract checks; strict all-feature Clippy.
- 20370 frozen oracle solves, unchanged prior adjudications, zero new discrepancies. Raw unresolved SciPy flags remain visible.
- 400 bound-availability models: 0 unsafe finite bounds under the exact Lagrangian checker.
- 84 captured-basis replay records, both solve orientations; worst normalized backward error 2.17e-12.
- 1319 retained benchmark solves; 1295 accepted. All 24 expected public-native failures are reported above.


## Negative results and remaining weaknesses

Row-gather U solves slowed packing and were rejected. Pivot-order work vectors
were mixed and were rejected. Unconditional propagation scanning was too costly;
a row-slack guard now skips non-tightening work. Default root strong branching
was rejected after increasing nodecopy search. Branch-score changes alone gave
mixed holdouts; certified domain fixing produced the substantial MIP gain.
Per-step raw files preserve these experiments; PLAN.md records decisions.

The fresh long-row112 case regressed 2.08x despite reducing nodes 65 to 50;
proof and propagation overhead can outweigh search savings. Other regressions
include adlittle (+17%), mixed-overload1016 (+16%), wide1024 (+3.5%) and the
large irregular control (+1.7%). These costs are retained, not hidden by an
accepted-only average. `reduced_cost_fixing=false` permits workload-specific
ablation; disabling it does not disable the other kernel/search changes.

MIP search still lacks a mature cutting-plane system and parallel connected
search. Finite bounds on every MIP variable remain required. Dense kernels
remain single-threaded. Packing4096 and the failed public LP are still concrete
weaknesses. Timing ratios on sub-millisecond cases are sensitive to host noise;
use node counts and the retained ranges alongside elapsed time. Improvements
are not monotonic on every fixture; the tables report all controls and seeds.
