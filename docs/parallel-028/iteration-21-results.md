# Iteration21: results

Added benchmark-only dense factor/forward/transpose work: three changed matrices, constructed-point checks, factor-bit checksum, separate numeric/workflow/cold-resource time. Declared concentrated column-work LP before weighted scheduling. Kernel samples report checked, not LP optimality.

Regular release Rust tests: 194 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 1 / 1 / 1 | 129.931 | 122.740 | 1.059 |
| dense / 384 / single / 8 / 1 / 1 | 140.823 | 156.111 | 0.902 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-21.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
