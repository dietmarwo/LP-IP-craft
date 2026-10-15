# Contract: model, solve, results

Interface draft v0.1.0. Names and signatures are provisional until the first
real backend is integrated; `publish = false` prevents accidental releases.

## The model

```text
minimize or maximize   objective_offset + cᵀx
subject to             row_lower <= A x <= row_upper        (m rows, m >= 0)
                       lower     <=  x  <= upper            (n variables, n >= 1)
                       x_j integer for kinds[j] in {integer, binary}
```

Pure LP, pure IP and mixed-integer models share this structure. Equal finite
bounds express equalities (fixed variables, equality rows); `-inf` lower or
`+inf` upper bounds express one-sided or free entries. NaN, `lower = +inf`,
`upper = -inf` and `lower > upper` are rejected. Binary variables must have
bounds inside `[0, 1]`; fixed binaries are allowed. Integer bounds are used as
given, also when fractional. Objective coefficients, the offset and matrix
values must be finite. All numbers are IEEE-754 `f64` in original, unscaled
units; no scaling is applied. There are no SOS, indicator, quadratic or
nonlinear constraints. This is an interchange contract, not a modelling
language: `good_lp` or a backend API remain the place for model building.

`A` is given in canonical zero-based CSR: `indptr` has `m + 1` entries,
starting at 0 and ending at `nnz`; within each row, `indices` are strictly
increasing and below `n`; `values` follows `indices`. **Duplicate entries are
rejected, not summed**: combine and sort them first (for a `scipy.sparse` CSR
matrix, `A.sum_duplicates()` does both). Explicit zeros are allowed. A model
without rows has `indptr = [0]` and empty row bounds, indices and values.
`CsrMatrix::from_dense` builds the matrix of a small dense example.

The model is validated once, at construction (`Model::new(ModelData { .. })`
in Rust, the keyword-only `Model(...)` constructor in Python), and immutable
afterwards. Errors name the field and entry, for example
`variable bounds: entry 1 has lower=5, upper=1; need lower <= upper, ...`.
One model serves any number of solves, also concurrently from several
threads; in Python it is picklable for process-based workers and compares by
value. Python `kinds` defaults to all `"continuous"`.

## Solving

Rust: `LpIpSolver::solve(&mut self, model, options, start, cancel)`.
Python: `solve(model, *, start=None, options=None, cancellation=None)`.

`start` is an optional complete primal point (`WarmStart::from_primal` in
Rust, a sequence in Python). It is used exactly as given: it need not be
feasible or integral, and is never rounded, clamped or repaired. Unlike in
SQP-craft, a start outside the variable bounds is accepted, because a linear
model is defined everywhere. Partial MIP starts, simplex bases and dual starts
are deferred until a backend uses them.

`Options` exposes only settings a backend honours: `feasibility_tolerance` and
`integrality_tolerance` (both absolute, unscaled; default `1e-7`; integrality
in `(0, 0.5)`) and `threads` (only `1`; anything else is
`Unsupported`/`NotImplementedError`). Time, node and gap limits, presolve,
scaling and seeds will arrive with the backend that implements them, so no
setting is silently ignored. For comparison, Gurobi's defaults are
`FeasibilityTol = 1e-6` and `IntFeasTol = 1e-5`: a reference solution accepted
by Gurobi can fail the stricter default check here. Set the tolerances from
the target card when checking reference solutions.

Input errors are reported before any work and take precedence over a pending
cancellation. In Python the interpreter is released during the solve.

## Results

| Field | Meaning |
| --- | --- |
| `status` | `stub` or `cancelled` (more with real backends; `Status` is `#[non_exhaustive]`) |
| `candidate` | the returned point (the incumbent, for a MIP), or `None` if none was evaluated |
| `evaluation` | independent evaluation of the candidate (below); present exactly when `candidate` is |
| `duals` | LP duals in the convention below, when the backend computes them |
| `best_bound` | a **proven** bound on the optimal objective in the original sense, offset included: a lower bound when minimizing, an upper bound when maximizing. Never estimated |
| `gap` | derived from `evaluation.objective` and `best_bound` (below); Python field, Rust `SolveResult::gap(sense)` |
| `nodes` | branch-and-bound nodes processed |
| `iterations` | LP iterations over all nodes |
| `message` | explanation of the status |

A cancelled solve keeps the best candidate found so far, as MIP solvers keep
their incumbent when interrupted.

### Candidate evaluation

`check::evaluate` (Rust) / `evaluate(model, x, ...)` (Python) recomputes, from
the model data alone:

| Value | Definition |
| --- | --- |
| `objective` | `objective_offset + cᵀx`, original sense |
| `row_activity` | `A x` |
| `primal_violation` | largest violation of a variable or row bound, absolute, `0` if none |
| `integrality_violation` | largest `|x_j − round(x_j)|` over integer and binary variables |
| `feasible_candidate` | both violations within their tolerances (`<=`) |

`feasible_candidate` is a numerical statement about one point. It is not a
solver status: a feasible candidate is not optimal, and an infeasible one does
not show that the model is infeasible. Every backend's candidate is evaluated
by this function, so backends with differently scaled internal tolerances are
compared on one definition. It works on solutions from any solver, including
commercial reference solvers. Overflow of the objective or a row activity is an
`Evaluation` error, never a candidate.

### Bounds and gaps

Solvers define the relative gap differently, so a backend's own gap value is
not reported. The crate computes one gap whenever there is a `best_bound` and the
candidate passed the feasibility check:

```text
absolute = objective − best_bound    (minimize)
absolute = best_bound − objective    (maximize)
relative = absolute / |objective|    (objective ≠ 0)
relative = 0 if absolute == 0, else ±inf    (objective = 0)
```

The quotient is the one Gurobi's `MIPGap` uses (`|bound − incumbent| /
|incumbent|`); SCIP's default gap divides by the smaller of the two magnitudes
and is infinite for opposite signs. The zero-objective case is this crate's
convention. `absolute` keeps its sign: a negative value means the bound is
invalid, and it is reported, not clamped. A backend may only report a bound it
has proven; an approximate LP dual value is not a bound (see design notes,
next steps).

### LP duals

At an optimal LP solution, in the original objective sense,

```text
c = Aᵀ row_duals + reduced_costs
```

When minimizing, a positive dual belongs to an active lower bound and a
negative dual to an active upper bound; when maximizing the signs are
reversed. Zero means inactive; equalities accept either sign. This is the
convention of Gurobi's `Pi`/`RC` and HiGHS's `row_dual`/`col_dual`: the tests
and the Rust example check it against the duals both report for Example 1 of
the LP/IP tutorial (row duals `0, 11/3, 25/3`, reduced costs `0, 0, −5`).
For a MIP, duals can only describe an LP relaxation or a fixed-integer LP, and
a backend must say which.

`check::lp_residuals` (Rust only for now) checks a primal point with duals as
a solution of the LP, ignoring integrality:

| Residual | Definition |
| --- | --- |
| `primal` | largest bound violation of variables and rows |
| `stationarity` | `‖c − Aᵀy − z‖∞` |
| `complementarity` | largest `|μ| · |distance to the bound selected by the sign of μ|` |
| `dual_sign` | largest `|μ|` whose sign selects an infinite bound |

All four are zero at an optimal pair. They are not a bound on the objective
error and do not certify a MIP.

### The dummy backend, exactly

1. Validate options and the start, even if cancellation was requested.
2. If cancelled, return `cancelled` without candidate or evaluation.
3. Take the start unchanged, or else zero clamped to each variable's bounds.
   Neither path rounds integer variables or repairs rows.
4. Evaluate the point and return it with `status = stub`.

`duals` and `best_bound` (hence `gap`) are always `None` and `nodes` and
`iterations` zero, even at an analytically optimal candidate: the dummy never
claims optimality, infeasibility or unboundedness.

## Errors

| Rust `Error` | Python exception | When |
| --- | --- | --- |
| `InvalidInput` | `ValueError` | malformed model, start, point, duals or options |
| `Evaluation` | `lp_ip_craft.EvaluationError` (a `RuntimeError`) | objective or row activity overflow |
| `Unsupported` | `NotImplementedError` | valid but unimplemented request (`threads != 1`) |
| — | `TypeError` | wrongly typed input (text where a sequence is expected, floats as indices, ...) |

Errors never carry partial results.

## Cancellation and threading

`CancellationToken` (Rust) / `Cancellation` (Python) is a shared, one-way flag
checked at each backend's documented checkpoints. It is not a hard interrupt
or a time limit; use a new token for a new run.

The core crate has no dependencies. `Model` is immutable and `Sync`;
`LpIpSolver::solve` takes `&mut self`, so each concurrent solve uses its own
solver instance. There are no Python callbacks: model data is copied into Rust
once, when the Python `Model` is built, and solves run with the interpreter
detached, so independent solves from several Python threads run in parallel.
Element-wise conversion of large NumPy arrays costs time at construction; a
buffer-protocol path can be added once it is measured on the real model.
