# Learning from ripsolve: a plan to close LP-IP-craft's MIP gaps

17 October 2026 · LP-IP-craft `main` 825ee42 (native 0.26) · ripsolve 0.4.0 (MIT, pure Rust)

## Goal

On the public MIP set, the native solver is an order of magnitude behind HiGHS and clearly behind ripsolve:

| Solver | Solved within 60 s | Shifted geometric mean |
|---|---:|---:|
| HiGHS | 9/9 | 0.08 s |
| ripsolve | 8/9 | 0.23 s |
| native | 8/9 | 0.66 s |

The worst cases:

| Instance | native | ripsolve | HiGHS |
|---|---|---|---|
| p0548 | no incumbent in 60 s | 0.07 s | 0.09 s |
| gt2 | 3.6 s | 0.22 s | 0.05 s |
| egout | 0.57 s | 0.05 s | 0.02 s |
| bell5 | 18 s | stops at the limit | 0.75 s |

This plan takes the ripsolve components that measurably explain its lead and rebuilds them inside native's correctness contract:

- certified-only pruning;
- outward-rounded bounds;
- an independent check of every candidate.

Two areas are out of scope:

- **Large degenerate LPs**, such as the SpOC4 relaxations. ripsolve is not built for them: it targets about 1,000 rows, and neither it nor microlp solved the 25,000-column matching-I relaxation in 60 s. That gap belongs to an interior-point or PDLP plan. On that LP, HiGHS IPM takes 5.9 s against 428 s for the native simplex.
- **The SpOC4 packing repair**, which already leads every solver tested.

## What the measurements say

### Root bounds decide most of the gap

Each table lists the root bound for a minimization or maximization instance at successive stages:

- **Plain LP:** the LP relaxation as given.
- **Native root:** native's root LP after its own preprocessing (`diagnostics.mip_search` root record).
- **ripsolve presolve:** ripsolve's root bound with cuts disabled. That run still includes its restart after reduced-cost fixing.
- **ripsolve presolve + cuts:** ripsolve's default root after cut rounds.

**Root bound**

| Instance | Optimum | Plain LP | Native root | ripsolve presolve | ripsolve presolve + cuts |
|---|---:|---:|---:|---:|---:|
| p0548 | 8,691 | 315.3 | 429.7 | 4,571 | 8,691 |
| egout | 568.10 | 149.59 | 367.09 | 511.62 | 567.75 |
| gt2 | 21,166 | 13,460 | 13,460 | 13,461 | 20,726 |
| lseu | 1,120 | 834.7 | 834.7 | 945 | 1,017 |
| bell5 | 8,966,406 | 8,608,418 | 8,608,418 | 8,608,418 | 8,943,663 |
| rgn | 82.20 | 48.80 | 48.80 | 48.80 | 54.78 |
| flugpl | 1,201,500 | 1,167,186 | 1,167,186 | 1,167,186 | 1,172,835 |

**Share of the LP-to-optimum gap closed at the root**

| Instance | Native | ripsolve |
|---|---:|---:|
| p0548 | 1% | 100% |
| egout | 52% | 99.9% |
| gt2 | 0% | 94% |
| lseu | 0% | 64% |
| bell5 | 0% | 94% |
| rgn | 0% | 18% |
| flugpl | 0% | 16% |

Native's node throughput is not the problem. On egout native explores about 22,000 nodes per second; ripsolve with every component off (`bare_bb`) explores about 10,500. The gap is the bound native starts the tree with.

### Which ripsolve components carry it

Each run below switches one component off, with a 20–30 s limit and gap tolerance 0. Cut families were switched off in a locally patched copy of ripsolve.

| Component off | p0548 | egout | gt2 | bell5 | lseu |
|---|---|---|---|---|---|
| none (default) | 3 nodes, 0.08 s | 32 nodes, 0.05 s | 197 nodes, 0.22 s | limit, root 8.944M | 4,428 nodes |
| presolve | 5 nodes, 0.23 s | 2,134 nodes, 0.53 s | 227 nodes | limit | 2,828 nodes |
| all root cuts | **limit**, root 4,571 | 98 nodes, 0.01 s | 6,000 nodes, 0.95 s | limit, more nodes | 11,038 nodes |
| presolve and cuts | **limit**, incumbent 24,163 | **72,008 nodes, 9.7 s** | **limit** | limit | 19,486 nodes |
| Gomory cuts | root 8,522, 224 nodes | root 511.6 | root 17,186 | root 8.608M (no cut progress) | no change |
| MIR cuts | root 8,724, 13 nodes | 16 nodes | 984 nodes, 1.1 s | small change | 2,800 nodes |
| covers, cliques, mod-2 or implied-aggregation cuts | no change or small change | no change | no change | no change | covers and mod-2 *reduce* nodes |
| primal heuristics | still optimal | no change | 1,310 nodes | worse incumbent | 10,424 nodes |
| CDCL, conflict analysis, aggregation, root perturbation, node propagation | no measurable effect | | | | |

Conclusions, in order of value:

1. **Gomory mixed-integer (GMI) cuts from the optimal tableau** carry the cut gains on p0548, gt2, bell5, rgn and flugpl. Native has none.
2. **Presolve** carries egout and half of p0548: 316 → 4,571 before any cut, and 150 → 512 on egout. The reductions are coefficient tightening of big-M rows, bound propagation, forcing rows and singleton dual fixing. Native has integral-row rounding and root bound inference, but no coefficient tightening.
3. **MIR with variable-upper-bound substitution** is second on gt2 and p0548.
4. **Primal heuristics** are worth 2–6× in nodes on gt2 and lseu. Native's p0548 run never found an incumbent.
5. **Branching on general integers.** ripsolve's `bare_bb` (pseudocosts, best bound, nothing else) solves gt2 in 6,631 nodes. Native's default needs 72,182, and 16,056 with reliability branching. Native's branching choices are weak on this model, independent of cuts.
6. Covers, cliques, mod-2 and CDCL show no gain on these instances. They wait.

### A warning: ripsolve's optimality claims can be wrong

With `restart_share = 0` (documented as "never restart"), ripsolve 0.4.0 stops after one node and reports `Optimal` with an open gap:

| Instance | Reported objective | Reported bound | True optimum |
|---|---:|---:|---:|
| lseu | 1,165 | 1,017 | 1,120 |
| bell5 | 8,992,602 | 8,943,663 | 8,966,406 |
| rgn | 82.2 | 54.78 | 82.2 |

The result is deterministic. Learn mechanisms from ripsolve, not its trust model: every borrowed reduction or cut passes native's own validity tests, and results keep native's certified-pruning rule. Report the bug upstream with these reproductions.

## Ground rules

- **Licence.** ripsolve is MIT, like LP-IP-craft, so code may be adapted. Adapted code keeps ripsolve's copyright notice in a `THIRD_PARTY_NOTICES` file and is named in the commit message. The default is to reimplement from the published method, with ripsolve's code as an engineering reference.
- **Correctness-critical code** (cuts, presolve reductions, bound changes) needs:
  - a written validity argument;
  - exhaustive tests on small models (every feasible integer point survives);
  - the existing 6,330-case oracle suite and 400-model bound audit, unchanged.
- **Numerics.** A cut derived in floating point is only accepted in a form that native can certify. Use the numerically safe GMI construction (Cook, Dash, Fukasawa and Goycoolea, 2009): outward-rounded coefficients and right-hand side, with coefficients that are too small relaxed safely rather than dropped.
- **Defaults** change only after a held-out check, as in earlier releases. Each feature ships opt-in first.
- **Measure through one harness.** Commit the comparison harness used for this plan (one binary that builds every solver from the same arrays) under `benchmarks/rust-solvers/`, together with the ripsolve ablation driver. Pin ripsolve to 0.4.0 for comparisons.

## Workstreams

Ordered by expected gain per effort. Time boxes are first estimates; an overrun of twice the box triggers a written re-scope.

### W0 — Measurement baseline (2 days)

- Add root-gap diagnostics to native: plain LP, after preprocessing, after each cut round, plus cut counts per family. They go in `diagnostics.*`, which lets the tables above be regenerated by script.
- Commit the harness, the ablation driver and the current tables as frozen reference data.
- Fix the evaluation sets:
  - **development:** the nine public MIPs, plus egout-like and knapsack-like random families;
  - **held-out:** 15–20 small MIPLIB-style instances not used before. Fetch them from the HiGHS check-instance set on GitHub, then freeze them with hashes.
  - The 6,330-case oracle remains the correctness gate.

**Acceptance:** the root-gap table reproduces from one command; held-out set hashes are recorded.

### W1 — Presolve: coefficient tightening and friends (1 week)

Port the in-place, no-postsolve design. Columns are fixed by bounds and redundant rows are freed, never renumbered, which suits native's model-preserving pipeline. Reductions, each with its own toggle and counter:

1. Activity-based redundancy and infeasibility detection. Native partly has this; unify it.
2. Bound propagation from residual activity, with integer rounding. Native's `infer_bounds` exists; extend it to a fixpoint inside presolve.
3. Forcing rows.
4. **Coefficient tightening** on rows with integer columns: reduce a coefficient that exceeds what the row can use. This is the big-M fix behind egout.
5. Singleton-column dual fixing, plus fixing columns that appear in no row. These are only valid with no separator active, so they are off when cuts run (ripsolve's `presolve_with(dual = false)` rule).

Use outward rounding for every derived bound, as `root::quotient` already does.

**Acceptance:**
- p0548 root ≥ 4,500 and egout root ≥ 500 before any cut;
- exhaustive validity on random 6–12-variable MIPs;
- oracle suite unchanged;
- no held-out regression above 10% in time.

### W2 — Safe GMI cuts from the root tableau (2 weeks)

1. **Tableau access.** Add an `Lp` method returning the tableau row of a basic integer variable. Use the existing `btran_unit` on the factorized basis, then dot it with the nonbasic columns of the bounded-variable form. Account for nonbasic-at-upper columns by complementing.
2. **GMI formula** for mixed-integer rows: continuous columns get the standard mixed-integer rounding coefficients, and integer columns get the fractional-part rule. Skip rows whose basic value is within 1e-6 of an integer, or whose fractionality f₀ lies outside [0.005, 0.995].
3. **Safety.** Compute in floating point, then make the cut valid in outward-rounded arithmetic:
   - relax tiny coefficients onto the right-hand side using the column bounds;
   - reject cuts whose dynamism (max/min coefficient) exceeds 1e6, or that have too many nonzeros.

   Verify each accepted cut against the exhaustive oracle in tests.
4. **Cut loop.** Separate, select and resolve with the dual simplex warm-started from the current basis. Stop when the bound improves by less than 0.1% over 3 rounds, or after 50 rounds. Native already appends root cuts as rows (`cover_cuts`, `root_separation`); extend that loop rather than writing a new one.
5. **Selection** (taken from ripsolve's `cuts::select`): rank by efficacy (violation over norm), drop cuts that are near-parallel to an accepted one, and cap the rows added per round. Age out cuts that stay slack over several rounds before the tree starts.

**Acceptance:**
- root bound with presolve: gt2 ≥ 20,000, bell5 ≥ 8.94M, p0548 ≥ 8,500;
- gt2, egout and p0548 solved in under 1 s;
- no invalid cut in exhaustive tests;
- certified bounds still never exceed the reference optimum on the 400-model audit.

### W3 — MIR with variable-upper-bound substitution (1 week)

This is the second family by the ablation (gt2, p0548). Aggregate a row with the variable upper bounds the model implies, complement binaries, apply MIR rounding, and use the same safety and selection path as W2.

**Acceptance:** measurable node reduction on gt2 and egout, with W2's validity gates.

### W4 — Primal heuristics (1–2 weeks)

Rounding at the root, then guided diving and a feasibility pump. In ripsolve, the pump supplies most solutions on its target family; native has only the opt-in Feasibility Jump. Use adaptive backoff when attempts keep failing, as ripsolve does in-tree. Every candidate goes through native's existing incumbent check, so a wrong heuristic costs time, never correctness. This is the safe surface for experiments.

**Acceptance:**
- an incumbent on p0548 at the root;
- gt2 and lseu nodes down at least 2× with W1–W2 in place;
- SpOC4 unchanged.

### W5 — Branching on general-integer models (1 week)

Explain the 72,182 against 6,631 node gap on gt2 before changing code. Record per node: candidates, pseudocost state and the chosen variable, against ripsolve `bare_bb`'s choices on the same LP solutions. Then test candidate fixes as separate opt-in policies:

- pseudocost initialization from strong-branching probes;
- the reliability threshold;
- the scoring function (product versus weighted sum).

**Acceptance:** an explained cause, and a policy that wins on development and held-out sets, or a recorded negative result.

### W6 — Later, only with evidence

- Restarts after root reduced-cost fixing. ripsolve restarts once 25% of the integer columns are fixed. Its own no-restart path is the buggy one, so study the restart condition carefully.
- Clique and mod-2 cuts, implied-bound aggregation, conflict analysis and CDCL. None moved these instances.
- Shared-pool parallel tree search: ripsolve reports about 8× throughput on 16 threads, against native's disjoint-frontier tree. Only after W1–W5, because it multiplies a strong search rather than replacing one.

## Validation for every workstream

1. Unit tests for each new reduction or cut: exhaustive integer enumeration on random models with 6–12 variables, every feasible point preserved, fixed and free bounds, both senses.
2. The frozen oracle suite (6,330 cases × policies) and the 400-model bound audit. Statuses must be unchanged unless an instance moves from a limit to optimal, and that change must be explained.
3. Public MIPs and the held-out set through the comparison harness: solved count, shifted geometric mean, root gap closed. Report against ripsolve 0.4.0 and HiGHS 1.15.1 at one thread.
4. SpOC4 suite rerun: vectors unchanged unless the packing path is touched.
5. Each feature lands as an opt-in control with `diagnostics.*` counters. Defaults change in a separate commit, with held-out evidence.

## Success criteria

After W1–W4:

- the public set is solved 9/9 within 60 s;
- the shifted geometric mean is no worse than ripsolve's on the development set;
- the held-out set shows no losses beyond 1.5× against the current native.

Stretch goal: within 3× of HiGHS on the public set.

## Upstream contribution

- Report the `restart_share = 0` wrong-optimality result to ripsolve, with the three reproductions above.
- Offer the comparison harness. It gives a neutral way to compare pure-Rust solvers that both projects can use.

## Reproduction (current measurements)

- **Comparison:** `solvercmp <solver> <model.txt> <limit> <x-out>`. Solvers are `native`, `native_rel`, `native_packing`, `microlp`, `ripsolve`, `einheit` and `clarabel`. The HiGHS reference runs through `highs_run.py`. A driver checks every returned point with `lp_ip_craft.evaluate`.
- **ripsolve ablations:** `ablate <model.txt> <limit>` for the option toggles. `ablate2`, with `RS_OFF=cover|mir|clique|gomory|mod2|aggregation`, uses a ripsolve copy patched so each root cut family can be switched off.
- Models are exported from `docs/packing-mip/corpus` and `docs/packing-ftran-mip-diagnostics/corpus/public`.

W0 commits these to the repository.
