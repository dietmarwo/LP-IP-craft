# Iteration 28: results

Added isolated public/SpOC4 application checks with original NumPy/resource feasibility, reference objectives, conservative-bound checks, full point/source/extension hashes and separate construction, workflow, API and root-phase time. Exploratory 15-configuration/30-run application matrix passed with fixed64 root iterations. Also repaired omitted top-level Python test staging in checkpoints 24–27: exact executed tests are now included, numerical/harness bytes unchanged, original refs preserved and mapping recorded.

Regular release Rust tests: 198 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 8 / 1 / 1 | 120.184 | 118.831 | 1.011 |
| packing / 128 / batch / 1 / 8 / 64 | 6.802 | 7.352 | 0.925 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-28.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
