# Native 0.17: ten performance iterations

Pinned baseline: native 0.16, upstream `76ca1c9a3c22f0076f282826bb1c1c6a3d5692a5`. Comparator: the previously delivered 0.4 Rust adapter with HiGHS 1.15.0, restored from its pinned source and patch. No HiGHS dependency was added to the production workspace. Both core and native numerical crates still have zero external dependencies.

The largest gain comes from strengthening MIP relaxations, not faster node processing: selection-64 now needs one LP node instead of 9,311. Large connected LPs improve more modestly and remain the main weakness. Results are workload-dependent; the data do not show monotonic improvement on every fixture in every iteration. The development logs preserve regressions and rejected tuning attempts.

## Method

Same input fingerprints, release builds, one numerical thread, native component cap one except explicitly labelled parallel rows. Five retained rotated/reversed process rounds, one discarded round; each process also warms once. Legacy fixtures have three measured solves per process (15 retained); other fixtures have one (5 retained). Times below measure the Rust solver call, including conversion and internal checks, excluding construction and external validation. Independent original-model objectives/feasibility and LP KKT checks pass on all 695 retained solves. Both engines use a 30-second limit and the same original budgets. No tolerances were loosened.

Host: AMD EPYC 9V74, Linux x86_64, Rust 1.99.1; shared host without exclusive CPU reservation. Times are medians, not universal hardware claims. Raw ranges, RSS, phase timings, node/pivot counts, hashes and options are in `data/fixed-comparison.json`. Whole-process RSS is not isolated solver allocation.

## All fixed performance checks

Native improvement is baseline time / final time; values below 1 are regressions. Family size is the generator parameter (network size counts vertices). Warm/cold applies to MIP node reoptimization.

| Fixture | Size | Mode | Workers | Native 0.16 ms | Native 0.17 ms | HiGHS ms | Native improvement |
|---|---:|---|---:|---:|---:|---:|---:|
| fixed | 0 | warm | 1 | 0.0922 | 0.0908 | 1.7256 | 1.02x |
| pivot | 0 | warm | 1 | 0.4804 | 0.4584 | 2.6047 | 1.05x |
| knapsack | 0 | warm | 1 | 0.0699 | 0.0741 | 4.3588 | 0.94x |
| partition | 0 | warm | 1 | 0.1589 | 0.0186 | 0.2693 | 8.53x |
| nodecopy | 0 | warm | 1 | 1.5983 | 1.2675 | 47.3670 | 1.26x |
| redundant | 0 | warm | 1 | 1.0195 | 0.9552 | 6.9848 | 1.07x |
| blocks | 0 | warm | 1 | 2.0317 | 1.9135 | 7.3145 | 1.06x |
| packing | 64 | warm | 1 | 0.2426 | 0.2401 | 0.6482 | 1.01x |
| packing | 256 | warm | 1 | 2.4412 | 2.1721 | 3.2408 | 1.12x |
| packing | 1024 | warm | 1 | 43.4414 | 35.8409 | 22.9835 | 1.21x |
| packing | 2048 | warm | 1 | 147.2427 | 118.7131 | 67.2189 | 1.24x |
| packing | 4096 | warm | 1 | 556.8729 | 489.4232 | 174.2264 | 1.14x |
| network | 32 | warm | 1 | 0.0725 | 0.0725 | 0.5307 | 1.00x |
| network | 128 | warm | 1 | 0.5976 | 0.5028 | 1.4667 | 1.19x |
| network | 512 | warm | 1 | 6.4676 | 6.3324 | 8.9918 | 1.02x |
| network | 2048 | warm | 1 | 96.4055 | 98.7441 | 101.8326 | 0.98x |
| irregular | 64 | warm | 1 | 0.4558 | 0.4411 | 0.9569 | 1.03x |
| irregular | 256 | warm | 1 | 7.0483 | 7.1208 | 6.3558 | 0.99x |
| irregular | 1024 | warm | 1 | 244.2993 | 224.6753 | 181.2855 | 1.09x |
| selection | 12 | warm | 1 | 0.1110 | 0.0328 | 4.6206 | 3.38x |
| selection | 12 | cold | 1 | 0.1521 | 0.0328 | 4.6206 | 4.64x |
| selection | 20 | warm | 1 | 0.7827 | 0.2925 | 4.3758 | 2.68x |
| selection | 20 | cold | 1 | 1.1949 | 0.3795 | 4.3758 | 3.15x |
| selection | 32 | warm | 1 | 2.4609 | 0.2945 | 5.2744 | 8.36x |
| selection | 32 | cold | 1 | 5.0130 | 0.4859 | 5.2744 | 10.32x |
| selection | 64 | warm | 1 | 175.0474 | 0.1226 | 4.9745 | 1427.27x |
| selection | 64 | cold | 1 | 305.7894 | 0.1225 | 4.9745 | 2496.55x |
| box-lp | 0 | warm | 1 | 16.5197 | 4.6424 | 11.0141 | 3.56x |
| sparse-import-lp | 0 | warm | 1 | 19.5826 | 18.4700 | 102.2041 | 1.06x |
| transport-lp | 0 | warm | 1 | 2.2203 | 2.2745 | 12.2090 | 0.98x |
| partition-mip | 0 | warm | 1 | 28.2465 | 27.2535 | 60.1879 | 1.04x |
| blocks | 0 | warm | 16 | 1.0960 | 1.1301 | — | 0.97x |
| partition-mip | 0 | warm | 16 | 24.8545 | 30.6622 | — | 0.81x |

## Original Python import benchmarks

Python solve wall time, including the binding; five retained rotated rounds. Model construction is excluded. These are separate measurements from the Rust table.

| Fixture | Native 0.17 ms | HiGHS ms |
|---|---:|---:|
| box-lp | 9.275 | 16.415 |
| sparse-import-lp | 24.525 | 103.125 |
| transport-lp | 2.379 | 13.163 |
| partition-mip | 30.701 | 61.477 |

## Correctness and broader workload checks

- 94 Rust tests/doctests, 156 Python tests, strict workspace Clippy: pass.
- 6,330 distinct frozen oracle models, three passes per engine: native has zero unresolved solver discrepancies after the previously recorded oracle adjudications.
- 3,680 native ablation cases: zero discrepancies (Dantzig pricing, frequent refactorization, warm reoptimization, parallel components).
- 400 conservative-bound cases: no unsafe bounds. Independent rational bounds checks retained.
- Additional signed-row propagation test: 80 models checked against exhaustive 256-point enumeration, each solved warm and cold.
- Original-model tolerance-start semantics and mixed-integer incumbent certification have permanent regression tests. Both regressions discovered during integration were fixed before the final runs.

The original dataset contains 18 unresolved SciPy LP statuses and one mistaken SciPy MIP infeasibility result. Previously recorded, separately checked feasible points/recession rays and a no-presolve MIP solve adjudicate these. This is numerical evidence, not exact certification. Raw oracle labels and outcomes remain unchanged. HiGHS reproduces the previous 66 adapter run errors on infeasible LPs and one infeasible/unbounded misclassification in each pass; these failures are excluded from successful-case timing ratios.

Median aggregate Python solve times on common successful oracle cases:

| Suite | Type | Common cases | Native seconds | HiGHS seconds |
|---|---|---:|---:|---:|
| random | LP | 3533 | 0.8955 | 2.4570 |
| random | IP/MIP | 1350 | 0.3121 | 8.2621 |
| exact | LP | 400 | 0.0140 | 0.0846 |
| exact | IP/MIP | 800 | 0.0562 | 3.0067 |
| components | LP | 60 | 0.0043 | 0.0145 |
| components | IP/MIP | 120 | 0.0173 | 0.6262 |

## What HiGHS does well and what this series implements

HiGHS combines presolve and domain propagation with branch-and-cut; its simplex code adapts between sparse and hyper-sparse work. Our changes target the observed gaps with independent Rust implementations:

1. Exact rounding of integral-row bounds strengthens the root relaxation.
2. Skip unnecessary component discovery for empty matrices.
3. Solve row-free continuous boxes directly, retaining independent checks.
4. Reachability-based transpose triangular solves.
5. Reachability-based forward triangular solves with density fallback/cooldown.
6. Reuse sparse entering-column support for primal, edge-weight and eta updates.
7. Carry sparse unit support through eta transformations, skipping provably disjoint updates.
8. Remove exact cancellation entries from Markowitz lists.
9. Exact i128 propagation on short integer rows and fractional integer domains.
10. Rebuild large bases when eta fill exceeds eight times the base factor after at least sixteen updates; the configured refactor interval remains a hard maximum.

Plans, measured outcomes and revisions: [ITERATIONS.md](ITERATIONS.md). These mechanisms are general; there is no fixture-name or expected-objective dispatch in the solver.

Primary method references inspected:

- [HiGHS overview](https://highs.dev/): dual revised simplex and branch-and-cut architecture.
- [HPresolve.cpp](https://github.com/ERGO-Code/HiGHS/blob/master/highs/presolve/HPresolve.cpp): presolve and integer/domain machinery.
- [HighsDomain.h](https://github.com/ERGO-Code/HiGHS/blob/master/highs/mip/HighsDomain.h): activity-based propagation.
- [HFactor.cpp, v1.15.0](https://github.com/ERGO-Code/HiGHS/blob/v1.15.0/highs/util/HFactor.cpp): sparse/hyper-sparse triangular solves (also inspected in the pinned downloaded source).
- [HEkkDualRHS.cpp](https://github.com/ERGO-Code/HiGHS/blob/master/highs/simplex/HEkkDualRHS.cpp): sparse pricing work.

## Remaining weaknesses

Large connected packing and irregular LPs still trail HiGHS. This series retains product-form eta updates; it does not implement Forrest–Tomlin updates, a full cut separator, reliability strong branching or incremental long-row propagation. Sparse reach discovery still performs some full-vector work, and factorization retains a dense tail. Network-2048 slightly regressed in the final median despite better development timings. Some tiny cases pay added presolve/setup costs. MIPs still require finite variable bounds of magnitude at most 2^53−1; this series does not remove that limitation.

Next priorities are profiling factor updates on real application instances, Forrest–Tomlin-style basis updates, more sparse pricing state, and incremental propagation/cuts for harder MIPs. Synthetic families are useful controlled benchmarks, not a substitute for your eventual application instance.

Reproduction instructions: [benchmarks/README.md](../../benchmarks/README.md). Input SHA-256: `7da1d445743d20c29c70e5bbe658c017f755fcf8769214c30a519401dbc21bb9`.

## Irregular-4096 exploratory probe

One warmup plus one retained solve per backend, not a five-sample median:

| Backend | Seconds | Status | Accepted |
|---|---:|---|---|
| baseline | 19.992 | optimal | True |
| highs | 8.983 | optimal | True |
| native | 18.835 | optimal | True |
