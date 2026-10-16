> Historical record for iterations 1–5. Current delivery: [validation-6-10.md](validation-6-10.md).

# Validation record — native 0.9.0

Base: public LP-IP-craft main a7a04074747a99048810204efe0421b4093c9fb4,
with the earlier delivered iteration-3 core retained. Five local native source
checkpoints were developed; this archive does not push or alter GitHub.

| Native iteration | Rust tests | Doctests | Numerical oracle |
|---|---:|---:|---:|
| 1 (0.5.0) | 59 | 1 | 400/400 SciPy LP/IP |
| 2 (0.6.0) | 60 | 1 | 400/400 SciPy LP/IP |
| 3 (0.7.0) | 61 | 1 | 400/400 SciPy LP/IP |
| 4 (0.8.0) | 62 | 1 | 400/400 SciPy LP/IP |
| 5 final (0.9.0) | 63 | 1 | 600/600; exact IP/MIP oracles |

Initial iteration-1 Python testing found one legacy assertion still expecting
the removed HiGHS feature error (81 passed, one failed, four skipped).
The removal now raises the documented unknown-backend error; that test was
corrected in iteration 2. Iterations 2–4 each passed 82 Python tests with four
backend-specific safe-bound tests still skipped under their old dispatch.
Those four were migrated to the native backend in iteration 5. Final compiled
Python testing passes all 98 tests with no skips. The final source preserves
the correct dispatch behavior and includes every migrated check.

Final checks:

- Debug workspace Rust tests: 63 tests plus one doctest pass.
- Release Linux abi3 wheel builds; install into a fresh virtual environment
  and both the LP/MIP example and all 98 Python tests pass.
- 600 generated models pass independent objective checks. IP has five integer
  variables in [0,3], enumerated exactly. MIP has three integer and two continuous
  variables; continuous vertices are evaluated with exact rational arithmetic.
- Existing safe-bound tests include exact rational LP vertices, perturbations,
  subnormals, overflow, infinite bounds and arithmetic-environment checks.
- Native regressions cover both senses, offsets, equality/ranged rows,
  upper-only/free/fixed columns, explicit empty rows, phase-I infeasibility,
  unboundedness, small coefficients, memory cap, limits, starts, independent
  threads, presolve/pricing/kernel toggles and warm/cold node equivalence.
- `cargo fmt --all --check`, workspace Clippy with `-D warnings`, rustdoc with
  warnings denied, Rust 1.85 workspace/all-targets check and strict mypy pass.
- Dependency audit: both numerical crates have no third-party dependency;
  the optional Python extension pulls in only PyO3 and its Rust dependencies.
  No highs/highs-sys/bindgen/cmake/BLAS/LAPACK entry remains in Cargo.lock.

Reproduction:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.85.0 check --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
python -m pip install ".[test]"
python -m pytest -q
python examples/solve_native.py
python examples/safe_bound.py
# Optional external LP oracle (not a project dependency):
python -m pip install scipy
python examples/validate_native.py
```

The source includes cross-platform CI definitions, but remote CI, macOS,
Windows and Python versions other than local CPython 3.12 were not executed
here. This is not a proof of numerical robustness for arbitrary inputs.
Dense storage, finite-box MIP scope and single-thread solves remain explicit
limitations. Exact dual-bound arithmetic does not certify numerical phase-I
infeasibility or the whole search tree. Real GTOC data is still absent.
