# Reproduce native 0.25

Use the complete source ZIP, or apply its included patch to baseline `9bf392c1f0f1cbf3242a31d0b7d8edd0b3352149`. Rust 1.85+ and Python 3.10+ are required. The measured environment uses Rust 1.99.1 and Python 3.12.14.

```bash
python -m venv .venv
. .venv/bin/activate
python -m pip install maturin pytest numpy scipy
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test -p lp-ip-craft -p lp-ip-craft-native --all-targets --all-features --locked
maturin develop --release --locked
python -m pytest -q
python benchmarks/spoc4_025.py --suite --seconds 10
```

The Python cdylib has standalone Rust tests disabled by design; its behavior is tested through pytest. Running `cargo test --workspace --all-targets` overrides that setting and needs a matching linkable libpython. The validation host's runtime supplied a broken `libpython3.12.so` symlink; the recorded all-targets run uses a local linker alias to the supplied real `.so.1.0` and sets `RUSTFLAGS=-L native=...`. This is a host workaround, not a source/build dependency added to the project. Normal numerical Rust tests need no Python library.

All timed solver processes execute serially, after compilation and correctness checks. The default policy receives a common greedy start and a 30-second budget, matching the earlier assessment. New policies start cold and use the requested suite budget. Root passes, neighborhoods, cuts and tree work all fit within that whole-call cooperative budget. Native simplex pivot limits do not count first-order steps; the first-order iteration/work controls bound those separately.

The suite imports exact decimal-thousandth weights and removes redundant singleton rows. It saves complete uint8 solution vectors in original transfer order, independently recomputes resource conflicts/integer scores and records effective controls, source/input pins, dimensions, status, bound and all diagnostics. See the ESA benchmark attribution in `benchmarks/spoc4-025/DATA-SOURCES.md`.

To run one case:

```bash
python benchmarks/spoc4_025.py --instance i --policy lns4 --seconds 30 --output my-run.json
```

Kernel profiling, from the repository root:

```bash
cargo build --release -p lp-ip-craft-native --features kernel-profile --example spoc4_profile
./target/release/examples/spoc4_profile benchmarks/spoc4-025/data/matching-i.txt 5 > profile-default.json
./target/release/examples/spoc4_profile benchmarks/spoc4-025/data/matching-i.txt 5 packing > profile-root.json
```

The 5-second profiles are separate instrumented probes, not headline speed comparisons. Operation times are exclusive on the solving thread. Shared-host time limits can overrun at cooperative checkpoints; actual elapsed time is always recorded.

The prior matching-I reference bound can be checked without running HiGHS: `python benchmarks/verify_spoc4_reference.py` uses the saved dyadic prices from the delivery evidence. To regenerate the frozen results and re-audit original-order vectors, run `python benchmarks/summarize_spoc4_025.py`.
