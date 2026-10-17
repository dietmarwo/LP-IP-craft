# Irregular LU: results and validation (0.19)

Baseline is delivered 0.18.0 at `1d6463dbcad0574c314e8f52aa7cba83108c7802`. The isolated old HiGHS reference is pinned to 1.15.0. Core and native production crates have no third-party numerical dependency; no HiGHS, C/C++ solver or BLAS is linked.

## Fresh full-solver comparison

The standard suite has five retained rotated/reversed process rounds after one discarded round. The large irregular-4096 comparison has three retained rounds after one discarded round. Each fresh process warms once. Single numerical thread; separately labelled component-parallel controls. Identical input fingerprints, unchanged time/node/iteration limits and acceptance tolerances. Internal conversion/checks are included, construction and independent external checks excluded. Rust 1.99.1 release, codegen-units=1, shared AMD EPYC 9V74 host without core exclusivity. New factorization/rebuild defaults are intentional changes and recorded per run.

| Fixture | 0.18 ms | 0.19 ms | HiGHS ms | Rust time reduction |
|---|---:|---:|---:|---:|
| irregular-1024 | 217.134 | 158.917 | 192.890 | 26.8% |
| irregular-4096 | 19355.033 | 7338.721 | 9445.104 | 62.1% |
| packing-1024 | 34.181 | 31.462 | 23.828 | 8.0% |
| packing-4096 | 266.967 | 269.923 | 172.380 | -1.1% |
| network-2048 | 88.751 | 90.456 | 107.933 | -1.9% |
| box-lp-0 | 4.597 | 4.831 | 12.354 | -5.1% |

All 695 standard retained solves and 9 large retained solves pass independent original-model feasibility/objective/KKT checks. No standard process errors or objective mismatches. Full ranges and diagnostics are in the JSON evidence.

### Large irregular details

| Backend | Median seconds | Range seconds | Peak RSS MiB (median) | Pivots | Rebuilds |
|---|---:|---|---:|---:|---|
| baseline | 19.355 | 18.929–19.776 | 229.5 | [4096] | 67 |
| native | 7.339 | 6.887–7.646 | 122.2 | [4096] | 60 |
| highs | 9.445 | 8.877–10.049 | 266.4 | [4097] | not exposed |

### Regressions and neutral results

This is not a universal speedup. These standard-suite medians regressed by more than 5%; raw ranges and smaller changes remain available. Tiny durations and a shared host limit interpretation. Packing-4096 is effectively unchanged in this run.

| Fixture | Warm/workers | Before ms | After ms | Increase |
|---|---|---:|---:|---:|
| knapsack-0 | True/1 | 0.0726 | 0.0839 | 15.6% |
| partition-0 | True/1 | 0.0145 | 0.0184 | 26.5% |
| nodecopy-0 | True/1 | 1.2463 | 1.4263 | 14.4% |
| selection-12 | True/1 | 0.0311 | 0.0338 | 8.6% |
| selection-12 | False/1 | 0.0309 | 0.0338 | 9.4% |
| selection-64 | False/1 | 0.1202 | 0.1281 | 6.5% |
| box-lp-0 | True/1 | 4.5974 | 4.8308 | 5.1% |
| sparse-import-lp-0 | True/1 | 19.3465 | 20.3844 | 5.4% |
| blocks-0 | True/16 | 1.1282 | 1.2827 | 13.7% |

## Captured-basis replay

Twenty-one actual 0.18 bases are frozen under `corpus`, covering early/middle/late irregular-1024/4096 and sparse packing/network controls. Each contains recent rho/pi vectors and deterministic unit/sparse/dense controls. The same matrix bytes are rebuilt by each binary. Final replay uses two retained rotated rounds after a discarded round, each with a process warmup; all RHSs are checked in both orientations. Times below are instrumented and separate from full-solver timings.

| Frozen basis | Before factor ms | After factor ms | Before RHS-set ms | After RHS-set ms | Max normalized backward error |
|---|---:|---:|---:|---:|---:|
| irregular-1024/basis-0017.bin | 33.609 | 8.277 | 2.838 | 1.150 | 1.07e-13 |
| irregular-4096/basis-0048.bin | 112.390 | 33.740 | 6.307 | 3.388 | 4.94e-13 |
| irregular-4096/basis-0064.bin | 1112.249 | 266.034 | 37.849 | 20.433 | 2.17e-12 |
| irregular-4096/basis-0066.bin | 1289.495 | 322.035 | 41.494 | 19.973 | 3.97e-13 |

Entry counts use the current storage representation: retained dense blocks include their zeros, whereas sparse lists omit them. Compare RSS and runtime rather than interpreting these counts alone as a fill reduction. Replay is untimed for file loading and residual validation; kernel instrumentation is compiled out of ordinary builds.

## Measured kernel costs on irregular-1024

Five paired retained process rounds. Categories below sum exclusive nested timings per run, then take medians. Their medians need not add exactly to the total median.

| Operation | Before ms | After ms |
|---|---:|---:|
| Factorization and setup | 78.541 | 31.263 |
| Forward triangular solves | 47.574 | 50.255 |
| Transpose triangular solves | 25.696 | 12.746 |
| Update-file application | 35.598 | 33.850 |
| Pricing outside basis solves | 16.359 | 16.269 |

## What each step delivered

1. **Replay and profiling:** optional `basis-replay` capture, validated binary reader, frozen corpus, independent forward/transpose checks, and exclusive assembly/search/extraction/sparse/dense/packing timers. Normal builds have no capture filesystem code.
2. **Sparse data structures:** O(1) cross-indexed removal and lookup, invalidated/cached column maxima, reusable matrix/scatter/pivot buffers. First native-sized links were too expensive; checked 32-bit links restore the old active-entry footprint. All step-2 replay pivot fingerprints and factor counts match baseline, but its overall replay time regresses. This negative result is preserved, not presented as a speedup.
3. **Dense factor and solves:** row-major partial-pivoted LU, panel width 32 and tiled trailing updates, retained dense L/U for contiguous solves, sparse-prefix coupling, and materialization for forced FT. Small tails, singular fallback and tight entry caps retain the old path. This is the largest measured improvement. It is a dense tail inside sparse LU, not a return to a full simplex tableau.
4. **Fill-reducing advice:** bounded merged-row approximate minimum degree, dense-row suppression and symbolic work budget; cache reuse across small structural changes. Search budgets 4/8/16 and natural/auto/forced ordering were tested. Setup cost offsets many fill savings; default search stays 4 and auto ordering is limited to m>=2048 after substantial observed dense fill. This is project-owned advice, not the COLAMD reference code.
5. **Rebuild policy:** count visited eta arithmetic and estimate factor work from sparse Schur products plus the dense cubic term. Large dense bases rebuild after at least 32 updates when update work exceeds one quarter of estimated factor work; configured hard maximum 256. Other bases keep an internal maximum of 64. Entry/fill limits, numerical fallback, cancellation and final acceptance remain in force. Fixed intervals and the rejected one-eighth budget are retained as evidence.

## Validation

- 105 Rust/all-feature checks and strict Clippy passed. Dense-tail tests cover partial permutations, sparse-prefix interaction, eta/FT updates, rejection/cap paths and workspace reuse. Twelve full dense LP solves have independently planted primal/dual certificates.
- 169 Python tests passed, including new controls and invalid values.
- Four complete 6,330-model passes: default, forced FT, forced ordering, fixed interval 128. Additionally 1,200 exact frequent-refactor forced-FT cases and 180 parallel-component cases: **26,700 oracle solves**.
- Each full pass has the identical 19 historical oracle flags. The unchanged prior adjudications leave zero discrepancies; raw records and flags are included. They are numerical checks, not universal/formal proofs.
- All 400 conservative-bound fixtures have zero unsafe finite bounds under the existing exact-rational checker.
- All captured replay residuals pass, with no tolerance relaxation. `audit_irregular.py` rechecks bundled outcomes without rerunning the solver.

## Controls and reproduction

Python adds `factor_ordering="auto"|"natural"|"min_degree"`, `factor_pivot_search=1..64`, and `adaptive_refactor=True|False`. Rust uses `FactorOrdering` and matching fields. Default `refactor_interval` is now 256, a maximum rather than a promised rebuild period. Set `adaptive_refactor=False` and an explicit interval for fixed-policy comparisons. `basis_update` controls from 0.18 remain available.

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --release --manifest-path benchmarks/Cargo.toml
benchmarks/target/release/performance-native irregular 4096 warm 1 3 auto 256 4 auto adaptive
cargo build --release --manifest-path benchmarks/Cargo.toml --features basis-replay
benchmarks/target/release/basis-replay docs/irregular-lu/corpus/irregular-4096/basis-0066.bin 3 4 natural
python benchmarks/audit_irregular.py
python benchmarks/report_irregular.py
```

`compare_three.py` reproduces the standard comparison; `compare_large.py` the large comparison; `profile_kernels.py` and `replay_compare.py` reproduce kernel/replay comparisons. Preserve separate instrumented/uninstrumented binaries. The original reference bootstrap reconstructs 0.16, not the 0.18 baseline: use its delivered archive or pinned commit above. Full source history is included in the ZIP.

## Scope and remaining work

All numerical work remains owned Rust without external numerical libraries. No parallel dense kernel was introduced; these comparisons use one thread. Dense-tail and update-policy changes alter arithmetic and sometimes pivot trajectories. The existing finite MIP-bound restrictions remain. Internal active-matrix links use checked 32-bit indices; dimensions above u32::MAX are rejected before allocation.

Ordering is a bounded heuristic and can be slower when forced. The auto gates are conservative synthetic-benchmark choices to revisit on the application instance. Whole-vector work and large dense triangular solves remain. Final original-model checks and the retained fallback paths are still necessary.

## Algorithm references

- [LAPACK DGETRF](https://www.netlib.org/lapack/explore-html/db/d04/group__getrf_gaea332d65e208d833716b405ea2a1ab69.html): blocked partial-pivoted LU background; no LAPACK/BLAS code is linked.
- [COLAMD resources](https://www.netlib.org/linalg/colamd/): fill-reducing column-ordering background; our bounded merged-element implementation is separate.
- The existing [0.18 report](../basis-updates/RESULTS.md) documents the retained FT method, isolated HiGHS reference and earlier oracle adjudications.
