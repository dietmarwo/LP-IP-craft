# Native 0.26: iterative packing repair and separate proof budgets

Baseline: `c2eb282` (native 0.25.0). This plan implements the five priorities proposed in the subsequent review. Existing algorithms and defaults remain available. All new numerical work stays in the owned, dependency-free Rust solver; Python exposes the same controls.

## Application and correctness contract

The supplied original SpOC4 matching tables have nonnegative weights in integer thousandths (including two/six zero-weight columns in I/II) and three disjoint resource namespaces. Preserve original column identities and independently validate every final selection against those identifiers. This is the inspected weighted formulation; equivalence to ESA's official validator and the historical Gurobi configuration remains unverified. Do not infer a general solver speedup from these two tables.

Recognize only the existing binary, nonnegative maximization, unit-packing contract. Preserve fixed bounds, objective offsets, finite-domain admission, cancellation, checked bounds and global node/pivot limits. Unsupported models use the existing general solver. Default settings retain 0.25 behavior. Experimental features are opt-in.

## 1. Combine compatible repairs within each snapshot batch

Problem: two independent 12→16 repairs from the same snapshot can yield 20 together; selecting the best whole candidate loses compatible gains.

Implementation:

1. Keep the fixed batch width of sixteen, independent of worker count. Each task solves against the same immutable incumbent snapshot.
2. Reconstruct and independently check every local candidate on the original model. Extract only changed columns as an atomic remove/add delta. Ignore candidates that do not improve their own snapshot.
3. Continue retaining the best standalone candidate. In deterministic task order, apply each improving delta to a second snapshot-derived candidate only if its changed columns do not overlap previously accepted changes and its added transfers conflict with no retained resource owner after its removals. Use sparse owner updates; never combine approximate fractional changes.
4. Independently check the merged vector before comparing it with the best standalone fallback. Commit only an objective improvement. Conservative overlap rejection can miss compatible opportunities but cannot lose the best standalone result.
5. Expose accepted/rejected merge counts and batches where combination improves the standalone result. Keep task/node/pivot accounting unchanged.

Acceptance: exact 12→16→20 fixture; mutually conflicting repairs rejected; shared removal/overlap rejected; fixed bounds respected; input snapshot unchanged; deterministic one/four/sixteen-worker fixed-work result; randomized merges remain feasible and dominate the standalone fallback.

## 2. Successive repair rounds with refreshed snapshots

Problem: a single fixed set of repair tasks finishes early; the rest of a long solve can be spent on an unfinished global LP with no further incumbent progress.

Implementation:

1. Add `packing_rounds` (default one, maximum 32). `packing_neighborhoods` remains the number of tasks per round; validate the product at most 4096. All batches in a round retain their existing refreshed-per-batch snapshot behavior.
2. Refresh the seed ranking at each round boundary. Add bounded `packing_neighborhood_growth`, capped at the existing 1024-column maximum. Larger neighborhoods are a deliberate later-round option, rather than an unbounded retry loop.
3. Retain promising rejected-transfer seeds that yielded improvements and attempt them before new seeds in the next round. Skip seeds now selected or fixed; never retain stale model mappings.
4. Enforce whole-call node/pivot budgets across every round, not separately per round. Allocate shares using remaining planned tasks. A separate optional repair phase time cap includes neighborhood construction, cuts, local solves and merging.
5. Record rounds started, tasks, improvements, maximum size and a per-round trajectory containing objective, cumulative nodes/pivots and phase elapsed time. Stop safely on phase limit or cancellation; do not stop merely because one round stalls when larger/different neighborhoods remain planned.

Acceptance: fixture with useful moves appearing in a later snapshot, first-round equivalence when rounds=1, nondecreasing round scores, bounded growth and cumulative budgets; original-table one-round/multiple-round quality comparison.

## 3. Neighborhood selection from rejected transfers and root prices

Problem: incumbent-centered BFS need not release the blockers of a valuable rejected transfer. Compare strategies with the same starts, task count, local solver and budgets.

Implementation:

1. Add `packing_seed_strategy`: `incumbent` (default), `rejected`, or `mixed`. For rejected transfers compute current distinct incumbent blockers and rank by direct exchange gain, then resource-price residual, fractional hint and stable original column index.
2. Preserve all movable blockers of the chosen transfer. Exclude seeds blocked by fixed-one columns, fixed-zero seeds or seeds whose blockers exceed the neighborhood capacity. Grow through incumbent conflict neighborhoods only after these required releases.
3. Include the seed itself before truncating eligible additions. Admit other transfers only when all outside occupied resources are released. Retain all released incumbent columns, preserving a feasible residual warm start.
4. Retain best checked root prices with the relaxation bound. Use them internally for ranking and optionally expose original resource-order prices as a finite comma-separated diagnostic under `packing_price_diagnostics`. This is opt-in because the vector can be large.
5. Expose rejected/incumbent seed task counts and successful seed persistence. Ranking guides heuristics only; prices never authorize pruning without the existing independent bound checker.

Acceptance: high-value blocked seed and fixed-blocker boundary fixtures; exhaustive local-to-global reconstruction; one/four-worker deterministic results; direct incumbent/rejected/mixed full-table comparison.

## 4. Local clique cuts inside residual repairs

Problem: triangles leave a four-transfer cross-resource clique relaxation at 4/3; its clique inequality gives 1 (the pairwise-row LP gives 2).

Implementation:

1. Add `packing_residual_cliques` (default false) and a per-task `packing_clique_work` cap. Operate on each compact residual, before local simplex construction, so cuts can affect repair search.
2. Build bounded local conflict adjacency from resource incidence; extend candidate cliques greedily in deterministic order. Verify every pair directly against incidence, reject duplicates and cliques covered by an existing resource row. Limit cut count, work and clique size; poll cancellation/deadline.
3. Append only independently valid inequalities `sum x_j <= 1`, retaining columns and residual warm starts unchanged. Unlike approximate-hint separation, these local structural cuts do not require a fractional hint.
4. Record cuts, work, cut-phase time and local solver nodes/pivots. Never promote a local bound to a whole-model bound.
5. Compare cuts on/off using identical repair settings and initial points. Report overhead even when no quality gain results.

Acceptance: four-clique LP 2→4/3→1; exhaustive preservation of every integer feasible point on random residuals, duplicate/resource domination, zero/tiny cut budgets and fixed-bound reconstruction.

## 5. Budget root, repair and proof work separately

Problem: solution improvement and proof progress are different objectives. Matching-I's 0.25 root-price bound 34,153.324 is only 32.795 above the independently checked plain-LP bound 34,120.529; tightening that gap cannot remove the much larger incumbent-to-LP gap. Matching-II still needs a tighter reference.

Implementation:

1. Retain task-specific root iteration/work caps. Add optional `packing_repair_seconds` and `packing_proof_seconds`; both are phase caps inside the existing whole-call cooperative deadline. Zero proof seconds returns the checked repair incumbent and existing safe bound without entering exact root search. A phase time cap yields `time_limit`, not an optimality claim.
2. Start the proof clock after repair/global cut setup. Clamp the exact simplex/tree budget to both remaining whole-call time and proof time. Preserve global node/pivot subtraction. Disable new phase/repair controls in private children to prevent nested phases.
3. Report root, repair, cut and proof seconds; proof nodes/pivots/status; incumbent and bound before/after proof; round trajectories. The result status remains the actual termination reason and all candidates are checked on the original model.
4. Run equal-work repair ablations with proof disabled, then explicit proof-budget comparisons. Regenerate an optional HiGHS IPM reference for matching-II under a documented cap; record its actual status. Recheck any finite reference prices by independent exact dyadic aggregation, even if IPM does not finish.
5. Keep branching and coarse parallel tree changes outside this release: measure them only if completed roots show they are the bottleneck.

Acceptance: zero proof work, proof cap versus whole-call cap, cancellation during repair, finite nonnegative phase validation, valid bounds retained on every limit; independent original-resource audit of all saved application solutions.

## Validation and delivery

Order: targeted Rust fixtures → Python interface/independent exhaustive oracles → existing Rust/Python regression suites → fmt and Clippy default/all features → release build → isolated fresh-process application ablations. Compile before timing. Run timed experiments serially on this shared host. Fixed-work parallel equality is distinct from wall-limited scheduling variability; no sixteen-core speed claim on an eight-core quota.

Deliver complete 0.26 source, patch against `c2eb282`, this plan, implementation/API/reproduction notes, results with honest limits, raw controls/trajectories/environment, original inputs and audited original-order solution vectors. Preserve historical 0.25 results. No remote push or publication.

## Completion record

All five steps are implemented and validated in 0.26.0. See IMPLEMENTATION.md for exact controls and limits, RESULTS.md for incumbent gains and non-benefits, and REPRODUCE.md for commands. The source package contains the baseline patch, original benchmark inputs, all 38 checked solution vectors, raw trajectories, the completed/limited reference attempts and test logs. The completed matching-II LP reference has an independently verified integer-objective upper bound. Full integer optimality and official-validator equivalence remain unproved. No remote push or publication is performed.
