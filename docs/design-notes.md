> Historical interface-draft review, retained for provenance. See
> [iteration-1-results.md](iteration-1-results.md) for the implemented 0.2.0 decisions, tests
> and limitations. Statements below about future integration describe v0.1.

# Design notes

Scope: the LP/MIP track of Planning2027 Idea 004 ("Open Rust solvers",
concept v3, 22 September 2026). The concept makes MIP part of the required
scope from the start: the confirmed application is a GTOC-based linear model
with integer variables. This repository covers only that track; the nonlinear
track lives in [SQP-craft](https://github.com/dietmarwo/SQP-craft), which
follows the same conventions. This repository defines contracts and test
fixtures. It contains no optimizer, makes no performance or parity claims, and
does not depend on the competition workflow.

## Review of the 15 October interface draft

The draft (Rust core, PyO3 binding, typed Python facade, dummy backend, 9 Rust
and 25 Python tests) built and passed as delivered, with clean `fmt` and
`clippy`. Running it and comparing it with the concept led to these changes:

| Draft | Change | Reason |
| --- | --- | --- |
| `Model` with public fields, validated in `solve`, again in `evaluate_candidate` and again in `CsrMatrix::multiply` (three passes per solve); the Python dataclass was deep-copied by `asdict` and converted again on every solve | `Model::new(ModelData)` validates once, immutable afterwards; the Python `Model` is a native, keyword-only, frozen class built once, picklable and comparable; solves borrow it | Errors surface where the model is built; one model serves many solves (multistart, parallel workers) without per-solve copies |
| Python `Model` took nine positional lists (`Model([2.,3.],[0.,0.],[1.,1.],...)` in the tests) | Keyword-only constructor; `kinds` defaults to all continuous | Swapping `lower` and `row_lower` was silent |
| Errors without context (`invalid bounds`, `CSR endpoints`); Rust `Display` printed `Debug` | Messages name field and entry; `Display` adds the category, `message()` does not | Debuggability on large models |
| `best_bound`, `absolute_gap`, `relative_gap` stored separately, always `None`, formula left to future adapters | `best_bound` is a proven bound in the original sense; the crate derives one `Gap` from it and the candidate, only for a feasible candidate | Solvers define the relative gap differently (Gurobi divides by the incumbent, SCIP by the smaller magnitude). Adapters filling the fields themselves would make gaps incomparable exactly where milestone 1 compares backends. Fixing it now costs nothing |
| No LP duals; sign conventions deferred | `Duals { row_duals, reduced_costs }` with `c = Aᵀy + z` in the original sense; independent `check::lp_residuals` | The convention is pinned by tests against the duals Gurobi and HiGHS report for the tutorial's production LP, including the minimization with flipped signs. Shares the KKT-style residual definitions of SQP-craft |
| Primal evaluator in Rust only | `evaluate` also in Python | Milestone 1 needs independent checks of HiGHS, SCIP and reference-solver solutions now, before any backend exists here |
| Cancellation observed after evaluation discarded the evaluated candidate | The dummy has one checkpoint (before evaluation); the contract says a cancelled solve keeps its best candidate | MIP solvers keep the incumbent when interrupted; a late cancellation should not throw work away. The removed second checkpoint was untestable |
| Nested `SolveOptions { common: Options, integrality_tolerance }` | `Options { tolerances: Tolerances, threads }`; `Tolerances` also parameterizes `evaluate` | The nesting was an artifact of sharing `common.rs` with the SQP archive |
| `threads = 0` reported as unsupported | `InvalidInput` / `ValueError`; `threads > 1` stays `Unsupported` | Malformed is not unimplemented |
| `Bounds::violation` silently truncated mismatched lengths | Asserts equal lengths | Under-reported violations |
| Overflow raised plain `RuntimeError` | `EvaluationError`, a `RuntimeError` subclass | Catchable specifically; same as SQP-craft |
| Indices extracted as `usize` (negative values gave an `OverflowError` without field) | Field and entry in every conversion error; negative indices are `ValueError` | |
| `include!("common.rs")` copy in the binding, archive checksums and notices, `open-lpip` names, PyO3 0.27.2, edition 2021 | Proper modules, repository names (`lp-ip-craft`, `lp_ip_craft`), MIT per the repository LICENSE, PyO3 0.30.1, edition 2024 with MSRV 1.85, CI | Repository rather than archive; same toolchain as SQP-craft |
| `Status`, `Error` exhaustive | `#[non_exhaustive]`, also `WarmStart` | Real backends add statuses and start data without a breaking change |

Kept unchanged in substance: the dummy's evaluation-only behavior and its
refusal to claim optimality, infeasibility or unboundedness (`stub` even at
the optimum, `None` for bounds and duals); starts used exactly as given,
never rounded or repaired; zero clamped to the bounds when there is no start;
canonical CSR with duplicates rejected; absolute tolerances in original units;
`threads = 1` only and no silently ignored settings; a core crate without
dependencies; the interpreter released during solves; the binding tested only
through the compiled extension.

One difference to SQP-craft is deliberate: a start outside the variable bounds
is accepted and its violation reported, because a linear model is defined
everywhere. SQP-craft rejects such starts since a nonlinear model may be
undefined there.

## Deliberately deferred

No setting is exposed before a backend honours it. Deferred until a real
backend or the GTOC application needs them:

- statuses beyond `stub`/`cancelled` (optimal, infeasible, unbounded,
  infeasible-or-unbounded, limit reached), each with a definition that says
  what was proven;
- time, node, gap and solution limits; presolve, scaling and seed settings;
  a run record with phase timings and thread counts;
- partial MIP starts, simplex bases and dual starts in `WarmStart`;
- backend capability negotiation (integer support, duals, bases, thread
  safety);
- an incumbent/bound observer for progress and early stopping, along the lines
  of SQP-craft's `Observer`;
- Python bindings for `lp_residuals`; NumPy buffer-protocol input;
- SOS, indicator and quadratic constraints;
- a shared crate for the code common to SQP-craft (bounds, CSR, errors,
  cancellation), once both real workloads exercise it.

## Numerical boundary

All checks are floating-point checks, not exact certificates. Infinite values
are allowed only as `-inf` lower or `+inf` upper bounds; objective, matrix,
points and duals must be finite. Feasibility uses absolute tolerances in
original units, without row scaling; badly scaled rows may need per-row
tolerances defined in the target card. Integer variables beyond `2^52` in
magnitude always pass the integrality check, because every such `f64` is an
integer. A feasible candidate is not optimal; an infeasible candidate does not
prove the model infeasible; small LP residuals do not bound the objective
error.

## Next steps

1. Fill `target-card.md` from the GTOC application and freeze its reference
   data before any tuning.
2. Check the reference solution and the open baselines (HiGHS and SCIP, via
   `good_lp` where it suffices) with `evaluate`, using the target card's
   tolerances. This needs nothing beyond this repository and the solvers.
3. First real adapter behind `LpIpSolver` (HiGHS is the natural first, being
   MIT-licensed with Rust bindings): map its statuses to new, precisely defined
   `Status` variants, report its proven bound as `best_bound` and its duals in
   the crate's convention, and run the contract tests against it.
4. Before anything prunes with an LP bound computed here: a safe dual bound
   (Neumaier–Shcherbina, Math. Programming 99, 2004) with a reviewed validity
   argument, including floating-point rounding. Approximate duals are not
   bounds.
5. Choose the first native LP/MIP component by the concept's track-selection
   rule. Performance-only SCIP plugins (primal heuristics, branching) are the
   safe first targets; cuts and propagation need a validity argument.
