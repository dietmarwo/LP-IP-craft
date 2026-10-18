# Iteration01: results

Frozen common harness against current upstream numeric code. Existing eight-worker batches and component solves are baselines. Dense and old connected fixtures have planted primal/dual objectives; selection uses its unchanged independent dynamic program.

Regular release Rust tests: 182 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 512 / single / 1 / 1 / 1 | — | 5.489 | — |
| packing / 32 / batch / 1 / 8 / 64 | — | 3.302 | — |
| packing / 8 / batch / 1 / 8 / 256 | — | 2.721 | — |
| packing / 32 / skewbatch / 1 / 8 / 32 | — | 3.108 | — |
| packing / 64 / skewcomponents / 1 / 8 / 1 | — | 4.141 | — |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-01.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
