# Validation — native iterations 6–10, version 0.14.0

Base is the previous native delivery at 7a1c811320a396e205e2592ade400cf0fe0eb253.
This archive contains five more implemented/tested iterations and preserves
all ten source checkpoints. It does not modify the remote GitHub repository.

| Iteration | Version | Rust tests | Doctests | Python tests | Independent oracle |
|---|---|---:|---:|---:|---|
| 6 | 0.10.0 | 64 | 1 | 98 | 600 base |
| 7 | 0.11.0 | 65 | 1 | 98 | 600 base |
| 8 | 0.12.0 | 66 | 1 | 100 | 600 base |
| 9 | 0.13.0 | 66 | 1 | 108 | 600 base + 180 component |
| 10 final | 0.14.0 | 68 | 1 | 134 | 600 base + 180 serial + 180 parallel component |

The last row represents 780 distinct generated models and 960 solver/oracle
comparisons. The base has 200 continuous LPs checked against SciPy, 200 integer
models by exact exhaustive enumeration, and 200 mixed models by exact rational
continuous-vertex enumeration over integer assignments. The extra component
suite glues pairs of independently checked blocks and verifies the objective
offset, primal reconstruction, LP duals and bound direction. The component
suite is repeated with forced workers in the final version.

New regressions cover replacement-bound equivalence to materialized models,
malformed bounds, streamed empty-row compaction, decimal/infinite activity
enclosures, presolve opt-outs, separable models in both senses, LP/IP/MIP,
free and upper-only columns, explicit zero entries, disconnected contradictory
rows, infeasible/unbounded status composition, per-dictionary caps, low global
budgets, preserved starts, cancellation, worker errors, all sixteen workers,
and unique Rust diagnostic keys. Existing core/exact-bound tests are retained.

Final release checks:

- Debug workspace Rust tests and compiled Python tests all pass without skips.
- Rust 1.85 workspace/all-targets check, strict Clippy, rustdoc with warnings
  denied, formatting and strict mypy pass.
- Release Linux abi3 wheel builds and a fresh virtual-environment install runs
  both the LP/MIP example and all Python tests.
- Extracted source archive passes the Rust-only tests and SHA-256 manifest
  verification; a local Git bundle preserves every source checkpoint.
- Numerical dependency audit shows only the local core dependency. The optional
  Python extension uses PyO3; no HiGHS, highs-sys, bindgen, CMake, BLAS or LAPACK
  dependency appears in the workspace/lockfile.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.85.0 check --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
python -m pip install ".[test]"
python -m pytest -q
python examples/solve_native.py
# Optional independent reference solver; not a package dependency:
python -m pip install scipy
python examples/validate_native.py
python examples/validate_components.py 1
python examples/validate_components.py 4
```

All execution here used Linux x86_64 and CPython 3.12. Cross-platform CI is
provided, but remote CI, macOS, Windows and other Python versions were not run here.
The model/solver statuses are numerical, not exact infeasibility or search-tree
certificates. Each dictionary is dense and each MIP variable must have finite
bounds. Parallelism applies to independent components, not connected search
trees. Per-dictionary caps are not a whole-process memory cap. Quota reservation
can stop a parallel run before a serial shared budget is exhausted. The actual
GTOC model and reference data remain absent.
