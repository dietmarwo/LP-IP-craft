# Iteration 1 implementation and validation

Version 0.2.0 · 16 October 2026 UTC. [Starting commit](baseline.md) and
[archived revision 2 implementation plan](iteration-1-plan.md).

## Delivered

| Plan package | Implementation and evidence |
| --- | --- |
| L0 baseline | Verified public main and retained existing contracts/tests; changes are local archive contents |
| L1 backend probe | Pinned highs 2.4.0 / highs-sys 1.15.0; real LP/MIP, offsets, starts, duals/bounds, native limits and cancellation exercised |
| L2 hardening | Checked bound distances, nonfinite transpose/KKT arithmetic, checked gaps, expanded results/statuses/errors and Python residual utility |
| L3 adapter | `crates/highs/src/lib.rs` validates/converts/solves/checks; only offset, int64 query and callback raw pointers live in `ffi.rs` |
| L4 limits and starts | Typed time/node/iteration controls, atomic interrupt callbacks, complete starts without rounding, submission metadata and late-stop incumbent preservation |
| L5 Python | Explicit backend dispatch within interpreter detach; versioned typed facade/stub; default feature and dummy-only feature builds |
| L6 acceptance/docs | Analytic LP and mixed-IP fixtures, exhaustive binary oracle, process workers, independent residuals, real Rust/Python examples, CI and clean-wheel tests |

The small conversion/result adapter remains together in `src/lib.rs`, rather
than prematurely splitting it into planned `convert.rs`/`result.rs`. Its native
handle never crosses threads. The public core has no dependencies or unsafe code.
The binding keeps the validated immutable model borrowed throughout the native
closure; it does not deep-copy a Python model at every solve.

## Backend findings that affect interpretation

The pinned native HiGHS version is reported at runtime (`1.15.0` in validation).
HiGHS reports a node cap as `ReachedSolutionLimit`; the adapter maps that status
to `node_limit` only when the configured node cap is reached. A deterministic
zero-node fixture verifies both absence of a candidate and retention of a
submitted feasible incumbent. Its absence of a finite bound is retained.

MIP bounds come only from `mip_dual_bound` in original objective sense including
offset. No bound is synthesized for LPs. Starts are `submitted` or `rejected`,
not labelled "accepted as incumbent" without evidence. MIP duals are absent;
valid continuous-LP duals pass the independent original-unit KKT checks.
All integer/real outputs retain their backend values; there is no rounding repair.

Magnitude restrictions and all settings are listed in [api.md](api.md).
Native defaults such as HiGHS's own presolve/scaling are retained; no new facade
scaling is applied. The public signed gap is separate from native gap-based
stopping. The general raw `Gap::new` API remains compatible; real result paths
use checked arithmetic and preserve the intentional zero-objective infinity.

## Validation

See [validation.md](validation.md) for exact completed gates, versions and counts.
Tests include minimization/maximization, positive/negative objective offsets,
fixed/free/no-row models, analytically infeasible and unbounded LPs, a genuinely
mixed binary/continuous case, and integer optima that differ from LP relaxations.
The exhaustive oracle uses 11 deterministic seven-binary models (128 assignments
each); the Python oracle is implemented independently of the Rust evaluator.

The original 35 Rust integration tests, doctest and 48 Python contract tests are
retained. New tests exercise the reproduced false-zero residual overflow,
malformed raw bounds, gap overflow, starts and limits, FFI callback interruption,
thread-triggered cancellation and two spawned process workers. The asynchronous
cancellation fixture permits a legitimate early optimum; it does not assert a
fragile wall-clock deadline. Direct callback tests deterministically pin the
interrupt request and zero-deadline behavior.

## What remains outside this iteration

There is no native Rust simplex, interior-point method, branch-and-bound or
cut generator. This project establishes the adapter and validation baseline.
There are no persistent basis sessions, partial starts, additional adapters,
SOS/indicators/quadratic constraints, exact certificates or package publication.
Only one engine thread is enabled. macOS/Windows native HiGHS builds were not
validated; their CI job retains the portable dummy interface.

No real application data, Gurobi/SCIP workload result or 16-core performance
measurement was supplied. The unchanged target card remains the next input for
choosing a useful native solver component. No speedup or production-readiness
claim follows from these small acceptance fixtures.

## Next measured task

Populate the target card with one representative LP and MIP instance, a reference
solution/bound and explicit unscaled tolerances. Replay the same starts, seeds
and one-thread resource budget through this adapter and the agreed external
baseline. Record construction, conversion, solve and checking time separately,
plus nodes/iterations, incumbent/bound and acceptance. Choose a native Rust
component only after those measurements identify a concrete bottleneck.
