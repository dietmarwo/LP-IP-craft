# Ten performance iterations

Baseline: upstream 76ca1c9, native 0.16.0. Preserve all numerical acceptance thresholds, budgets, original-model feasibility checks and the dependency-free Rust kernel.

HiGHS comparison motivates integer presolve (selection-64: 9,311 versus 1 node), sparse basis work (packing-4096: 559 versus 174 ms), and direct box processing (100,000 columns: 23.6 versus 11.2 ms). Reference methods: HiGHS HPresolve.cpp, HighsDomain.cpp, HFactor.cpp and HEkkDualRHS.cpp; our implementations are independent Rust code. HiGHS also uses branch-and-cut, reliability branching and more advanced factor updates; this series does not claim to reproduce all of them.

Measurements use identical frozen fixtures and independent objective/KKT checks. Per-iteration measurements are development evidence; final rotated paired runs are authoritative. Timing noise and regressions must be reported, not hidden.

## 1. Integral row strengthening — plan
Round finite row bounds inward only when every nonzero coefficient is an exactly represented integer on an integral variable. Skip contradictory rounded ranges for now (the ordinary solver can classify them). Work on an internal immutable-model copy only when a bound changes; preserve original model evaluation. Test negative/ranged rows, mixed rows and existing numerical/oracle suites; benchmark selection at all sizes and legacy MIPs.

Outcome: all Rust workspace tests pass. Selection-64 fell 9,311 -> 1 node and 179.96 -> 0.117 ms; selection-32 232 -> 10 nodes, 2.380 -> 0.251 ms. The old test's unconditional expectation of a child visit was restricted to runs actually visiting more than one node. Initial measurements include an unnecessary original-model evaluation on unchanged LPs; removed before committing. Numerical acceptance tolerances unchanged.

## 2. Skip vacuous component discovery — plan
For a model with no stored matrix entries, independent-component graph construction cannot produce a useful nontrivial component. Skip its O(n) singleton allocations, leaving the existing LP path and all checks intact. Test the complete workspace and compare the 100,000-column box case plus LP/MIP regressions.
Outcome: workspace tests pass; box-100k 15.82 -> 12.17 ms (1.30x) in three rotated paired runs. Other fixtures retain their pivot/node counts.

## 3. Direct row-free LP solve — plan
Choose each bound from the objective sign for continuous models with zero rows. Construct original-unit duals and run the same independent KKT, candidate and conservative bound checks as simplex. Preserve validation, starts and budget/cancellation semantics; decline the fast path when presolve is off. Test mixed signs, infinite bounds, zero costs and limits. Benchmark box-100k and all development regressions.
Outcome: workspace tests pass, including new direct/general box equivalence, objective offsets, both senses, unboundedness and zero-iteration budget tests. Box-100k 11.90 -> 4.50 ms (2.65x). Other pivot/node counts unchanged; small wall-time fluctuations remain.

## 4. Reachability-based BTRAN — plan
Maintain reusable sparse reach sets and inverse pivot permutations. For sparse right-hand sides on sufficiently large bases, traverse the U-transpose and L-transpose dependency graphs and solve only the reachable entries in pivot order. Abort discovery at one eighth of the basis and use the existing sweep. Keep all nonzeros (no new numerical dropping). Test residuals on large sparse bases, including eta updates, plus the workspace; benchmark packing, network and irregular LPs.
Outcome: all workspace tests pass, including sparse unit right-hand sides on permuted 512-row bases before and after eta updates. Packing-4096 560.25 -> 511.84 ms (1.09x), packing-1024 42.29 -> 39.74 ms. Network was 3.6% slower in this small sample; retain this limitation for the final paired run. No pivot-count changes.

## 5. Reachability-based FTRAN — plan
Reuse bounded reach discovery for L and U forward solves, with a row-to-position map for U. This targets the entering-column and steepest-edge solves that run every pivot. Preserve elimination order and eta application. Verify sparse and dense residuals, all workspace tests, and the same paired development fixtures.
Outcome: first attempt regressed large LPs; retained its raw data. Revised the FTRAN reach cap to 1/32 of the basis and added per-triangle eight-call cooldowns after dense discovery. All tests pass. Revised paired medians: packing-1024 40.22 -> 36.78 ms, packing-4096 556.06 -> 526.41 ms, irregular-1024 250.78 -> 236.15 ms; network essentially flat (1.4% slower).

## 6. Reuse entering-column sparsity — plan
Collect the nonzero basis coordinates once after FTRAN and reuse that list for primal updates, steepest-edge updates, and eta storage (including basis restoration). Preserve ascending arithmetic order and exact nonzero tests. Test the complete workspace, especially warm/cold MIP and factor-update residuals; measure all development fixtures.
Outcome: workspace tests pass. Network-2048 104.42 -> 91.43 ms (1.14x); packing and irregular within about 2%. Reusing support also avoids repeated full scans during basis restoration. Box-path time varied despite no box code changes; final comparisons will use more samples.

## 7. Skip disjoint eta transformations for unit BTRAN — plan
Store a conservative 64-bit hashed support mask on each eta update. Unit pivot-row BTRAN starts with a known support; skip an eta only when its entire support (including pivot position) is disjoint. Mask collisions merely cause extra work; no nonzeros are discarded. Accumulate support conservatively through transformations. Test unit solves with permutations and updates, dense solves, full workspace and paired performance.
Outcome: all tests pass. Mask-only timing was inconclusive; added reusable exact support seeds so unit BTRAN also avoids rediscovering its RHS by a full scan. Revised packing-1024 44.45 -> 38.43 ms, network 97.12 -> 95.81 ms; packing-4096 and irregular regressed about 3%. This is a workload-dependent improvement, not a universal speedup; both attempts are retained in the evidence.

## 8. Remove exact cancellation entries during sparse factorization — plan
After a Schur update, delete exactly-zero entries from both active row and column lists and update Markowitz counts immediately. This avoids pricing artificial fill and traversing canceled dependencies. Do not drop small nonzero values. Verify sparse-factor residuals, singularity and cap tests, and all workspace tests; compare LP families.
Outcome: all workspace tests pass. Packing-4096 540.55 -> 529.32 ms, irregular-1024 240.55 -> 236.04 ms (about 2% each); network flat within 1.1%. Packing-1024 requires one fewer pivot because exact Markowitz counts change the factor ordering.

## 9. Exact integer domain propagation at MIP nodes — plan
Round integer variable domains and propagate eligible all-integer rows using i128 arithmetic. Restrict coefficient/domain magnitudes so activity sums cannot overflow. Use at most three row passes, honoring cancellation/time checks. Apply implied bounds to the existing LP for reoptimization, restore them between nodes, and prune exact contradictions before solving an LP. Mixed or large-arithmetic rows retain the existing path. Test negative coefficients, fractional domains, contradictions and enumeration oracles, then paired MIP/LP performance.
