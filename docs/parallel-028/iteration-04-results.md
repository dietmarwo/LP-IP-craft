# Iteration04: results

Stable expensive-first scheduling is enabled only for sufficiently long queues with more than2x structural cost spread. Job ordering, private budgets and original result indices remain independent of execution order.

Regular release Rust tests: 183 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 32 / skewbatch / 1 / 8 / 32 | 2.461 | 2.938 | 0.838 |
| packing / 8 / batch / 1 / 8 / 256 | 3.706 | 3.307 | 1.121 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-04.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
