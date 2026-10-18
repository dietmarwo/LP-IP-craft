# Iteration15: results

Reusable numeric execution reaches LU dense-tail factorization through an explicit borrowed resource. Sparse Markowitz selection, replay, factor caps, singular fallback and current scalar arithmetic are unchanged.

Regular release Rust tests: 192 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 1 / 1 / 1 | 137.069 | 119.179 | 1.150 |
| dense / 384 / single / 8 / 1 / 1 | 155.046 | 156.933 | 0.988 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-15.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
