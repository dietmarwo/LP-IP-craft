# Iteration 22: results

Native/Python batches support bounded mixed budgets 1x8/2x4/4x2/8x1, rejecting products above 8 while retaining the old 16-worker serial-inner API. Other nested mechanisms stay disabled; per-job limits, errors and original order remain.

Regular release Rust tests: 195 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / batch / 1 / 8 / 8 | 219.188 | 224.103 | 0.978 |
| packing / 128 / batch / 1 / 8 / 64 | 6.872 | 8.352 | 0.823 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-22.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
