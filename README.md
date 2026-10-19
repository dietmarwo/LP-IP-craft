> **Shared agent job API 0.1:** this source tree includes the common Rust job
> protocol/runner and a native solver worker. See the [API guide](docs/job-api.md),
> [runnable tutorial](tutorials/job-api/README.md) and
> [validation report](docs/job-api/VALIDATION.md); the [review](docs/job-api/REVIEW.md)
> records the corrections made before the merge. The shared crates (0.1.1) are
> identical in fcmaes-rust, LP-IP-craft and SQP-craft. Existing solver APIs remain available.

> Native 0.28: explicit 1–8 numerical workers, reusable local Rayon resources, parallel dense-LU/pricing and packing-root phases, improved ordered batch/component/tree/residual orchestration, bounded mixed batches and a fail-closed scaling/oracle harness. See [30 iteration plans](docs/parallel-028/PLAN.md), [execution and numerical contract](docs/parallel-028/VALIDITY.md) and [reproduction](docs/parallel-028/REPRODUCE.md). Default numerical width stays 1; current MIP presolve/coefficient-tightening defaults stay on. See [measured gains and regressions](docs/parallel-028/RESULTS.md) and [the review's corrections](docs/parallel-028/REVIEW.md); raw evidence is included with the delivery archive.

> Default change after 0.27: MIP presolve (bound propagation, probing on binaries, redundant rows) and binary coefficient tightening now run by default. Both keep every feasible point. Dual fixing, cuts and root heuristics stay opt-in. Pass `mip_presolve=False, coefficient_tightening=False` for the previous behaviour. Evidence: [default change](docs/mip-iterations/ITERATIONS.md#default-change).

> Native 0.27: opt-in MIP presolve, exact GMI/MIR root cuts, checked root heuristics and branching experiments, informed by ripsolve. See the [reviewed plan](docs/ripsolve-027/PLAN.md), [API](docs/ripsolve-027/IMPLEMENTATION.md), [validity arguments](docs/ripsolve-027/VALIDITY.md), [results](docs/ripsolve-027/RESULTS.md), [reproduction](docs/ripsolve-027/REPRODUCE.md) and [review changes](docs/ripsolve-027/REVIEW.md). Generic search defaults remain unchanged.

> Native 0.26: compatible repair merging, successive rounds, rejected-transfer/root-price seeds, local residual clique cuts and separate repair/proof budgets. See [plan](docs/spoc4-026/PLAN.md), [API](docs/spoc4-026/IMPLEMENTATION.md), [results](docs/spoc4-026/RESULTS.md), [reproduction](docs/spoc4-026/REPRODUCE.md) and [review changes](docs/spoc4-026/REVIEW.md), which cover 0.25 too.

> Native 0.25: opt-in packing primal-dual root bounds, early weighted incumbents, parallel residual neighborhoods, cross-resource triangle/odd-cycle cuts, work-budgeted scalable branching and disjoint-frontier parallel B&B. See [the plan](docs/spoc4-025/PLAN.md), [implementation/API](docs/spoc4-025/IMPLEMENTATION.md), [results](docs/spoc4-025/RESULTS.md) and [reproduction](docs/spoc4-025/REPRODUCE.md). Existing default search modes remain unchanged.

> Native 0.24: bounded exact rational incumbent witnesses, reliability branching, serial Feasibility Jump, LP-directed lifted covers, and reversible mixed-row node propagation. See [the implementation plan](docs/mip-search-024/PLAN.md), [results](docs/mip-search-024/RESULTS.md), [reproduction instructions](docs/mip-search-024/REPRODUCE.md), and [review changes](docs/mip-search-024/REVIEW.md). Raw benchmark data ships in the delivery archive ([evidence manifest](docs/EVIDENCE.md)).

> Native 0.23: reuse FT output support and captured spike seeds to remove redundant scans; add opt-in bounded MIP search diagnostics and six additional public MIPs. See [the plan](docs/packing-ftran-mip-diagnostics/PLAN.md), [diagnostic contract](docs/packing-ftran-mip-diagnostics/DIAGNOSTICS.md) and [results](docs/packing-ftran-mip-diagnostics/RESULTS.md).

> Native 0.22: sparse-support propagation, broader guarded FT updates, selectable replay/Devex/column pricing, reversible singleton presolve, sound MIP root bounds, exact root cover cuts and ordered Rust/Python batch solving. See [the implementation plan](docs/highs-lessons-implementation/PLAN.md) and [results](docs/highs-lessons-implementation/RESULTS.md).

> Native 0.20: faster packing basis arithmetic and exact block pricing, incremental integer propagation, certified reduced-cost fixing, and broader public/seeded coverage. See [the staged plan](docs/packing-mip/PLAN.md) and [full results](docs/packing-mip/RESULTS.md), including remaining failures and regressions.

> Native 0.19: captured-basis replay, compact indexed elimination, retained blocked dense LU tails, bounded minimum-degree ordering and work-based rebuilds. See [the five plans](docs/irregular-lu/STEPS.md) and [results and validation](docs/irregular-lu/RESULTS.md). The production solver remains pure Rust with no HiGHS or BLAS dependency.

> Native 0.18: measured kernel profiling, Forrest–Tomlin basis updates with checked fallback, and indexed sparse pricing. See [the plan and implementation](docs/basis-updates/PLAN.md) and [benchmarks and validation](docs/basis-updates/RESULTS.md). Choose `basis_update="auto"` (default), `"eta"`, or `"forrest_tomlin"` in Python; Rust uses `BasisUpdate`. The production solver remains pure Rust without HiGHS.

> Native 0.17 performance update: ten measured iterations add integral row strengthening, short-row integer propagation, a direct box-LP path, reachability-based sparse triangular solves, sparse eta work and adaptive refactorization. See [the iteration plans](docs/performance-ten/ITERATIONS.md) and [the complete results](docs/performance-ten/RESULTS.md). The core and native solver remain dependency-free Rust.

# LP-IP-craft — native Rust LP/MIP solver

Version **0.28.0** extends the 0.16 architecture, which replaced both 0.15
linear kernels (dense tableau and the sparse revised-simplex pilot) with one **bounded-variable dual revised
simplex on a sparse LU factorization**, and replaces depth-first search with
best-bound search, diving and pseudocost branching. The optimization engine
is owned by this project: the core remains dependency-free; the native scheduler uses pinned pure-Rust Rayon and its dependencies, and no HiGHS, C/C++ solver, CMake, libclang, BLAS or external
numerical library is built or linked. The optional Python extension uses PyO3.

What changed, and why it is faster (details in [docs/simplex](docs/simplex/README.md)):

- **Bounds stay bounds.** Variable bounds are no longer explicit rows and
  branching no longer appends rows: a branch changes a bound in place, and the
  child reoptimizes from the factorized parent basis with the dual simplex.
- **Factorization.** Markowitz LU with threshold pivoting and a dense tail,
  adaptive Forrest–Tomlin / eta updates, and sparse triangular solves in both directions.
- **Dual simplex.** Dual steepest edge, bound-flipping ratio test with Harris
  tolerances, power-of-two scaling, auxiliary dual phase 1 and cost
  perturbation against degeneracy.
- **Search.** Best-bound node selection with diving, pseudocost branching,
  and an incumbent-certification LP that lets conservative pruning work on
  mixed models, where the delivered engine could almost never prune.

Every result is still checked independently: LP optima in original units,
warm infeasibility claims by a Farkas re-check, incumbents by the core
evaluator, bounds by the outward-rounded `safe_dual_bound`. Measured against
the delivered 0.15 build ([docs/simplex](docs/simplex/README.md#performance)):
the LP benchmark families run 2.6–30× faster than the better 0.15 kernel and
solve network-2048, which neither 0.15 kernel could; the small
selection MIPs match dense 0.15 and the warm-start regression of the 0.15
sparse pilot is gone (selection-20: 1,505 → 93 nodes, 51× faster). On
randomized models checked against SciPy, the new engine agrees on all 5,500
cases, where 0.15 failed on up to 2 of 550 per seed, and its MIP search needs
20–29× fewer nodes and 13–27× less time than dense 0.15.

MIPs still require finite bounds on **all** variables, including continuous
variables, of magnitude at most 2^53-1 after justified root inference; unresolved
infinities remain unsupported. Numerical kernels accept a maximum of 1–8 workers through `Options.threads`; small work stays scalar. Independent components, residual neighborhoods and binary frontiers keep their existing worker controls. Explicit numerical width > 1 serializes these outer mechanisms. Ordered batches also support outer × inner ≤ 8; the existing 16-worker serial-inner API remains compatible. The two real SpOC4 matching tables are now application benchmarks; their current quality/bound progress and remaining root bottleneck are documented in [the 0.26 results](docs/spoc4-026/RESULTS.md). GTOC application readiness remains unverified.

## Build and use

Rust 1.85 or later. Plain `cargo build` builds only the two Rust numerical
crates; Python is an optional workspace member. Rust-only checks need no
Python installation:

```sh
cargo test -p lp-ip-craft -p lp-ip-craft-native --locked
cargo run --release -p lp-ip-craft-native --example solve --locked
cargo run --release -p lp-ip-craft-native --example benchmark --locked -- 7
cargo run --release -p lp-ip-craft-native --example family_benchmark --locked -- packing 4096
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
a point and reports `stub`. Select `native` to optimize. `NativeOptions`
lists only controls the engine honours; 0.16 removed the kernel-selection
fields of 0.15 (`sparse`, `max_tableau_entries`, `fast_pivots`,
`dantzig_pricing`) and renamed the factorization controls to
`max_factor_entries` and `refactor_interval`. Unknown names are rejected.

## Numerical guarantees and limits

LP `optimal` requires independent primal feasibility, stationarity, dual-sign
and relative duality-gap checks in original units. MIP incumbents pass the
independent primal/integrality check. All solve statuses are numerical,
including infeasibility; this is not an exact search-tree certificate.

The outward-rounded `safe_dual_bound` is retained. Returned bounds use this
calculation when available; LP bounds can be absent with infinite variable
bounds. Pruning requires an original-model incumbent whose constraint
activities and objective are enclosed by outward-rounded arithmetic; the
certification LP makes such incumbents available on most mixed models, but
equality rows with continuous variables can still prevent it, and search then
continues without that pruning opportunity. Integer-lattice tightening has
explicit sufficient conditions. Neither mechanism certifies infeasibility or
the whole branch-and-bound tree. Standard IEEE binary64 with gradual
underflow is required; fast-math is unsupported.

Starts are complete primal points. For a MIP they are evaluated as incumbent
candidates; LP starts are explicitly rejected because public basis import is
not implemented. Time/cancellation limits are cooperative and retain an
incumbent when present. Node limits count relaxations; iteration limits apply
across all nodes, phase 1, certification LPs and retries. Parallel components
reserve shares of the global node/iteration limits; unused shares are not
redistributed. Serial component execution shares the remaining budget
instead. Bases with heavy LU fill (random sparse structure) remain
solve-bound; hypersparse solves and Forrest–Tomlin updates are the next steps.

See [API](docs/api.md), [the dual simplex design, validation and
benchmarks](docs/simplex/README.md) and the still-incomplete [application
target card](docs/target-card.md). Earlier native iterations are under
[docs/native](docs/native) and the superseded 0.15 sparse pilot under
[docs/sparse](docs/sparse); adapter records are under [docs/history](docs/history).

## Layout

| Path | Purpose |
|---|---|
| `crates/core` | Immutable CSR model, contracts and independent checks |
| `crates/native` | Dual simplex (`lp.rs`), sparse LU (`lu.rs`), presolve, components, integer search |
| `crates/python`, `python/lp_ip_craft` | Optional PyO3/Python interface |
| `tests`, `crates/*/tests` | Contract, numeric, exact-oracle and engine tests |
| `examples/validate_native.py` | Optional SciPy/exact oracle on small models; not a package dependency |
| `examples/validate_random.py` | Optional randomized LP/MIP differential check against SciPy |
| `scripts/benchmark_families.py` | Isolated-process family benchmark, optionally against a 0.15 baseline |
| `docs/simplex` | Current kernel and search design, validation and measurements |
| `docs/native`, `docs/sparse` | Earlier plans, results and raw measurements |
| [`ai-context.md`](ai-context.md) | Operational context for AI agents: modelling, options, results, parallel layouts, job API |

Project code is MIT. See [dependencies](docs/dependencies.md) and `Cargo.lock`.

### Packing and MIP work (0.20)

The [staged plan](docs/packing-mip/PLAN.md) and
[complete results](docs/packing-mip/RESULTS.md) cover basis-solve arithmetic,
exact block pricing, incremental integer propagation and certified reduced-cost
fixing, with public and seeded coverage cases. Production remains owned Rust.
`NativeOptions.reduced_cost_fixing` defaults to true and can be disabled for
comparison. It fixes only integer bound values whose opposite domain is proved
unable to improve a certified incumbent by the conservative core dual-bound
calculation. `strong_branching` defaults to false: optional bounded root probes
can guide branching, and all probe nodes/pivots consume the global limits.
Public finite-bound restrictions and known numerical failures are documented
alongside the measured gains and regressions.

## Independent-job batches (0.22)

```python
from lp_ip_craft import NativeOptions, solve_batch
results = solve_batch(models, workers=4,
                      backend_options=NativeOptions(time_limit_seconds=30.0))
for result in results:
    if isinstance(result, Exception):
        print(result)  # one failed job does not discard the others
    else:
        print(result.status, result.evaluation)
```

`models` is an iterable of immutable `Model` objects. Results keep input order;
limits apply per job. `workers` is 1–16; each worker owns its numerical state,
and component threading is disabled inside the batch. The interpreter is
released. A shared `Cancellation` stops running and queued jobs at cooperative
checkpoints. There are no warm starts in this batch API. Rust callers use
`NativeSolver::solve_batch(&[&Model], &Options, workers, &CancellationToken)`.

New native controls are `singleton_presolve`, `root_propagation`, `root_cuts`,
`factor_replay`, `adaptive_pricing`, `devex`, and `avoid_dominant_components`.
Replay, approximate Devex weights and root cover cuts default to false. Root
inference can make an initially unbounded MIP box admissible when constraints
prove finite bounds; unresolved infinities and magnitudes beyond 2^53-1 still
return `Unsupported`. Presolve reconstructs singleton LP duals and checks them
against the original model. Root cuts are restricted to verified positive
integer binary covers, inserted before factorization; no dynamic local cuts.

## Eight-worker execution (0.28)

For independent small models use `solve_batch(models, workers=8)` with default `Options(threads=1)`. For a large single LP or packing root use `Options(threads=8)`; scalar reductions, sparse pivot selection, triangular dependencies and independent acceptance remain ordered. To use existing packing residual workers choose numerical width 1 and `NativeOptions(packing_workers=8, ...)`. Do not multiply eight outer workers by eight numerical workers. Mixed batches support 1×8, 2×4, 4×2 and 8×1, with per-job limits and input-order results.

The native implementation remains owned safe Rust; Rayon 1.12 supplies scheduling, not an external LP/MIP algorithm or BLAS. Local pools are owned, compatible live widths share weakly indexed resources, and no global Rayon pool is configured. Separate callers must manage their combined budgets. See the detailed execution contract and benchmark scopes before interpreting 1/8 ratios.
