# Iteration19: results

Scalar Schur traversal now consumes a target row before the next row tile. Each output retains ascending panel arithmetic; serial/parallel factor bits and known solves match. Panel prototype evidence remains available for final selection.

Regular release Rust tests: 193 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 1 / 1 / 1 | 115.917 | 117.298 | 0.988 |
| dense / 384 / single / 8 / 1 / 1 | 161.428 | 175.684 | 0.919 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-19.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
