# Learning from ripsolve: reviewed implementation plan

Baseline: LP-IP-craft main `825ee42679c3892310d8658d2a71678b88cb6dd3`, native 0.26.0. Reference: the published ripsolve 0.4.0 source distribution, SHA256 `a46e01c8e528e24adf9207f7b17a7ca7442ec03b9fe01e04ab33ddf975641871`, MIT, Andrew Henshaw; source https://github.com/ripsolve/ripsolve. User's original plan is retained in INITIAL-PLAN.md. This revised plan is the implementation contract, with completion and unmet performance targets reported separately.

## Review and corrections

The priorities are sound: strengthen root relaxations before investing in more tree throughput. Keep presolve, GMI, MIR, primal heuristics and branching independently controlled and opt-in. Keep the SpOC4 repair dispatch and its private workers unchanged by these general-MIP modes.

1. The original benchmark numbers are historical observations, not independently reproduced evidence yet. Freeze them with attribution and distinguish new results, root LP objective, independently checked bound, incumbent and integer optimum. The nine-instance development list is avgas, bell5, egout, flugpl, gt2, lseu, p0548, p01 and rgn: inspection shows eight MIPs and the continuous LP control avgas. The existing corpus already supplies these; do not create new formulations or finite surrogate bounds.
2. **Floating tableau rounding is not a validity proof.** A BTRAN row has numerical residuals on other basic columns. Merely rounding its final GMI coefficients cannot repair an invalid starting equality. Instead treat approximate BTRAN multipliers as exact dyadics, aggregate the original equations exactly using native's bounded Rat arithmetic, and retain every residual structural/logical coefficient. Normalize exactly by the chosen basic integer coefficient. Apply GMI in exact rationals. Convert the resulting original-column inequality to binary64 with an exact box compensation for every rounded/dropped coefficient and an outward RHS. Fail closed on work/size/infinite-bound limits. Exhaustive tests supplement this construction; they do not certify unseen production cuts.
3. ripsolve's coefficient tightening is for **unfixed binary columns**, not arbitrary general integers. Implement that domain with an exact derivation. Mixed rows are permitted if the other variables have finite bounds. Handle upper/lower sides independently; skip ranged/equality rows initially so changing one side cannot invalidate the other. Its presolve also contains conflict-aware tightening beyond the box-only implementation here; its entire presolve ablation cannot be attributed to these W1 reductions.
4. Dual singleton/empty-column fixing preserves an optimum, not every feasible point. Its tests must compare optima rather than require every feasible point to survive. The prohibition in ripsolve concerns an external lazy separator with constraints absent from the model, not all globally valid root cuts. Native has no such separator API. Still use a separate opt-in toggle and retain better original-feasible user starts rather than rejecting them because of an optimum-preserving reduction.
5. Existing root cut rounds rebuild LPs; there is no dynamic row append in the current kernel. Extend the root separation stage with a retained basis extension: append logical basic columns for new rows, preserve old basis/nonbasic states, and refactor the enlarged system. This reuses the combinatorial basis, not a numerical LU update across changing matrix shapes. Count that cost explicitly. Never reuse a stale factorization or cache.
6. Plain-LP measurement belongs in the comparison harness. Production diagnostics should report the presolved relaxation and each resolved cut round without silently adding an extra original-LP solve to every call.
7. Fixed time targets (<1 s, shifted means, no >10% regression) are hypotheses to evaluate on this host, not correctness acceptance criteria. A negative result leaves the feature opt-in and is a completed experiment. Correctness failures block delivery of the affected mode.
8. W6 remains deferred exactly as in the original plan: no CDCL, mod-2/clique expansion or shared global tree in this release. Prepare reproducible upstream bug evidence and an issue draft; publication is separate from local solver implementation.
9. The reference's root fields are copied from its returned restart pass rather than a frozen first-pass trajectory. A family ablation returns `root_bound_after_cuts=8724` for p0548 while its final objective/bound is 8691. Retain the observation, but do not interpret that root field as a certified lower bound for the unchanged original model or compare it blindly with native's checked root stages.

## W0 — Reproducible measurement baseline

- Pin the live native commit, ripsolve sdist hash/version and HiGHS reference version. Read current source, not inferred algorithms from a marketing page. The restored 6,330-case input/adjudication hashes must match the original frozen files; never regenerate them to conceal a discrepancy.
- Add `benchmarks/rust-solvers/`: one Rust executable parses a common frozen model and creates identical arrays for native and ripsolve, with explicit one-thread, time/node/pivot limits where supported. Include a ripsolve options-ablation mode. Reference HiGHS through a separate driver using the same input arrays and independent output checks.
- Audit actual concurrency: ripsolve's `threads=1` can still launch background CDCL. Retain its default-option experiment with that caveat and add a strictly serial reference profile (`cdcl=false`). Do not equate its B&B worker limit with a total OS-thread limit.
- Export presolve counters, root/cut round objectives and checked bounds, added cuts per family, exact-arithmetic work, LP calls/pivots, retained-basis usage, heuristic counters and bounded branching observations under `diagnostics.*`.
- Freeze a held-out set before inspecting new-mode performance: deterministically select parser-supported, non-development MIPs from the pinned HiGHS check instances (up to twenty; explicitly report available count and exclusions). Preserve original MPS bytes, input dimensions and SHA256. Record parser failures and unsupported native domains; do not replace missing finite bounds with arbitrary constants. Add independent seeded big-M and knapsack families.

Acceptance: one command reproduces new comparison/root tables from raw records; full frozen oracle input and adjudication hashes are checked; available held-out instances and exclusions are listed. Historical uncommitted comparison binaries cannot be claimed as recovered; replace them with a checked-in reproducible harness.

## W1 — Exact and model-preserving presolve

- Reuse native's existing exact integer propagation and outward mixed-row propagation for fixpoint bound tightening, with bounded rounds/work and cancellation. Keep column/row identity. Bound endpoints on integer columns are rounded inward only after a conservative derivation.
- Remove a row only when an enclosing activity interval proves both sides redundant. Classify an infeasibility only from an exact/outward contradiction; otherwise let the checked LP do so. Forcing rows are the limiting case of propagation, with an explicit diagnostic.
- Binary coefficient tightening: for an upper row `a x + r <= b`, compute an exact upper activity M. If `0 < M-b < |a|`, replace a positive a by `M-b` and the RHS by `M-a`, or replace negative a by `-(M-b)` with RHS unchanged. The complementary/lower-side forms follow by negation. Convert the exact changed row with box compensation; skipping a nonrepresentable or non-tightening result is safe.
- Empty/singleton objective-directed fixing is optional, sense-aware, finite-target only and integral-endpoint safe. It is optimum-preserving; never use it as a claim of feasible-set equality.
- Expose separate `mip_presolve`, `coefficient_tightening`, `dual_presolve`, bounded presolve rounds/work controls. Defaults off.

Correctness acceptance: exhaustive preservation for primal reductions and tightening; optimum preservation for dual fixes; mixed/both-sense/fixed-bound fixtures; 6,330 original oracle cases and the unchanged 400-model bound formula. Evaluate (do not promise) p0548 root ≥4500, egout ≥500 and held-out timing effects.

## W2 — Certified GMI root cuts

- Expose basic fractional structural integer candidates and original-row BTRAN multipliers from Lp. Interpret these multipliers exactly and rederive the equality from original input rows, including fixed columns, scaling and row activities.
- Shift/complement other variables to finite nonnegative bounds; integer shifts must be integral. Logical row activities are integer only when their original row has exclusively integral variables and exact integral coefficients. Otherwise they remain continuous. If a nonzero logical coefficient has no finite valid anchor, reject the candidate.
- Form the standard GMI inequality in Rat arithmetic, rejecting tiny f0 outside [0.005,0.995], singular/oversized derivations and exhausted exact-work budgets. Retain the numerical residuals of other basic variables rather than assuming they are zero.
- Substitute logical activities back to original structural columns. For exact `c x <= b`, emit finite coefficients ĉ and RHS upper(b + Σ max over [l,u] of (ĉ-c)x). This proves safe rounding and tiny-term removal over the actual root box. Bound dynamism, density, candidates and inserted rows.
- Select by efficacy and conservative near-parallel filtering; deduplicate against existing rows. A root pool retains bounded rows; discard slack new cuts only between rounds, rebuilding the basis safely if removal is used. Avoid cut aging machinery if the bounded root pool never needs it; report this explicit implementation choice.
- Integrate into the existing root separation stage; resolve with retained combinatorial basis plus new logical columns. Every LP optimum and bound passes existing independent checks. Count all LP work against whole-call limits; preserve valid starts and safe bounds on interruption.

Correctness acceptance: generated cuts preserve every feasible integer point in small exhaustive mixed/integer fixtures, with upper complements, negative bounds, zero/tiny coefficients, ill-scaled dyadics and row scaling. 400-model bound audit has no unsafe bound. Measure original root targets gt2 ≥20000, bell5 ≥8.94M, p0548 ≥8500; report any shortfall. No claim that bounded exact cut construction is the full Cook–Dash–Fukasawa–Goycoolea implementation.

## W3 — MIR with variable-upper-bound substitution

- Recognize exact two-term implied upper bounds `x <= d y`, where y is a free binary, x continuous, and the row side is exactly zero. Use rational d from the original coefficients, not an uncertified rounded factor.
- Shift/complement structural variables on valid bounds; optionally replace x by d y minus a nonnegative residual. Aggregate original row sides, try a small bounded divisor set, and apply the standard MIR function in exact rationals.
- Substitute VUB residuals back and use the same rounding, efficacy, deduplication and work caps as GMI. Any unsupported structure or arithmetic limit skips that candidate.

Acceptance: planted fixed-charge examples tighten their LP; exhaustive integer/continuous vertex tests preserve feasible points; compare MIR-only, GMI-only and combined on development/held-out with identical starts. Report negative node/time results.

## W4 — Checked primal heuristics

- Reuse existing rounding/incumbent admission. Add opt-in bounded root guided diving and a genuine L1 feasibility pump (absolute-deviation auxiliary columns for general integers). Each projection is a private LP under global work/time limits; its objective is heuristic and never a pruning bound.
- Preserve the parent basis and bounds after dives; record failures/limits. Cycle detection, bounded perturbation and early backoff prevent repeated failed targets from consuming unbounded work. No in-tree scheduler or parallel sharing is introduced.
- Retain only candidates rechecked on the original model and certified by the existing machinery in the final native search. A local heuristic LP status cannot prove whole-model infeasibility or optimality.

Acceptance: root-incumbent and cycling/limit fixtures; measure p0548 incumbent availability and gt2/lseu node effects; unchanged SpOC4 original-order vectors at fixed work.

## W5 — General-integer branching experiments

- Explain the actual native/ripsolve differences before making default changes: fallback pseudocost initialization, minimum step used for normalization, scoring function, reliability candidates/budgets, different LP points and tie rules.
- Add a separately controlled product versus current weighted-sum score, a minimum-step normalization experiment, and bounded trace records containing fractional candidates, directional history and chosen variable. Native already has directional global-average initialization with smoothing; retain that implementation. Reuse existing reliability probes and their parent restoration; estimates remain advisory.
- Compare policies at the same native LP solutions (a direct causal experiment). Cross-solver trace comparisons cannot attribute every difference to branching because bases/LP optima/presolve may differ. Preserve negative results. No defaults change without held-out evidence in a separate decision.

Acceptance: deterministic scoring fixtures; original oracle/bound gates; cause and measured win or negative result on development and held-out.

## Release validation and delivery

Implement all W0–W5 mechanisms, test targeted fixtures, then existing Rust/Python suites, fmt/Clippy default/all features, the frozen oracle under default and new combined policies, independent exhaustive reduction/cut tests and the unchanged 400-model audit under both policies. Run timed solver processes serially after builds. Public comparisons use one thread and a fixed cap; report solved counts, shifted geometric means with an explicit shift and failure handling, root-bound progress, incumbents, node/pivot counts and raw termination reasons. Rerun both full SpOC4 repairs at fixed work and compare vectors to the pinned baseline.

Deliver reviewed plan, validity arguments/API notes, results with every unmet target, all source/tests, a patch against the pinned GitHub baseline, frozen held-out provenance, pinned reference harness/source licence notices, raw observations and reproduction commands. The package is a local tested implementation, with no remote push, default promotion or release publication.

## Completion record — 2026-10-18

W0 and all five development workstreams W1–W5 are implemented as opt-in controls in native 0.27.0. W6 remains deferred. Detailed final measurements, negative results and limitations are in RESULTS.md and TABLES.md; API details and mathematical arguments are in IMPLEMENTATION.md and VALIDITY.md.

- Passed: 172 Rust workspace tests, 257 Python tests, numerical all-target/all-feature checks, formatting and both Clippy profiles.
- Passed: frozen 6,330-case oracle under each of default/combined settings with unchanged adjudications, and 400-model exact bound audit under each setting. Both SpOC4 original-order vectors are byte-identical to baseline at 512 repair tasks per instance.
- Improvement: p0548 is proved optimal at 8691 in 1.70–1.72 seconds with combined cuts, root objective 8677.0366. Baseline reaches its 60-second limit. MIR-only reduces gt2 from 2.730 to 0.234 seconds; presolve-only reduces egout from 0.407 to 0.011 seconds.
- Unmet: p0548 presolve-only root target, bell5 root target, uniform under-one-second solving, root-heuristic incumbent on p0548, 9/9 development proofs in 60 seconds, reference timing goals and held-out no-regression goals. bell5 and easy holdouts regress with combined cuts. No defaults were promoted.
- Delivered: full source/tests, patch against the pinned GitHub base, frozen inputs/provenance, validation logs/raw results and an unsubmitted reference status-bug issue draft. No remote push or release publication.
