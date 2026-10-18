# Iteration 06: results

Parallel component queues now dispatch costly parts first. Node/pivot reservations stay attached to the original part index, and bounded-work configurations retain original dispatch order.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 64 / skewcomponents / 1 / 8 / 1 | 4.645 | 4.098 | 1.134 |
| packing / 64 / components / 1 / 8 / 1 | 2.690 | 2.291 | 1.174 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-06.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
