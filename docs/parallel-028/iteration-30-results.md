# Iteration30: results

Frozen corrected0.28 after full-matrix review: restored original scalar reduced-cost and basic-aware pricing loops below the parallel gate, removing the extra clear/gather regression. Seven-repeat confirmation compares upstream, the initial30 and corrected source. Original scalar packing root and direct default dispatcher remain, with a131,072-nonzero packing-root gate; large work retains safe parallel outputs. Initial30 source/full campaign/audit and invalid unsupported-baseline trial are retained. Complete release/debug/all-feature/all-target Rust, compiled Python, lint/docs and full three-way oracle audit are rerun before final measurements.

Regular release Rust tests: 198 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 512 / single / 1 / 1 / 1 | 4.767 | 7.362 | 0.647 |
| dense / 384 / single / 8 / 1 / 1 | 113.047 | 118.915 | 0.951 |
| packing / 128 / batch / 1 / 8 / 64 | 7.644 | 12.247 | 0.624 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-30.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
