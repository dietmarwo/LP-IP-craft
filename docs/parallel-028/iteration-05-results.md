# Iteration 05: results

The caller runs one of the worker loops, so a width 8 batch starts only seven helper OS threads. Local solver ownership and joined linear result collection remain; no ninth numerical worker is introduced.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 8 / batch / 1 / 8 / 256 | 2.937 | 3.134 | 0.937 |
| packing / 32 / batch / 1 / 8 / 8 | 0.888 | 1.306 | 0.680 |
| packing / 32 / skewbatch / 1 / 8 / 32 | 2.217 | 2.099 | 1.057 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-05.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
