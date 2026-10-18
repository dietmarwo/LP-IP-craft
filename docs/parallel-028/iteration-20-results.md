# Iteration20: results

Matched pool creation to65,536-coefficient work and skip already-expired entry deadlines. Reverted losing panel18 and scalar traversal19 prototypes, preserving original panel/tile arithmetic and useful disjoint Schur rows. An overly broad clock edit was caught at compile time and restricted to solve entry before the successful full rerun.

Regular release Rust tests: 193 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 8 / 1 / 1 | 15.902 | 15.574 | 1.021 |
| dense / 384 / single / 1 / 1 / 1 | 123.569 | 120.537 | 1.025 |
| dense / 384 / single / 8 / 1 / 1 | 162.934 | 145.146 | 1.123 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-20.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
