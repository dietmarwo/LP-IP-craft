# Contract: model, solve, results

Version 0.3.0 (iteration 2). Interfaces are pre-release; `publish = false`
prevents accidental releases.

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

Rust: `LpIpSolver::solve(&mut self, model, options, start, cancel)`, implemented
by `lp_ip_craft_highs::HighsSolver` (real LP/MIP solves) and
`lp_ip_craft::DummySolver` (contract fixture).

Python: `solve(model, *, backend="dummy", start=None, options=None,
backend_options=None, cancellation=None)`. Pass `backend="highs"` to optimize;
the default `"dummy"` is kept from v0.1 and only evaluates one candidate.
`lp_ip_craft.BACKENDS` lists the backends compiled into the extension. An
unknown backend is a `ValueError`; a known one that is not compiled in (a build
with `--no-default-features`) is a `NotImplementedError`. There is never a
silent fallback to the dummy or to an LP relaxation.

`start` is an optional complete primal point (`WarmStart::from_primal` in
Rust, a sequence in Python). It is used exactly as given: it need not be
feasible or integral, and is never rounded, clamped or repaired. Unlike in
SQP-craft, a start outside the variable bounds is accepted, because a linear
model is defined everywhere. Partial MIP starts, simplex bases and dual starts
are deferred until a backend uses them.

`Options` holds the settings every backend honours: `feasibility_tolerance` and
`integrality_tolerance` (both absolute, unscaled; default `1e-7`; integrality
in `(0, 0.5)`) and `threads` (only `1`; anything else is
`Unsupported`/`NotImplementedError`). Backend-specific settings are typed
(`HighsOptions`, below); there is no untyped option pass-through, and no setting
is silently ignored. For comparison, Gurobi's defaults are
`FeasibilityTol = 1e-6` and `IntFeasTol = 1e-5`: set the tolerances from the
target card when checking reference solutions.

Input errors are reported before any work and take precedence over a pending
cancellation. In Python the interpreter is released during the solve.

## Results

| Field | Meaning |
| --- | --- |
| `status` | why the solve ended (below) |
| `candidate` | the returned point (the incumbent, for a MIP), or `None` if the backend declared no usable primal solution |
| `evaluation` | independent evaluation of the candidate (below); present exactly when `candidate` is |
| `duals` | LP duals in the convention below; never for a MIP |
| `best_bound` | a **proven** bound on the optimal objective in the original sense, offset included: a lower bound when minimizing, an upper bound when maximizing. Never estimated |
| `gap` | derived from `evaluation.objective` and `best_bound` (below); Python field, Rust `SolveResult::gap(sense)` |
| `nodes` | branch-and-bound nodes processed |
| `iterations` | LP iterations over all nodes |
| `message` | explanation of the status, including why an optimum was rejected |
| `backend` | `BackendReport`: provenance, applied settings, timings (below) |

### Statuses

The status is the termination reason; candidate quality is reported
separately. A limit can end a solve with a feasible incumbent, and a solve
without candidate can carry a proven bound. A solve stopped by a limit or by
cancellation keeps the best candidate found so far.

| Status | Meaning |
| --- | --- |
| `stub` | the dummy evaluated one candidate; no claim of any kind |
| `cancelled` | the cancellation token stopped the solve |
| `optimal` | the engine reported optimality **and** the independent checks passed: the candidate is a feasible candidate in original units, and for an LP with duals, stationarity, dual signs and the relative duality gap are within the backend's thresholds. Numerical, not an exact certificate |
| `infeasible` | the engine reported the model infeasible |
| `unbounded` | the engine reported the model unbounded; a returned candidate is just a feasible point |
| `infeasible_or_unbounded` | the engine established one of the two, not which |
| `time_limit`, `node_limit`, `iteration_limit`, `solution_limit`, `objective_limit`, `memory_limit` | a limit stopped the engine |
| `numerical_failure` | the engine failed numerically, or reported optimality but the independent checks rejected the result; the message says which check |
| `unknown` | the engine stopped without a definite outcome |

Backends map every native status explicitly and keep the native name in
`backend.raw_status`. They never report a more precise outcome than the engine.

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
commercial reference solvers. Overflow of the objective, a row activity or a
bound distance is an `Evaluation` error, never a candidate.

The tolerances are absolute. On rows whose activity is large (around `1e5`),
an absolute `1e-7` is close to the precision of `f64` sums, and an engine that
checks feasibility on its scaled model can miss it by roundoff. Such a result
is reported as `numerical_failure` with the violation in the message; choose
tolerances that fit the model's magnitudes.

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

The quotient is the one Gurobi's `MIPGap` and HiGHS's `mip_rel_gap` use
(`|bound − incumbent| / |incumbent|`); SCIP's default gap divides by the smaller
of the two magnitudes and is infinite for opposite signs. The zero-objective
case is this crate's convention. `absolute` keeps its sign: a negative value
means the bound is invalid, and it is reported, not clamped. Rust's
`SolveResult::gap` and `Gap::try_new` return an `Evaluation` error if the
subtraction overflows; `Gap::new` is the raw IEEE formula. A backend may only
report a bound it has proven; an approximate LP dual value is not a bound.
The separate `safe_dual_bound` API below computes a conservative bound with
roundoff correction but does not change `best_bound` or the gap semantics.

### LP duals

At an optimal LP solution, in the original objective sense,

```text
c = Aᵀ row_duals + reduced_costs
```

When minimizing, a positive dual belongs to an active lower bound and a
negative dual to an active upper bound; when maximizing the signs are
reversed. Zero means inactive; equalities accept either sign. This is the
convention of Gurobi's `Pi`/`RC` and HiGHS's `row_dual`/`col_dual`: the tests
check it against the duals HiGHS returns for Example 1 of the LP/IP tutorial
(row duals `0, 11/3, 25/3`, reduced costs `0, 0, −5`), the values Gurobi also
reports. MIP results carry no duals.

`check::lp_residuals` / Python `lp_residuals(model, x, duals)` checks a primal
point with duals as a solution of the LP, ignoring integrality:

| Residual | Definition |
| --- | --- |
| `primal` | largest bound violation of variables and rows |
| `stationarity` | `‖c − Aᵀy − z‖∞` |
| `complementarity` | largest `|μ| · |distance to the bound selected by the sign of μ|` |
| `dual_sign` | largest `|μ|` whose sign selects an infinite bound |
| `duality_gap` | `|P − D| / (1 + |P| + |D|)` with `P = cᵀx` and `D = Σ μ · (bound selected by the sign of μ)` over finite selected bounds |

All are zero at an optimal pair. `complementarity` is a product of a dual and
a distance and grows with both magnitudes; `duality_gap` measures the same
property scale-free and is what acceptance checks use. None of them bounds the
objective error or certifies a MIP. Intermediate overflow is an `Evaluation`
error, so a NaN cannot disappear inside a maximum.

### Independent conservative dual bounds (iteration 2)

Rust exports `safe_dual_bound(&Model, &[f64]) -> Result<SafeDualBound>`;
Python exposes `safe_dual_bound(model, row_duals) -> SafeDualBound`.
Multipliers use the same original-sense sign convention as `Duals.row_duals`.
They need not be optimal. The checker recomputes the entire dual residual
from the model, so no reduced costs or primal point is supplied.

`SafeDualBound` contains `bound: Option<f64>` / `Optional[float]`, typed
`status: SafeBoundStatus`, and `index: Option<usize>` / `Optional[int]`.

| Rust / Python status | Bound | Index / interpretation |
| --- | --- | --- |
| `Finite` / `finite` | finite, original sense and offset | None; lower bound for minimization, upper for maximization |
| `UnboundedRow` / `unbounded_row` | None | First row whose nonzero multiplier selects an infinite side |
| `UnboundedVariable` / `unbounded_variable` | None | First variable whose residual enclosure has an adverse infinite direction |
| `ArithmeticOverflow` / `arithmetic_overflow` | None | None; a finite intermediate enclosure could not be maintained |
| `UnsupportedArithmetic` / `unsupported_arithmetic` | None | None; runtime gradual-underflow probes failed |

An unavailable result describes the calculation, not the model: it does not
prove infeasibility or unboundedness. Incorrect multiplier length or nonfinite
entries raise `InvalidInput` / `ValueError` with field context. Neither inputs
nor multipliers are repaired. Integrality is relaxed without changing the model
or rounding its bounds. No feasibility tolerance enters this function.

Validity is for the exact real model encoded by the stored binary64 inputs.
Outward enclosures require standard binary64 operations and gradual underflow;
do not compile with fast-math or change the arithmetic environment during the
call. Runtime probes reject common flush-to-zero modes.

Columns with an infinite bound limit availability. The exact residual
`c_j − a_jᵀy` (oriented to minimization) must be `≥ 0` without an upper bound,
`≤ 0` without a lower bound, and exactly `0` for a free column. LP solver
duals meet this only to the solver's tolerance, so with HiGHS duals a model
with such columns rarely gets a bound: 5 of 100 random `x ≥ 0` LPs did. For
most of the others, the exact Lagrangian bound of those duals is itself
infinite. Duals of the LP with one-sided costs shifted by `1e-6 · max(1, |c_j|)`
in the safe direction (`margin_duals` in `examples/safe_bound.py`) gave a
bound for all 100, about `1e-6` relative below the optimum. Free columns need
finite bounds; interval width can also prevent a bound when the exact
residual is zero. See [availability with solver duals](safe-dual-bounds.md#availability-with-solver-duals).

The function is dependency-free, uses O(nnz + n + m) time and O(n) workspace,
runs with Python detached and is available in dummy-only wheels. It is an
explicit independent calculation: it neither certifies HiGHS's MIP bound/tree
nor validates a primal candidate exactly. An ordinary tolerance-feasible
candidate and its rounded objective cannot automatically be used to make a
rigorous pruning decision. See the [derivation and examples](safe-dual-bounds.md).

### Backend report

| Field | Meaning |
| --- | --- |
| `name`, `version` | backend (`dummy`, `highs`) and native engine version |
| `raw_status` | the engine's own status name; `None` if it never ran |
| `start_status` | `not_provided`, `submitted` (the engine took the start, which does not mean it became the incumbent), `rejected`, or `evaluated` (dummy) |
| `effective_options` | native settings applied, by native name; not an export of every native default |
| `total_seconds`, `conversion_seconds`, `solve_seconds`, `check_seconds` | wall-clock phases after input validation |
| `time_budget_exceeded` | the call outlasted its time budget; `status` still says why the engine stopped |

Timings are for reporting; no correctness check depends on them.

### The dummy backend, exactly

1. Validate options and the start, even if cancellation was requested.
2. If cancelled, return `cancelled` without candidate or evaluation.
3. Take the start unchanged, or else zero clamped to each variable's bounds.
   Neither path rounds integer variables or repairs rows.
4. Evaluate the point and return it with `status = stub`.

`duals` and `best_bound` (hence `gap`) are always `None` and `nodes` and
`iterations` zero, even at an analytically optimal candidate: the dummy never
claims optimality, infeasibility or unboundedness.

## The HiGHS backend

`lp_ip_craft_highs::HighsSolver` drives HiGHS 1.15.0 through the `highs` 2.4.0
Rust bindings; `highs-sys` builds HiGHS from source. Each solve builds a fresh
native model on the calling thread and destroys it before returning.

| `HighsOptions` field | Default | Meaning |
| --- | --- | --- |
| `time_limit_seconds` | `None` | cooperative budget for the whole call after validation (conversion, run, checks); HiGHS gets the remaining time as `time_limit` and the interrupt callback polls it |
| `node_limit` | `None` | `mip_max_nodes`; MIP models only (an LP with a node limit is `Unsupported`, HiGHS would ignore it) |
| `iteration_limit` | `None` | `simplex_iteration_limit` and `ipm_iteration_limit`; LP models only (HiGHS does not apply them to the LP subproblems of a MIP) |
| `seed` | `0` | `random_seed` |
| `mip_relative_gap`, `mip_absolute_gap` | `0`, `0` | HiGHS stopping rules `mip_rel_gap`, `mip_abs_gap` (HiGHS defaults `1e-4`, `1e-6`); they do not change the result's `gap` |
| `dual_tolerance` | `1e-7` | `dual_feasibility_tolerance`, and the LP acceptance threshold below |

Integer settings are limited to `0..=2^31−1`; violations are `InvalidInput`
naming the field. The adapter always sets `threads = 1`, `output_flag = false`,
`small_matrix_value = 1e-12` and `primal_feasibility_tolerance` from `Options`;
for MIPs it sets `mip_feasibility_tolerance` to the smaller of the two
tolerances, because HiGHS uses one MIP tolerance for integrality and rows.

**Backend limits** (`Unsupported`/`NotImplementedError`, checked before any
work): tolerances below `1e-10`; finite bounds, objective coefficients, offset
or start values with magnitude `>= 1e20` (HiGHS's infinity); nonzero matrix
values outside `(1e-12, 1e15)`; integer bounds or start values beyond
`±(2^53 − 1)`; more than `2^31 − 1` variables, rows or entries. The general
model and evaluator keep their wider finite-`f64` domain.

**Status mapping.** Every status of the pinned bindings is mapped explicitly
(see `crates/highs/src/status.rs`). HiGHS reports a reached `mip_max_nodes` as
`ReachedSolutionLimit`; with a node limit set and reached, the adapter reports
`node_limit`. An interrupt becomes `cancelled` or `time_limit` depending on
which requested it. `NotSet`, `LoadError`, `ModelError` and `ModelEmpty` mean
HiGHS did not solve the model: a `Backend` error, never a finding about it.

**Acceptance.** A candidate is read only when HiGHS declares a feasible primal
solution, then evaluated with `evaluate`. For an LP with feasible duals, the
duals are returned and checked with `lp_residuals`: `optimal` requires
stationarity and dual sign residuals up to `dual_tolerance · (1 + ‖c‖∞)` and a
relative duality gap up to `dual_tolerance`. MIP results get no duals;
`best_bound` is HiGHS's finite `mip_dual_bound` (original sense, offset
included). A bound on the wrong side of the incumbent by more than
`feasibility_tolerance · (1 + |objective|)` rejects an optimum. LP results have
no `best_bound` yet.

**Cancellation.** The interrupt callback (simplex, IPM, MIP) reads the atomic
token and the time budget; it never calls Python or unwinds. Conversion is
checked before and after, not interrupted midway. Cancellation is cooperative:
HiGHS stops at its next interrupt point, which took about 0.3 s in a MIP test.
A cancellation or budget overrun after HiGHS has returned does not change the
status; an overrun sets `time_budget_exceeded`.

**Concurrency.** Independent solves on several threads (Rust threads, or
Python threads with the interpreter released) each use their own HiGHS
instance with one thread; the tests run them concurrently. Thread counts above
one per solve are not supported yet.

## Errors

| Rust `Error` | Python exception | When |
| --- | --- | --- |
| `InvalidInput` | `ValueError` | malformed model, start, point, duals, options or backend settings |
| `Evaluation` | `lp_ip_craft.EvaluationError` (a `RuntimeError`) | objective, activity, residual or gap overflow |
| `Unsupported` | `NotImplementedError` | valid but unimplemented request (`threads != 1`, a backend limit, a backend not compiled in) |
| `Backend` | `lp_ip_craft.BackendError` (a `RuntimeError`) | the engine failed to set up or run the model |
| — | `TypeError` | wrongly typed input (text where a sequence is expected, floats as indices, a string as `node_limit`, ...) |

Errors never carry partial results.

## Cancellation and threading

`CancellationToken` (Rust) / `Cancellation` (Python) is a shared, one-way flag
checked at each backend's documented checkpoints. It is not a hard interrupt
or a time limit; use a new token for a new run. Use a separate process where a
hard stop is needed.

The core crate has no dependencies. `Model` is immutable and `Sync`;
`LpIpSolver::solve` takes `&mut self`, so each concurrent solve uses its own
solver instance. There are no Python callbacks: model data is copied into Rust
once, when the Python `Model` is built, and solves run with the interpreter
detached, so independent solves from several Python threads run in parallel.
Element-wise conversion of large NumPy arrays costs time at construction; a
buffer-protocol path can be added once it is measured on the real model.
