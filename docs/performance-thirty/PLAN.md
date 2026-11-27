# LP-IP-craft performance assessment and 30-iteration optimization plan

27 November 2026 · Measured base **v0.31.0**, commit `2d6c431d968623877894f350456289ef102f4791` · Native Rust LP and MIP solver

**Start with three changes:** use a certified incumbent to bound the improving search on partially unbounded MIPs, reduce the cost of dense basis solves on irregular LPs, and separate incumbent discovery from proof progress on difficult MIPs. They address three different observations on the current release. A source-free experiment moved `pp08a` from rejection to a feasible solution. The largest generated LP took about six seconds, with 43% of one diagnostic solve in the forward and transpose dense-basis paths. On `mod008` the solver found the reference objective and still ran out of time proving it. On `vpm1` it had no incumbent.

The requested deliverable is this plan. The measurements below are completed work. The thirty numbered entries are proposed implementation trials. The scope includes the shipping defaults, the opt-in adaptive policies, and the packing and parallel paths relevant to SpOC. The plan builds on the merged Gurobi-gap and LocalMIP changes in the release. [Pinned source](https://github.com/dietmarwo/LP-IP-craft/tree/2d6c431d968623877894f350456289ef102f4791)

## What the current release does

### Native LP baseline

I built the repository's Rust benchmark twice from the pinned checkout, with locked dependencies and release optimizations. One build supplied wall times, the other supplied exclusive single-thread kernel timings. Each row below uses one discarded warm-up and five measured solves of the same generated model on logical CPU 4. The component worker cap and numerical width were both one. All 25 measured solves passed the harness's original-model feasibility, reference objective and KKT checks. Counts are simplex pivots per solve. Model construction and the independent check are outside the timer.

| Generated LP | Rows × columns | Nonzeros | Pivots | Median ms | Min–max ms | Largest exclusive categories in a separate instrumented solve |
|---|---:|---:|---:|---:|---:|---|
| Packing 1,024 | 1,024 × 1,024 | 3,072 | 1,025 | 47.9 | 44.0–48.9 | FT upper solve 24%, sparse elimination 10% |
| Packing 4,096 | 4,096 × 4,096 | 12,288 | 4,098 | 490.3 | 438.7–776.1 | FT upper solve 32%, other factorization work 8%, other pivot work 8% |
| Network 2,048 | 2,048 × 4,093 | 8,186 | 2,047 | 155.4 | 151.3–211.0 | Row pricing 22%, upper forward solve 13%, transpose triangular solve 12% |
| Irregular 1,024 | 1,024 × 1,024 | 6,137 | 1,025 | 328.1 | 288.2–440.6 | Forward eta application 20%, dense forward solve 16% |
| Irregular 4,096 | 4,096 × 4,096 | 24,568 | 4,096 | 6,163.9 | 5,522.7–12,867.6 | Dense forward solve 28%, dense factor update 18%, dense transpose solve 15%, forward eta 12% |

A later three-solve block for irregular 4,096 measured **6.22, 5.87 and 6.58 s**, again at 4,096 pivots. The original five-run block peaked around 120 MiB RSS. The instrumented block took 5.67 s. Its `ftran_dense` and `btran_dense` categories include the surrounding sparse prefix and transfers. Split these costs before rewriting an inner loop. For packing 4,096 a subsequent single-block mode check at a common maximum refactor interval of 64 gave median times of 834 ms (eta), 486 ms (forced Forrest–Tomlin) and 235 ms (auto), all at 4,098 pivots. The main LP table used the harness maximum interval of 256. The modes ran in that order and the host speed varied substantially. Use these numbers to select replay cases. A mode-selection claim needs interleaved A/B timing.

### Ten matched model comparisons

These are **fresh v0.31.0 runs** against Gurobi 14.0.0. Each arm received the same original arrays, one solver thread, no supplied start and a two-second solve limit. Three repetitions used separate processes, rotated arm order and logical CPU 5. Times below are median solve-call milliseconds. `adaptive + equality` enables only `adaptive_root` and `integer_equality_presolve` over native defaults. The three LP controls exercise the same native LP policy in both arms and illustrate timing noise. Native LP and mixed-model measurements used distinct pinned CPUs on the same shared EPYC 9V74 host. No core was reserved exclusively.

| Case | Native default | Native adaptive + equality | Gurobi | Interpretation |
|---|---:|---:|---:|---|
| adlittle (LP) | 0.72 | 0.98 | 1.11 | Sub-millisecond native control |
| e226 (LP) | 13.67 | 6.86 | 9.05 | Same native objective and policy across repetitions |
| 25fv47 (LP) | 364.60 | 352.12 | 209.36 | Public LP check for changes to the simplex path |
| gap-241 | 151.65 | 86.94 | 63.52 | Native work fell from 1,789 to 483 nodes |
| intcover-233 | 23.83 | 21.76 | 114.14 | Protect this family when increasing root effort |
| lotsize-233 | 553.02 | 7.17 | 8.81 | Native work fell from 15,110 to 24 nodes |
| bell5 | time limit | time limit | 103.41 | Substantial proof work after finding incumbents |
| mod008 | time limit | time limit | 72.97 | Reference objective 307 reached by native, proof incomplete |
| vpm1 | time limit | time limit | 20.72 | Native returned no candidate in the timed screen |
| pp08a | unsupported | unsupported | 765.30 | Native finite-domain admission blocked search |

Both native arms solved six of ten models on every repetition. Gurobi solved ten. The journal auditor checked 78 returned candidates across 90 observations and found zero discrepancies. Unsupported cases count against coverage. The cases were deliberately chosen from previously exercised models. This diagnostic comparison covers small license-eligible inputs. Historical context comes from a different ten-second study, where the repository review reproduced a 0.727 score ratio for the revised **opt-in** build against its predecessor. Its unpatched-default versus revised-default replay scored 1.002. Use today's release defaults as the baseline for new gains. [Repository review and benchmark definitions](https://github.com/dietmarwo/LP-IP-craft/blob/2d6c431d968623877894f350456289ef102f4791/docs/gurobi-gap/REVIEW.md)

### Which MIP mechanisms merit work

A second fresh diagnostic gave each arm at most 1,000 nodes, 100,000 pivots and 20 seconds, with search observation enabled. Default, adaptive only, equality only, and both settings ran on the same five inputs. All twenty runs terminated by work limit or numerical optimality. The equality-only arm matched the default node, pivot, incumbent and bound values on these five inputs. The combined arm matched adaptive only on the same fields. The dedicated equality-chain corpus remains the appropriate control for that option.

| Case | Default at the work cap (incumbent / lower bound) | Adaptive at the work cap | Engineering consequence |
|---|---|---|---|
| lotsize-233 | None / 179 | 3,391 / 3,391, solved in 24 nodes and 197 pivots | Attribute the gain to adaptive separation in this comparison. Protect it when reducing cut overhead. |
| mod008 | 307 / 292 | 347 / 296 | Adaptive separation improved the bound and delayed the good incumbent. Score both trajectories. |
| bell5 | 9,216,166 / 8,934,581 | 9,219,766 / 8,915,804 | Root effort can worsen both sides at equal node count. |
| noswot | −39 / −43, 4,564 pivots | −41 / −43, 6,067 pivots | Preserve early incumbents when changing pruning and tie-breaking. |
| vpm1 | None / 0, 2,581 pivots | None / 0, 2,581 pivots | A different primal search deserves a trial. Run a supplied-start control to measure the remaining proof gap. |

Values are rounded for display. The fixed-work audit checked 14 returned candidates with no discrepancy. Its generated lot-sizing fixture lacks a stored reference in that manifest. The separate Gurobi run above confirmed 3,391 on the same original model.

### Why pp08a needs an incumbent-based experiment

I optimized each missing bound direction separately over the original **continuous relaxation** with single-thread HiGHS 1.16.0. On `pp08a`, 112 of 176 missing directions were unbounded and the other 64 had finite optima. Repeating the diagnostic under the known objective cutoff 7,350 made all 176 finite. The corresponding unrestricted counts were 12 unbounded out of 472 on `set1ch`, 112 out of 112 on `pp08aCUTS`, and one out of 457 on `danoint`. These are numerical LP diagnostics. Additional ordinary row-propagation sweeps cannot in general recover globally valid bounds on genuinely unbounded directions.

I then tried a route **without the reference optimum**. In `pp08a` all variables have lower bound zero, all costs are nonnegative and the objective offset is zero. Fixing the integer setup variables at their upper bounds and solving the resulting continuous LP with the native solver gave a point of cost **18,100**. Exact rational arithmetic on the stored arrays verified every original row, bound, integer and objective. Therefore every solution at least as good as that point satisfies `x[j] ≤ 18100 / c[j]` for each positive-cost column. I supplied exactly representable caps to the 112 positive-cost columns missing upper bounds, left the other 64 directions to the ordinary solver, and passed the verified point as a start. The existing native solver returned **8,440** with zero original-model primal and integrality violation in a three-second run. Its lower bound was **4,635.42** and it visited **38,256 nodes**. Gurobi's checked optimum on the original input was **7,350**.

This is evidence for admission and for a genuine remaining proof gap. The capped search preserves the optimum because it contains the verified incumbent and every improving point. The new bounds describe that search region only. The current release requires the caller to perform this construction. Iterations 1–3 test whether the solver can do it correctly and economically across models.

## Campaign protocol

**Reference builds.** Freeze A0 at the above commit, then use identical compiler, lockfiles, release flags and CPU features for A (last retained revision) and B (exactly one new change). Keep plain and profiled binaries separately. Record SHA-256 for binaries, extension, input arrays and manifests, effective settings, CPU affinity, seed, host, compiler and commands. Compare B/A every iteration and the accumulated build against A0 every five. Use Gurobi and HiGHS as external processes or benchmark-only dependencies. The production numerical crates stay native Rust.

**Freeze cohorts before tuning.** Development uses the five generated LPs, the ten rows above, the five fixed-work traces and the frozen basis replay corpus. Regression adds Netlib controls, degenerate/ranged/ill-scaled LPs, the decimal equality and mixed-objective edge cases, zero budgets, both objective senses, known infeasible and unbounded inputs, cold/warm restores and unsupported domains. Use `docs/irregular-lu/corpus`, `docs/packing-mip/corpus`, the existing exact/SciPy oracles and the six signed equality-chain models. Retain the 35-case and 24-generator archives for historical comparability; both have informed development.

For final confirmation, generate four **unused** seeds per existing MIP family, select additional eligible public models by a recorded filename-hash order, and hold out LPs by source family or matrix structure. Freeze hashes, reference statuses, expected coverage, exclusions and test budgets before the first edit. Include hard cases that time out. Reserve one unseen set for the final integrated build. Repeated inspections turn a set into development data. Where available, use the two full SpOC matching tables in addition to the redistributable small fixtures and report coverage explicitly.

**Measure the right quantity.** For LPs, interleave A/B and B/A in fresh processes, discard one warm-up per process, collect at least seven paired process rounds on target families, and report median time, paired ratios, ranges, pivots, refactors, factor fill and RSS. For MIPs run (i) a deterministic node/pivot budget for the search trace, (ii) a ten-second benchmark with at least three paired repetitions, and (iii) sixty seconds on hard finalists. Record original-model primal feasibility, conservative bound, incumbent, termination reason, time to first feasible point, bound after root, nodes, pivots and time to proof. For a heuristic's score integrate the normalized checked-incumbent gap over the same clock horizon, with the existing LocalMIP convention for missing points. For proof performance reuse the repository's shifted PAR-2 score. Charge twice the cap when optimal termination is missing, take each case's median, add 0.01 s and geometric-average across cases. Report solved counts, unsupported counts and family scores alongside it. Use 95% paired bootstrap intervals over process rounds for single-fixture timing, and over independent models for generalization. Aggregate repetitions within each model first. Report a separate family-balanced estimate so that one prolific generator cannot dominate.

**Decision rules proposed for this campaign.** A kernel trial advances after a reproducible ≥5% target-family solve-time reduction, a paired interval excluding parity on its primary target, and no reproducible >5% regression on an established non-target family. Time tiny cases in batches and investigate a regression only after repeating the pair. For search trials accept ≥10% improvement in the declared proof or primal metric, or additional checked solutions/proofs on at least two independent held-out models with previous coverage preserved. Admission changes can merit an opt-in route on one independently checked new public model with exact small analogues; label that as a capability gain. Always check numerical validity, bounded memory, termination and budget handling. Report borderline results as inconclusive. Default promotions require fresh family-level evidence and A0 parity on protected workloads. A single attractive generator or a kernel-only speedup cannot decide a default.

**Iteration loop.** Before each change write its target, one primary metric, expected mechanism and counterexample. First capture the relevant profile on A. Build one candidate, run its gates below, measure A/B, inspect the worst regression and record keep, opt-in, revise or revert. When the proposed path is absent from the workload, use a named positive control and document its frequency in ordinary solves. Discard a failed prototype before the next independent experiment. For dependent experiments use the last sound implementation or a benchmark-only scaffold and keep attribution explicit. No wall-time improvement is assumed in advance.

## The 30 implementation trials

Paths in this section are relative to `crates/native/src/` unless otherwise stated. Each numbered entry changes a different mechanism. Target metrics supplement the common acceptance rules.

### I. Admit and bound more MIPs

| Iteration | Single change to try | Experiment and decision |
|---:|---|---|
| 1 | **Use a supplied certified incumbent for objective-implied domains.** In `root.rs` before finite-domain admission, use `incumbent.rs` to validate the original point, orient the objective, bound the other objective terms outward and derive each eligible column side. Restore the incumbent if the restricted search stops. | Repeat the 18,100-start `pp08a` experiment through the ordinary Rust and Python calls. Compare work with the external capped control. Enumerate tiny signed, offset, maximization and zero-cost analogues. An uncertified start, overflow or an infinite residual must leave the affected side unresolved. Preserve valid original-problem bound/status on cancellation. |
| 2 | **Generate a start before domain rejection.** For an eligible on/off structure, try one deterministic all-enabled assignment for the setup columns, convert the remaining fixed-integer subproblem to a continuous LP, and validate the original point. Charge the LP to the shared time/pivot allowance. | Run `pp08a` without a supplied start and measure setup time, admission rate and three-/ten-second quality. Use infeasible all-enabled assignments and models with unassigned discrete columns as controls. Keep a failed attempt cheap and preserve ordinary unsupported status when no safe route exists. |
| 3 | **Queue rows for finite-side inference.** In `root::infer_bounds`, replace four full sweeps experimentally with a bounded incident-row queue. Enqueue only after a bound changes. Preserve outward rounding, cancellation and work limits. | Reverse row order in long signed implication chains, include zero/tiny coefficients and test `set1ch`, `danoint`, `pp08aCUTS` with and without certified starts. Count row visits, proved finite sides and remaining unresolved directions. Charge every extra admission cost and retain coverage failures. |
| 4 | **Prune with a safe objective lattice while preserving queue order.** In the serial search use the existing conservatively rounded mixed-model bound only at the prune decision. Retain the original priority key and tie-breaking. | Compare status, nodes to proof, first incumbent and bound trajectories on `noswot`, `sp150x300d`, `mod008` and small enumerated mixed models. Exercise nonzero continuous cost, fractional offset, max sense and near-integer bounds. A displayed zero gap alone cannot terminate search. |

### II. Reduce the cost of irregular LP bases

| Iteration | Single change to try | Experiment and decision |
|---:|---|---|
| 5 | **Block the dense forward solve.** Split timing for sparse prefix, dense tail and transfers in `Lu::ftran_dense`. Prototype a cache-blocked lower/upper triangular solve in `DenseTail::ftran` retaining the short-row path. | Replay early/middle/late irregular bases at 1,024 and 4,096. Measure ns/RHS by tail size and end-to-end seconds. Check scaled backward error on sparse and dense RHS, KKT, pivot counts and near-singular fallbacks. |
| 6 | **Tile the dense transpose solve.** In `DenseTail::btran`, update a small panel before streaming the rest of the vector. Test one tile policy independently of iteration 5. | Time transpose RHSs and dual pricing on the same frozen bases. Check an independent solve of the transposed matrix, dual signs and original-model residuals. Keep only where repeated solves repay the panel overhead. |
| 7 | **Pack the right-hand panel of the dense Schur update.** In `dense.rs`, stage a contiguous reusable panel for `schur_serial` while preserving the threshold pivot decision and arithmetic order per output. | Measure packing cost, update time, tail sizes and full irregular solve. Test singular and cap-limited dense tails and mixed sparse/dense bases. Compare peak storage and the existing scalar fallback. |
| 8 | **Flatten conventional eta chains.** Replace separate per-update coefficient allocations with an offset table and flat index/value storage for the forward and reverse traversals in `lu.rs`. | Force eta on frozen bases, then test auto on both irregular sizes. Time ns/eta entry in both directions, allocations, peak entries and full solves. Compare each replacement chain with a freshly refactored basis. |
| 9 | **Adjust the rebuild decision to measured work counts.** Estimate subsequent eta traversals from recent RHS density and chain length relative to the most recent factor work. Keep minimum spacing, hard caps and error-triggered rebuilds. | Compare with auto and fixed intervals 16/32/64 on irregular, packing and network bases. Primary metric is solve time. Publish refactor counts, fill, update work and largest residual. Tune using counts only so a schedule or a noisy clock cannot change a fixed-work solve. |
| 10 | **Reuse factorization scratch for pivot buckets.** Extend `FactorWorkspace` to retain bucket and completed-row/column arrays now allocated anew in `factor_with_execution`. Reset only valid ranges for the new basis. | Use repeated refactors on the replay corpus, varying dimensions and injecting failed factorizations. Count allocations and exclusive setup time. Verify the old usable factor survives failed replacement; bound retained capacity on size changes. |

### III. Sparse forward solves and simplex pricing

| Iteration | Single change to try | Experiment and decision |
|---:|---|---|
| 11 | **Repack the hot FT upper adjacency.** Trial contiguous storage for stable portions of `Ft::cols` with a small edit area for changed columns. Preserve the numerical iteration order and rebuild the representation only on a measured edit budget. | Packing 4,096 spent 32% of the profile in the FT upper solve. Time traversal and update costs together. Force FT, eta fallback and auto on real bases, compare `B x − b` and refactor results, track peak bytes. |
| 12 | **Use prior output support to clear FT scratch.** Replace the full `out.fill(0.)` on eligible sparse calls by clearing the previous dirty output indices. Mark transitions from dense calls and row-eta expansion explicitly. | Exercise sparse→dense→sparse, FTRAN→BTRAN, cancellation-to-zero and rejected-update cases. Count zero stores and validate all vector entries, including untouched positions. Keep only with measurable packing/network gains. |
| 13 | **Remove the full spike scan on eligible entering columns.** Pass verified structural RHS support through the lower solve and affected row etas to form the spike in stable order. Existing spike seeds already accelerate the later reach search. | Compare spike values and order with dense capture after permutations, exact cancellations, and a trailing conventional eta. Measure entries examined per capture plus total basis-update time. Revert to full capture when the tracked support becomes dense. |
| 14 | **Update FT cross-indices on removal.** In `Ft::update`, replace the linear opposite-adjacency searches with back-pointers modeled on `active.rs`'s swap removal. | Compare update time against the extra storage in forward and transpose traversal. Test arbitrary row/column replacements, failed updates and FT-to-eta fallback. Keep the simpler representation if the end-to-end result loses. |
| 15 | **Specialize row pricing for degree-two incidence.** In `lp.rs::compute_row`, preclassify sparse incidence entries that admit a short path; preserve logical-column handling, drop threshold, touched order and tie behavior. Fall back for all other rows. | Lead with network 2,048, where row pricing took 22%, and test transport LPs plus density-matched non-network controls. Compare priced coefficient vectors against the general path. Record pricing ns/pivot, pivots and accepted objectives. |
| 16 | **Avoid rescanning clean pricing blocks for the winner.** Maintain a deterministic tree of block winners for large bases over the existing `price_dirty` cache in `choose_row`. | Measure dirty blocks and winner-lookup time at sizes 512, 1,024 and 4,096. Compare the chosen row with exhaustive scoring after flips, rescaling and restore. Preserve the lowest-index tie rule and benchmark dense invalidation as a negative control. |

### IV. Presolve and root relaxation

| Iteration | Single change to try | Experiment and decision |
|---:|---|---|
| 17 | **Index the two binary probe outcomes.** In `mip_presolve::probe`, replace repeated reverse searches of the two changed-column lists with stamped last-value slots and one union traversal. Preserve the existing in-place trial/undo scheme. | Use implication chains, wide probing models, set covering and small random binaries. Compare the entire tightened box with the old implementation, including repeated touches and interruption. Measure root time versus changed-column count. |
| 18 | **Deduplicate certified root cuts before adding rows.** Fingerprint canonical exact coefficients, sense and RHS, then check full equality on a hash match. Narrow the first trial to exact duplicates within/across rounds. | Count duplicate candidates, rows added, exact-work units and subsequent pivots on lot sizing, GAP and `mod008`. Preserve validity, shared ceilings, strongest retained bound and a deterministic insertion order. If duplicates are rare, discard the index. |
| 19 | **Stream sparse tableau multipliers to the exact separator.** `Lp::tableau_candidates` currently builds full original-row vectors. Pass sorted nonzero multipliers for one candidate at a time to `certified_cuts.rs`, retaining every nonzero used in exact reaggregation. | On frozen fractional roots compare each reconstructed exact cut, activity and rejection reason with the dense version. Include a tiny multiplier whose omission invalidates a cut. Measure allocated bytes, exact arithmetic time and root wall time. |
| 20 | **Stop unproductive cut families earlier.** Extend the existing adaptive policy with per-family deterministic work and conservative bound improvement across successive rounds. Trial one predeclared stop rule. | Keep explicit user-requested cut modes as controls. Measure root time, incumbent and bound at fixed work, and time to proof on lot sizing, assignment, `bell5`, `mod008` and unseen families. Protect the 24-node lot-sizing result. Reject a rule that buys cheaper roots by increasing total proof time. |
| 21 | **Reuse immutable LP data when appending cut rows.** `Lp::extend_rows` rebuilds through `Self::new`. Trial reuse of unchanged coefficient/index data during append, preserving new scaling, logical variables and a freshly validated factorization. | Compare with full reconstruction at every cut round on a fractional root trace. Exercise changed row scales, pruned rows and incompatible column maps. Fall back to rebuilding when equivalence cannot be established. Measure bytes copied, extension time and full root time. |

### V. Improve incumbent discovery and proof throughput

| Iteration | Single change to try | Experiment and decision |
|---:|---|---|
| 22 | **Move between related nodes using a common-ancestor trail.** `NodeBox::enter` currently returns changed columns to root and walks the full target path. Undo to the common ancestor, then apply the suffix. Track propagated/reduced-cost changes with their actual owner. | Replay captured child, sibling and distant best-bound jumps on `bell5`, `noswot` and the nodecopy fixture. Compare every active row/column domain, incumbent, bound, nodes and pivots at fixed work. Count bound writes per node, including repeated branching on one variable. |
| 23 | **Compress pending basis snapshots.** Store nonbasic states packed in a bounded representation while retaining full basic identities and pricing weights. Checkpoint count and the existing snapshot memory ceiling must include all metadata. | Stress deep and wide frontiers with tight memory caps. Verify round-trip and fallback restores, then measure bytes/live node, peak RSS, restore time and extra pivots. More retained bases are valuable only if net time or coverage improves. |
| 24 | **Cache structural scores for scalable strong branching.** Move the invariant column score calculation in `strong_branch` out of repeated probes. Key it to the actual matrix after root cuts and presolve. | Compare the candidate order, directional probes, work charged and selected branches at fixed work with scalable branching enabled. Test a changed cut matrix and a default-arm control. Measure time per probe and full solves. |
| 25 | **Prioritize reliable directional probes.** On the existing reliability brancher, allocate the same global probe/pivot allowance first to the uncertain direction whose outcome can still change the best candidate. Preserve an unknown outcome on a truncated probe. | Test `bell5`, `mod008`, `noswot`, GAP and general integers. Compare complete/aborted probes, dual bound, time to first point and time to proof. Never convert a partially solved probe into a valid bound or an infeasibility claim. |
| 26 | **Try a bounded mixed-integer root dive.** At the first fractional root, fix a small subset of discrete columns guided by LP fractionality and existing pseudocosts, leave continuous columns free, then reoptimize. Release all local fixings on exit. | Lead with no-start `vpm1/vpm2`. Compare against defaults, existing root heuristics, RENS and an equal-work dive control. Give both solvers the same checked start in a separate proof-only diagnostic. Advance on time to a verified first point and primal integral at equal total effort. |
| 27 | **Change open-node selection after incumbent stagnation.** Trial a deterministic, bounded burst of best-estimate nodes, followed by ordinary best-bound search. Keep a separate minimum valid bound over the entire live frontier. | Test `mod008` and `bell5` with and without a checked start, and `noswot` at 250/1,000/150,000 nodes. Measure proof time, bound integral, first/best incumbent and frontier size. Include an interrupted burst where the selected node does not own the global bound. |

### VI. Scale the useful work

| Iteration | Single change to try | Experiment and decision |
|---:|---|---|
| 28 | **Reuse neighborhood scratch in packing repairs.** Keep per-worker resource/column marks and touched lists across `packing::neighborhood_seed` calls. Preserve seed, task order and conflict checks before merging repairs. | Compare exact neighbourhood memberships, repair deltas and the checked incumbent at equal task budget on ring/planted packing and SpOC matching I/II when available. Measure tasks/s, time to score and memory at 1/4/8 workers. Check cancellation between neighborhoods. |
| 29 | **Balance dense numerical work by estimated operations.** Trial work-weighted row chunks for the parallel dense trailing update inside the existing local execution pool. Retain serial execution below measured crossover. | Sweep 1/2/4/8 numerical workers on several dense-tail sizes and irregular LPs, with one outer solve. Compare with old chunks at the same width. Record throughput, full-solve latency, worker utilization and determinism. Do not time thread-local kernel profiles as multi-worker totals. |
| 30 | **Schedule skewed solve batches using a model-aware cost estimate.** In `batch.rs`, extend the current size estimate with cheap signals such as integrality count and row-degree distribution. Freeze the estimator on development data and preserve result order. | Compare the existing size sort and the new order on homogeneous, skewed, mixed LP/MIP batches and shuffled arrival orders. Use fixed total core budgets (outer × inner), report jobs/s, makespan, p95 latency, RSS and per-model equality. Then run the integrated release gate below with only retained changes. |

## Release gate and handoff for execution

1. **Numerical gates.** Run locked Rust workspace tests, formatting, all-feature Clippy, release Python tests, native/exact and SciPy oracle suites for both defaults and opt-ins. Recheck candidates against original arrays. Verify min/max bound orientation, Farkas evidence for infeasibility where required, KKT for solved LPs and no cut or domain tightening excluding a small enumerated optimum. Preserve exact stored-double semantics for the default path and the documented behavior of optional decimal equality handling.
2. **Search gates.** Require zero new unsupported/numerical-failure cases on the previously admitted control set. Preserve the documented `noswot` guard recovery at a 150,000-node cap and report earlier incumbents. Test zero time, cancellation, exhausted pivots, missing starts and supplied starts. Report proof of optimality separately from a checked feasible incumbent and from a certified bound.
3. **Integrated A0 comparison.** Rebuild from clean checkouts. Compare baseline defaults against final defaults, and baseline opt-ins against the same final opt-ins. Report changed defaults in a third explicit arm. Repeat the original ten-case/two-second screen, the five generated LPs, the historical ten-second roster and the untouched final roster. Rerun large losses in isolation. Recheck interaction pairs 1+2, 5+7, 8+9, 11+14 and 20+26 before merging their gains.
4. **Application entry points.** Smoke-test Rust, Python, ordered batches and the job worker. Validate original-model results through `lp-ip.evaluate/1` for new admission paths. Existing configurations remain replayable. If an experiment introduces a public setting, carry it through option validation, Python bindings, job discovery, effective-option reporting, docs and tutorial-summary checks.
5. **Execution record.** Deliver one log row per trial with parent/candidate revision, input and binary hashes, primary metric, evidence of the mechanism, paired result with uncertainty, worst regression, correctness result and decision. Save the complete JSON/JSONL journals including timeouts and negative experiments. Publish retained changes as a cumulative patch from the pinned base with apply-and-replay instructions. A run with no promoted changes still produces a usable record of what was tried.

## Reproducing the baseline

Use a checked-out v0.31.0, Linux, Rust 1.100.0 and CPython 3.12.14 to match this assessment. Start with clear `LPIP_*` environment overrides. The command-line LP timing harness uses `auto` and a maximum refactor interval of **256** when the optional arguments are omitted. The separate forced-mode check used **64** in all three arms. The generated family harness disables component decomposition. The fourth positional argument is the component worker cap. Use the parallel matrix driver for explicit numerical widths.

```bash
git checkout --detach 2d6c431d968623877894f350456289ef102f4791
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1
out=$(mktemp -d)
cpu=$(python -c 'import os; print(min(os.sched_getaffinity(0)))')
cargo build --release --locked --manifest-path benchmarks/Cargo.toml --bin performance-native
cp benchmarks/target/release/performance-native "$out/plain"
cargo build --release --locked --manifest-path benchmarks/Cargo.toml \
  --bin performance-native --features kernel-profile
cp benchmarks/target/release/performance-native "$out/profiled"
for pair in 'packing 1024' 'packing 4096' 'network 2048' 'irregular 1024' 'irregular 4096'; do
  read -r fixture size <<< "$pair"
  taskset -c "$cpu" "$out/plain" "$fixture" "$size" warm 1 5 > "$out/$fixture-$size.json"
  taskset -c "$cpu" "$out/profiled" "$fixture" "$size" warm 1 1 > "$out/$fixture-$size-profile.json"
done
```

For the comparison below, use a Python environment with the repository's benchmark requirements and, for the reference arm, gurobipy 14.0.0 with a usable license. Use the existing frozen `evidence/input/manifest.json` from the Gurobi-gap-v2 bundle or regenerate the models with the repository's freeze recipe. The original comparison used the archived arrays. Use the Python executable in that environment for all commands. `stock` and `candidate` intentionally point to **the same fresh release** when comparing settings. During implementation point them to separate staged builds.

```bash
PYO3_BUILD_EXTENSION_MODULE=1 cargo build --release --locked -p lp-ip-craft-python
pkg="$out/release"
mkdir -p "$pkg/lp_ip_craft"
cp python/lp_ip_craft/*.py "$pkg/lp_ip_craft/"
cp target/release/lib_native.so "$pkg/lp_ip_craft/_native.abi3.so"
bench=benchmarks/gurobi-gap
# Replace with the actual location of the frozen manifest.
manifest=/path/to/evidence/input/manifest.json
python "$bench/run.py" --manifest "$manifest" --output "$out/screen.jsonl" \
  --group development blind lp --cases lotsize-233 gap-241 intcover-233 mod008 bell5 vpm1 pp08a adlittle e226 25fv47 \
  --stock "$pkg" --candidate "$pkg" --arms stock:default candidate:adaptive+lattice gurobi:default \
  --repeats 3 --seconds 2 --cpus "$cpu"
python "$bench/audit.py" "$manifest" "$out/screen.jsonl" --verified-journal "$out/screen-checked.jsonl"
python "$bench/run.py" --manifest "$manifest" --output "$out/work.jsonl" \
  --group development blind --cases bell5 vpm1 mod008 lotsize-233 noswot \
  --stock "$pkg" --candidate "$pkg" --arms stock:default candidate:adaptive candidate:lattice candidate:adaptive+lattice \
  --repeats 1 --seconds 20 --nodes 1000 --pivots 100000 --observe --cpus "$cpu"
python "$bench/audit.py" "$manifest" "$out/work.jsonl"
```

The timing harness creates new journals. Use a fresh output path for each replay. The independent exact-cutoff probe below uses the same input and package. It verifies the starter using the exact binary values of the supplied coefficients and checks the final point against the original arrays. Time-limited results can vary between hosts.

```bash
PYTHONPATH="$pkg:$bench" python - "$(dirname "$manifest")/models/blind-pp08a.json" <<'PY'
import json, sys
from fractions import Fraction as F
from run import checked_point, native

with open(sys.argv[1]) as f:
    model = json.load(f)["model"]
settings = dict(seconds=3, nodes=None, pivots=None, observe=False)
def solve(spec, start=None):
    return native(spec, settings, "default", start)["result"]

lo, hi, kinds = model["lower"], model["upper"], model["kinds"]
assert model["sense"] == "minimize" and model["objective_offset"] == 0
assert all(b == 0 for b in lo) and min(model["objective"]) >= 0
fixed = dict(model, lower=[u if k == "integer" else l for l, u, k in zip(lo, hi, kinds)],
             kinds=["continuous"] * len(kinds))
first = solve(fixed)
assert first["status"] == "optimal"
start = first["candidate"]
x = list(map(F, start))
assert all(l <= z and (u is None or z <= u) and (k == "continuous" or z.denominator == 1)
           for z, l, u, k in zip(x, lo, hi, kinds))
for i, (l, u) in enumerate(zip(model["row_lower"], model["row_upper"])):
    activity = sum(F(model["values"][k]) * x[model["indices"][k]]
                   for k in range(model["indptr"][i], model["indptr"][i + 1]))
    assert (l is None or l <= activity) and (u is None or activity <= u)
cost = list(map(F, model["objective"]))
upper = sum(a * z for a, z in zip(cost, x))
assert upper == 18100
caps = [float(upper / a) if u is None and a > 0 else u for u, a in zip(hi, cost)]
assert all(u is not None or a == 0 or F(cap) >= upper / a
           for u, a, cap in zip(hi, cost, caps))
result = solve(dict(model, upper=caps), start)
check = checked_point(model, result["candidate"])
assert check["feasible"] and check["objective"] == result["objective"]
print(upper, result["status"], result["nodes"], result["bound"], check)
PY
```

**Source and replay references.** [LP harness, replay and oracle instructions](https://github.com/dietmarwo/LP-IP-craft/blob/2d6c431d968623877894f350456289ef102f4791/benchmarks/README.md) · [MIP input freeze, solver staging and auditor](https://github.com/dietmarwo/LP-IP-craft/blob/2d6c431d968623877894f350456289ef102f4791/benchmarks/gurobi-gap/README.md) · [LocalMIP review](https://github.com/dietmarwo/LP-IP-craft/blob/2d6c431d968623877894f350456289ef102f4791/docs/local-mip-pilot/REVIEW.md) · [Parallel reproduction](https://github.com/dietmarwo/LP-IP-craft/blob/2d6c431d968623877894f350456289ef102f4791/docs/parallel-028/REPRODUCE.md) · [Conservative dual bounds](https://github.com/dietmarwo/LP-IP-craft/blob/2d6c431d968623877894f350456289ef102f4791/docs/safe-dual-bounds.md)
