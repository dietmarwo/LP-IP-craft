#!/usr/bin/env bash
# Runs every local check. Requires an activated virtual environment with
# maturin, pytest and (optionally) numpy installed; see README.md.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${VIRTUAL_ENV:-}" ]; then
  echo "Activate a Python virtual environment first; see README.md." >&2
  exit 1
fi
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p lp-ip-craft --example production --locked
python -m maturin develop --release --locked
python -m pytest -q
python examples/demo.py
