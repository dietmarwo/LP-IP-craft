# Iteration 10: results

Residual rounds use linear indexed gather instead of sorting, retaining skipped tasks, deterministic snapshot merges, task counters and original full-model admission checks.

Regular release Rust tests: 184 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| ring / 64 / residual / 1 / 1 / 1 | 3.098 | 2.632 | 1.177 |
| ring / 64 / residual / 1 / 8 / 1 | 2.583 | 2.789 | 0.926 |
| packing / 128 / batch / 1 / 8 / 64 | 9.038 | 7.902 | 1.144 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-10.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
