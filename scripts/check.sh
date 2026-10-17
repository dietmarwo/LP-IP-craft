#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p lp-ip-craft-native --example solve --locked
python -m maturin develop --release --locked
python -m pytest -q
python examples/demo.py
python examples/solve_native.py
python examples/safe_bound.py
# Optional external oracle: install SciPy separately, then run
# python examples/validate_native.py
