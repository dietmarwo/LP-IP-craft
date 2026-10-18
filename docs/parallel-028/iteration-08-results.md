# Iteration08: results

Single-worker residual rounds run directly on the caller. A separate unit-packing ring has a DP optimum. The first comparison failed because checkpoint07 did not understand the new fixture; that failed protocol is retained. The paired baseline here is exact checkpoint07 production code rebuilt with the identical benchmark-only fixture from08.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| ring / 64 / residual / 1 / 1 / 1 | 3.526 | 2.936 | 1.201 |
| ring / 64 / residual / 1 / 8 / 1 | 2.302 | 2.598 | 0.886 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-08.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
