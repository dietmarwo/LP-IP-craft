# Native 0.25 implementation and API

## Source layout

| File | Added behavior |
|---|---|
| `crates/native/src/packing.rs` | conservative unit-packing recognition, greedy seed, projected primal-dual root pass, conservative price bounds, residual construction/private workers, cross-resource triangles and short odd cycles |
| `crates/native/src/tree.rs` | exhaustive disjoint binary frontier, private subtree solves, ordered candidate merge, conservative bound aggregation and global work reservations |
| `crates/native/src/lib.rs` | explicit options/validation/metadata, interrupted-LP bounds, scalable reliability shortlist and probe work reservations, dispatch into new modes |
| `python/lp_ip_craft/__init__.py`, `crates/python/src/lib.rs` | Python option dataclass and strict parsing |
| `crates/native/examples/spoc4_profile.rs` | real-table loader and exclusive kernel profiling example |
| `benchmarks/spoc4_025.py` | serial isolated-process application measurements and independent original-resource candidate checks |
| `tests/test_spoc4_025.py` and Rust unit modules | exhaustive oracles, resource-boundary reconstruction, cut strength/validity, fallback, limits, activation above the old size gate and frontier coverage |

The numerical crates retain zero third-party dependencies. Python benchmark/oracle tools use NumPy/SciPy; they are not production solver dependencies. ESA matching tables are separate benchmark inputs with their source GPL-3.0 attribution.

## New controls

All new algorithm modes are off by default. Default kernel/search decisions from 0.24 are preserved. Existing reliability branching and component controls retain their meaning.

| Control | Default | Meaning / range |
|---|---:|---|
| `packing_root` | false | enable recognized-packing early incumbent and factorization-free root pass |
| `packing_root_iterations` | 1000 | first-order step cap, 0–100000 |
| `packing_root_work` | 200000000 | cap on the root pass's matrix/vector work reservation, at most 10000000000 |
| `packing_neighborhoods` | 0 | residual task count, at most 4096; also enables the packing driver |
| `packing_neighborhood_size` | 192 | maximum residual columns, 4–1024 |
| `packing_workers` | 1 | private residual workers, 1–16; actual count is bounded by batch/task count |
| `packing_neighborhood_nodes` | 128 | per-task node ceiling, 0–100000; also limited by parent shares |
| `packing_cuts` | false | bounded LP-hint-directed triangle/odd-cycle separation; use with `packing_root` to obtain a fractional hint |
| `packing_cut_work` | 2000000 | adjacency/search edge-visit cap, at most 100000000 |
| `partial_dual_bounds` | false | conservative bound from finite current prices on a limited nonoptimal simplex solve; enabled internally by the packing driver |
| `scalable_branching` | false | bounded reliability probing without the old whole-model dimension gate |
| `branch_probe_candidates` | 4 | shortlist capacity, 1–16 |
| `branch_probe_pivots` | 32 | maximum reserved pivots per directional probe, 0–1024 |
| `branch_probe_work` | 2000000 | cumulative matrix-size probe-work reservation per serial search, at most 10000000000 |
| `tree_threads` | 1 | private subtree workers, 1–16 |
| `tree_split_depth` | 2 | binary frontier depth, 0–8; depth 0 disables splitting |

`packing_root_work` reserves `2*nnz+4*n+4*m` units per primal-dual step. Bound checkpoints and setup add separately timed work; this is a bounded iteration-size proxy rather than an instruction count. The core bound evaluation is linear in matrix size. `branch_probe_work` reserves `(nnz+n+m)*reserved_probe_pivots` per direction, before entering the probe; it does not claim to bound sparse-LU fill or CPU instructions. Probes also obey global pivot/time/factor guards. Residual and private-subtree solver invocations each have their own probe reservation cap inside their assigned parent node/pivot shares; this control is not a global instruction-count cap across all workers.

Packing recognition requires maximization, nonnegative finite weights, integral columns with exact 0/1 bound endpoints, and nonnegative unit-coefficient row activities bounded above by 1 (row lower bound at most 0). Fixed 0/1 columns are supported. Ineligible models use the general solver. Tree splitting supports finite binary domains of either sense; mixed/nonbinary models use serial search. Requested settings remain visible in effective metadata even when the model is ineligible.

## Root relaxation

For normalized objective `c`, the root pass applies box-projected primal steps and nonnegative dual-price steps to the unit-packing saddle problem. It uses step `0.9/sqrt(max_row_degree*max_column_degree)`, relying on the sparse matrix norm bound `||A||_2^2 <= ||A||_1 ||A||_infinity`. It maintains averaged iterates and periodically tests both current and averaged prices. Fractional hints guide rounding, cut discovery and frontier choices; they are never returned as integer incumbents.

Every reported price bound goes through the existing outward-rounded core calculation on the actual model and variable box. With `r=c-A^T y`, the box-bound residual handles imperfect dual feasibility. The best checked price bound survives a time/pivot limit. Integer-lattice rounding uses the existing sufficient conditions and the caller's lattice option. This first-order accelerator does not report an optimal LP; exact LP status still comes from the checked simplex solver.

A graph-weighted greedy seed is available before the first large factorization. Every new heuristic candidate is checked in original coordinates and conservatively admitted by the incumbent helper. A better valid user start is retained. The first price cover assigns each column to one incident resource and raises that resource's price to its maximum assigned weight; columns without resources use the residual upper-bound term.

## Residual neighborhoods

A deterministic task center selects incumbent transfers, then expands through resource conflicts to release a bounded region. Every unreleased selected transfer remains fixed outside. An alternative transfer is eligible only if none of its resource rows is occupied by the fixed outside matching. Released incumbent columns are always included, so the existing matching is a feasible residual start.

Residuals are compact native models, not full-model clones. Each worker owns its matrix/domain/basis/search state. Tasks in one fixed-width batch of sixteen use the same incumbent snapshot. Results reconstruct complete original vectors and merge in task order through the original-model checker. This provides scheduling-independent output at fixed work when time/cancellation does not truncate a batch. Timed results can differ with worker count.

Task nodes and pivots consume the parent budgets; all task limits are explicit. Per-task pivot shares are additionally capped at 4096. Ordinary solver errors in heuristic jobs are counted and their reserved work is charged conservatively. No worker runs a nested component, residual or tree pool. The existing independent-job batch and component APIs also force residual/tree worker counts to one inside their workers. Complete-solution exchange happens at batch boundaries.

## Cuts

The separator caches only bounded implicit conflict adjacency. It first scans triangles across high-value hint vertices, then searches chordless cycles of length 3, 5 and 7 with a fractional edge-cost pruning rule, independently rechecks consecutive conflicts, rejects duplicate vertex sets and resource-clique-dominated candidates, and inserts at most 64 violated inequalities. For an odd cycle C, `sum(x[C]) <= floor(|C|/2)` is valid because consecutive vertices cannot both be selected. Triangles are the length-three case.

Cuts are inserted before simplex caches are built, and can be used with a primal start. There is no live cut-pool sharing or dynamic node cut insertion. Columns incident to more than 64 resource rows disable this bounded separator, while the other packing mechanisms remain available. Final candidates are checked on the original model, not only the strengthened one.

## Branching

Scalable reliability mode scans fractional unreliable candidates into a bounded shortlist using fractionality and resource-size scores. It reuses the parent basis, reserves probe work before each direction, and charges all actual probe nodes/pivots to global budgets. Limited/failed probes do not justify pruning. Bounds and basis are restored after every executed probe. The legacy mode and its original dimension gates remain unchanged when scalable mode is off.

## Parallel tree search

The first release uses a coarse exhaustive frontier, not dynamic shared-queue B&B. Fixing k selected free binary variables produces `2^k` disjoint boxes whose union covers the original binary domain. Actual workers are `min(tree_threads, 2^k)`; use depth 4 to expose sixteen subtrees when enough free variables and budget remain.

Each worker builds one private model at a time and runs the native warm-reoptimizing B&B within that subtree. Node/pivot shares are reserved deterministically by frontier index, and unused shares are not redistributed. Whole-call remaining time and the shared cancellation token apply. Memory is proportional to active workers' private models/factors, rather than the number of frontier boxes times the full model.

For maximization, the aggregate upper bound is the maximum over unfinished/feasible subtree upper bounds, optionally capped by a valid inherited global bound. Minimization reverses the senses. An unknown subtree with no valid inherited bound keeps the aggregate unavailable. Numerical infeasible subtrees are handled under the existing numerical-search contract. Solver errors retain checked candidates and produce a numerical-failure status; they do not disappear from bound coverage.

There is no cross-worker basis import, shared cut learning or live incumbent broadcasting in this implementation. It should not be assumed to accelerate a large unfinished root; the application measurements explicitly compare that cost.

## Diagnostics and scope

New root, greedy, residual and separation phase timings are `diagnostics.packing_*_seconds`. Other keys report first-order iterations/work, root bound, greedy/neighborhood objective, residual tasks/improvements/errors/workers/nodes/pivots, triangle/odd-cycle counts, cut work, probe reservations/limit exits and tree frontier/worker/closure/error counts. Existing `diagnostics.mip_search` is the final serial child's search trace; preprocessing/residual/tree totals are reported separately. Top-level `nodes` and `iterations` include residual work.

Build with `kernel-profile` for exclusive pricing, solve, factorization and update timings through the existing thread-local profiler. Worker kernel timings are not summed into the parent's snapshot. Coarse phase timings and total elapsed wall time remain available in the normal build. All numerical statuses retain the project's existing guarantees; no complete exact search-tree certificate is introduced.
