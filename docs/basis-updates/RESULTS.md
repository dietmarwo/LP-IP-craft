# Basis updates: performance and validation (0.18)

Baseline: the delivered 0.17.0 at `ba4fd6b92b8d161955f9489ac9916188376dbd25`. HiGHS is the same isolated, pinned 1.15.0 reference used previously. The production workspace has no HiGHS dependency.

## Fresh uninstrumented comparison

Five retained rotated/reversed process rounds after one discarded round. Every process warms up once. Numerical kernels use one thread; component-parallel checks are labelled separately. Identical original model fingerprints, unchanged tolerances and budgets. Model construction and external validation are excluded; internal conversion and acceptance are included. Shared AMD EPYC 9V74 host, Rust 1.99.1, release codegen-units=1; no exclusive-core guarantee.

| Fixture | Rust 0.17 ms | Rust 0.18 ms | HiGHS ms | Rust time reduction |
|---|---:|---:|---:|---:|
| packing-1024 | 34.709 | 27.360 | 22.113 | 21.2% |
| packing-2048 | 120.969 | 72.635 | 62.927 | 40.0% |
| packing-4096 | 478.296 | 285.860 | 164.665 | 40.2% |
| irregular-1024 | 206.617 | 206.640 | 191.455 | -0.0% |
| network-2048 | 94.196 | 88.537 | 99.588 | 6.0% |
| box-lp-0 | 4.371 | 4.468 | 11.913 | -2.2% |
| selection-64 | 0.137 | 0.120 | 5.482 | 12.3% |

All 695 retained solves across 93 configurations passed the independent original-model checks. No process errors or objective mismatches. Full timings, ranges, pivots, memory and checks are in `data/final-comparison.json`.

Packing-4096 uses 4,098 pivots in both Rust versions (HiGHS: 4,404). Its remaining gap to HiGHS is about 1.74×, down from 2.90× in this run. Packing-1024 changes from 1,012 to 1,025 pivots because update/rebuild arithmetic affects the path; its speedup is not due to fewer pivots. Irregular-1024 is effectively unchanged. Network-2048 uses 2,047 pivots in both versions.

### Regressions

No universal speedup is claimed. These final medians regressed by more than 5%; smaller differences and all timing ranges remain in the raw data. Small absolute times and shared-host noise limit interpretation.

| Fixture | Mode/workers | Before ms | After ms | Increase |
|---|---|---:|---:|---:|
| pivot-0 | warm/1 | 0.4597 | 0.5309 | 15.5% |
| selection-20 | cold/1 | 0.3823 | 0.4264 | 11.5% |
| selection-32 | cold/1 | 0.4701 | 0.5002 | 6.4% |
| sparse-import-lp-0 | warm/1 | 18.6959 | 19.6364 | 5.0% |

## What the profiling measured

Instrumented timings are a separate paired experiment, not the performance table above. Times are exclusive of nested measured operations; medians are computed independently and need not sum to the median total. Clocks are compiled out of normal builds.

| Packing-4096 operation | Before ms | After ms |
|---|---:|---:|
| Apply update files (eta + FT row transforms) | 117.298 | 3.188 |
| Record basis updates | 27.127 | 2.722 |
| Numerical LU factorization | 69.107 | 37.956 |
| FT setup and remaining refactor work | not separated | 9.425 |
| Forward triangular solves, traversal and result copying | 84.329 | 160.165 |
| Transpose triangular solves | 27.145 | 12.919 |
| Pivot-row pricing, excluding basis solves | 29.317 | 1.913 |
| Choose leaving row | 27.422 | 26.748 |
| Entering-column setup, excluding basis solves | 20.053 | 4.251 |
| FT spike capture | 0.000 | 12.618 |
| Ratio test | 0.423 | 0.393 |
| Remaining pivot work | 32.408 | 31.394 |

The measurements support the intended mechanism: FT removes much of the long column-eta application work and update storage cost. Indexed support sharply reduces pricing and entering-column setup scans. FTRAN through the updated U becomes more expensive and is now the dominant packing kernel. This is work moved and reduced overall, not an improvement in every kernel.

For irregular-1024, automatic selection retains etas. Its final profile spends about 80 ms in factorization and 73 ms in triangular solves; a more efficient factorization/dense-tail implementation is the next priority for that family. This series does not claim to have solved that bottleneck.

## Method ablation

Fresh uninstrumented runs of this same 0.18 binary; five retained rounds per mode. These form a separate timing experiment. All 60 retained solves passed checks.

| Fixture | Auto ms | Eta ms | Forced FT ms |
|---|---:|---:|---:|
| packing-1024 | 31.582 | 32.330 | 24.976 |
| packing-4096 | 267.993 | 443.217 | 297.983 |
| irregular-1024 | 204.894 | 207.682 | 256.096 |
| network-2048 | 97.176 | 100.682 | 94.661 |

Forcing FT on irregular-1024 is slower. Auto is intentionally conservative; it does not always choose the fastest method on every instance (forced FT wins packing-1024 here). It uses factor structure and observed update support, not fixture identities. The full algorithm is available via `basis_update="forrest_tomlin"` for future application tuning.

## Correctness and numerical safeguards

- 97 Rust tests/doctests passed; strict Clippy with all targets/features passed. The direct FT tests include 2,120 repeated replacements with forward/transpose residual checks, sparse support, cached/noncached elimination and safe eta fallback.
- All 400 conservative-bound fixtures passed exact rational safety checks (zero unsafe finite bounds).
- 160 Python tests passed, including method selection and invalid option rejection.
- All 6,330 frozen oracle models were solved with auto and again with forced FT (12,660 solves); the retained eta path additionally passed all 1,200 exact cases.
- Each full oracle run has the same 19 pre-existing raw oracle flags: 18 unresolved SciPy LP classifications and one incorrect SciPy MIP infeasibility result. Applying the unchanged previously documented adjudications leaves zero discrepancies. The raw flags are retained, not hidden or reclassified to fit this implementation.
- These are numerical checks, not universal or formal correctness proofs. No tolerances were loosened. Refactor interval, factor-entry cap, cancellation/limits, original-model acceptance and conservative-bound checks remain in force.

## Reproduce

```sh
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --release --manifest-path benchmarks/Cargo.toml
benchmarks/target/release/performance-native packing 4096 warm 1 5 auto 64
python benchmarks/compare_basis_updates.py --binary benchmarks/target/release/performance-native --output methods.json
cargo build --release --manifest-path benchmarks/Cargo.toml --features kernel-profile
benchmarks/target/release/performance-native packing 4096 warm 1 5 ft 64
```

Preserve uninstrumented/profiled binaries separately. `profile_kernels.py` accepts `--before`, `--after`, `--output`. `compare_three.py` accepts pinned baseline, native and HiGHS binaries. The existing reference bootstrap reconstructs 0.16, not this series’ 0.17 baseline: use the previous delivered ZIP or commit above for the latter. The original 0.17 source changes are in the archive history. See `benchmarks/README.md` for Python oracle commands; use `--variant ft` to force the new update algorithm.

## Sources and implementation scope

- [Gleixner, Factorization and update of a reduced basis matrix for the revised simplex method (2012)](https://webdoc.sub.gwdg.de/ebook/serien/ah/ZIB/ZR-12-36.pdf): mathematical background for spike insertion, permutations and row elimination.
- [Permutations in the factorization of simplex bases](https://optimization-online.org/wp-content/uploads/2016/12/5770.pdf): update/permutation alternatives.
- [HiGHS HFactor.cpp](https://github.com/ERGO-Code/HiGHS/blob/v1.15.0/highs/util/HFactor.cpp): the pinned reference’s update and sparse-solve structure was inspected. The delivered Rust implementation is project-owned and adds no numerical dependency.

This is a conventional move-to-end FT implementation, not the complete HiGHS linear algebra stack or a Suhl–Suhl bump update. Dynamic U uses row/column vectors; full-vector work and dense traversal remain for dense reach. Forced FT can regress some workloads. Auto is deliberately narrow until representative application instances are available. Finite MIP-bound restrictions are unchanged. Larger irregular LPs remain a substantial gap; the exploratory large probe is reported separately below.

## Exploratory irregular-4096 probe

One warmup and **one retained solve** per backend; these are not five-sample medians. All solves were optimal and independently accepted. No speedup claim is made from this small sample.

| Backend | Retained seconds |
|---|---:|
| baseline | 20.249 |
| highs | 9.590 |
| native | 19.279 |

This larger irregular case still takes about twice HiGHS’s time. The factorization/dense-solve bottleneck remains.
