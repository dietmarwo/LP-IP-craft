# Iteration 23: results

Nonzero-weighted recursive splits balance highly skewed columns without output reductions or task-buffer allocation. Uniform matrices keep their ordinary partitions. Added empty/skewed prefix and ownership tests. Mixed batches retain compatible numeric resources across jobs; cold setup belongs to total workflow, with per-solve deadlines unchanged.

Regular release Rust tests: 196 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| skewdense / 384 / single / 8 / 1 / 1 | 45.162 | 47.803 | 0.945 |
| skewdense / 512 / single / 8 / 1 / 1 | 97.038 | 97.309 | 0.997 |
| dense / 384 / single / 8 / 1 / 1 | 142.594 | 139.701 | 1.021 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-23.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
