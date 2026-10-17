# Sparse pilot results — 0.15.0

All figures below describe synthetic regression fixtures, not the GTOC application.
All 14 sparse configurations (12 distinct models, with warm/cold comparisons for two MIPs) reached their independently known optimum in every measured run.
No failed or guarded dense result is counted as a speedup.

## Validation

- 78 Rust tests and one doctest passed; formatting and workspace/all-target Clippy passed with warnings denied.
- 146 Python tests passed against the newly built release wheel; mypy passed.
- 1,200 independent small cases passed: 400 LPs against SciPy, 400 IPs against exhaustive integer enumeration and 400 mixed cases against exact rational vertex enumeration. Seeds 59103 and 91027; 200 cases of each domain per seed.
- Additional analytical, cycling, nearly dependent, scaled, sparse-factor and dynamic-programming tests are in the Rust suite.
- The numerical dependency tree is only the native crate and the local core crate. The optional Python binding uses PyO3; HiGHS is absent from the workspace dependency graph.
- Linux/CPython 3.12 local validation only. Cross-platform and minimum-declared-Rust execution were not performed in this session.

## Measurement protocol

Host CPU: AMD EPYC 9V74 80-Core Processor. Compiler: rustc 1.99.1 (58cf8f9c3 2026-10-13). One numerical thread; decomposition off. No CPU affinity or exclusive-host guarantee.
Each of 28 configurations ran in a fresh process for one warmup and five measured samples, in rotating/reversed order: 168 raw records. Reported times are medians of solver calls, excluding model generation and process startup. Peak RSS includes model generation and the whole benchmark process; each process reads its own Linux `/proc/self/status` VmHWM after solving. Sparse entry counters are storage estimates and are not substituted for RSS.

## LP comparisons

| Family / size | Variables / rows / input nnz | Dense median | Sparse median | Dense / sparse peak RSS (MiB) | Result |
| --- | --- | --- | --- | --- | --- |
| packing 64 | 64 / 64 / 192 | 0.759 ms | 0.736 ms | 1.46 / 1.57 | 1.03x dense/sparse time |
| packing 256 | 256 / 256 / 768 | numerical_failure | 10.903 ms | 2.25 / 2.44 | sparse optimum; no speed ratio |
| packing 1024 | 1024 / 1024 / 3072 | time_limit | 138.711 ms | 17.50 / 5.38 | sparse optimum; no speed ratio |
| packing 2048 | 2048 / 2048 / 6144 | dense_guard | 525.692 ms | 1.83 / 9.63 | sparse optimum; no speed ratio |
| packing 4096 | 4096 / 4096 / 12288 | dense_guard | 1986.048 ms | 2.27 / 18.01 | sparse optimum; no speed ratio |
| network 32 | 61 / 32 / 122 | 0.414 ms | 0.660 ms | 1.46 / 1.51 | 0.63x dense/sparse time |
| network 128 | 253 / 128 / 506 | 24.854 ms | 16.606 ms | 2.25 / 2.59 | 1.50x dense/sparse time |
| network 512 | 1021 / 512 / 2042 | 1723.564 ms | 259.949 ms | 17.57 / 5.63 | 6.63x dense/sparse time |
| irregular 64 | 64 / 64 / 376 | 0.814 ms | 1.524 ms | 1.47 / 1.66 | 0.53x dense/sparse time |
| irregular 256 | 256 / 256 / 1525 | 44.973 ms | 49.661 ms | 2.38 / 3.75 | 0.91x dense/sparse time |

The dense 256-variable ring case returned numerical_failure. The 1,024-variable dense ring case hit the 30-second wall limit in every run (pivot counts vary with elapsed time and are recorded as a range). The 2,048- and 4,096-variable dense conversions were rejected by the unchanged eight-million-entry guard. Guarded-process RSS does not represent the memory needed for a completed dense solve.

The 4,096-variable ring case used 612,973 peak tracked sparse numerical entries. Its dense transformed dictionary would have 33,579,012 entries (about 256 MiB for that dictionary alone), exceeding the default guard. This family has favorable structure; linear-looking storage growth here is not a general sparse-LU complexity guarantee.

## MIP reoptimization comparison

| Variables | Kernel | Child reuse | Median ms | Nodes | Pivots | Cold retries |
| --- | --- | --- | --- | --- | --- | --- |
| 12 | dense | False | 0.142 | 15 | 74 | 0 |
| 12 | dense | True | 0.174 | 15 | 43 | 1 |
| 12 | sparse | False | 0.278 | 15 | 75 | 0 |
| 12 | sparse | True | 0.921 | 39 | 123 | 8 |
| 20 | dense | False | 1.215 | 91 | 691 | 0 |
| 20 | dense | True | 0.953 | 91 | 317 | 8 |
| 20 | sparse | False | 1.850 | 91 | 718 | 0 |
| 20 | sparse | True | 54.709 | 1505 | 6388 | 359 |

All objectives agree with the independent integer dynamic program. Sparse warm child solves are functionally validated but perform substantially worse on this fixture: for 20 variables, 1,505 nodes versus 91 cold, and approximately 29.6x the runtime of cold sparse search. This is an unresolved performance regression, not a claimed speedup. Diagnosis must separate degenerate basis/branch choices, conservative bound strength and cold-retry cost. Set `node_reoptimization=False` when comparing the cold sparse path. Small dense problems also remain faster in several other cases.

## Development failure ledger

- The first sparse ring implementation lost primal basis accuracy at 256 variables (minimum basic value around -1.11e-9 after 258 pivots) and also failed the 2,048-variable case. Harris two-pass primal ratios and a bounded fresh-factor retry resolved the observed failures under the original independent acceptance thresholds. The 256-variable case is a permanent regression test; larger cases remain in the benchmark suite.
- The initial measurement wrapper assumed child namespace PIDs matched `/proc` PIDs. Measurement was moved inside each benchmark process using `/proc/self/status`; final raw records use only that method.
- The first summary utility incorrectly required identical pivot counts on time-limited runs. All 168 measurements were retained; summaries were regenerated with median/range counters for incomplete solves. Successful runs retain deterministic-counter checks.

## Next priorities

1. Diagnose the sparse warm-search regression using root/node objectives, bound strength and controlled branching comparisons.
2. Reduce fill and allocation through column ordering, solve workspaces and sparse/hyper-sparse triangular operations.
3. Improve pricing/update policies against all current families, preserving numerical and exact-oracle regressions.
4. Add public basis sessions for repeated LP changes only with clear structure compatibility and checked cold fallback semantics.
5. In January, capture the real GTOC model and reserve application validation variants before tuning; update the still-unfilled target card.

Raw data: [benchmarks.json](benchmarks.json), [oracle seed 59103](oracle-59103.json), [oracle seed 91027](oracle-91027.json). Validation logs are under `validation-logs/`. Numerical limitations and reproduction commands are in [README.md](README.md).
