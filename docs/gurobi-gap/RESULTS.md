# LP-IP-craft performance review against Gurobi, revision 2

27 November 2026 · Upstream base `bcc4af35c66c2afe00066f50d736b2a06da48c72`

**The final revision reduced the ten-second solve-time score by 27.4% on the 35 cases reviewed from the previous campaign and by 23.5% on 24 fresh generator instances.** The largest repeatable improvement came from choosing GMI cuts for short flow balances with binary capacity decisions. The revised solver also continued beyond the reproducible numerical failure on `noswot`. The optional root policy remains off by default.

## Performance against the preceding patch and Gurobi

| Cohort | Previous patch solved | Revision 2 solved | Gurobi solved | Revision 2 / previous score | Native / Gurobi score, previous → revision 2 |
|---|---:|---:|---:|---:|---:|
| 35 previously benchmarked cases | 29/35 | 29/35 | 34/35 | 0.726× | 2.76× → 2.00× |
| 24 fresh generator cases | 24/24 | 24/24 | 24/24 | 0.765× | 0.85× → 0.65× |
| 6 additional public MIPLIB cases | 3/6 | 3/6 | 5/6 | Single observation | See public results below |

Both native arms enabled `adaptive_root` and `integer_equality_presolve`. The previous arm used the exact release extension from the first patch archive. Each of the first two cohorts had three rotated repetitions with a ten-second cap and one solver thread. Models, starts, tolerances and limits were matched. Timings cover the solve call. Each unsolved run receives a twenty-second penalty. For each arm, take the median penalized time per case, add 10 ms, and take the geometric mean. Lower is better. A case counts as solved when the majority of runs reach checked numerical optimality within the cap.

The 35-case score closed **31.5%** of the log-runtime gap from the previous patch to Gurobi. Case-resampled 95% intervals for revision 2 / previous were **0.566–0.888** on those cases and **0.583–0.964** on the fresh inputs. Equal weighting of generator families on the fresh set gave **0.470–1.064**. The six public cases used one observation per arm. Their equal native solved counts and the family interval limit claims about broader workloads.

A separate, three-repeat unpatched-source run on the same 35 fixtures gave 29/35. Revision 2 scored 0.836× unpatched, with a 20.5% log-gap closure relative to that starting point. The case-resampled 95% interval for revision 2 / upstream was 0.655–1.027. The upstream run was collected in a separate timing block. The previous 35 cases informed this review, so they cannot provide a new independent default-promotion test.

### Where runtime changed

Median wall times from the final 35-case comparison follow. The full journals retain statuses, incumbents, bounds and work counters for each repetition.

| Case | Previous | Revision 2 | Gurobi |
|---|---:|---:|---:|
| `lotsize-233` | 354.01 ms | 3.56 ms | 6.98 ms |
| `setcover-241` | 41.36 ms | 1.26 ms | 8.23 ms |
| `mknap-229` | 2.36 s | 1.81 s | 3.35 s |
| `stein45` | 10 s limit | 10 s limit | 6.03 s |

The 24 fresh cases used seeds 607, 613, 617 and 619 from each of six generators. Their manifest was frozen before policy selection. Each cell below is the unshifted geometric mean of four per-case median times in milliseconds.

| Generator family | Previous ms | Revision 2 ms | Gurobi ms |
|---|---:|---:|---:|
| Lot sizing | 50.18 | 4.74 | 5.33 |
| Set covering | 2.25 | 0.92 | 2.82 |
| Multidimensional knapsack | 411.36 | 338.54 | 583.14 |
| Capacitated facility location | 14.44 | 13.82 | 62.49 |
| Generalized assignment | 11.28 | 10.89 | 7.77 |
| Integer covering | 9.65 | 14.36 | 54.52 |

The fresh-run outlier `intcover-617` rose from 10.94 to 38.50 ms. A separate nine-pair serial replay gave 10.82 and 10.72 ms, both at 384 nodes. On the previous roster, `gap-241` rose from 46.19 to 77.50 ms at 483 nodes in both builds. Its nine-pair replay gave 38.04 and 38.24 ms. The scores retain both flagged comparisons.

## Findings and implementation

**Root-cut selection.** The previous policy spent multiple exact separation rounds on binary covering and dense packing systems. It also skipped short continuous balances with binary on/off capacity decisions. In an explicit-cut screen, GMI closed the difficult `lotsize-233` search in 24 nodes. Revision 2 now recognizes systems made up of two-term capacity links and two- or three-term equal-magnitude continuous equalities. It allows bounded GMI passes there. It skips generic adaptive separation on uniformly weighted one-sided binary covering systems and on binary packing systems whose active rows each use at least half the live binary variables. The classification accepts either row sign. Explicit cut requests, numerical validity checks and shared budgets keep their existing behavior. Short integer equalities with no inferred residues can again use the existing fast slack-row check.

**Numerical recovery.** On `noswot`, the weighted dual simplex hit its internal iteration guard at a depth-44 node. Its ordinary cold retry exhausted the guard again. Revision 2 permits one additional cold solve of that MIP node using largest-infeasibility pricing. The retry shares the caller’s pivot, time and cancellation budgets, restores the caller’s pricing mode, and leaves subsequent node checks in force. It accepts a recovered infeasibility only after checking the Farkas row over the node’s variable bounds. An independent HiGHS solve confirmed infeasibility of the saved 128-column, 187-row node relaxation.

At a fixed 150,000-node budget, the previous build stopped at **122,185 nodes and 704,435 pivots** with `numerical_failure`. Revision 2 used one alternate-pricing retry and reached the requested limit at **150,000 nodes and 864,337 pivots**. Both had a checked incumbent of −41 and a lower bound of −43. At 60 seconds, revision 2 reached 408,417 nodes and kept the same incumbent and bound. The solver can continue the proof search. The optimum is still open on this run.

**Reported integer-objective bounds.** Continuous variables carrying exactly zero objective cost preserve the integer objective lattice when the existing offset, coefficient and magnitude guards hold. An early implementation applied that observation to priorities inside the mixed-integer tree. In a 250-node replay it lost the −39 incumbent previously found on `noswot`. The delivered implementation preserves mixed-model branch priorities and reduced-cost fixing, and tightens the serial tree's returned conservative bound. All 69 fixed-work default comparisons now match status, candidate, objective, nodes and pivots. 68 also match the bound exactly. On `sp150x300d`, the lower bound at 250 nodes improved from 35.134798 to 36, with the same work and incumbent. A nonzero continuous objective coefficient, even a tiny one, excludes a model from this rule.

**Benchmark review.** The preceding harness still initialized new `noswot` freezes to −43. The corrected target is −41. Both the runner and auditor now recognize the old payload by SHA-256 and verify its old target before applying the correction. The audit checks callback candidates against the original constraints and rechecks historical Gurobi references after correction. The Gurobi adapter now forwards a supplied simplex iteration limit. The scorer rejects mismatched model hashes, starts, budgets or repetitions. The archive retains original inputs and raw measurements.

## Public cases and remaining gaps

The additional public roster was selected after fixing the policy, in SHA-256 order of filenames, from the pinned Data-Miplib3 collection. The selection excluded the previous roster, local pilot names and bundled corpus names. It required Gurobi size eligibility and agreement between the input parser and an independent HiGHS read. The selection manifest records the skipped file and all six source hashes.

Both native builds solved `p0033`, `p0201` and `bell3a`. Gurobi also solved `gesa2_o`, where native reached the time limit, and `pp08aCUTS`, where native could not infer finite variable bounds. Native also declined `danoint` on its bounds. Gurobi reached its deadline there, so that instance has no independently established optimum in this replay. On the previous roster, `vpm1` and `vpm2` still timed out and `pp08a` and `set1ch` still hit native domain limitations. In the development controls, revision 2 solved all eight. `bell5` took 4.07 s and 84,140 nodes, above the earlier 1.5 s and 30,000-node targets.

## Validation and delivery

| Check | Delivered result |
|---|---|
| Independent checks in the headline cohorts | 531 calls, 507 returned points checked against original arrays, zero discrepancies |
| Native and Python suites | 231 locked Rust workspace tests, 164 tests in the overlapping native feature run, 409 Python tests using the release worker. Formatting and all-feature Clippy passed. |
| Independent numerical oracles | 600 existing LP/exact checks passed. SciPy classified 549 of 550 additional cases with zero mismatches. One oracle result remained unclassified. |
| Equalities and job API | Six signed equality chains retained the previous optimal points, bounds, nodes and pivots. All four schema-checked job tutorial examples completed. |
| Full SpOC4 tables | Matching-I reproduced 32,847.565 with upper bound 34,153.324. Matching-II reproduced 70,945.093 with upper bound 76,765.388. At one and four workers the vectors, nodes and pivots matched revision 1. |
| Patch integrity | Both application routes passed `git apply --check` and actual application. All 35 changed files matched the reviewed checkout byte for byte. |

The benchmarks ran on a shared AMD EPYC 9V74 host using release builds, Rust 1.100.0, Python 3.12.14 and Gurobi 14.0.0. The 24-case comparison ran serially on logical CPU 5. On the 35-case roster, three independent workers each pinned a single-threaded solver process to CPU 0, 1 or 2. The machines’ other workloads can affect wall times, especially for millisecond cases. All optimality statuses are numerical under the stated `1e-7` tolerances.

For a clean upstream checkout use **LP-IP-craft-Gurobi-gap-v2.patch**. For a checkout with the first patch applied use **LP-IP-craft-Gurobi-gap-v1-to-v2.patch**. The ZIP README contains the apply and replay commands. Each patch and evidence file is covered by the archive's SHA-256 inventory.
