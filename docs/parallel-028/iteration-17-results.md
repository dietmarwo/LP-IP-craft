# Iteration17: results

Raised dispatch gates to avoid fine-grained late updates: at least1,048,576 Schur operations and65,536 coefficient operations. Scalar fallbacks preserve the same values and ordered decisions.

Regular release Rust tests: 193 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 8 / 1 / 1 | 44.805 | 18.115 | 2.473 |
| dense / 384 / single / 1 / 1 / 1 | 119.677 | 116.190 | 1.030 |
| dense / 384 / single / 8 / 1 / 1 | 165.558 | 184.130 | 0.899 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-17.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
