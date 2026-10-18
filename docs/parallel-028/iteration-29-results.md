# Iteration29: results

Root iterations enter the owned pool once; gradients fuse with independent primal projections before row joins, avoiding a buffer pass and one phase. The original scalar loop is retained. Bitwise1/2/4/8 tests, compiled Python, warnings-denied Clippy/rustdoc and SpOC4 confirmation gate the checkpoint. Lint caught two unnecessary closure borrows and an equivalent benchmark range-loop style; corrected without changing model values. Added API, license, numerical and reproduction docs.

Regular release Rust tests: 198 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 8 / 1 / 1 | 116.537 | 111.146 | 1.049 |
| packing / 128 / batch / 1 / 8 / 64 | 7.712 | 6.716 | 1.148 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-29.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
