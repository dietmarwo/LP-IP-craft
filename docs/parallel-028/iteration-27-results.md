# Iteration27: results

Removed the losing weighted-column prototype and raised pricing dispatch to262,144 actual nonbasic coefficient operations; late cheap bases remain scalar. Pre-cancelled batches avoid helpers while preserving serial validation/errors. Added independent-column gradient and independent primal/row projections in the SpOC4 packing root using bounded joins, preserving operation order, step/work accounting and serial conservative certification. A35-iteration32,768-column test requires bitwise hint/price/bound and identical work for1/2/4/8, including the32-step check boundary.

Regular release Rust tests: 198 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 192 / single / 8 / 1 / 1 | 19.055 | 16.523 | 1.153 |
| dense / 384 / single / 8 / 1 / 1 | 147.643 | 116.586 | 1.266 |
| dense / 768 / single / 8 / 1 / 1 | 1043.860 | 939.239 | 1.111 |
| ring / 64 / residual / 1 / 8 / 1 | 2.696 | 2.433 | 1.108 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-27.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
