# LP-IP-craft

LP and mixed-integer linear programming in Rust, with Python bindings.
**Iteration 1 / v0.2.0:** a real HiGHS backend implements the checked-in interface.
The dependency-free core and the original dummy remain available. This is a
Rust adapter to HiGHS, not a newly written simplex or MIP engine.

## Use from Python

```python
from lp_ip_craft import Model, HighsOptions, solve

model = Model(
    objective=[2.0, 3.0], objective_offset=5.0,
    lower=[0.0, 0.0], upper=[1.0, 1.0], kinds=["binary", "binary"],
    indptr=[0, 2], indices=[0, 1], values=[1.0, 1.0],
    row_lower=[1.0], row_upper=[1.0],
)
result = solve(model, backend="highs",
               backend_options=HighsOptions(time_limit_seconds=10))
assert result.status == "optimal"
assert result.evaluation.objective == 7.0
print(result.candidate, result.best_bound, result.gap)
```

Always select `backend="highs"` for optimization. The compatibility default
`"dummy"` returns `"stub"`. Unknown or unavailable backends fail explicitly.
Candidates, primal/integrality checks, LP duals and MIP bounds use original
model units and objective offsets. Numerical optimality is not an exact certificate.

## Layout

| Path | Role |
| --- | --- |
| `crates/core` | Dependency-free model, solver/result contracts, independent checks, dummy |
| `crates/highs` | HiGHS 2.4.0 Rust wrapper / highs-sys 1.15.0 adapter and scoped C API |
| `crates/python`, `python/lp_ip_craft` | PyO3 native extension, typed facade and stub |
| `examples/solve_highs.py`, `crates/highs/examples/solve.rs` | Real Python and Rust solves |
| `tests/test_highs.py`, `crates/highs/tests` | Analytic, exhaustive-IP, cancellation and process tests |
| [docs/api.md](docs/api.md) | Data, options, results and errors |
| [docs/iteration-1-results.md](docs/iteration-1-results.md) | Implemented plan, validation and limitations |
| [docs/target-card.md](docs/target-card.md) | Real application inputs still needed |

## Build and test

Requires Rust **1.85+**, Python **3.10+**, a C/C++ compiler, CMake and a shared
libclang for bindgen. HiGHS is built from source by Cargo; no separately
installed solver is needed. Cargo.lock pins the dependency graph.

Ubuntu/Debian prerequisites:

```bash
sudo apt-get install build-essential cmake libclang-dev
python3 -m venv .venv
source .venv/bin/activate
python -m pip install 'maturin>=1.9.4,<2' pytest numpy patchelf
bash scripts/check.sh
```

Without system CMake/libclang, `pip install cmake libclang` is another route;
set `LIBCLANG_PATH` to that environment's `site-packages/clang/native` directory.
The validation environment used this route and maturin 1.9.4.

```bash
# Distributable Linux wheel (patchelf required)
PYO3_BUILD_EXTENSION_MODULE=1 python -m maturin build --release --locked
# Portable contracts alone, without native backend tool dependencies
cargo test -p lp-ip-craft --locked
# Explicit dummy-only extension
PYO3_BUILD_EXTENSION_MODULE=1 python -m maturin develop --release --locked --no-default-features
```

`scripts/check.sh` runs formatting, Clippy, release tests, Rust examples,
compiled Python tests and Python examples. Native backend CI targets Linux;
the portable dummy interface retains macOS/Windows CI. Only Linux was executed
for this archive. No package is published (`publish = false`).

## Related and license

[SQP-craft](https://github.com/dietmarwo/SQP-craft) is the nonlinear sibling.
Project code is MIT; see [LICENSE](LICENSE) and [docs/dependencies.md](docs/dependencies.md).
