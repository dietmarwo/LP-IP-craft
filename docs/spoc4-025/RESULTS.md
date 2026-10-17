# Native 0.25 results: all five SpOC4 stages

Baseline: `9bf392c1f0f1cbf3242a31d0b7d8edd0b3352149` (0.24 plus review fixes). The delivery implements every stage in PLAN.md as an explicit bounded mode; defaults remain on the prior numerical path. No external numerical dependency is added to the Rust solver.

## Application result

The useful combination is the checked packing root pass plus residual neighborhoods. At a **one-second solver budget**, it returns **32,120.012** on matching-I and **68,914.100** on matching-II. These improve the deterministic greedy baseline by **7.82%** and **8.85%** respectively. The same scores occur in the 10-second one/four-worker runs.

These are feasible incumbent scores, not proven optimal integer values. Actual one-second solve times are 1.010 s and 1.036 s because limits are cooperative. Input/model construction is separate (0.100 s / 0.346 s). Every stored solution is rechecked using original E/L/D identifiers and exact integer-thousandth weights; the aggregate audit has 14 records and zero failures.

| Mode | Budget | I score | I upper bound | II score | II upper bound |
|---|---:|---:|---:|---:|---:|
| Default, greedy start | 30 s | 29,791.692 | none | 63,313.022 | none |
| Packing root, cold | 10 s | 32,037.550 | 34,153.324 | 68,814.475 | 76,765.388 |
| Root + 64 residuals, 1 worker | 10 s | 32,120.012 | 34,153.324 | 68,914.100 | 76,765.388 |
| Root + 64 residuals, 4 workers | 10 s | 32,120.012 | 34,153.324 | 68,914.100 | 76,765.388 |
| Residuals + packing cuts | 10 s | 32,120.012 | 34,153.324 | 68,914.100 | 76,765.388 |
| Residuals + cuts + 4 subtrees | 10 s | 32,120.012 | 34,153.324 | 68,914.100 | 76,765.388 |

The default runs receive a greedy start to reproduce the earlier assessment. New modes start cold and generate their own checked incumbent. These are budget/quality comparisons, not claims of a fixed speed ratio. All full-instance statuses are `time_limit`; no full exact simplex root or integer optimum is established.

## Stage 1: root relaxation and partial bounds

A sparse factorization-free primal-dual pass supplies a useful fractional hint and a conservative dual-price bound before the large simplex root finishes. It performs 645 steps / 199,950,000 reserved work units on I, and 192 / 199,868,544 on II. In the main four-worker runs the root pass takes 0.201 s / 0.316 s.

The returned integer-objective upper bounds are **34,153.324** and **76,765.388**. Bound gaps relative to the incumbents, `(upper-incumbent)/incumbent`, are 6.33% / 11.39%. The I bound is about 0.10% above the independently checked 34,120.529 upper bound obtained from HiGHS IPM in the previous assessment. II has no tight reference LP optimum from that assessment.

Bounds use the existing outward-rounded Lagrangian checker, including residual costs over variable upper bounds. Approximate primal points are not promoted to integer solutions or optimal LP statuses. Limited simplex solves can also expose checked bounds under the explicit partial-bound mode. The remaining simplex kernel is unchanged; completion of the full root is still a limitation.

The separate instrumented 5-second prefix profiles identify FT upper solves and pricing as the largest named kernel components. They are not whole-solve speed measurements or a proof that the same operations dominate after substantial fill. Raw exclusive timing records are included.

## Stage 2: parallel residual neighborhoods

The fixed-work one/four-worker results have the same final original-order solution and identical residual task, improvement, node and pivot totals. Scheduling is deterministic at fixed work; wall-limited truncation need not be.

Neighborhood phase times, one worker / four workers:

- matching-I: 0.130 / 0.101 s (1.29x phase ratio).
- matching-II: 0.473 / 0.376 s (1.26x phase ratio).

These are single shared-host pilot observations, not statistically stable scaling factors. Root and ordered validation/merge remain serial. Sixteen residual workers are supported by the fixed batch width and tested on the exact regression fixture; the host has an eight-core CPU quota, so no 16-core application speedup is claimed.

All 64 residual tasks finish in one root node each on these full-table pilots. Thus their quality benefit is demonstrated, while substantial hard-neighborhood branching or high worker-count scaling is not. Residual errors are zero. Parent nodes/pivots include all residual work, and independent batch/component workers disable nested pools.

## Stage 3: packing-specific cuts

The triangle, five-cycle and seven-cycle fixtures change their LP values from 1.5/2.5/3.5 to 1/2/3 while preserving every integer point. Exhaustive random oracle checks find no changed integer optimum.

On the original tables the bounded separator adds **9 triangles and 3 longer odd cycles** for I, and **4 triangles** for II. These are directed by the approximate root hint, so their count need not equal violations at an exact LP optimum. Within ten seconds they do not improve score or bound over the residual-only mode. Separation adds overhead; it stays off by default.

The separator prioritizes a broad triangle pass before bounded longer-cycle traversal. It checks distinct vertices and consecutive resource conflicts, rejects duplicate/resource-dominated cuts, and caps work/model growth. No dense global conflict matrix or live shared cut pool is introduced.

## Stage 4: scalable branching

A 1,033-variable fractional-root regression activates reliability probes above the old dimension gate. The old mode still makes zero probes there; zero-work scalable mode also makes none. The test checks the common optimum and cumulative probe reservations. Other tests cover global node/pivot limits and parent-basis restoration through limited probes.

Full-table main simplex roots still do not complete in these pilots, so the scalable brancher has no demonstrated whole-table search benefit yet. Work and candidate caps prevent merely lifting the old gate from causing unbounded probe effort. The probe-work proxy is per serial search invocation; private subsolvers additionally remain inside assigned parent node/pivot budgets.

## Stage 5: parallel tree search

The implemented mode partitions free binary variables into an exhaustive disjoint frontier. It is parallel B&B over subtrees, not duplicate whole-model portfolio runs. Tiny oracle tests check frontier coverage once per binary point, minimization/maximization, safe aggregate bounds on interruption, and global reservations.

Four-worker full-instance runs retain the residual score/bound and process four private roots, with total pivots 71,762 / 66,795. They show **no incumbent or bound gain** in the ten-second pilot and spend more aggregate LP work than serial root search. This mode stays experimental and off by default. There is no parent-basis import, dynamic frontier redistribution or live cut/incumbent broadcast in this first version.

## Validation

- Rust default/workspace: 155 tests, zero failures.
- Rust numerical crates, all targets/features: 155 tests, zero failures.
- Python: 228 tests, zero failures; includes 400 small MIP solves compared with exhaustive integer oracles across four new-mode combinations.
- Rust packing tests additionally check 120 random bound/cut fixtures against exhaustive optima, exact residual boundary reconstruction and one/four/sixteen-worker fixed-work equivalence.
- In-flight parallel cancellation retains a checked incumbent; invalid settings are rejected even on cancellation; independent-batch nesting is suppressed.
- Formatting and Clippy pass with default features and all features, warnings denied.
- All 14 full-instance saved vectors pass the independent audit; global node/pivot and root/cut work caps hold in those records.

The host uses Rust 1.99.1, Python 3.12.14, NumPy 2.3.5 and SciPy 1.17.0 on an AMD EPYC 9V74 virtual host (nine visible logical CPUs; eight-core CPU quota). Timed solver processes are serial fresh processes; no compilation or other solver benchmark runs concurrently. Detailed environment and logs are shipped.

## Recommended use and limits

Start with `packing_root=True`, `packing_neighborhoods=64`, `packing_workers=4` and `scalable_branching=True` on the supplied unit-packing models. Keep cuts and tree search off until a longer-budget comparison demonstrates value. Use explicit caps and retain the original-resource checker.

All five mechanisms are implemented and tested, but their performance evidence differs: root guidance/bounds and residual quality improve; cuts strengthen exact fixtures but show no short full-table benefit; scalable branching activates correctly above the old gate; coarse tree parallelism is correct but not beneficial in this application pilot. This is not an owned IPM implementation or a general shared-queue industrial parallel MIP solver.

The full Gurobi comparison remains blocked by the installed size-limited license, as in the preceding assessment. No competition-scale Gurobi replacement claim or new HiGHS/Gurobi timing comparison is made. See PLAN.md, IMPLEMENTATION.md and REPRODUCE.md for contracts, scope and commands.
