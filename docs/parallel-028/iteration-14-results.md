# Iteration14: results

Dense column pricing computes independent dot outputs, then gathers touched indices in original column order. Basic/logical columns, DROP and exact-zero rules remain. The dense-basis bitwise/support test requires actual column-pricing dispatch.

Regular release Rust tests: 192 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 1 / 1 / 1 | 17.626 | 17.445 | 1.010 |
| dense / 192 / single / 8 / 1 / 1 | 17.112 | 25.915 | 0.660 |
| dense / 384 / single / 8 / 1 / 1 | 122.670 | 155.549 | 0.789 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-14.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
