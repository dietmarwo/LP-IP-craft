# Completed validation

Validated 16 October 2026 UTC on Linux x86_64, CPython 3.12.14, Rust 1.99.1.
The declared MSRV Rust 1.85.0 was checked separately. PyO3 is 0.30.1;
maturin 1.9.4, pytest 9.1.1 and NumPy 2.5.4 were used.

| Gate | Observed result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --release --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --release --workspace --locked` | **45 unit/integration tests + 1 doctest passed** |
| `cargo +1.85.0 check --release --workspace --all-targets --locked` | Pass, including examples/tests and native backend |
| Real Rust example | Pass; actual optimizer selected |
| Native release wheel build | Pass, `cp310-abi3-manylinux_2_38_x86_64` |
| Clean installed-wheel Python suite | **57 passed**, including original 48 contract tests |
| Real Python example from installed wheel | Pass |
| Dummy-only release wheel build | Pass with `--no-default-features` |
| Dummy-only Python contract suite | **48 passed** |
| Missing backend request in dummy-only build | `NotImplementedError`; no silent fallback |
| `git diff --check` | Pass |

The clean wheel environment was separate from the editable development install.
Imports resolved to its `site-packages`, not either source tree. Default/native
and dummy-only wheels were built into separate directories. Both distributions
were installed together for the clean-wheel suites.

Representative command sequence from the repository root:

```bash
cargo fmt --all --check
cargo clippy --release --workspace --all-targets --locked -- -D warnings
cargo test --release --workspace --locked
cargo +1.85.0 check --release --workspace --all-targets --locked
PYO3_BUILD_EXTENSION_MODULE=1 python -m maturin build --release --locked --out dist
# In a fresh virtual environment:
python -m pip install dist/*.whl pytest numpy
python -m pytest -q tests
```

These wheel tags describe the build host's resulting glibc floor, not a promise
of older Linux compatibility. The archive ships source only, with Cargo.lock;
rebuild in the intended deployment/build image. Python 3.10/3.14 and
macOS/Windows jobs are defined in CI but were not executed here. There was no
remote push or CI run for these local changes.

Full solver evidence and limits are in [iteration-1-results.md](iteration-1-results.md).
The sanitized [validation logs](validation/) preserve exact test results and
build output. Build warnings from maturin 1.9.4 about the PyO3 extension feature
are emitted despite the supported `PYO3_BUILD_EXTENSION_MODULE=1` environment
setting; the resulting wheels were installed and exercised successfully.
