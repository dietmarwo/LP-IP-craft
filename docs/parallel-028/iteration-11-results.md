# Iteration11: results

Added explicit reusable1..8-worker local Rayon execution with weak sharing by compatible width, ordered maps, worker-local state, safe disjoint slices and no global pool mutation. Default1 has no pool. This introduces a pinned pure-Rust scheduler dependency; numerical algorithms remain owned Rust.

Regular release Rust tests: 189 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 128 / batch / 1 / 8 / 64 | 8.747 | 9.002 | 0.972 |
| packing / 64 / skewcomponents / 1 / 8 / 1 | 4.113 | 3.506 | 1.173 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-11.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
