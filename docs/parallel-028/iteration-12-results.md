# Iteration12: results

Native Options.threads accepts1..8 and dummy still rejects multiple workers. Numerical budgets serialize outer nesting, reach LP controls, and charge worker setup to deadlines. The first test run caught duplicate aggregate numeric_threads metadata; final aggregation replaces the child key before adding the actual owner budget. Full suite rerun passes.

Regular release Rust tests: 191 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 1 / 1 / 1 | 16.410 | 17.159 | 0.956 |
| packing / 128 / batch / 1 / 8 / 64 | 6.341 | 5.817 | 1.090 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-12.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
