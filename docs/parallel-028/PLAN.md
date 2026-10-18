# LP-IP-craft 0.28: thirty eight-thread performance iterations

Base: current upstream `a1180e06a85685fa82ca977d521ef33f285f8f51`, including its 20 MIP iterations and default MIP presolve/coefficient tightening. The earlier local 0.27 working copy is preserved independently. This is a new parallel series 01–30.

## Execution and numerical contract

Plan, implement, test and measure each checkpoint before advancing. Target one total eight-thread CPU budget, with 1/2/4/8 scaling and fixed-total-work batches. Existing independent batch/component/packing/tree parallelism is a baseline, not a new invention. Preserve scalar defaults, frozen models/seeds, objective/feasibility/integrality/KKT tolerances, finite-bound limits, conservative dual bounds, original-model validation, per-task budget accounting and schedule-independent ordered merges. No fast-math, unsafe parallel mutation or relaxed acceptance.

Run builds/stress and timing sequentially. Timed children have eight-CPU affinity and record quota, workflow/native/CPU time, hashes, every sample and failures. Use one warmup plus at least three final repetitions, with build order rotated. Freeze primary and held-out families before tuning. Distinguish single-solve latency, standalone kernel time, multi-solve throughput and application progress under limits. Numerical equivalence is checked independently, not inferred from faster termination.

Each listed plan is provisional until its checkpoint; adapt to measured findings, recording reversions and failures. Thirty iterations cannot promise thirty monotone wall-time wins on a shared host. Retain useful capabilities and reduce identifiable work; reject losing prototypes.

## Iteration ledger

| Iteration | Plan | Results |
|---:|---|---|
| 01 | [Freeze eight-worker performance harness](iteration-01-plan.md) | [Outcome](iteration-01-results.md) |
| 02 | [Remove batch result mutex](iteration-02-plan.md) | [Outcome](iteration-02-results.md) |
| 03 | [Bound batch task claims](iteration-03-plan.md) | [Outcome](iteration-03-results.md) |
| 04 | [Schedule large batch models first](iteration-04-plan.md) | [Outcome](iteration-04-results.md) |
| 05 | [Let the batch caller participate](iteration-05-plan.md) | [Outcome](iteration-05-results.md) |
| 06 | [Schedule expensive components first](iteration-06-plan.md) | [Outcome](iteration-06-results.md) |
| 07 | [Collect component results privately](iteration-07-plan.md) | [Outcome](iteration-07-results.md) |
| 08 | [Use a serial residual fast path](iteration-08-plan.md) | [Outcome](iteration-08-results.md) |
| 09 | [Collect tree outputs in linear slots](iteration-09-plan.md) | [Outcome](iteration-09-results.md) |
| 10 | [Collect residual outputs in linear slots](iteration-10-plan.md) | [Outcome](iteration-10-results.md) |
| 11 | [Add bounded reusable numeric execution](iteration-11-plan.md) | [Outcome](iteration-11-results.md) |
| 12 | [Expose native numerical thread budgets](iteration-12-plan.md) | [Outcome](iteration-12-results.md) |
| 13 | [Parallelize fresh reduced costs](iteration-13-plan.md) | [Outcome](iteration-13-results.md) |
| 14 | [Parallelize dense column pricing](iteration-14-plan.md) | [Outcome](iteration-14-results.md) |
| 15 | [Pass execution through basis factorization](iteration-15-plan.md) | [Outcome](iteration-15-results.md) |
| 16 | [Parallelize blocked Schur updates](iteration-16-plan.md) | [Outcome](iteration-16-results.md) |
| 17 | [Tune dense update dispatch thresholds](iteration-17-plan.md) | [Outcome](iteration-17-results.md) |
| 18 | [Parallelize dense panel row updates](iteration-18-plan.md) | [Outcome](iteration-18-results.md) |
| 19 | [Improve dense scalar tile locality](iteration-19-plan.md) | [Outcome](iteration-19-results.md) |
| 20 | [Avoid idle kernel resources](iteration-20-plan.md) | [Outcome](iteration-20-results.md) |
| 21 | [Add kernel scaling and reuse measurements](iteration-21-plan.md) | [Outcome](iteration-21-results.md) |
| 22 | [Add bounded mixed outer/inner batches](iteration-22-plan.md) | [Outcome](iteration-22-results.md) |
| 23 | [Balance irregular column dispatch](iteration-23-plan.md) | [Outcome](iteration-23-results.md) |
| 24 | [Stress concurrent solves and isolation](iteration-24-plan.md) | [Outcome](iteration-24-results.md) |
| 25 | [Make benchmark failures impossible to hide](iteration-25-plan.md) | [Outcome](iteration-25-results.md) |
| 26 | [Independent serial/eight numerical audit](iteration-26-plan.md) | [Outcome](iteration-26-results.md) |
| 27 | [Gate actual work; parallelize packing root](iteration-27-plan.md) | [Outcome](iteration-27-results.md) |
| 28 | [Run public/SpOC4 and held-out applications](iteration-28-plan.md) | [Outcome](iteration-28-results.md) |
| 29 | [Fuse root phases within the owned pool; document APIs](iteration-29-plan.md) | [Outcome](iteration-29-results.md) |
| 30 | [Freeze 0.28, repair scalar fallbacks and benchmark provenance](iteration-30-plan.md) | [Outcome](iteration-30-results.md) |

## Completion and evidence

All 30 checkpoints have an implemented change, release build, complete regular Rust test log and checked paired timing record. Iterations 18–19 and 23 were reverted by later checkpoints after losses; the series is not 30 monotone wins. Plans 27–30 record application-driven adaptations before execution. See [final results](RESULTS.md), [complete tables](TABLES.md), and `data/validation-summary.json` for exact revisions, counts and log hashes.

The original iteration 30 timing table used an instrumented example and is excluded from production claims. Final production campaigns explicitly reject profiling builds; all eight Rust campaign JSONs pass complete-count validation. An incomplete final factor journal was retained and rerun in full; journal writes now flush/sync and replace atomically, and the matrix driver reopens and validates completed evidence.
