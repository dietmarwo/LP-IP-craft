# Iteration 30: results

Completed the 0.28 freeze, scalar fallbacks, full release checks and build-mode guard. The final source preserves the original scalar reduced-cost/basic-aware pricing loops below parallel gates. Matched production isolation has mixed timings: packing128 serial 0.765→0.698 ms, packing512 4.976→5.287 ms, packing128×128/outer8 15.912→15.240 ms and dense384 serial 116.545→119.175 ms. This supports preserving the simpler original fallback, without claiming a universal scalar speedup.

Default debug Rust: 197 passed. Release regular/all-target variants: 198 each; all features including its doctest: 199, zero failures. Compiled Python including final harness guards: 289 passed. Warnings-denied Clippy and rustdoc, fmt and the 15,255-call independent audit pass. Every final production timing has the original-model checks and eight-CPU budget. Complete final measurements are in [RESULTS.md](RESULTS.md) and [TABLES.md](TABLES.md).

The earlier initial/corrected Rust runs picked up an example binary with kernel-profile enabled after all-target/all-feature testing. Those before/after gaps are confounded. Exact features/hash evidence and both campaigns are preserved, and excluded from production claims. A real profiled executable is now rejected by the driver. After all checks, production examples were rebuilt without profiling and all seven Rust matrix campaigns plus a three-build scalar-isolation campaign were rerun. Python applications and oracle audits used the unchanged nonprofiled extension.

The final factor journal was found incomplete at 127/128 records despite its success log. It remains as `data/factor-scaling-incomplete-production.json`; a fresh complete 128-record campaign replaces it. Atomic synced journal writes and parent-side complete-record validation guard future runs. No missing sample is synthesized.

The following table is retained as **excluded, instrumented development evidence**, not a production speedup:

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| packing / 512 / single / 1 / 1 / 1 | 4.767 | 7.362 | 0.647 |
| dense / 384 / single / 8 / 1 / 1 | 113.047 | 118.915 | 0.951 |
| packing / 128 / batch / 1 / 8 / 64 | 7.644 | 12.247 | 0.624 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-30.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
