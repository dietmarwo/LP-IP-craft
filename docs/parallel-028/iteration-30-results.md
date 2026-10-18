# Iteration30: results

Frozen0.28 with original scalar packing-root function and a131,072-nonzero root gate after the measured small-case loss. Default1 uses the direct dispatcher without per-call settings cloning. Large root work retains fused in-pool phases; useful LP/factor/outer capabilities remain. Complete debug/release/all-feature/all-target Rust, compiled Python, lint/doc and full frozen three-way oracle audit gate final paired timing. Application evidence preserves SHA-verified full float64 points once and requires identical fixed-work outcomes.

Regular release Rust tests: 198 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 1 / 1 / 1 | 15.839 | 16.889 | 0.938 |
| dense / 384 / single / 8 / 1 / 1 | 123.814 | 115.720 | 1.070 |
| packing / 128 / batch / 1 / 8 / 64 | 7.620 | 11.259 | 0.677 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-30.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
