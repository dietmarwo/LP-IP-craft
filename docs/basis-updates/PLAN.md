# Measured basis-update work (0.18)

Baseline: delivered 0.17.0, commit `ba4fd6b92b8d161955f9489ac9916188376dbd25`.
The objective is to reduce the large-LP gap to the isolated HiGHS reference,
while preserving the pure-Rust production engine and existing acceptance checks.

## 1. Profile before changing the algorithm

Plan: instrument basis solves, eta application, factorization, pricing,
ratio tests and remaining pivot work. Use exclusive nested timings so that
basis solves inside pricing are not double-counted. Compile instrumentation
out of normal builds; time single-threaded synthetic LP families.

Implemented: optional `kernel-profile`, thread-local reset/snapshot API,
JSON benchmark output, paired profiling script. No numerical dependencies.
Initial measurements identified expensive column-eta work on packing,
factorization on irregular LPs, and triangular/pricing work on network LPs.

Validation: original workspace suite; all independently checked profiling
fixtures. Final paired measurements appear in RESULTS.md.

## 2. Implement Forrest–Tomlin updates with a checked fallback

Plan: maintain both orientations of the changing U factor, retain the original
L factor, insert the transformed entering column, move the replaced row/column
to the end of the pivot order, and record the row elimination. Reuse the
partial unit BTRAN already required for pivot-row pricing. Preserve its cache
across the intervening edge-weight FTRAN. Retain product-form etas and rebuild
when an update is numerically unsuitable.

Implemented: `ft.rs`; partial entering-column capture; partial unit-BTRAN cache;
checked new pivot and elimination consistency; finite/magnitude guards;
eta fallback with forced refactorization at the next safe point. A rejected
update still has a valid factor representation for cancellation/limit cleanup.
Basis restoration can compute its elimination without the pricing cache.
The configured interval and numerical-entry cap remain enforced.

Validation: direct forward/transpose residuals after repeated replacements,
permuted bases, cached/noncached updates, intervening FTRAN, sparse support
checks, near-singular fallback; complete LP/MIP tests and forced-FT oracles.
The direct tests perform 2,120 repeated updates across their case loops.

## 3. Carry sparse support through the solver

Plan: avoid full pivot-row and entering-column rescans; use sparse U traversal
when few entries are reachable, and dense sweeps otherwise. Do not introduce
new numerical dropping thresholds.

Implemented: bounded symbolic reach in both U orientations, reusable marks
and an eight-call cooldown after dense reach; row support passed into L-transpose
reach and LP pricing; entering-column support collected during U solve;
sparse clearing when cheap; skip L entirely when it is the identity.
Pricing retains ascending row order, so its accumulation order is preserved.
Entering-column indices need not be sorted: their consumer updates are
independent per basis position. No dense tableau was introduced.

Validation: sparse/dense factor residuals and all existing acceptance checks.
A row-gather alternative for dense upper solves was rejected: it slowed large
packing and failed acceptance on packing-1024. Its raw record is retained.

## 4. Select update method conservatively, then compare and package

Plan: use observed factor/update work to avoid imposing the FT cost on cheap
eta or dense-factor workloads. Expose forced methods for repeatable experiments.
Run fresh uninstrumented comparisons against 0.17 and HiGHS, full oracle
checks, strict Clippy, Python tests, and archive all source and evidence.

Implemented: Rust `BasisUpdate::{Auto,Eta,ForrestTomlin}` and Python
`basis_update="auto"|"eta"|"forrest_tomlin"`. Auto starts with etas and at a
refactor enables FT only if all these hold:

- Basis dimension is at least 128.
- L contains at most m/8 off-diagonal entries and the base factor at most 16m entries.
- Previous mean entering-column support times the configured update interval
  exceeds four times the new base-factor entry count.

These are conservative, deterministic work estimates. There is no fixture-name
recognition or runtime timing dependence. Forced FT remains available for
other applications; auto does not yet attempt to learn a general optimal policy.
The first density-only policies regressed the irregular fixture and were rejected.

See RESULTS.md for measured outcomes, limitations and commands.
