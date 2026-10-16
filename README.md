# LP-IP-craft — native Rust LP/MIP pilot

Version **0.15.0** replaces the HiGHS adapter with an optimization engine owned
by this project. Both numerical crates have **no third-party dependencies**.
The optional Python extension uses PyO3. No HiGHS, C/C++ solver, CMake,
libclang, BLAS or external numerical library is built or linked by the solver.

Ten implemented native iterations are documented in [docs/native](docs/native):

1. Two-phase primal simplex, dual postsolve and finite-box branch-and-bound.
2. Fixed-column/empty-row presolve and dense allocation preflight.
3. In-place pivots and most-negative pricing with Bland anti-stall fallback.
4. Shared parent dictionaries and dual-simplex child reoptimization, with a
   bounded cold fallback and independent original-unit acceptance checks.
5. Checked rounding/unit-move incumbents, conservative pruning and conditional
   integer-objective lattice bounds.
6. Borrowed node bounds without copying the original sparse model.
7. Sparse-row streaming into one dictionary allocation and inline substitutions.
8. Conservative redundant-row presolve with a configurable scan threshold.
9. Independent component solves with checked primal/dual reconstruction.
10. Scoped parallel component solves with up to 16 workers and bounded quotas.

Version 0.15 adds an **opt-in sparse revised-simplex pilot** with sparse
row-pivoted LU, sparse product-form updates, periodic refactorization and
bordered-row/dual-simplex reuse for MIP children. Select
`NativeOptions(sparse=True)` in Python or `NativeOptions { sparse: true,
..Default::default() }` in Rust. The sparse path never allocates a dense
tableau or basis inverse, and never silently falls back to the dense kernel.
The previous dense kernel remains the default and a comparison reference.

Both paths are numerical pilots. MIPs still require finite bounds on **all**
variables, including continuous variables, of magnitude at most 2^53-1. The
dense cap is 8 million binary64 entries (64 MB per dictionary). Sparse states
have a separate default cap of 2 million tracked numerical entries. This is
not a process-memory cap: indexing, work vectors, allocation temporaries and
pending-node snapshots need additional memory. Sparse LU fill can still grow
quadratically on unfavorable matrices.

Each connected solve uses one thread. Independent components can use up to
16 workers. The new [sparse plan, results and benchmark protocol](docs/sparse/README.md)
uses connected synthetic cases and independent known answers. No real GTOC
formulation has been supplied; application readiness remains unverified.

## Build and use

Rust 1.85 or later. Plain `cargo build` builds only the two Rust numerical
crates; Python is an optional workspace member. Rust-only checks need no
Python installation:

```sh
cargo test -p lp-ip-craft -p lp-ip-craft-native --locked
cargo run --release -p lp-ip-craft-native --example solve --locked
cargo run --release -p lp-ip-craft-native --example benchmark --locked -- 7
cargo run --release -p lp-ip-craft-native --example sparse_benchmark --locked -- packing 4096 sparse
```

Rust API:

```rust,ignore
use lp_ip_craft::{CancellationToken, LpIpSolver, Options};
use lp_ip_craft_native::{NativeOptions, NativeSolver};
let mut solver = NativeSolver::new(NativeOptions::default());
let result = solver.solve(&model, &Options::default(), None,
                          &CancellationToken::default())?;
```

Python 3.10 or later:

```sh
python -m venv .venv
. .venv/bin/activate  # Windows: .venv\Scripts\activate
python -m pip install ".[test]"
python examples/solve_native.py
python -m pytest -q
```

```python
from lp_ip_craft import Model, NativeOptions, solve
m = Model(objective=[3., 2.], sense="maximize",
          lower=[0., 0.], upper=[10., 10.],
          row_lower=[float("-inf"), float("-inf")], row_upper=[4., 5.],
          indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[1., 1., 2., 1.])
r = solve(m, backend="native", backend_options=NativeOptions())
assert r.status == "optimal"
assert abs(r.evaluation.objective - 9.) < 1e-7
```

The default `backend="dummy"` remains for contract compatibility: it evaluates
a point and reports `stub`. Select `native` to optimize. `backend="highs"`
and `HighsOptions` have been removed. `NativeOptions` supports actual native
controls; seed and HiGHS-specific gap options have no replacement here.

## Numerical guarantees and limits

LP `optimal` requires independent primal feasibility, stationarity, dual-sign
and relative duality-gap checks in original units. MIP incumbents pass the
independent primal/integrality check. All solve statuses are numerical,
including infeasibility; this is not an exact search-tree certificate.

The existing outward-rounded `safe_dual_bound` is retained. Returned bounds
use this calculation when available; LP bounds can be absent with infinite
variable bounds. Conservative pruning additionally requires an original-model
incumbent whose constraint activities are enclosed inside their bounds. This
can fail on floating-point equality rows; search then continues without that
pruning opportunity. Integer-lattice tightening has explicit sufficient
conditions. Neither mechanism certifies phase-I infeasibility or the whole
branch-and-bound tree. Standard IEEE binary64 with gradual underflow is
required; fast-math is unsupported.

Starts are complete primal points. For a MIP they are evaluated as incumbent
candidates; LP starts are explicitly rejected because public basis import is not
implemented. Internal child-node basis reuse works in both kernels. Time/cancellation limits are cooperative and retain an incumbent
when present. Node limits count relaxations; pivot limits apply across all
nodes, phase I, dual simplex and cold retries. Parallel components reserve
shares of the global node/pivot limits; unused shares are not redistributed.
Serial component execution shares the remaining budget instead. See the API
for this deliberate limit/performance tradeoff and memory accounting.

See [API](docs/api.md), [validation and results](docs/native/validation-6-10.md),
[performance](docs/native/performance-6-10.md) and the still-incomplete
[application target card](docs/target-card.md). Earlier adapter records are
under [docs/history](docs/history); their timings are not native-engine claims.

## Layout

| Path | Purpose |
|---|---|
| `crates/core` | Immutable CSR model, contracts and independent checks |
| `crates/native` | Sparse revised simplex/LU, dense reference, presolve, integer search |
| `crates/python`, `python/lp_ip_craft` | Optional PyO3/Python interface |
| `tests`, `crates/*/tests` | Contract, numeric, exact-oracle and engine tests |
| `examples/validate_native.py` | Optional SciPy oracle; not a package dependency |
| `docs/native` | Ten earlier plans, results and raw measurements |
| `docs/sparse` | Sparse implementation plan, validation and raw scaling results |

Project code is MIT. See [dependencies](docs/dependencies.md) and `Cargo.lock`.
