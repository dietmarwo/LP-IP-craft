# Iteration18: results

Experimental panel elimination distributes target rows after ordered pivot/swap. The unchanged per-target arithmetic is checked by factor-bit tests that exercise the8192-operation gate. Prototype remains subject to measured rejection.

Regular release Rust tests: 193 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 1 / 1 / 1 | 135.376 | 140.596 | 0.963 |
| dense / 384 / single / 8 / 1 / 1 | 186.886 | 202.560 | 0.923 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-18.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
