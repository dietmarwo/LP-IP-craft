# Iteration 1: HiGHS backend — results

Version 0.2.0, 16 October 2026. Implements the [iteration-1 plan](iteration-1-plan.md)
(revision 2) on top of `main` at `769e556`. The implementation was delivered as
an archive and reviewed before merging; the review changes are listed below.

## Delivered

| Plan package | Result |
| --- | --- |
| L1 backend probe | `highs` 2.4.0 / `highs-sys` 1.15.0 pinned exactly (HiGHS 1.15.0 built from source). LP, MIP, objective offset, primal starts, solution status queries, `mip_dual_bound`, time/node/iteration limits and interrupt callbacks exercised |
| L2 hardening | Overflow-checked bound distances, LP residuals (including the reproduced false-zero stationarity) and gaps; `Bounds::checked_violation`; statuses, `BackendReport`, `Error::Backend`; Python `lp_residuals` |
| L3 adapter | `crates/highs`: validation of backend limits, conversion from the validated `Model`, typed settings, explicit status mapping (`status.rs`), independent acceptance checks; raw C API only in `ffi.rs` (offset, int64 info, interrupt callback) |
| L4 limits and starts | Cooperative whole-call time budget, node and iteration limits, cancellation through the interrupt callback, complete starts submitted unrounded, start status reported |
| L5 Python | `solve(..., backend="highs", backend_options=HighsOptions(...))` inside the detached interpreter section; `BACKENDS`; a dummy-only build without the native dependency |
| L6 acceptance and docs | Analytic LP/IP/mixed fixtures, exhaustive integer oracles, random-LP acceptance regression, pure-Python residual check, limits, cancellation, thread and process workers; Rust and Python examples; CI on Linux, macOS and Windows |

## Review of the delivered archive

The archive built and its 45 Rust tests, doctest and 57 Python tests passed.
Probing the adapter found four behaviour bugs and several smaller issues:

| Delivered | Changed to | Why |
| --- | --- | --- |
| `optimal` required LP `complementarity` (largest `|dual| × distance`) `<= 1e-7` absolute | `optimal` requires the relative duality gap `|P − D| / (1 + |P| + |D|) <= dual_tolerance` and stationarity and dual-sign residuals `<= dual_tolerance · (1 + ‖c‖∞)`; `duality_gap` added to `LpResiduals` | The product grows with dual and bound magnitudes. 1 of 39 small random 80×40 LPs that HiGHS solved to optimality was reported as `numerical_failure` (primal violation 1.7e-8, stationarity 1e-10, complementarity 1.5e-7). With the scale-free gap, 598 of 599 probe LPs are `optimal` (worst gap 2.3e-14); the other one fails a primal check (see the open decision) |
| A cancellation or budget overrun after HiGHS returned turned `optimal` into `cancelled` / `time_limit` | The status is the engine's termination reason; an overrun sets `BackendReport::time_budget_exceeded` | The plan requires keeping the actual termination reason; an optimum found in time does not become a limit because extraction took long |
| `iteration_limit` set for MIPs, `node_limit` set for LPs | Both rejected as `Unsupported` | HiGHS documents that `simplex_iteration_limit` does not apply to MIP subproblems, and a node limit has no effect on an LP: silently ignored settings |
| Wildcard status mapping; `LoadError`/`ModelError` reported as `numerical_failure`; `ReachedMemoryLimit` as `unknown` | Every pinned status mapped explicitly and unit-tested; not-run statuses are `Error::Backend`; `Status::MemoryLimit` added | Exhaustive mapping was a plan requirement; a load error is not a numerical finding about the model |
| Late `Status` docs ("Backend outcome: optimal.") | Each status defined, including what `optimal` guarantees | The plan's §3.2 semantics belong in the type |
| `start_status: String`; Python result with nine flat metadata fields; `status: str` | `StartStatus` enum; nested `result.backend` report mirroring Rust; `Status` literal type | Typed, and the Python and Rust shapes match |
| `SolveResult::gap` and `checked_gap` side by side | One `gap()`, checked (returns `Result`); `Gap::new` stays the raw formula | One path; the delivered Python result already used the checked one |
| `Error::Backend` raised a plain `RuntimeError` | `lp_ip_craft.BackendError` (a `RuntimeError`) | Catchable specifically, like `EvaluationError` |
| Option and limit errors without the field name ("invalid HiGHS options or limit exceeds i32 range") | Messages name the field, entry and limit | Debuggability |
| Rust tests and Clippy in `--release` | Debug profile again | `highs-sys` always builds HiGHS in Release; `--release` only disabled Rust overflow checks in our own code |
| HiGHS built and tested on Linux only; macOS/Windows ran the dummy | HiGHS Python tests on Linux, macOS and Windows; a separate dummy-only job | The native backend is the point of the iteration |
| `docs/validation/*.log`, `docs/validation.md`, `docs/baseline.md` committed | Removed; validation is CI plus the PR description | Archive-specific logs go stale |
| License texts named after the Rust crates; wheel shipped only `LICENSE` | Named after HiGHS and `highs`; packaged in the wheel | The wheel links HiGHS statically |

Kept as delivered: the core's independence from native tooling, the checked
arithmetic, the backend-limit validation, the conversion through the safe
bindings, the interrupt-callback design (context boxed before the handle, no
Python, no unwinding), the node-limit refinement of `ReachedSolutionLimit`,
`mip_dual_bound` as the only bound source, no MIP duals, no LP bound, and the
default `backend="dummy"` that the plan asked for.

## Findings that affect interpretation

- HiGHS reports a reached `mip_max_nodes` as `ReachedSolutionLimit`; the
  adapter reports `node_limit` when a node limit was set and reached.
- `mip_dual_bound` is in the original sense with the offset included; the
  tests check it for both senses and positive and negative offsets.
- HiGHS's duals follow the crate's convention for minimization and
  maximization (checked against the tutorial's published duals).
- Cancellation reaches HiGHS at its next interrupt point; in a 400-variable
  binary MIP the run stopped about 0.3 s after the request, with the incumbent
  and bound kept.
- Several HiGHS instances on separate threads (one HiGHS thread each) gave
  identical results in the concurrency tests.

## Open decision: absolute feasibility tolerance

One of 599 random LPs with row activities near `1e5` was reported as
`numerical_failure` because HiGHS's solution violated a row by `1.4e-7`
against the absolute `1e-7` (a relative error of `1.4e-12`, at the limit of
`f64` sums). Tightening HiGHS's own tolerance did not change it: HiGHS checks
feasibility on its scaled model. The contract keeps absolute tolerances in
original units, as the plan requires; the message reports the violation. If
the GTOC model has large magnitudes, either set tolerances from the target card
or decide on a mixed absolute/relative criterion (violation `<= tol · max(1,
|bound|)`) for `evaluate`. That is a contract change and is left to the owner.

## Validation

Local (Linux x86_64, Rust 1.97, CPython 3.13, PyO3 0.30.1, maturin 1.15):
`cargo fmt --check`; Clippy with `-D warnings` for the workspace and the
dummy-only binding; all Rust tests and doctests in the debug profile; both
Rust examples; the extension built with `maturin develop` and the full Python
suite; a non-editable install of the default build and of the dummy-only build
in fresh virtual environments, each running the Python suite and examples.
CI runs the same on Linux, macOS and Windows with Python 3.10 and 3.14, plus
the MSRV 1.85 check. Exact counts are in the pull request.

## Outside this iteration

No native Rust simplex, interior-point method, branch-and-bound or cuts; no
persistent sessions, basis reuse, partial starts, second adapter (SCIP),
SOS/indicator/quadratic constraints, safe LP bounds or exact certificates;
one HiGHS thread per solve. No real application data, Gurobi or SCIP result or
16-core measurement was available, so no performance or GTOC acceptance claim
follows from these fixtures.

## Next measured task

1. Fill `target-card.md` with one representative GTOC LP/MIP instance, its
   reference solution and bound, and explicit unscaled tolerances.
2. Run it through this adapter with fixed seeds and one thread, and through the
   agreed external baseline (SCIP), checking every solution with `evaluate`.
   Record conversion, solve and check time, nodes, iterations, incumbent and
   bound.
3. Before anything prunes with an LP bound computed here: a safe dual bound
   (Neumaier–Shcherbina) with a reviewed validity argument.
4. Choose a native Rust component only from a measured gap, by the concept's
   track-selection rule.
