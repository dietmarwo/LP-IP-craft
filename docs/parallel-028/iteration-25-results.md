# Iteration 25: results

The timing protocol now creates an exact build/case/repetition record before launching each child, preserving faults/timeouts instead of attributing them to a previous build. Regression tests inject incomplete jobs, nonoptimal outcomes, nonfinite/negative/invalid certificates, missing fields, CPU oversubscription and real child failures. Full Python suite is gated before timing. Checkpoint 21 auxiliary factor data also received an explicitly recorded exact-executable rerun after its initial JSON was incomplete.

Regular release Rust tests: 197 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 8 / 1 / 1 | 159.039 | 149.041 | 1.067 |
| packing / 128 / batch / 1 / 8 / 64 | 7.724 | 10.844 | 0.712 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-25.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
