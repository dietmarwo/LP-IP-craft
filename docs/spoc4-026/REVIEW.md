# Review of the 0.25.0 and 0.26.0 deliveries

0.25 and 0.26 were imported as delivered: 0.25 reconstructed from the 0.26
archive, see [../EVIDENCE.md](../EVIDENCE.md). They were then reviewed
together, since neither had been reviewed here before. The delivered
`RESULTS.md` files describe the delivered builds and are left unchanged.
This page records what the review changed, what it measured, and what it
tried and left out.

## Changes

1. **The repair merge was a serial bottleneck.** Callgrind on matching-I (2
   rounds of 64 rejected-seed repairs, one worker) put 52% of all
   instructions in the merge loop's candidate checks and 6% in the residual
   solves. Each candidate got up to two full-model checks (tolerance check
   plus the outward-rounded incumbent check), one against the batch
   snapshot and one against the current best. Meanwhile the workers waited
   at the batch barrier, which is why four workers ran only about 1.1x
   faster than one. Now each worker checks its own candidate once, and only
   if the residual changed a column, and the ordered merge only reads the
   outcome. The incumbent check also skips zero entries of the point. It
   and core `check::evaluate` now test integrality without a libm call.
   Building a residual selects the kept alternatives instead of sorting all
   of them. Results are identical (see Verification).

2. **Workers take tasks from a shared counter.** Residual tasks and frontier
   subtrees vary a lot in cost; the fixed round-robin split left workers
   idle. Results are still combined in task (subtree) order and work shares
   depend only on the index, so fixed-work outcomes do not depend on the
   schedule.

3. **The tree hid failures.** The frontier's final status was the status of
   the last unfinished subtree in index order. A subtree that ended in
   `numerical_failure` or on the wall clock was therefore reported as
   `node_limit` whenever a later subtree reached its node share.
   Unfinished statuses now combine by severity. A subtree error was
   counted but its text dropped; the message now names the first failing
   subtree and its error. Bounds were already handled correctly.

4. **Validation errors name the field.** The new checks returned "invalid
   packing repair/phase controls" or "invalid bounded packing/probe/tree
   controls"; they now name the field and its range, like the older checks.
   The accepted values are unchanged. The 25 new `NativeOptions` fields
   get doc comments.

5. **Local clique ordering.** The separator re-counted a dense conflict-graph
   row inside every sort comparison (O(n² log n) per residual, outside
   `packing_clique_work`); degrees are now counted once as edges appear.

## Verification

All 22 records of the delivered 0.26 suite (`benchmarks/spoc4-026/results`)
were rerun in fresh processes with the delivered build and with this branch,
with the recorded controls (four workers) on a two-core host.
Every run reproduces the delivered solution vector, objective and bound.
Nodes and pivots match too, except in the two `proof` runs: their 2-second
proof phase is wall-clock limited, and that is also true when the delivered
build is rerun. Whole solve-call seconds, same host:

| Record | Score | Delivered build | This branch |
| --- | ---: | ---: | ---: |
| i-legacy | 32,120.012 | 0.60 | 0.41 |
| i-mixed | 32,701.772 | 1.27 | 0.61 |
| i-rejected-8round | 32,847.565 | 2.19 | 0.89 |
| ii-legacy | 68,914.100 | 1.57 | 0.83 |
| ii-mixed | 70,157.806 | 4.57 | 1.61 |
| ii-mixed-root1b | 70,715.443 | 6.26 | 3.21 |
| ii-rejected-8round | 70,945.093 | 7.71 | 2.52 |

The Rust and Python suites pass, including the new tests for status
severity, field-named validation and the integer helpers, and the CI
oracle scripts pass.

## What the faster repair phase buys

Same host, this branch, two workers, proof disabled, the delivered root
settings (200 million work units, 1,000 steps), single runs. Results are
deterministic at fixed work, so they reproduce exactly; they are two tables,
not a general claim.

| Table | Rounds × repairs, seeds | Score | Seconds |
| --- | --- | ---: | ---: |
| matching-I | 8 × 64, rejected (delivered best) | 32,847.565 | 0.89 |
| matching-I | 8 × 128, rejected | 32,943.379 | 1.14 |
| matching-I | 8 × 128, mixed | 32,987.195 | 1.12 |
| matching-II | 8 × 64, rejected (delivered best) | 70,945.093 | 2.52 |
| matching-II | 8 × 128, rejected | 71,247.653 | 3.38 |
| matching-II | 16 × 128, rejected | 71,337.426 | 5.02 |

With the delivered build, the 8 × 64 schedules took 2.19 s and 7.71 s on
this host. matching-II's 2,048-repair schedule therefore runs in less time
than the delivered 512-repair one and scores 392.333 (0.55%) higher.
matching-I gains less: 16 × 128 rejected repairs give the same 32,943.379
as 8 × 128.

These are measurements, not new defaults; the defaults are unchanged.

## Tried and not adopted

**Merging improving repairs in order of gain** instead of task order. Over
the same 12 schedule/instance pairs, it scored higher in 5 of 6 matching-II
schedules and lower in 5 of 6 matching-I schedules. The differences were
small in both directions (for example, I at 4 rounds × 64 incumbent seeds:
32,705.745 to 32,674.907; II at 16 rounds × 64 rejected seeds: 71,041.327
to 71,133.397). Task order stays.

## Checked, no defect found

- **Packing driver.** 3,000 random packing models (4–13 binary columns,
  fixed columns, zero weights, objective offsets, optional starts), each
  solved under a random mix of every packing, tree and branching control.
  Each result was compared against exhaustive enumeration: no wrong
  optimum, false infeasibility, unsafe bound, infeasible candidate,
  candidate above the optimum, or worker error.
- **Tree, partial bounds and scalable branching.** Checked with:
  - 1,500 tree-mode runs on random binary MIPs of both senses against
    brute force;
  - 4,000 interrupted-simplex bound runs;
  - identical results for 2, 4, 8 and 16 threads at fixed work;
  - 832 scalable-branching runs, with probe work within its reservation.

  The Python dataclass and parser carry all 25 new controls with the Rust
  defaults, and no API newer than Rust 1.85 is used.

## Left as is

- Scalable branching rebuilds a column-structure vector, O(nnz), at each
  probing node. That is small next to the node's LP work.
- Tree mode returns no `diagnostics.mip_search` trace, because children
  disable diagnostics.
- The ESA matching tables are GPL-3.0 data. They are committed with their
  license text and provenance (`benchmarks/spoc4-025/DATA-SOURCES.md`),
  separate from the MIT code.
