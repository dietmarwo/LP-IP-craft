# Iteration 03: results

Bounded claims amortize atomic queue traffic in long queues; small queues retain individual claims. All 513 distinct-job/boundary tests and original error/cancellation tests pass.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 8 / batch / 1 / 8 / 256 | 3.209 | 3.429 | 0.936 |
| packing / 32 / skewbatch / 1 / 8 / 32 | 3.429 | 3.256 | 1.053 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-03.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
