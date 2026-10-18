# Iteration 13: results

Fresh structural reduced costs use disjoint column outputs with original subtraction order. Added nonlogical dense-basis1/2/4/8 tests with bitwise reduced-cost/pivot-row/support identity, nonzero prices and changed costs on retained resources.

Regular release Rust tests: 192 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 1 / 1 / 1 | 16.002 | 17.808 | 0.899 |
| dense / 192 / single / 8 / 1 / 1 | 16.327 | 17.003 | 0.960 |
| dense / 384 / single / 8 / 1 / 1 | 116.801 | 125.585 | 0.930 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-13.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
