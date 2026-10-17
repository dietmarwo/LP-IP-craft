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
