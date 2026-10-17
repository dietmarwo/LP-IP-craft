# LP-IP-craft 0.24 — implementation and validation plan

Baseline: accepted 0.23.0 delivery. Requested order: incumbent certification, item 4 reliability branching, item 3 Feasibility Jump, item 5 stronger root relaxation, item 6 mixed-row node propagation. Written before implementation on 2026-10-17.

## Scope and invariants

Implement all five work packages in owned safe Rust, with matching Python controls and metadata. Keep the production native engine free of third-party numerical engines and arithmetic dependencies. Preserve the original model, objective sense/offset, tolerance acceptance, conservative bounds, shared cancellation/time/pivot/node budgets, private solver state and existing component support. Do not push to GitHub. Experimental search policies remain independently selectable; enable by default only when the regression evidence supports it. No general branch-and-cut, conflict learning, parallel tree or parallel FJ in this release.

## A. Mixed-model incumbent certification

Problem: egout, flugpl and rgn accept candidates without obtaining a certified pruning upper bound. Existing inward LP shrinking is insufficient for equality/boundary solutions.

1. Preserve tolerance candidate acceptance and existing outward enclosure certification.
2. Add owned bounded arbitrary-precision rational arithmetic for exact binary64 input values. Test signed arithmetic, division, normalization, cancellation, extreme exponents and outward conversion.
3. Reconstruct a nearby rational witness: fix integers exactly, anchor continuous bounds, solve a bounded independent active subsystem exactly, and verify every original variable bound and row in rational arithmetic. A rounded display point alone is never the proof.
4. Compute a conservative oriented objective upper endpoint from that witness. Enforce dimension, bit-size, attempt and cancellation limits. Unsupported/large/inconsistent systems produce no certificate and retain the existing shrinking fallback.
5. Record witness attempts/successes separately from LP repair attempts. Validate infeasible boundary candidates cannot acquire certificates, and equality solutions with no exactly feasible binary64 representation can be certified through existence of a rational witness.

Acceptance: no relaxed feasibility threshold, no unsafe upper bounds on exact small enumerated models, original model verification, public-case outcomes and limitations documented.

## B. Item 4 — reliability branching

1. Reuse directional pseudocost observations and parent-origin learning.
2. Require a minimum observation count in both directions before trusting a variable. Rank fractional candidates deterministically.
3. Probe only a small set of unreliable candidates, with per-direction pivot limits and a global probe cap across several early nodes. Respect model-size guards, shared work limits and cancellation.
4. Restore the parent bounds and basis after every outcome. Probe estimates guide selection only; no probe status or estimate justifies pruning.
5. Keep the original root-only strong-branching switch available for comparisons. Expose reliability selection, thresholds and counters in Rust/Python.

Acceptance: deterministic policy tests, parent restoration on failed/limited probes, budget/cancel checks, correctness matrix and node/pivot comparisons on gt2, lseu and synthetic MIPs.

## C. Item 3 — bounded serial Feasibility Jump prototype

1. Add deterministic LP-free weighted violation local search, optionally seeded from the root relaxation.
2. Round integral columns within bounds, retain continuous columns, and generate coordinate jumps at incident row breakpoints and variable bounds. Evaluate the full incident-row weighted piecewise-linear violation change.
3. Select improving jumps; at a local minimum increase violated-row weights and apply deterministic diversification/restarts. Keep sparse incidence, incremental row activities and explicit term-evaluation/step caps.
4. Recompute activities periodically. Pass all proposals through the existing original-model acceptance and certification machinery; heuristic arithmetic never proves pruning/infeasibility.
5. Run only a bounded root call initially. Expose enable/effort controls and steps/work/candidate counters. No thread scheduler or new tree sharing.

Acceptance: an infeasible rounded seed is repaired on targeted tests, mixed/equality/negative-coefficient cases remain valid, work/cancel limits hold, no global LP budget is hidden, p0548 plus holdout outcomes reported even if no speedup.

## D. Item 5 — LP-directed exact covers and sequential lifting

1. Preserve static covers as a comparison control. Add a separately controlled root separation pass using an initial root LP point.
2. Restrict proofs to positive integral binary knapsack rows in the existing supported range. Use fractional values to propose minimal covers; check validity in integer arithmetic and actual LP violation before insertion.
3. Sequentially lift outside variables using an exact bounded dynamic program over attainable cut profit with minimum knapsack weight. If a work/profit cap is reached, retain a valid unlifted coefficient zero.
4. Verify generated coefficients independently with the same exact knapsack maximum bound; enumerate small rows exhaustively in tests.
5. Run a small number of root rounds, rebuilding all LP/presolve/propagation caches between rounds. Deduct initial LP work from the shared node/pivot/time budget, preserve dual/postsolve dimensions and original-model checks. Never mutate a live search matrix.

Acceptance: exhaustive cut validity, planted violation and root-bound improvements, no stronger-bound claims on unsupported rows, total work accounting, public/synthetic comparisons including regressions.

## E. Item 6 — reversible mixed-row node propagation

1. Retain the exact i128 integer-row propagator. Add a mixed-row queue for remaining finite-box rows.
2. Recompute outward-rounded prefix/suffix activities to exclude one term without unsafe cancellation. Derive outward quotient bounds, round only integer targets inward to their integer domain, skip nonfinite/unsupported arithmetic.
3. Apply only provable tightenings/infeasibilities. Record row reasons, old/new endpoints and node-local changes. Restore all implications when entering siblings using NodeBox's existing changed-column contract.
4. Maintain bounded incident-row queuing and work/cancellation polling. Alternate with exact integer propagation only within a fixed work cap; reaching the cap preserves sound partial deductions.
5. Expose a switch and work/change/infeasibility counters; do not introduce conflict learning.

Acceptance: enumerated feasible points survive mixed propagation, continuous/negative/tight/equality/extreme cases, sibling restoration, cancellation and work caps, diagnostics coverage, on/off equivalence and public-case comparisons.

## Validation and delivery

- For each stage: focused meaningful Rust/Python tests, formatting, warnings-as-errors Clippy; retain logs and stage snapshots/patches.
- Final: full Rust all-features checks, full Python tests, frozen oracle policy matrix and unchanged adjudications, exact finite-bound audit. Add new policy combinations to the same small exact oracle corpus.
- Compare accepted 0.23 executable with each policy and the integrated candidate on the same public models. Use fixed node/pivot/time caps; record statuses, objective/checks, conservative bounds, first accepted/certified incumbent, nodes/pivots, root LP, policy work and source/binary hashes. Repeat headline timing comparisons serially; do not time concurrently with builds/tests.
- Retain failures/regressions, avoid claiming universal improvement, choose defaults based on evidence. Document capability/effort limits explicitly.
- Update API/README/version, record exact commands, include full source, detailed results, original/final plan, patches, input hashes and raw validation data in a checksummed downloadable ZIP. Save the plan and ZIP for download.

## References

Algorithm references guide mechanisms; implementation and correctness checks remain owned.

- Achterberg, Koch, Martin (2005), Branching rules revisited: https://doi.org/10.1016/j.orl.2004.04.002
- Luteberget, Sartor (2023), Feasibility Jump: https://doi.org/10.1007/s12532-023-00234-8 ; author preprint https://luteberget.github.io/preprints/feasibilityjump-2023-01-09.pdf
- Sequential lifting overview and formulations: https://optimization-online.org/wp-content/uploads/2019/04/7162.pdf

## Execution log

- Plan written; implementation pending. The original 0.23 git worktree metadata points into an expired workspace. Its source and previous ZIP remain intact; a new local git baseline was created for reviewable patches.
