# Iteration02: results

Removed the shared result mutex. Each worker retains indexed outputs and one private solver; post-join linear collection preserves per-input errors and order. Added513 distinct-job and claim-boundary checks across1/2/4/8/16 workers.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 8 / batch / 1 / 8 / 256 | 2.903 | 3.635 | 0.799 |
| packing / 32 / skewbatch / 1 / 8 / 32 | 2.552 | 2.227 | 1.146 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-02.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
