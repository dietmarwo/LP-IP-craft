# Iteration 07: results

Component workers retain private indexed results instead of locking a shared result vector. Post-join collection preserves missing/stopped parts and original-index error propagation; all existing component limits/cancellation tests pass.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 64 / skewcomponents / 1 / 8 / 1 | 4.841 | 5.508 | 0.879 |
| packing / 64 / components / 1 / 8 / 1 | 2.586 | 2.099 | 1.232 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-07.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
