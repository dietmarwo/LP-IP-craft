# LP-IP-craft

Linear and mixed-integer linear programming (LP / MIP) in Rust, with Python
bindings.

> **Status: iteration 1 (v0.2.0).** Real LP/MIP solves go through an adapter
> to the open-source [HiGHS](https://highs.dev/) solver. This is a validated
> Rust interface to an existing engine, not yet a native Rust simplex or
> branch-and-bound. The dependency-free core contracts and the original dummy
> backend remain.

## What is here

| Component | Purpose |
| --- | --- |
| `crates/core` (`lp-ip-craft`) | Dependency-free contracts: validated `Model`, the `LpIpSolver` trait, statuses and results with fixed bound/gap and dual conventions, the independent checks `evaluate` and `lp_residuals`, `DummySolver` |
| `crates/highs` (`lp-ip-craft-highs`) | `HighsSolver`: HiGHS 1.15.0 through the `highs` 2.4.0 bindings, with typed settings, explicit status mapping and independent acceptance checks |
| `crates/python`, `python/lp_ip_craft` | PyO3 extension and typed Python API (`Model`, `solve`, `evaluate`, `lp_residuals`, `HighsOptions`, result dataclasses) |
| [`docs/api.md`](docs/api.md) | The contract: data conventions, statuses, bounds and gaps, duals, HiGHS settings and limits, errors |
| [`docs/iteration-1-results.md`](docs/iteration-1-results.md) | What iteration 1 delivered, the review record, open decisions, next steps |
| [`docs/target-card.md`](docs/target-card.md) | Application inputs still to be supplied (left TBD on purpose) |

## Python example

```python
import math
from lp_ip_craft import HighsOptions, Model, evaluate, solve

# minimize 2 x + 3 y + 5  subject to  x + 2 y >= 2.5,  0 <= x <= 3,  y binary
model = Model(
    objective=[2.0, 3.0], objective_offset=5.0,
    lower=[0.0, 0.0], upper=[3.0, 1.0], kinds=["continuous", "binary"],
    indptr=[0, 2], indices=[0, 1], values=[1.0, 2.0],   # A in CSR form
    row_lower=[2.5], row_upper=[math.inf],
)
result = solve(model, backend="highs",
               backend_options=HighsOptions(time_limit_seconds=10))
assert result.status == "optimal"
assert abs(result.evaluation.objective - 9.0) < 1e-9   # recomputed, offset included
print(result.candidate, result.best_bound, result.gap, result.backend.raw_status)

# The independent check works on any solver's solution:
assert evaluate(model, [0.5, 1.0]).feasible_candidate
```

Always pass `backend="highs"` to optimize: the default `"dummy"` is kept for
compatibility and only evaluates one candidate (`status == "stub"`). Sequences
may be lists, tuples or 1-D NumPy arrays. The Rust equivalents are in
`crates/highs/examples/solve.rs` and the crate documentation
(`cargo doc --open -p lp-ip-craft-highs`).

## Build and test

Requirements: stable Rust (edition 2024, 1.85 or newer), CPython 3.10 or
newer, and for the HiGHS backend a C++ compiler, CMake and libclang: `highs-sys`
builds HiGHS from source and generates its bindings. No separately installed
solver is needed.

```bash
sudo apt-get install build-essential cmake libclang-dev   # Debian/Ubuntu
python3 -m venv .venv && source .venv/bin/activate
python -m pip install 'maturin>=1.9.4,<2' pytest numpy
bash scripts/check.sh
```

`scripts/check.sh` runs `cargo fmt --check`, Clippy with `-D warnings`, the
Rust tests and both Rust examples, builds the extension with `maturin develop`,
then runs `pytest` and both Python examples. `python -m pip install .` builds
and installs the package; `python -m maturin build --release` builds an abi3
wheel. Without the native toolchain:

```bash
cargo test -p lp-ip-craft                                      # core contracts only
python -m maturin develop --release --no-default-features      # dummy-only extension
```

In a dummy-only build `lp_ip_craft.BACKENDS == ("dummy",)` and
`backend="highs"` raises `NotImplementedError`. The binding crate has no Rust
test executable (it would need to link libpython); it is tested through pytest
against the compiled module. CI covers Linux, macOS and Windows.

## Related

[SQP-craft](https://github.com/dietmarwo/SQP-craft) is the sibling repository
for the nonlinear (SQP) track, with the same conventions.

## License

MIT, see [LICENSE](LICENSE). HiGHS and the `highs` bindings are MIT licensed;
see [docs/dependencies.md](docs/dependencies.md).
