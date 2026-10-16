# LP-IP-craft

Building blocks for linear and mixed-integer linear programming (LP / MIP)
solvers in Rust, with Python bindings.

> **Status: interface draft. No optimizer is implemented yet.**
> The only backend evaluates one candidate point and returns it with
> `status == "stub"`. It exists so the model, result, bound/gap, dual and
> cancellation contracts can be exercised end to end, from Rust and from
> Python, before real backends (HiGHS, SCIP, native components) are
> integrated.

## What is here

| Component | Purpose |
| --- | --- |
| `crates/core` (`lp-ip-craft`) | Dependency-free Rust contracts: validated `Model`, the `LpIpSolver` trait, typed results with fixed bound/gap and dual conventions, the independent `check::evaluate` and `check::lp_residuals`, and `DummySolver` |
| `crates/python` | PyO3 extension module `lp_ip_craft._native` |
| `python/lp_ip_craft` | Typed Python API (`Model`, `solve`, `evaluate`, `Options`, result dataclasses) |
| `docs/api.md` | The contract: data conventions, statuses, bounds and gaps, duals, errors |
| `docs/design-notes.md` | Review of the 15 October draft, decisions, what is deferred, next steps |
| `docs/target-card.md` | Application inputs still to be supplied (left TBD on purpose) |

## Python example

```python
from lp_ip_craft import Model, evaluate, solve

# minimize 2 x0 + 3 x1 + 5  subject to  x0 + x1 = 1,  x binary
model = Model(
    objective=[2.0, 3.0], objective_offset=5.0,
    lower=[0.0, 0.0], upper=[1.0, 1.0], kinds=["binary", "binary"],
    indptr=[0, 2], indices=[0, 1], values=[1.0, 1.0],   # A in CSR form
    row_lower=[1.0], row_upper=[1.0],
)
result = solve(model, start=[1.0, 0.0])
assert result.status == "stub"                  # never an optimality claim
assert result.evaluation.objective == 7.0       # recomputed, offset included
assert result.best_bound is None and result.gap is None

# The independent check works on any solver's solution:
assert evaluate(model, [0.0, 1.0]).feasible_candidate
```

Sequences may be lists, tuples or 1-D NumPy arrays. The Rust equivalent is in
the crate documentation (`cargo doc --open -p lp-ip-craft`) and in
`crates/core/examples/production.rs`.

## Build and test

Requirements: stable Rust (edition 2024, 1.85 or newer), a C linker, and
CPython 3.10 or newer. No external optimization libraries are needed.

```bash
python3 -m venv .venv && source .venv/bin/activate
python -m pip install 'maturin>=1.9.4,<2' pytest numpy
bash scripts/check.sh
```

`scripts/check.sh` runs `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test`, the Rust example, builds the extension with `maturin develop`,
then runs `pytest` and the Python example. `python -m pip install .` builds and
installs the package; `python -m maturin build --release` builds an abi3
wheel. The core crate is usable without Python: `cargo test -p lp-ip-craft`.

The binding crate has no Rust test executable (it would need to link
libpython); it is tested through pytest against the compiled module.

## Related

[SQP-craft](https://github.com/dietmarwo/SQP-craft) is the sibling repository
for the nonlinear (SQP) track, with the same conventions.

## License

MIT, see [LICENSE](LICENSE).
