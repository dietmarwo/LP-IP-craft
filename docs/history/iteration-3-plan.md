# Iteration 3 plan — efficient, faithful HiGHS model loading

16 October 2026. Base: reviewed public `main` at
`a7a04074747a99048810204efe0421b4093c9fb4` (iteration 2, version 0.3.0).

## Review and scope

The base adds reviewed outward-rounded conservative bounds and margin-dual
examples for one-sided columns. Preserve their arithmetic, exact-oracle tests,
statuses and availability behavior. The GTOC target card remains incomplete;
this iteration must not claim performance on an unavailable application.

The efficiency target is the adapter's setup path. Every solve currently
copies immutable CSR data through `highs::RowProblem`: per-column growable
index/value vectors, flattening into CSC, and copying costs/bounds into the
builder before HiGHS imports them. This adds allocation and memory traffic
before LP/MIP search. The safe binding itself ultimately uses the bulk C API.
The pinned HiGHS 1.15.0 API accepts CSR directly, allowing those intermediates
to be removed while retaining the same solver and independent checks.

A related correctness issue is visible in the existing order: the adapter
loads the matrix before applying `small_matrix_value=1e-12`. HiGHS's initial
threshold is 1e-9, so some supported nonzero coefficients can be discarded
before the intended option takes effect. Reproduce this on the base and fix
it as part of arranging the new import path.

## Measurement first

Reproduce the reviewed base tests. Before modifying the adapter, build a release
wheel and use `examples/benchmark_efficiency.py` to record setup, engine,
independent-check and end-to-end times on four fixed fixtures:

1. A 100,000-column row-free LP, to expose column allocation overhead.
2. A 40,000-row/column sparse LP with 200,000 entries and an analytic optimum,
   explicitly an import fixture rather than a hard optimization problem.
3. A balanced 60-by-80 transportation LP requiring a simplex solve.
4. A 12,000-binary-variable partition MIP with an analytic optimum.

Keep model construction outside each timed solve and report it separately.
Use existing `BackendReport` phase times; no new telemetry API is necessary.
Warm each case, record every run, and compare identical fixtures using separately
installed baseline/new wheels. Alternate process order in the final comparison.
Require the same accepted objectives, unchanged residual thresholds and record
iteration/node counts. Use medians, ranges and full raw JSON; never add CI timing
thresholds. Report negative or neutral results as well as improvements.

## Implementation

Baseline reproduced: 71 Rust tests plus one doctest and 94 compiled Python
tests pass. A three-run profile measured median setup times of 3.37 ms (box),
8.88 ms (sparse import), 0.64 ms (transport) and 1.37 ms (partition). Setup is
only part of end-to-end time: approximately 23%, 9%, 5% and 2%, respectively,
in this initial sample. Therefore the main expected gain is reduced import
overhead; engine-dominated workloads may show little total improvement.
The small-coefficient regression fails on the base with a false `Infeasible`
status for an analytically feasible minimization LP.

- Create an empty handle owned by the existing safe `highs::Model` wrapper.
  Apply existing typed settings before importing user data.
- Add one narrow, documented unsafe boundary in `ffi.rs` calling
  `Highs_passLp` or `Highs_passMip` with `kHighsMatrixFormatRowwise`.
- Borrow original cost, bound and coefficient slices for the synchronous C
  call. Convert `usize` CSR indices to checked `HighsInt` buffers; allocate
  an integrality buffer only for a MIP. Pass sense and offset in the same call.
- Preserve canonical row ordering, finite/infinite bounds, empty rows,
  row-free models, explicit zeros, variable domains, error handling, starts,
  cancellation and cooperative time-budget semantics. HiGHS copies the arrays;
  the adapter retains no borrowed pointer after import.
- Preserve warning handling for legal imports, including explicit zeros. Check
  that the imported nonzero count equals the count of stored nonzero values,
  so unexpected coefficient removal cannot silently change a model.
- Keep the core and Python API unchanged except version 0.4.0. This path applies
  to all normal HiGHS solves without a new option. Do not remove independent
  primal/dual checks to make a timing result look better.
- No persistent native handle, basis reuse, matrix mutation API, native search
  algorithm, new dependency or parallelism in this iteration. Those require
  a distinct workload contract and lifecycle design.

## Correctness and release gates

- Existing Rust, compiled Python and dummy-only suites, including all exact
  conservative-bound oracles, margin duals, cancellation, limits, starts and
  independent worker tests.
- New analytic LP/MIP cases for coefficients between 1e-12 and 1e-9 in both
  senses, plus rejection at/below the documented lower threshold.
- Sparse import cases with explicit zeros, empty/free rows, unused columns,
  fixed and ranged bounds, objective offsets and mixed domains. Compare LP
  residuals and MIP objective/bound against analytic or enumerated answers.
- Deterministic generated small LPs/MIPs checked against exhaustive integer
  oracles where applicable, plus identical benchmark objectives across wheels.
- Formatting, Clippy (native and feature-disabled), debug Rust tests, rustdoc,
  Rust 1.85 compatibility, strict Python type checking and clean wheel installs.
- Re-run bound availability. Preserve and report all available/unavailable
  counts; never weaken tolerances or interval arithmetic to obtain speed.
- Add benchmark smoke checks to CI, retain historical evidence, and package
  full source, plan, results, raw measurements and reproduction commands.

## Stop rule and follow-up

Keep the optimization if it reduces setup cost on the large import fixtures
without an unexplained material regression on the transportation or partition
cases. If engine time dominates those cases, report the limited end-to-end
benefit rather than adding unrelated changes. Application data is still needed
to choose basis reuse, batching, parallel workers or native LP/MIP components.

## Completion record

Implemented as version 0.4.0 with the planned API-compatible bulk CSR path.
The small-coefficient regression now passes. All 74 Rust tests plus one
doctest, 102 native Python tests and 71 dummy-only tests pass, as do Rust 1.85,
formatting, Clippy, rustdoc and mypy. Bound availability matches the reviewed
base. Seven alternating baseline/new measurements show 2.4–4.1× faster setup;
median end-to-end reductions are 8.3% (box), 11.6% (sparse import), 2.9%
(transport) and 2.1% (partition). The small engine-dominated improvements have
overlapping timing ranges. Full source, tests, raw measurements and detailed
limits are in `docs/iteration-3-results.md` and
`docs/iteration-3-validation.md` in the source ZIP.

## Primary implementation references

- Pinned HiGHS source, `highs/interfaces/highs_c_api.h` and `Highs.cpp` from
  the `highs-sys=1.15.0` crate: CSR import, synchronous copying, model validation.
- Pinned `highs=2.4.0`, `src/matrix_row.rs` and `src/lib.rs`: builder allocation,
  CSC flattening, safe handle ownership and warning handling.
- [HiGHS source](https://github.com/ERGO-Code/HiGHS) and
  [Rust bindings](https://github.com/rust-or/highs).
- Repository `docs/iteration-2-results.md`, `docs/safe-dual-bounds.md` and
  `docs/target-card.md` for the current correctness and application boundaries.
