# Closing LP-IP-craft’s MIP performance gap with Gurobi

**Implementation and benchmark plan · 27 November 2026**  
**Baseline** — LP-IP-craft 0.30.0 Python wheel and Gurobi 14.0.0. **Implementation base** — LP-IP-craft `bcc4af35c66c2afe00066f50d736b2a06da48c72`. Build the pinned source as a second baseline. It includes the reviewed, opt-in LocalMIP pilot. Gurobi serves as an optional benchmark reference. The production solver remains native Rust.

## 1. Engineering decision

Start with **selective root processing**. The existing dual presolve and Gomory mixed-integer (GMI) and mixed-integer rounding (MIR) cuts already turn two ten-second timeouts into solves. The immediate opportunity is to capture those gains automatically at a cost easy models can afford. Then investigate general-integer presolve on `flugpl` and the cost of closing the bound on `bell5`. There is little evidence from this comparison for making an LP kernel rewrite or wider parallel search the first investment.

The intended outcome is a reproducible Gurobi comparison and **one native policy that works without knowing the instance name**. A faster tuned setting for a single model is a diagnostic result. Promotion to the default depends on untouched models and independent numerical checks.

## 2. What we measured

Eight MIPs from the repository’s nine-case development manifest, one thread, a ten-second limit, and three runs per original model. Both solvers targeted full optimality under numerical tolerances. Candidate feasibility and objective were rechecked on the original arrays. Times exclude model construction and include the respective Python solve call. A limit in the table means the solver returned without closing the gap. Native and Gurobi node counters have different conventions and are best read as indicators of search effort.

| MIP | Native default, median | Gurobi, median | Diagnostic use |
|---|---:|---:|---|
| `bell5` | Limit, ~314,000 nodes | 95.6 ms, 897 nodes | Native found the optimal incumbent in all three runs. Isolate proof cost. |
| `p0548` | Limit, incumbent 26,667 | 20.5 ms, 1 node, optimum 8,691 | Both the incumbent and bound need attention. |
| `gt2` | 1.475 s, 38,867 nodes | 4.39 ms, 1 node | Compare the bounds after plain LP, presolve, cuts and root search. |
| `flugpl` | 235 ms, 25,081 nodes | 0.43 ms, 0 nodes | Inspect general-integer equalities and bounds. |
| `egout` | 108 ms, 3,245 nodes | 2.75 ms, 1 node | Earlier native runs favored presolve alone. |
| `rgn` | 315 ms, 7,249 nodes | 17.7 ms, 1 node | Check bound strengthening on another root-solved model. |
| `lseu` | 232 ms, 8,681 nodes | 102 ms, 129 nodes | Detect cases where smaller trees cost more time. |
| `p01` | 1.13 ms, 1 node | 1.15 ms, 1 node | Control for avoidable root overhead. |

The published native wheel reached **6/8** within ten seconds and Gurobi reached **8/8**. With `dual_presolve=true`, `gmi_cuts=true`, `mir_cuts=true` and `mip_cut_rounds=16`, native reached 8,691 on `p0548` in **0.105 s / 49 nodes** and closed `bell5` in **2.43 s / 52,495 nodes**. This setting was tested on those two cases only. Both solvers solved all eight standalone LP relaxations to matching objectives. On six additional Netlib LPs both succeeded. Native/Gurobi medians on `e226` and `25fv47` were 4.54/4.41 ms and 167/95 ms.

### Follow-up diagnostic for choosing experiments

I ran Gurobi’s presolve and then three separate solves per model on the same frozen arrays. Each solve used one thread and a two-second cap. All twelve solves reported the known optimum. Dimensions are rows × columns from an explicit `Model.presolve()` call. Each switch changes one setting from automatic. A switch can change later decisions throughout the solver.

| Case | Original → presolved | Default nodes | `Presolve=0` | `Cuts=0` |
|---|---|---:|---:|---:|
| `flugpl` | 18 × 18 → 0 × 0 | 0 | 1 | 0 |
| `gt2` | 29 × 188 → 28 × 173 | 1 | 1 | 1 |
| `p0548` | 176 × 548 → 117 × 365 | 1 | 1 | 395 |
| `bell5` | 91 × 104 → 34 × 56 | 897 | 2,292 | 1,013 |

`flugpl` gives a concrete presolve target, although Gurobi still solves it at the root with presolve disabled. `p0548` is the best test for selective cuts. A one-node result on `gt2` survives either individual switch, so it warrants a combined-switch experiment and inspection of incumbent and bound trajectories. These observations establish no particular proprietary reduction or branching rule. [Gurobi MIP log semantics](https://docs.gurobi.com/projects/optimizer/en/current/concepts/logging/mip.html)

## 3. Benchmark contract

**Freeze three comparable baselines.** Save the wheel, pinned source release build and Gurobi version, executable/wheel hashes, input hashes, effective options, seeds, CPU and thread affinity. Record source baseline versus wheel separately. The initial run used native `component_threads=1`, no node or pivot cap, default decomposition, and one numerical thread. Gurobi used `Threads=1`, both MIP gap tolerances zero and feasibility, integrality and optimality tolerances of `1e-7`. Preserve objective constants, integer kinds, signed bounds and equality/ranged-row semantics. Share original arrays between adapters. Keep any diagnostic MPS-import results separately identifiable.

| Cohort | Cases and role |
|---|---|
| Development | The eight MIPs above, `avgas` LP control, all eight relaxed MIPs and six Netlib LPs. Use for mechanism diagnosis and setting thresholds. |
| Known regression cases | Keep the existing 11-case public holdout as a regression suite. In particular retain `2171`, `issue-2173`, `sp150x300d`, infeasible cases and the historical unsupported-domain case. Previous reports already inspected these inputs. |
| Blind decision set | Before tuning, freeze at least 24 models with hashes, families and expected status. Include new seeds from at least four existing generator families and at least eight eligible public instances outside the previous pilots, covering ≥3 distinct public model families. Freeze the inclusion rule, size/license eligibility and exclusions. Keep related variants in the same split. Publish results even if the target sample cannot be assembled. |
| Application | Full SpOC4 matching-I and matching-II, identical original tables and starts; include existing restricted slices as import/solution checks. Full-table sizes are 25,000 and 92,103 variables. The available Gurobi license rejected both with error 10010. |

**Time and work.** Use 10 s for direct comparison, and a separate 60 s run on survivors and all previously timed-out cases. Collect snapshots at 1, 5 and 10 s during the same run. Pair builds on one machine, alternate order, warm up imports, and use fresh solver models. Use three runs for screening and five rotated repetitions of finalists. Separate instrumented traces from headline timings and report median plus range. For very fast cases, batch timings over fresh solves and also report absolute milliseconds. Parallelize independent screening jobs on separate cores if useful. Run final comparisons without competing jobs. Add an equal-node or equal-pivot diagnostic when testing a cause; charge probes, cuts and sub-MIPs to their actual budgets.

**Measures.** Report solved count; time to first checked incumbent, best checked incumbent and valid global bound at each checkpoint; time to 1% and 0.1% gap and to final numerical optimality; search nodes, auxiliary LP calls, pivots, peak memory and root wall time. Record no-incumbent, missing-bound, numerical failure and unsupported statuses explicitly. For minimization, a bound must lie below the reference optimum within tolerance; reverse the inequality for maximization. For a zero incumbent, report the absolute gap. Keep raw root LP objective, valid root bound and whole-tree frontier bound in separate fields. For anytime ranking, reuse the checked primal-integral scorer and failure sensitivity from the LocalMIP pilot.

**Direct measure of the gap.** For a fixed cohort, define `t` as median time to independently checked optimal termination and charge `2T` to an unsolved/error case at cap `T`. Report solved counts and failures alongside this penalized measure. For each model use `r = (t_native + 0.010)/(t_gurobi + 0.010)` in seconds, and report the geometric mean of `r`. Where the baseline geometric mean `R₀ > 1`, report the fraction of the original log gap closed, `1 − log(R_new)/log(R₀)`. Predeclare the cohort. For missing licenses show coverage and compute the ratio only for models runnable by both; retain the other models in native-versus-native reporting. Use case-level paired intervals on the blind set, grouping related generator variants by family in a sensitivity analysis. Freeze the candidate and the best fixed policy before opening the blind results. Use a fresh confirmation set if those results inform another change.

As a worked example, the eight measured development MIPs give **R₀ = 22.69** under this precise 10 s scoring rule. Closing 30% of that log gap would bring the score to **8.89**. This is a shifted, timeout-penalized summary of these eight cases. The blind cohort gets its own baseline.

## 4. Implementation sequence

### W0 · Make the comparison reproducible

1. Extend the existing `benchmarks/rust-solvers` runner with an optional Gurobi adapter. Use one canonical model-loading and candidate-checking path. A machine without Gurobi can run the full native/HiGHS regression. Add a smoke case for sense, offset, ranged rows and mixed variable types.
2. Replay the table, the default and the fixed combined profile (three cut/presolve flags enabled together with 16 rounds), and the wheel/source comparison. Specifically reconcile the older `bell5` result where combined cuts exhausted one million nodes. Match build, limits, decomposition, presolve and the four options before attributing differences. Archive any discrepancy with both configurations.
3. Extend the optional diagnostic path with pre/post presolve dimensions and fixings, accepted/rejected cuts and exact work by family, root LP/probe/heuristic time, first certified pruning incumbent, and a timestamped global-bound trace. Retain node-at-entry and node-at-exit counters for auxiliary LPs. Collect the corresponding Gurobi log/callback fields where available; a root outcome can occur before a callback supplies the desired observation.
4. **Exit condition.** All original feasible points pass the shared checker, known optima match, statuses and model hashes reconcile. For the nine-case reference replay, list every wheel/source divergence. A missing old binary or historical trace affects attribution of that result only.

### W1 · Measure the value of each existing root action

1. Run the full `2 × 2 × 2` factorial for dual presolve, GMI and MIR on the eight development MIPs with up to 16 cut rounds. Keep model decomposition, heuristic settings, tolerances and budgets fixed. At three repetitions and 10 s, the initial eight-model factorial has 192 native calls and up to 32 solver-minutes before setup and cooperative overrun. For the best families test ceilings of 1, 2, 4, 8 and 16 rounds. Capture gain in valid bound per round, separation time, exact arithmetic work, added nonzeros, pivots, incumbent quality and final solve time.
2. On `bell5` and `p0548`, compare a cold solve with a separate diagnostic that starts from the same verified optimum in both solvers. Never use the diagnostic as a score for the cold competition. A large residual search with the optimum supplied quantifies the cost of closing the bound. For `gt2` add the Gurobi paired switch `Presolve=0, Cuts=0` and a documented no-heuristics diagnostic if the cause remains unclear.
3. Map each development case to the cheapest successful action after counting selection and setup overhead. This hindsight table is an upper bound on the value of perfect choice. Compare the best *single fixed* setting, tuned on development and frozen, against the adaptive candidate later. If the fixed setting does equally well on blind inputs, prefer its simpler implementation.
4. **Exit condition.** Identify at least one model where extra work pays and one where it regresses or buys no measurable improvement. Distinguish an incumbent gain from a stronger bound, a smaller tree and a lower wall time.

### W2 · Implement a bounded adaptive root policy

1. Start with an opt-in setting on the general native MIP path. At entry use cheap existing information such as the number of unfixed integer variables, eligible variable-bound rows, and changes from ordinary propagation. After an accepted LP, add integer fractionality and whether an admissible incumbent exists. Use thresholds fixed on development. Model names, hashes, known optima and Gurobi traces are unavailable to the policy.
2. Compare three progressively more expensive actions: finish with the existing root, allow a short single-family separation, or continue productive rounds with both families. Give the initial trial a deterministic work allowance. Record eligible rows, fraction of candidates accepted, added nonzeros, objective progress and conservative bound progress where available. Stop on exhaustion, repeated low gain or excessive fill. Treat an unavailable conservative bound explicitly. Calibrate work thresholds offline; use wall time only to honor the caller’s deadline.
3. Integrate with `root.rs` and `rootcuts.rs`. The current code already drops families that produced no selected cut, checks stagnation, limits dense supports, extends the LP after adding rows and prunes slack cut rows. Reuse those mechanisms. Trace whether the transition to the main search rebuilds or resolves the LP and count the cost. Reuse a basis only when row/column ordering, bounds and the active model match. Preserve the existing checked fallback.
4. Reserve resources for the tree before admitting new root work. Charge all presolve, separation, reoptimization and heuristic subcalls to the parent. Test cancellation mid-round and nested sub-MIPs. Give documented explicit user controls precedence; verify a disabled policy reproduces the previous status, objective, bound and work counts in deterministic fixed-work runs.
5. **Exit condition.** One policy solves all eight development MIPs within 10 s with no per-model override. Diagnose any loss against the frozen fixed setting and check end-to-end time on `p01` and `lseu` before enabling additional work.

### W3 · Close the general-integer presolve gap on flugpl

Inspect the original `flugpl` equations first. There are six periods, eleven integer variables, and no binaries. Several balance rows link consecutive staffing variables with a coefficient entered as `0.9`. The current native presolver already propagates exact/mixed rows and probes binaries. A larger binary-probing budget cannot directly explore these eleven general-integer domains.

1. Dump bounds and unfixed columns after each existing reduction. Produce a tiny two- or three-period witness for the first useful implication the native propagator misses. Enumerate it independently. Also test the always-on and opt-in native reductions before adding another.
2. Try one general rule at a time. First propagate fixed columns and two-variable equalities to a fixed point. Next test a sparse equality substitution with an exact inverse and restricted fill. Then test bounded integer divisibility or residue tightening on rows whose continuous terms have been eliminated or rigorously bounded. Select the first rule with measurable incremental value.
3. Treat the imported binary64 coefficients as authoritative. For example, a factor of ten suggested by `0.9` needs a bounded rounding-error enclosure and a proof that each excluded integer value is infeasible under the applicable tolerance. Skip the reduction if the proof or overflow check fails. Preserve substituted bounds, objective offset, integrality and a reverse map for starts and reconstructed solutions.
4. Stress with reordered rows/columns, perturbed coefficients, scaled rows, negative bounds, a continuous column that blocks integer inference and near-equalities. Check all feasible small points for feasibility-preserving rules; for objective-preserving fixings verify the optimum and a reconstructed optimal witness in both senses.
5. **Target.** Reduce `flugpl` from 25,081 nodes to **≤250**, ideally to zero, and gain on at least one independently generated general-integer family. Ship a bounded rule only when its proof and transfer result survive the blind test.

### W4 · Reduce the work remaining after the root

For `bell5`, start with W1’s optimal-start diagnostic and compare presolved dimensions and bound trajectories. For `gt2`, start with the cheapest root profile delivering a reliable gain. Use `egout`, `rgn` and `lseu` to test transfer across models.

- If bounds remain weak immediately after presolve, use W1’s traces to test one targeted cut improvement. Verify each emitted inequality by the existing certified-cut path. Track separation cost and fill.
- If the root bound is competitive but a large frontier persists, compare existing pseudocost branching, reliability probing, and its bounded scalable variant with identical root state. Limit candidates, pivots and total work. Measure pruned children, global bound at equal work and time to close the gap. Treat a probe stopped by a work limit as inconclusive unless it yields an admissible bound.
- If the optimal-start run is fast while the cold run lacks a good point, trial one work-capped invocation of existing rounding/diving, feasibility pump or RENS. Check time to first point, incumbent quality integral and *final* outcome. Recent LocalMIP revision 2 had a held-out additive primal-integral ratio of 0.9999 and unchanged 30-second coverage. Keep its default effort at zero in this campaign.
- **Targets.** On the comparison host, aim for `gt2` ≤0.25 s and ≤4,000 reported native nodes; `bell5` ≤1.5 s and ≤30,000. For `p0548`, target ≤0.20 s including policy overhead, then try to beat the 0.105 s fixed profile. These are engineering targets. A smaller tree that takes longer is a negative timing result.

### W5 · Validate, decide and package

| Decision | Proposed measurable gate |
|---|---|
| Numerical correctness | Zero new false optimal/infeasible statuses, invalid bounds or rejected advertised incumbents across the frozen public, exact-enumeration, random and application suites. Check original objective and constraints for every saved candidate. Record existing unsupported inputs separately. |
| Direct Gurobi comparison | Reach 8/8 on the development MIPs at 10 s. Target at least 30% closure of the baseline mean log runtime gap on the license-eligible blind cohort when `R₀ > 1`. Publish Gurobi absolute times, coverage and intervals. |
| Default promotion | On the blind set, target ≥15% improvement in native penalized runtime against pinned stock, with no loss in solved count and a paired 95% upper ratio bound below 1.00. Compare with the best frozen static profile. Flag any case losing both >25% and >20 ms and inspect it before promotion. With inconclusive evidence, deliver the policy opt-in and report the uncertainty. |
| Easy cases and continuous LPs | Target ≤1 ms added median time on `p01`; ≤5% aggregate regression on the six Netlib controls using the 10 ms shift. Confirm the generic MIP policy is bypassed for LP-only calls. |
| SpOC4 transfer | Recheck both full native incumbents against original transfer conflicts and integer-thousandth scores. In a fixed-work replay of the published eight-round recipe recover at least 32,847.565 / 70,945.093 for matching-I/II and report safe bounds separately. At equal wall budget compare score, bound and time to a given score. Pair single-worker runs, then compare the same four-core allowance with native residual workers and four Gurobi threads when available. A licensed Gurobi run on the same full tables is the requirement for a competition-scale speed claim. |

Run the existing Rust/Python regression, exhaustive native oracle, SciPy reference and gap-limit tests. Add targeted tests for each new mathematical rule and policy decision, including tied choices, both objective senses, starts that are transformed by presolve, unknown/infinite bounds, time limits and disabled-path equivalence. Exercise Rust, Python and the job worker if exposing a production option; update option validation, job discovery schema/examples, effective-options reporting and API documentation together. Use an uninstrumented release build for final timings.

Execute W0–W2 first and record that decision before increasing solver complexity. W3 can proceed once W0 reproduces the presolve symptom. Start W4 only on losses that survive the accepted root policy. If the LP controls or matched-work profiles show that basis solves dominate remaining end-to-end time, open a separate LP kernel experiment using `25fv47` and the existing irregular-LU replay. The single-host campaign can run now. Multi-machine and 16-core scaling can follow once a serial algorithmic gain is established.

**Execution deliverables** — an apply-clean patch against the recorded SHA; the native and optional Gurobi adapters; frozen cohort manifest and runnable commands; raw machine-readable settings, events and independently checked results; and a Markdown implementation log with the gate table, worst regressions, failed experiments and the proposed default. Record unavailable licenses and timeouts in that log. Keep reference solver libraries outside the production dependency graph.

## Source map

| Work | Existing entry points |
|---|---|
| Benchmarking and independent checks | [Comparison harness and frozen families](https://github.com/dietmarwo/LP-IP-craft/tree/bcc4af35c66c2afe00066f50d736b2a06da48c72/benchmarks/rust-solvers) · [Original-model checks](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/crates/core/src/check.rs) · [Gurobi callback codes](https://docs.gurobi.com/projects/optimizer/en/current/reference/numericcodes/callbacks.html) |
| Presolve and root policy | [Root dispatch and postsolve](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/crates/native/src/root.rs) · [MIP presolve](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/crates/native/src/mip_presolve.rs) · [flugpl fixture](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/docs/packing-mip/corpus/flugpl.mps) |
| Cuts, branching and heuristics | [Root cut loop](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/crates/native/src/rootcuts.rs) · [Certified cuts](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/crates/native/src/certified_cuts.rs) · [Branching and admission](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/crates/native/src/lib.rs) |
| Earlier evidence and regressions | [Root and cut ablations](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/docs/ripsolve-027/RESULTS.md) · [LocalMIP revision 2](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/docs/local-mip-pilot/RESULTS.md) · [Full SpOC4 results](https://github.com/dietmarwo/LP-IP-craft/blob/bcc4af35c66c2afe00066f50d736b2a06da48c72/docs/spoc4-026/RESULTS.md) |
