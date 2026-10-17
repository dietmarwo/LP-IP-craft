# LP-IP-craft 0.23 — packing FTRAN and MIP diagnosis

Written before numerical implementation, 17 October 2026.

## Scope and baseline

Execute only continuation items 1 and 2: investigate/improve packing forward
basis solves and instrument/diagnose MIP search with broader coverage. Do not add
Feasibility Jump, reliability branching, new cuts, mixed-node propagation,
conflict learning, parallel simplex or a scheduler in this release.

Accepted baseline: 0.22.0, commit
`4b57aabf77d46e26a85b6eb1a48efdf8dc569fc2`. The delivered ZIP SHA-256 is
`5be46f4af89c840de768b2c182a914b2c2481f64fda18e89d0a659519ae68fbd`.
Work in an isolated checkout; preserve the baseline, prior evidence, frozen
oracle inputs/adjudications, all original-model checks and existing budgets.
Production numerics remain owned safe Rust with no external numerical engine.

## 1. Establish reproducible controls

- Preserve uninstrumented 0.22 and HiGHS comparison executables and their hashes.
- Build candidate uninstrumented and profile/replay executables separately.
- Use identical shared fixture code for comparisons. Include ordinary and
  degree-nine packing, fresh seeds/permutations, genuine wide/tall models,
  networks, irregular models, public LPs and MIPs. Record dimensions and hashes.
- Use fresh rotated/reversed processes, one discarded round and at least three
  measured rounds, with one warmup per process. Report medians and ranges.
  No builds/correctness workloads run concurrently with timing. Shared hardware
  is a measurement limitation. Preserve failures and unsupported models.

## 2. Investigate packing FTRAN

First instrument mutually exclusive work categories: lower/upper arithmetic,
symbolic reach, FT row transformations, spike capture, workspace clearing,
support maintenance and output scatter/copy. Counters expose sparse/dense path
frequency and entries touched where they can be collected without changing
uninstrumented arithmetic. Profiled times are diagnostic, not headline speed.

Inspect input/output/workspace lifetimes through lower solves, FT, eta updates,
and BTRAN. The 0.22 implementation already carries output support; build on it.
Candidates, evaluated separately and capped at three substantive experiments:

1. Reuse valid sparse workspace metadata across FT forward/backward calls,
   clearing only touched entries and scattering only when complete input and
   output support are known.
2. Avoid redundant support traversals/copies using ownership or borrowed
   support, with explicit invalidation on orientation/fallback changes.
3. If evidence supports it, carry known RHS support into symbolic reach or
   spike capture, retaining dense fallbacks and the numerical evaluation order.

Record the precise selected design before each experiment. Do not reintroduce
the failed 0.22 broad/narrow density switches without a new causal mechanism.
Reach sets are structural supersets: preserve cancellation, zero RHS, small
dimensions, updates, failures, dense tails and alternating FTRAN/BTRAN calls.

Acceptance: unchanged numerical checks and repeatable whole-solve packing
benefit over multiple cases without a material unexplained holdout regression.
A kernel-only gain is insufficient. Reject/revert inconclusive experiments;
retaining diagnostics and a documented negative outcome completes this item
honestly if no safe production optimization passes.

## 3. Add bounded observational MIP diagnostics

Reuse the existing result/backend metadata contract. Add opt-in search
diagnostics, with bounded event storage and explicit truncation. Default solve
behavior, starts, pruning, node/iteration/time/cancellation limits and public
status conventions remain unchanged. The disabled path should add minimal
overhead; enabled overhead is measured separately.

Record original objective units/sense and distinguish the following:

- Root relaxation objective, conservative root bound, fractional integer count
  and root simplex work; do not call an uncertified estimate a pruning bound.
- First original-model-accepted candidate versus first certified incumbent,
  subsequent certified incumbent improvements, global conservative open-tree
  bound progress, and terminal status/bound/gap. Missing bounds stay missing.
- Event elapsed time and work coordinates (processed nodes and simplex pivots),
  so time limits and work-limited runs can be interpreted separately.
- LP solves/pivots per node, reoptimization/fallback work and prune reasons.
- Exact integer propagation eligibility/coverage, queue work, deductions and
  infeasibility detection. Root mixed inference is a separate capability.
- Heuristic attempts/accepted candidates, probe work, maximum depth and open
  queue size. No full solution vectors in trace events.

Limit/unsupported/cancelled/root-integral paths must produce truthful summaries.
Component solves must explicitly indicate diagnostic scope rather than imply a
global merged search trace. Deterministic work/event order is expected for a
fixed serial run, while elapsed times can vary.

## 4. Diagnose broader MIP coverage

Retain egout, flugpl and bell5 and expand with original public models available
in the pinned reference source plus fresh generated coupled integer/binary and
mixed families. Keep original MPS, conversion provenance, hashes, independent
reference objectives and all native capability failures. Never replace an
infinite bound by a guessed finite value.

For matched fixed budgets, compare baseline/candidate/HiGHS wall time, nodes,
pivots, accepted solutions and bounds. Run candidate diagnostics separately and
measure enabled/disabled overhead. Where useful run existing controls (heuristic
off, propagation off, strong branching on, cuts on) as explicitly labelled
ablations, not additions to the algorithm. Root gaps use an independent known
optimum when available. Report a hypothesis per hard case: relaxation strength,
incumbent discovery, branching, node LP cost or unsupported model features.
Do not infer a single cause solely from HiGHS node counts.

## 5. Verification and delivery

- Rust all-feature tests, formatting and strict all-target/all-feature Clippy;
  Python suite and binding/stub compatibility.
- Meaningful numerical regressions for changed FTRAN paths: alternating
  orientation/support, cancellations, FT/eta updates, residuals and dense tails.
- Frozen 45,690-run oracle policy matrix under unchanged adjudications;
  original 19 raw flags remain visible on each full variant.
- Exact 400-model bound audit and frozen basis replay residual checks.
- Diagnostic tests for root-integral search, successful branching, budget exits,
  infeasibility, cancellation, negative/maximization objectives, event caps and
  enabled/disabled solution/work equivalence.
- Retain raw performance/diagnostic records, build/model hashes, tests and
  rejected experiment patches. Document reproduction and measurement limits.
- Deliver full 0.23 source ZIP, this plan, RESULTS.md, audit/evidence and a patch
  against 0.22. Exclude build products and downloaded reference source trees.
  Supply a separate downloadable copy of this plan. Do not push to GitHub.

## Execution log

- Plan created before edits to production numerical code. Baseline source and
  existing options inspected; isolated branch created at the accepted commit.
- Instrumentation-only baseline separates costs without changing arithmetic.
  On ordinary packing4096, one diagnostic observation attributes about 10 ms
  each to support handoff/maintenance and support export, versus 95 ms FT upper
  arithmetic and 14 ms FT reach. These are instrumented observations, not a
  headline benchmark. Experiment 1 will remove redundant FT support copying
  and marking when no trailing eta updates exist: borrow FT's exact captured
  output list directly into the caller's index vector. The general eta merge
  and dense-tail paths retain their old contracts. No density switch is added.
- Experiment 1 passed native all-feature checks and a five-round comparison:
  ordinary packing1024 27.117 -> 24.062 ms; packing4096 244.934 -> 231.487 ms.
  Degree-nine packing1024 is essentially unchanged; network2048 has a noisy
  73.628 -> 76.532 ms regression requiring final controlled remeasurement.
  Experiment 2 will seed FT forward symbolic reach from the already captured
  spike (when present), removing its duplicate dense seed-discovery scan.
  Traversal thresholds, cooldowns, seed ordering and arithmetic stay unchanged.
- Experiment 2 passed native all-feature checks. Its five-round median for
  ordinary packing4096 was 217.106 ms versus 252.343 ms baseline and 236.332 ms
  experiment 1. Network2048 was 74.560 versus 75.239 ms baseline. Both changes
  proceed to the full release gates; no third optimization experiment is needed.
- Diagnostics implementation decision: use a single versioned JSON observation
  in existing backend metadata, not a second result format. Retain at most
  4096 events (default 256, zero for summaries); summarize independently of
  truncation. Bound snapshots use the existing conservative heap/frontier
  bounds, never LP objective estimates. Original-model candidate checks are
  observational. Components explicitly report aggregate scope without a global
  trace. Root preprocessing has a separately reported time offset. Disabled
  tracing avoids event allocation and original-model rechecks.
- The broad candidate-2 run passes numerical gates but fresh packing holdouts
  expose regressions (notably degree-three 4096, seed 20261123: 415.419 ->
  447.732 ms, with non-overlapping three-sample ranges). Acceptance is therefore
  pending, rather than justified solely by the original fixture's gain.
- Experiment 3 design, before editing: when captured spike length already
  exceeds the existing symbolic-reach input limit, reject hyper-sparse reach
  immediately and set the same existing cooldown. The captured spike is a
  unique exact nonzero input list, so the old code necessarily rejected this
  input after walking limit+1 seeds. Sparse captured seeds can also be trusted
  without reloading each known nonzero from the RHS. This changes neither the
  reach threshold nor the chosen numerical path and is not a new density
  switch. Compare baseline, export-only, candidate 2 and candidate 3 on fresh
  holdouts and network controls. Retain earlier broad results explicitly as
  candidate-2 evidence if the accepted source changes.
- Experiment 3 is rejected/reverted: in the five-round ablation it provides no
  consistent whole-solve improvement over candidate 2 (ordinary packing4096:
  216.694 -> 218.110 ms). Its patch and binary hash are retained. The same
  controlled study does not reproduce the earlier large fresh-seed regression:
  degree-three 4096 seed20261123 is 439.565 ms baseline versus 420.257 ms
  candidate 2; the other degree-three 4096 holdout is essentially neutral
  (535.915 versus 538.127 ms). Network medians differ by about 3.5% with
  overlapping ranges, so a longer final control and default-decomposition
  fresh-holdout rerun follow. Accepted production source is restored exactly
  to commit 0734da6 pending these final controls; no third optimization ships.
- Final acceptance: the nine-round control measures ordinary packing4096 at
  249.810 -> 224.401 ms (about 10.2% lower time) and network2048 at
  77.218 -> 73.531 ms. Aspect-wide512 is 9.834 -> 10.122 ms, with overlapping
  ranges; retain that regression. In the five-round default-decomposition fresh
  rerun, changes span approximately -2.25% to +0.45%; the earlier large seed
  regression does not persist. No uniform or statistically established
  sub-percent gain is claimed on this shared host.
- The accepted source is exactly the tested candidate-2 production code at
  0734da6. Rust/Python checks and the entire frozen 45690-run matrix were rerun
  after reverting experiment 3. Initial gate records are retained separately.
  HiGHS's own original MPS reader independently confirms all six conversions.
- MIP diagnosis is complete: egout/flugpl never obtain a certified incumbent
  bound, while rgn finds a reference-quality accepted point at node 45 but
  remains uncertified. P0548 finds no accepted point before the unchanged cap.
  See DIAGNOSIS.md, RESULTS.md and all 28 labelled existing-policy probes.
