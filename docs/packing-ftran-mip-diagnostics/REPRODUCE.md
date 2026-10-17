# Reproduce 0.23

Run from the extracted source root. Build products, Python environments,
downloaded reference source trees and machine-specific binaries are not bundled.
Raw observations, original public inputs, planted holdouts and frozen inputs
are bundled. Python numerical/reference packages are not engine dependencies.

## Correctness

Use Rust 1.85+ and Python 3.10+ in an isolated environment with maturin, pytest,
NumPy and SciPy. The retained run used the exact versions in
`data/build-manifest.json`. The optional reference C++ build additionally needs
CMake and libclang. Do not run timings concurrently with builds or tests.

```sh
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
maturin develop --release
python -m pytest -q
python benchmarks/run_packing_diagnostics_gates.py
```

The gate command writes new 0.23 results only. It does not edit frozen inputs,
adjudications or historical 0.22 evidence. Re-auditing bundled files alone is
not a rerun of these numerical solves.

## Build matched comparison binaries

1. Extract the accepted `LP-IP-craft-0.22.0-highs-lessons.zip` to a separate
   baseline directory; verify SHA-256
   `5be46f4af89c840de768b2c182a914b2c2481f64fda18e89d0a659519ae68fbd`.
   Those local changes were not pushed to GitHub; a GitHub checkout is not an
   interchangeable baseline. Keep its source immutable.
2. Run `python benchmarks/reference/bootstrap_sources.py` to obtain the pinned
   isolated old HiGHS adapter when absent. Run
   `python benchmarks/configure_highs_reference.py PATH_TO_OLD_HIGHS_REPO` to
   apply the saved benchmark-only presolve/serial-dual controls. HiGHS 1.15.0
   and the Rust adapter dependencies are exactly pinned. Do not add that adapter
   to the production workspace.
3. Build current uninstrumented native code with
   `cargo build --release --manifest-path benchmarks/Cargo.toml` and **copy**
   `performance-native` to a distinct `native` path before any profile build.
   `CARGO_TARGET_DIR` changes the binary location.
4. Build references with
   `python benchmarks/build_packing_references.py --baseline-repo BASELINE_ROOT
   --highs-repo OLD_HIGHS_ROOT --output MATCHED_HARNESS_DIR`. Save the resulting
   `packing-baseline` and `packing-highs` binaries separately. They use the same
   fixture/checking harness as the candidate; source hashes are recorded.
5. For a detailed baseline profile, copy the 0.22 source to a third directory
   and apply `INSTRUMENTATION.patch` there using `patch -p1`. This patch adds
   instrumentation only, with unchanged arithmetic. Build its benchmark
   workspace with `--features basis-replay`; save `performance-native` as
   `baseline-profile` and `basis-replay` as `baseline-replay`.
6. Build the current benchmark workspace with `--features basis-replay`; save
   its two binaries as `native-profile` and `native-replay`. Never substitute
   either instrumented executable for the uninstrumented headline binary.

The delivery contains `CHANGESET.patch` relative to accepted 0.22, excluding
large evidence/corpus files; full source is supplied as the primary artifact.

## Timing and diagnosis

After all builds/checks finish, run sequentially:

```sh
python benchmarks/run_packing_diagnostics_performance.py \
  --baseline /absolute/path/baseline \
  --native /absolute/path/native \
  --highs /absolute/path/highs \
  --baseline-profile /absolute/path/baseline-profile \
  --native-profile /absolute/path/native-profile \
  --baseline-replay /absolute/path/baseline-replay \
  --native-replay /absolute/path/native-replay --rounds 3
```

`--only NAME` repeats selected groups explicitly. Fresh-process rotations,
warmups, discarded rounds, failures and binary/model hashes are retained.
`mip-diagnostics` compares enabled/disabled tracing on the same executable,
then probes existing policies. Root and node propagation are not conflated:
presolve-off is labelled as a combined reduction control, not an isolated
propagation study. These probes are single observations, not timing claims.

The network timing ambiguity also warranted a focused nine-round comparison:

```sh
python benchmarks/compare_packing_mip.py \
  --binary baseline=/absolute/path/baseline \
  --binary native=/absolute/path/native \
  --binary highs=/absolute/path/highs \
  --case network:2048 --case packing:4096 --case aspect-wide:512 \
  --rounds 9 --output docs/packing-ftran-mip-diagnostics/data/final-network-repeat.json
```

Set `LPIP_HIGHS_SOLVER=simplex` for the same isolated serial-dual LP control.
The initial standard-suite network result is retained alongside the repeat.

```sh
python benchmarks/audit_packing_diagnostics.py
python benchmarks/report_packing_diagnostics.py
```

## Recreate added inputs

The supplied converted inputs and originals suffice for all comparisons.
To reproduce their preparation using the pinned source, run:

```sh
python benchmarks/prepare_diagnostic_corpus.py \
  --source PINNED_HIGHS_SOURCE/check/instances \
  --output docs/packing-ftran-mip-diagnostics/corpus
```

This uses untimed SciPy/HiGHS references and verifies returned primal
feasibility. JSON null preserves each infinite bound. Generated packing models
have a planted x=1 and matching positive row duals; seeds, dimensions, degrees
and content hashes are retained. No guessed finite boxes are inserted.

The added conversions were additionally checked against HiGHS's own MPS
reader. Build `benchmarks/verify_public_mps.cpp` against the pinned reference
headers/static library (C++11, pthread and dl), then run
`verify_public_conversion.py --reader READER_BINARY --corpus
docs/packing-ftran-mip-diagnostics/corpus/public --output
docs/packing-ftran-mip-diagnostics/data/mps-conversion-check.json`.
This isolated C++ reader is not part of the production build.

The five-way ablation explicitly disables decomposition and compares the
baseline, early export-only and seeded binaries, accepted candidate 2 and
rejected candidate 3. See the per-file binary hashes and build manifest.
`experiments/experiment3-rejected.patch` reproduces the rejected extra shortcut
on the accepted source. Its successful correctness tests do not establish a
performance gain. A subsequent five-round fresh-packing comparison restores
default decomposition; both the initial and follow-up observations are retained.
