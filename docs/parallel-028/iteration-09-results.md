# Iteration09: results

Frontier outputs use linear indexed collection with explicit duplicate/out-of-range/missing guards, preserving subtree merge order and original task quotas. Sparse/incomplete collections are represented explicitly for residual reuse.

Regular release Rust tests: 184 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| selection / 28 / tree / 1 / 8 / 1 | 1.055 | 1.566 | 0.674 |
| ring / 64 / residual / 1 / 8 / 1 | 2.848 | 2.546 | 1.119 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-09.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
