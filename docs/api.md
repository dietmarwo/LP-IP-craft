# Native API, version 0.15.0

## Model and facade

The core CSR model and result types are preserved. A model minimizes or
maximizes `objective_offset + c^T x`, subject to row and variable intervals,
with continuous, integer or binary variable kinds. Sparse row indices must be
sorted and unique. Explicit zeros are allowed. The model is immutable.

Rust: `NativeSolver::new(NativeOptions).solve(&model, &Options, start, &cancel)`.
Python: `solve(model, backend="native", backend_options=NativeOptions(),
options=Options(), start=None, cancellation=None)`. The facade's default stays
`dummy`. The Python GIL is released during numerical work. Solvers do not
call Python during optimization.

## Controls

| NativeOptions field | Default | Meaning |
|---|---:|---|
| time_limit_seconds | None | Whole-call cooperative budget after validation |
| node_limit | 100000 | MIP relaxations processed; None removes the limit |
| iteration_limit | 100000 | Pivots across phase I/II, all nodes and retries |
| pivot_tolerance | 1e-10 | Scaled pricing and pivot eligibility threshold |
| dual_tolerance | 1e-7 | Independent LP dual acceptance threshold |
| sparse | false | Select sparse revised simplex; dense remains the reference default |
| max_sparse_entries | 2000000 | Tracked sparse matrix/factor/update entries and refactor overlap estimate; not an RSS limit |
| sparse_refactor_interval | 64 | Maximum column/border updates before LU rebuild |
| max_tableau_entries | 8000000 | Cap on one dense dictionary, not whole-process memory |
| presolve | true | Eliminate fixed columns/empty rows and enable conservative row presolve |
| redundant_rows | true | Omit rows conservatively implied by variable bounds |
| redundant_row_min_ratio | 1.0 | Scan redundancy only if m/n >= this ratio; zero forces scanning |
| fast_pivots | true | Dense only: in-place update; false selects reference copying kernel |
| dantzig_pricing | true | Most-negative pricing; Bland after 32 stalled pivots |
| node_reoptimization | true | Parent dictionary reuse and child dual simplex |
| incumbent_heuristic | true | Root/uncertified-incumbent rounding and at most 64 unit moves per candidate |
| objective_lattice | true | Integral-objective tightening when sufficient conditions hold |
| decompose | true | Discover independent restricted-row components |
| decomposition_min_variables | 32 | Skip graph discovery below this size |
| component_threads | 16 | Maximum component workers, in 1..=16 |
| component_parallel_min_entries | 4096 | Keep smaller average estimated component dictionaries serial; zero forces workers |

`Options` keeps original-unit feasibility/integrality tolerances (1e-7 each)
and `threads=1` for each numerical kernel; other values are unsupported.
`NativeOptions.component_threads` controls the independent-component workers.
Independent solves can also use external threads/processes; choose a component
worker cap appropriate to the total CPU and memory budget. Node controls have no effect on an LP; the
node count remains zero. A primal start is evaluated only for a MIP; an LP
start has `start_status="rejected"`.

## Algorithms and postsolve

Finite-lower variables use `x=l+y`; upper-only use `x=u-y`; free variables
split into two nonnegative variables. Finite upper and row bounds become
inequalities. Rows and objective are scaled; postsolve reverses scaling and
sense. Exactly fixed variables are substituted and reconstructed. Conversion
streams nonzero CSR entries into sparse columns or a single dense dictionary
allocation, according to the kernel selection. Both reuse sparse row scratch. Node solves borrow the original matrix/objective.
The additional Rust APIs `check::lp_residuals_with_bounds` and
`safe_dual_bound_with_bounds` accept replacement variable bounds; their results
refer to that modified subproblem, not automatically to the whole original
feasible set. Existing API calls continue to use the original model bounds.

Row presolve encloses each original row activity over the variable box and
omits the row only if the full enclosure satisfies its bounds. An unsupported
floating-point environment or unavailable enclosure prevents that reduction.
The m/n threshold was added after measuring scan overhead on row-light models.

Two-phase primal simplex establishes a feasible basis and optimizes. Child
nodes can insert a bound row into a parent dictionary and reoptimize with
dual simplex. An unacceptable warm result or numerical/infeasibility outcome
gets at most one cold solve. Limits never retry. Branching is deterministic
on the most fractional integral variable. If rounding near an integer cannot
be certified feasible, a smaller fractional split is tried when both children
strictly restrict the integer domain.

In original objective sense, duals satisfy `c=A^T row_duals+reduced_costs`.
For minimization, positive multipliers select lower bounds; negative select
upper bounds. Maximization reverses these signs. MIP results do not expose
LP duals. Independent primal and LP residual checks remain callable.

## Component execution and budgets

Restricted rows connect their nonzero columns; explicit zeros and free rows
do not connect components. All-singleton partitions and models below the size
threshold stay on the ordinary path. Fixed-column or redundant cross-row
structure may hide separability; graph detection uses the original rows.
Each component gets a native solver with decomposition disabled. Original
variable/row order, objective offset and LP multipliers are reconstructed and
global feasibility/KKT checks run again. Unfinished components never produce a
partial candidate vector. Incomplete MIP solves retain a feasible original
start when available; partial component improvements are not combined with it.

Serial execution spends the remaining global node/pivot budgets. Parallel
execution reserves deterministic shares across all k components: each gets
floor(B/k), with one extra unit for the first B mod k components. Unused shares
are not redistributed. Consequently a parallel solve can hit a component quota
before an equivalent serial solve exhausts the full budget. Both respect the
global cap. Set component_threads=1 for the sequential policy.

All workers use the same whole-call deadline and external cancellation flag.
Work dispatch stops after an error/limit; already running solves finish at
their cooperative checkpoints. Dictionaries and numerical state stay private;
only dispatch and completed results cross workers. Results are assembled in
original component order. The maximum dictionary size is per component, not a
global memory cap; worker count also controls simultaneous memory use.

`total_seconds` is wall time. Component conversion/solve/check phase times are
sums of worker durations and may exceed wall time, identified by
`component_timing_mode=worker_duration_sums`. `component_budget_mode` records
`sequential_shared` or `parallel_reserved`. Diagnostics report actual launched
workers and component count with unique keys in both Rust and Python.

## Results and conservative bounds

`status` is a numerical termination reason, independent of candidate quality.
`candidate` and `evaluation` appear together. An LP optimum requires primal
violation <= feasibility tolerance; stationarity and dual signs <=
`dual_tolerance*(1+max(abs(c)))`; normalized primal-dual gap <= dual_tolerance.
A root LP infeasibility contradicting an accepted MIP start reports numerical_failure.
Infeasibility and unboundedness are phase-I/II numerical findings, not exact
certificates. An unbounded MIP relaxation returns `unknown`, not integer
unboundedness. Limits retain the incumbent, when available.

`best_bound`, when available, uses the existing outward-rounded Lagrangian
bound. Surviving/closed branch bounds are retained when aggregating a MIP
bound. A `None` bound is never replaced by the LP objective. Numerical pruning
of infeasible LP nodes means this is still not an exact tree certificate.
`gap` retains the core definition: oriented candidate-minus-bound divided by
absolute candidate objective, with its sign preserved.

Conservative incumbent pruning additionally encloses original row activities
and objective arithmetic. Tolerance-only candidates cannot justify it.
The best objective candidate and the conservative pruning incumbent are kept
separately. If a tolerance-only candidate contradicts a conservative bound,
an available certified candidate is substituted or numerical failure reported. Integral-objective tightening
requires: all variables integral; integral objective coefficients; zero
offset; finite box; and a magnitude bound leaving substantial space below
2^53. Mixed-continuous or decimal objectives do not take that path.

`BackendReport.effective_options` contains all controls and `diagnostics.*`
counters (cold LPs, reoptimized nodes, cold fallbacks, heuristic candidates,
pruned nodes and whether lattice tightening was enabled). Counters are
diagnostics, not input options. Timings are cooperative wall-clock phase
measurements; some retry/acceptance work belongs to the solve phase.

## Deliberate limits

Dense memory/time costs; no sparse LU, cuts, strong branching, basis import,
parallel search within a connected tree, advanced presolve or MIP relative-gap stop. MIPs require
finite variable bounds of magnitude <=2^53-1, including continuous columns.
No application-performance claim until the target card is supplied. See the
ten iteration plans and measured results in `docs/native`.

## Sparse numerical path (0.15)

Sparse revised simplex retains structural and identity slack columns, one
phase-I artificial column, a basis index vector, row-pivoted sparse LU and
sparse product-form updates. Forward and transpose solves use dense work
vectors of row dimension; there is no dense matrix or inverse. Full pricing
scans sparse columns. Harris two-pass ratios prefer stable pivots; repeated
zero-length steps switch to Bland pricing/ratios. Basis residual checks with
one refinement pass precede decisions, and observed lost primal accuracy can
trigger one fresh factorization. Final original-model checks are unchanged.

An appended branch bound uses a sparse bordered-row inverse update, preserving
the parent's LU and update chain. Dual revised simplex restores feasibility.
Existing independently checked cold retries remain sparse and cover rejected,
numerically failed or numerically infeasible warm solves. Public LP basis
import/export and objective/RHS-update sessions are not implemented.

`max_tableau_entries` and `fast_pivots` apply only to the dense path; sparse
entry/refactor controls apply only to the sparse path. Sparse fill exhaustion
during factorization returns `numerical_failure`; conversion/branch admission
can return an unsupported-resource error (a branch admission failure may take
the existing cold sparse retry). The cap is an engineering guard, not a hard
whole-process RAM guarantee. See `docs/sparse` for the measurement boundary and
remaining numerical/performance limits.
