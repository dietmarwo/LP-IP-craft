# Iteration 26: results

Added a fail-closed three-way audit using unchanged upstream randomized SciPy and exact integer/rational mixed generators. Every actual solve records actual-model/input fingerprints, point, solver-owned duals, objective/bound/status/work, plus independent NumPy feasibility/objective/stationarity. Supplemental larger planted LPs have independent known primal/dual objectives. Missing calls, mismatched inputs, wrong outcomes and inconsistent extensions have fault tests; quick audit gates this checkpoint before timing.

Regular release Rust tests: 197 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 8 / 1 / 1 | 145.737 | 149.238 | 0.977 |
| packing / 128 / batch / 1 / 8 / 64 | 7.896 | 8.689 | 0.909 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-26.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
