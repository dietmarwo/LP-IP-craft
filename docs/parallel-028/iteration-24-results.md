# Iteration 24: results

Added concurrent Rust and compiled-Python callers with total numeric budget 8, immutable shared dense models, changed dimensions and independent private numerical state. Each worker exercises cancellation, invalid starts and zero deadlines before successful later solves; results retain original-model certificates. Full compiled Python suite is gated before timing.

Regular release Rust tests: 197 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 8 / 1 / 1 | 154.757 | 155.781 | 0.993 |
| packing / 128 / batch / 1 / 8 / 64 | 10.209 | 8.261 | 1.236 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-24.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
