# AGENTS.md

LP-IP-craft is a native Rust LP/MIP solver: a bounded-variable dual revised
simplex on a sparse LU factorization inside best-bound branch and bound, with
no C/C++ solver or BLAS. It contains the crates `lp-ip-craft` (`crates/core`:
model, options, independent checks) and `lp-ip-craft-native`
(`crates/native`: the solver), the optional Python package `lp_ip_craft`
(`crates/python`, `python/lp_ip_craft`) and `lp-ip-job-worker`
(`crates/lp-ip-job-worker`), the worker for the shared job API with the
profiles `lp-ip.solve/1` and `lp-ip.evaluate/1`.

## Read first

- [ai-context.md](ai-context.md): when the solver fits, modelling, options,
  results, parallel layouts and the job profiles. Read it before modelling a
  problem or claiming a capability.
- Choosing between solvers (fast-cma-es, fcmaes-rust, LP-IP-craft,
  SQP-craft): https://github.com/dietmarwo/optimization-jobs/blob/main/ai-context.md
- Jobs without a checkout: the worker's `describe` lists payload and model
  JSON Schemas and runnable examples (`crates/lp-ip-job-worker/discovery/`).
- MPS models: `python3 scripts/mps_to_model.py model.mps -o model.json`.

## Build and test

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python -m pip install ".[test]" && python -m pytest -q   # CPython 3.10+
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
python3 tutorials/job-api/run.py --root /tmp/lp-ip-job-tutorial
```

The complete CI list, with the shared job harness, is in
[ai-context.md](ai-context.md#test-commands) and `.github/workflows/`.

## Conventions

- MSRV: Rust 1.85 for the numerical crates and the Python crate (CI checks
  the workspace without the worker with 1.85); Rust 1.88 for the job worker
  and the shared job crates. The job layer is Linux only.
- Edition 2024 and `--locked` builds; rustfmt and clippy with `-D warnings`
  must pass.
- No external numerical libraries: CI fails if `highs`, `bindgen`, `cmake`,
  `blas` or `lapack` enter the native dependency tree.
- The shared job crates come from optimization-jobs at the revision pinned
  in the root `Cargo.toml`; change them there first.
- Solve statuses are numerical findings under tolerances, not exact
  certificates; report them that way. Record user-visible changes under
  `[Unreleased]` in CHANGELOG.md.
