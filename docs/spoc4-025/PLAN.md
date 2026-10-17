# Native 0.25: SpOC4 implementation plan

Baseline: `9bf392c1f0f1cbf3242a31d0b7d8edd0b3352149` (0.24.0 including review fixes). Implement every stage below in owned dependency-free Rust, with Python controls, tests, reproducible application measurements and a source ZIP. Keep historical 0.24 records unchanged.

## Contract and scope

The two original ESA tables are pure binary unit-coefficient set packing, with positive objective weights encoded as integer thousandths. Each transfer touches three resource rows. Work against that application contract without claiming to implement a full industrial IPM, general mixed-integer parallel presolve, or exact whole-tree proof logging.

New modes are opt-in. A conservative recognizer enables packing algorithms only for maximization, binary [0,1] columns, nonnegative finite weights, and upper resource rows with coefficients 1 and RHS 1. Unsupported structure falls back to the general solver. Fixed binary bounds are supported in residuals/tree subproblems. Retain independent original-model checks, safe dual bounds, cancellation, finite-domain admission, node/pivot limits and stable numerical statuses.

The existing early weighted matching construction is included as necessary support: root and parallel neighborhood work must retain an incumbent before the first large simplex LP completes. This support is not a sixth unrelated workstream.

## Stage 1 — root-LP and root-bound improvements

1. Add a shared implicit incidence representation, checked model recognizer and deterministic objective-weighted greedy seed. Respect fixed columns; never overwrite a better valid user start.
2. Add a factorization-free primal-dual hybrid-gradient relaxation pass for packing. Normalize objective weights; choose step sizes from a conservative row/column degree bound on the matrix norm. Project primal variables onto their box and dual resource prices onto nonnegative values.
3. Bound iterations and matrix-entry work, and poll cancellation/deadline. Produce a fractional hint for heuristics/cut selection and resource prices for a valid bound. Do not label an approximate point an optimal LP or an integer incumbent.
4. Compute bounds from arbitrary prices through the existing outward-rounded core Lagrangian calculation; retain the best bound seen. Use objective-lattice rounding only when the existing sufficient conditions hold. Also construct a cheap initial dual cover bound before simplex. Retain a bound on any limited result.
5. Expose phase seconds and work/counter diagnostics. On an interrupted simplex solve, compute a conservative bound from its finite current row multipliers; a limited primal need not be feasible for this calculation.
6. Keep exact LP status determination in the current checked simplex kernel. The first-order pass improves anytime root information and search guidance; implementing a new IPM is outside this release.

Acceptance: checked bound on both full tables under a short budget; feasible seed even when simplex cannot finish; no unsafe bound on exhaustive tiny models or oracle differential tests. Report root status independently of bound progress and compare full-instance score/bound progress, not raw pivot counts alone.

## Stage 2 — parallel residual neighborhoods

1. Release a bounded set of incumbent variables around deterministic seeded centers. Fix the rest of the matching; reject every candidate transfer conflicting with fixed outside resources. Include all released incumbent transfers so that the current solution remains feasible in the residual.
2. Build a compact residual matrix, retain a mapping to original variables and impose fixed-bound semantics. Solve residuals with the owned native engine, small-model reliability branching and checked warm starts.
3. Use private worker solver states and immutable shared incidence data. Run batches whose tasks all use the same incumbent snapshot; merge complete reconstructed candidates in task-index order, by original objective and stable tie rule.
4. Bound neighborhood size, task count, per-task node/pivot limits, total reservations and shared deadline. Disable nested component/neighborhood/tree workers inside tasks. Account for all residual simplex nodes and pivots in the parent call.
5. Record task count, improvements, workers, reconstruction rejections, consumed nodes/pivots and phase seconds.

Acceptance: exact boundary feasibility on adversarial neighborhoods; no invalid outside mutation; deterministic fixed-work output independent of worker scheduling; nondecreasing incumbent score; full-table quality comparison with a pure greedy baseline. Neighborhood success does not certify the original optimum.

## Stage 3 — packing-specific cuts

1. Use implicit resource adjacency with bounded traversal instead of building a dense global conflict graph.
2. Generate cross-resource conflict triangles and bounded short odd cycles. Verify each consecutive conflict directly against original resource incidence; cycle vertices must be distinct. A triangle permits at most one member; an odd cycle of length k permits at most floor(k/2).
3. Retain only independently valid, nonduplicate cuts; use a fractional relaxation hint to prioritize violated candidates. Bound adjacency work, cycle lengths and inserted rows. Poll cancellation/deadline.
4. Insert cuts before constructing simplex caches; keep variable mapping unchanged. Every final candidate is checked on the original model. Cuts can be enabled with a primal warm start.
5. Record enumeration work, candidate/added triangle and odd-cycle counts, separation time and model growth. Evaluate bound gain and cost; do not equate cut count with effectiveness.

Acceptance: the pairwise-conflict triangle LP decreases from 1.5 to 1; a five-cycle LP decreases from 2.5 to 2; exhaustive validity on generated small packings; cut-free versus cut-enabled oracle objectives agree. Full-table timings may regress and must be reported.

## Stage 4 — scalable branching

1. Add an explicit scalable reliability mode independent of the old whole-model n/m eligibility gate. Preserve the old mode and defaults.
2. Form a bounded candidate shortlist using fractionality, pseudocost reliability and sparse structural scores. Use bounded storage/selection rather than sorting every fractional variable.
3. Probe from the parent basis; cap candidate count, directional probes and pivots. Enforce a separate cumulative matrix-size work reservation (`nnz+n+m` per reserved probe pivot); document this as a work proxy, not an instruction-count or LU-fill guarantee. Wall/factor safeguards still apply.
4. Charge every probe node/pivot to global budgets. Restore bounds and basis after failed, limited or interrupted probes; never use their status as a pruning proof.
5. Expose probe work reservations and eligibility/limit exits. Preserve existing search behavior when the mode is disabled.

Acceptance: prove activation on a model exceeding the old size gate with a fractional root; test zero/tiny limits, cancellation/restoration and fixed-work reproducibility; compare objectives and valid bounds with exhaustive/HiGHS references.

## Stage 5 — parallel tree search

1. Create a bounded exhaustive binary frontier using fractional hints/structural scores and exact 0/1 branching. Each frontier box is disjoint, and their union covers the original binary domain.
2. Solve each subtree with private native domain, basis, queue and pseudocost state. This first implementation is coarse frontier parallelism: workers reuse bases inside their own subtree, but do not import the parent's basis or share live cut pools.
3. Reserve global node/pivot shares in frontier order; no oversubscription or hidden nested worker pools. Use the shared cancellation token and whole-call remaining deadline. Unstarted/unfinished subtrees retain an inherited valid global bound when available.
4. Merge complete candidates in frontier order after original-model checks. Combine subtree bounds in the correct objective sense; absent bounds stay absent unless a valid inherited bound covers them. Never substitute an incumbent for an unexplored subtree bound.
5. Return numerical optimality only if every subtree is closed under the existing solver contract or a checked global bound meets a conservatively certified incumbent. Retain candidates and all accounting on limits or worker errors. Report failures instead of silently discarding subtrees.

Acceptance: frontier coverage/disjointness by exhaustive binary enumeration; serial/parallel tiny oracle equality; safe aggregate bounds on interrupted solves; cancellation, zero budgets and global node/pivot reservations; deterministic merges. Measure application overhead separately: this feature is not assumed to speed up an unfinished large root.

## Controls and release

Expose Rust/Python controls for packing root iterations/work, packing neighborhoods/size/workers, packing cuts/work, scalable probe candidates/pivots/work, and tree workers/frontier depth. Validate ranges before work and include effective options and actual diagnostics. Keep all new optimization modes off by default; provide a documented SpOC4 recipe.

Validation order: targeted unit/integration tests; exhaustive/random small binary packing and general MIP oracle comparisons; existing Rust/Python regression suite; formatting and Clippy with default and all features; release build; serial application measurements and 1/4-worker comparisons under recorded budgets. Compile before running timed measurements; run timed solver processes serially.

Deliver PLAN.md, implementation/results/reproduction/API notes, raw measurements and tests, complete source archive, and a patch against the pinned baseline. Distinguish implemented mechanisms from demonstrated performance gains and remaining limitations. No remote push or release publication is part of this task.


## Completion record

All five stages are implemented in native 0.25.0 and tested. See IMPLEMENTATION.md for exact scope and controls, RESULTS.md for measured benefits and non-benefits, and REPRODUCE.md for commands. The source ZIP includes an applyable patch against the pinned baseline, original application inputs, saved candidate vectors and validation logs. No remote push or package publication was performed.
