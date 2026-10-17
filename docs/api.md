# Native API, version 0.16.0

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
| iteration_limit | 1000000 | Simplex iterations across phase 1/2, all nodes, certification LPs and retries |
| pivot_tolerance | 1e-9 | Smallest pivot-row entry accepted by the scaled dual ratio test |
| dual_tolerance | 1e-7 | Independent LP dual acceptance threshold |
| max_factor_entries | 4000000 | Stored LU plus eta entries of one basis factorization; a fresh factorization above it is `Unsupported` |
| refactor_interval | 64 | Basis updates (eta columns) before the LU is rebuilt |
| steepest_edge | true | Dual steepest-edge pricing; false prices by largest infeasibility |
| presolve | true | Substitute root-fixed columns; drop free rows and feasible empty rows |
| redundant_rows | true | Also drop rows conservatively implied by the root variable box |
| redundant_row_min_ratio | 1.0 | Scan redundancy only if m/n >= this ratio; zero forces scanning |
| node_reoptimization | true | Start children from the parent basis; false starts every node from the slack basis |
| incumbent_heuristic | true | Root/uncertified-incumbent rounding and at most 64 unit moves per candidate |
| objective_lattice | true | Integral-objective tightening when sufficient conditions hold |
| decompose | true | Discover independent restricted-row components |
| decomposition_min_variables | 32 | Skip graph discovery below this size |
| component_threads | 16 | Maximum component workers, in 1..=16 |
| component_parallel_min_entries | 2048 | Solve components serially unless their average size (nonzeros + rows + columns) reaches this; zero forces workers |

Version 0.16 removed `sparse`, `max_tableau_entries`, `fast_pivots` and
`dantzig_pricing` together with the two kernels they selected, and renamed
`max_sparse_entries` / `sparse_refactor_interval` to `max_factor_entries` /
`refactor_interval`. Unknown option names are rejected, so stale settings
fail loudly instead of being ignored.

`Options` keeps original-unit feasibility/integrality tolerances (1e-7 each)
and `threads=1` for each numerical kernel; other values are unsupported.
`NativeOptions.component_threads` controls the independent-component workers.
Independent solves can also use external threads/processes; choose a component
worker cap appropriate to the total CPU and memory budget. Node controls have
no effect on an LP; the node count remains zero. A primal start is evaluated
only for a MIP; an LP start has `start_status="rejected"`.

## LP kernel

One kernel solves every LP and node relaxation: a bounded-variable dual
revised simplex on a sparse LU factorization. The design and its validation
are in [simplex/README.md](simplex/README.md); in brief:

- **Internal form.** `A x − r = 0` with bounds on the structural columns `x`
  and on one logical `r_i` per kept row (basis column `−e_i`). Variable bounds
  are never rows, so branching changes a bound in place and keeps the basis.
- **Scaling.** Geometric row/column scaling with column equilibration,
  rounded to powers of two (skipped when coefficients already lie in
  [1/32, 32]). Bounds and nonbasic values stay exact; primal and dual
  tolerances are set per variable so that the targets hold in original units.
- **Factorization.** Markowitz pivot search with threshold 0.1 over row and
  column count buckets, switching to dense partial-pivoting elimination once
  the active submatrix reaches 30% density. L and U are stored by pivot and
  transposed, so both triangular solves skip zeros. Column replacements are
  product-form (eta) updates; a singular basis is repaired by logicals.
- **Iteration.** Dual steepest-edge pricing, the tableau row from
  `ρ = B⁻ᵀ e_r` with a row-wise matrix copy, a bound-flipping ratio test with
  Harris tolerances, and a row/column consistency check that forces a
  refactorization. Dual degeneracy switches on a temporary cost perturbation.
- **Phase 1.** Columns without a finite box that start dual infeasible are
  handled by an auxiliary problem whose bounds are `{0}`, `[0,1]`, `[−1,0]`
  or `[−1,1]`. If it ends dual infeasible, a zero-cost solve decides between
  `unbounded` (primal feasible) and `infeasible`.
- **Optimality.** Declared only after the perturbation is removed and the
  primal and dual values are recomputed from scratch; cold solves also
  refactorize first. Basic logical rows get exactly zero price, and a row
  price whose sign would select an infinite row bound (within the dual
  tolerance at an optimum) is reported as zero.

Duals map back as `y = σ C R π` (sense `σ`, cost scale `C`, row scale `R`);
reduced costs are recomputed in original units as `c − Aᵀ y`. In original
objective sense, duals satisfy `c=A^T row_duals+reduced_costs`. For
minimization, positive multipliers select lower bounds; negative select upper
bounds. Maximization reverses these signs. MIP results do not expose LP
duals. Independent primal and LP residual checks remain callable.

The additional Rust APIs `check::lp_residuals_with_bounds` and
`safe_dual_bound_with_bounds` accept replacement variable bounds; their
results refer to that modified subproblem, not automatically to the whole
original feasible set. Row presolve encloses each original row activity over
the root variable box and omits the row only if the full enclosure satisfies
its bounds; a row redundant over the root box stays redundant at every node.

## Integer search

Branch and bound selects the open node with the best conservative bound and
dives: after branching, the child nearer to the LP value is solved next from
the unchanged factorized parent basis; the sibling is queued. Open nodes keep
their box as a shared chain of bound changes, and the parent basis as a
snapshot while snapshots stay below 256 MiB; beyond that a queued node starts
from the current basis, which is always dual feasible because every MIP
column is boxed. A snapshot that differs from the current basis in at most 16
positions (within the refactor interval) is reached by eta updates instead of
a refactorization.

The branching variable maximizes the product of pseudocost estimates (down
and up degradation per unit, learned from every solved child), with most
fractional as the rule until the first child is solved. If rounding an
integral-looking LP point cannot be certified feasible, a smaller fractional
split is tried when both children strictly restrict the integer domain.

Every warm node result is checked independently before it is used: an
optimum must pass the original-unit acceptance below, and an infeasibility
claim must pass a Farkas re-check (`ρᵀ(A x − r) = 0` bounded away from zero
over the node box, ignoring negligible coefficients). Either failure, or a
numerical failure, gets one slack-basis retry. Limits never retry.

Pruning uses only incumbents whose original-row activities and objective are
certified by outward-rounded arithmetic. When the best tolerance-feasible
incumbent does not certify (typically continuous values on tight rows), its
continuous part is re-solved once on a second LP whose finite row and
continuous column bounds are shrunk inward by sixteen primal tolerances, with
the integers fixed; that interior point usually certifies. At most 64 such
solves run per call. The reported candidate remains the best
tolerance-feasible point.

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

Serial execution spends the remaining global node/iteration budgets. Parallel
execution reserves deterministic shares across all k components: each gets
floor(B/k), with one extra unit for the first B mod k components. Unused shares
are not redistributed. Consequently a parallel solve can hit a component quota
before an equivalent serial solve exhausts the full budget. Both respect the
global cap. Set component_threads=1 for the sequential policy.

All workers use the same whole-call deadline and external cancellation flag.
Work dispatch stops after an error/limit; already running solves finish at
their cooperative checkpoints. Factorizations and numerical state stay private;
only dispatch and completed results cross workers. Results are assembled in
original component order. `max_factor_entries` applies per factorization, not
as a global memory cap; worker count also controls simultaneous memory use.

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
Infeasibility and unboundedness are numerical findings, not exact
certificates. An unbounded MIP relaxation returns `unknown`, not integer
unboundedness. Limits retain the incumbent, when available.

`best_bound`, when available, uses the existing outward-rounded Lagrangian
bound. Surviving/closed branch bounds are retained when aggregating a MIP
bound. A `None` bound is never replaced by the LP objective. Numerical pruning
of infeasible LP nodes means this is still not an exact tree certificate.
`gap` retains the core definition: oriented candidate-minus-bound divided by
absolute candidate objective, with its sign preserved.

The best objective candidate and the conservative pruning incumbent are kept
separately. If a tolerance-only candidate contradicts a conservative bound,
an available certified candidate is substituted or numerical failure
reported. Integral-objective tightening requires: all variables integral;
integral objective coefficients; zero offset; finite box; and a magnitude
bound leaving substantial space below 2^53. Mixed-continuous or decimal
objectives do not take that path.

`BackendReport.effective_options` contains all controls and `diagnostics.*`
counters: factorization (`factor_peak_entries`, `refactorizations`,
`basis_updates`, `bound_flips`, `basis_repairs`, `basis_restores`,
`cost_perturbations`), presolve (`presolve_removed_rows`,
`presolve_removed_columns`), search (`reoptimized_nodes`, `cold_lp_solves`,
`cold_fallbacks`, `unverified_infeasible_nodes`, `heuristic_candidates`,
`pruned_nodes`, `certification_solves`, `certified_incumbents`) and
`objective_lattice`. Counters are diagnostics, not input options. Timings
are cooperative wall-clock phase measurements; some retry/acceptance work
belongs to the solve phase.

## Deliberate limits

No cuts, strong branching, basis import/export, parallel search within a
connected tree, advanced presolve or MIP relative-gap stop. MIPs require
finite variable bounds of magnitude <=2^53-1, including continuous columns.
Triangular solves use dense work vectors of the row dimension (no
hypersparse solves), and product-form updates store dense eta columns when
`B⁻¹` is dense; bases with heavy LU fill are solve-bound. `max_factor_entries`
is an engineering guard, not a whole-process memory cap: matrix copies, work
vectors, the open-node queue and basis snapshots need additional memory. No
application-performance claim until the target card is supplied.
