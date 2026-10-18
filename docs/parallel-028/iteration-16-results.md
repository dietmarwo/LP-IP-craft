# Iteration 16: results

Blocked Schur updates partition trailing rows and share read-only solved panels; every cell retains original operation order.193/385-size1/2/4/8 tests require factor-bit/permutation identity and forward/transpose known solutions. An initial test fixture incorrectly used a private Entry literal; it now builds the actual indexed Active matrix before testing and full rerun passes.

Regular release Rust tests: 193 passed, zero failed. Release examples built. Builds/tests completed before timing; timed children run serially with at most eight numerical workers, pinned to eight allowed CPUs under the recorded quota. Every child checks complete job counts, finite outputs, the frozen known objective, original-model feasibility and LP dual residuals. One warmup and three measured repetitions, rotated build order.

| Family / size / mode / inner / outer / jobs | Previous ms | Candidate ms | Previous/candidate |
|---|---:|---:|---:|
| dense / 384 / single / 1 / 1 / 1 | 120.624 | 117.507 | 1.027 |
| dense / 384 / single / 8 / 1 / 1 | 154.820 | 148.610 | 1.042 |

All raw samples, CPU time, binary hashes and failures are retained in `data/iteration-16.json`. Small differences may be shared-host noise; no monotone per-iteration speedup is inferred.
