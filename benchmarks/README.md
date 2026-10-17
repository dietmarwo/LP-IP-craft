# Reproduce the performance checks

The production workspace (root Cargo.toml) has no HiGHS dependency. This separate benchmark workspace adds only serde_json. An optional isolated reference workspace can reconstruct the old HiGHS adapter; it is never built by the production workspace.

From the project root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --manifest-path benchmarks/Cargo.toml
benchmarks/target/release/performance-native packing 4096 warm 1 3
```

Arguments are fixture, size (0 for legacy/import fixtures), warm/cold node reoptimization, component worker cap, and measured repetitions. Every process warms up once. Construction, fingerprinting and independent checks are excluded from the solve timer. Results include objective, feasibility/KKT acceptance, nodes, pivots, phase times and effective options. `CARGO_TARGET_DIR`, when set, changes the binary location.

Fixtures: fixed, pivot, knapsack, partition, nodecopy, redundant, blocks; packing, network, irregular, selection; box-lp, sparse-import-lp, transport-lp, partition-mip. The final comparison uses all previously collected sizes, warm/cold selection and separately labelled parallel components. The 4096-row irregular probe is separately labelled because it has only one retained timing.

Optional old references (requires network plus HiGHS's C++ build prerequisites):

```sh
python benchmarks/reference/bootstrap_sources.py
cargo build --release --manifest-path benchmarks/reference/harness/Cargo.toml
python benchmarks/compare_three.py \
  --native benchmarks/target/release/performance-native \
  --baseline benchmarks/reference/harness/target/release/comparison-native \
  --highs benchmarks/reference/harness/target/release/comparison-highs \
  --output comparison.json --rounds 5
```

The bootstrap pins upstream native 0.16 and the pre-native git revision, applies the exact delivered 0.4 adapter patch, and verifies all old backend code hashes. HiGHS version 1.15.0 is pinned by that reference Cargo.lock. Do not compare unpinned system installations.

For Python validation, install a release build using `maturin develop --release` in a virtual environment with pytest and NumPy, then `python -m pytest -q`. The frozen oracle input and adjudications are in `docs/performance-ten/data`. Run `oracle_worker.py DATASET native OUTPUT`; variants are default, dantzig, refactor1, refactor8 and workers4. Original SciPy status-4 cases and one mistaken MIP classification are preserved in raw inputs: apply `oracle-adjudication.json` when interpreting status comparisons. These adjudications are numerical evidence, not exact certificates. Exact rational/enumeration fixtures are labelled separately in the input.

`measure.py` reproduces a development iteration comparison when `binaries/iteration-N` and its preceding binary are present next to the script. Historical changes are supplied as ten ordered patches in the ZIP's `history` folder. Final results are in `docs/performance-ten/RESULTS.md`; all regressions and unsuccessful tuning attempts remain visible in the raw evidence.

Run `python benchmarks/audit.py` to re-audit the bundled raw oracle and ablation records against the preserved adjudications. `analyze_results.py` recomputes the summary tables from that data. This is a read/reanalysis of the saved solver outcomes, not a new solve.
