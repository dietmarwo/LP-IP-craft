# Review of the 0.28 delivery

18 October 2026. The delivery (32 commits on upstream `a1180e0` ending at
`af9a9a4`: thirty checkpoints and two release commits) was reviewed, tested
and corrected before it was merged into `main`. This page records what the
review found and changed. The delivered [RESULTS.md](RESULTS.md),
[TABLES.md](TABLES.md) and [VALIDITY.md](VALIDITY.md) stay as delivered,
apart from the corrections listed here.

## Corrections

1. **CI failed on every macOS and Windows Python job.**
   - `scripts/parallel028.py` and `scripts/parallel028_applications.py`
     called `os.sched_getaffinity` (missing on macOS) and imported
     `resource` (missing on Windows) unconditionally.
   - The failing-child driver test runs a POSIX script.
   - The drivers now record affinity, the cgroup v2 quota and child CPU time
     where the platform provides them, and that test is skipped on Windows.
   - All CI jobs now pass: Rust with Clippy and tests, `cargo check` with
     Rust 1.85, the oracle runs, and the Python suite on Linux, macOS and
     Windows with CPython 3.10 and 3.14. The suites are therefore no longer
     validated on Linux only. The timings still come from the one recorded
     host.
2. **Python `solve_batch` ignored `Options.threads`.**
   - The binding built its options with width 1. So
     `solve_batch(models, workers=2, options=Options(threads=4))`, the
     example in [../api.md](../api.md), checked the 2 × 4 budget and then
     ran every job at width 1. It also accepted `threads=0`, which a direct
     solve rejects.
   - The width is now passed on and validated as for a direct solve. The
     Rust API was not affected.
   - The mixed-batch test now checks each job's reported `numeric_threads`.
     It fails on the delivered package for 1 × 8, 2 × 4 and 4 × 2.
3. **Concurrent callers could stall each other.**
   - Live owners of the same width share one Rayon pool through the
     registry. The packing root runs its whole loop as one pool task.
   - A pool worker that waits for its own subtasks runs other queued tasks
     meanwhile. A concurrent caller's worker could therefore pick up the
     root loop and then not resume its own solve, or see its deadline, until
     the root ended.
   - In a demonstration, three LP callers had a 0.3 s limit (about 0.05 s
     alone) while a fourth caller ran packing roots at the same width. Their
     worst solve followed the root length: 0.65 s, 1.5 s and 2.5 s. With the
     fix it is 0.24 s.
   - The root now uses `Execution::exclusive`, a pool no other owner
     shares, above its 131,072-nonzero gate. That costs one pool creation
     per root: a median of 0.1 ms at width 2 and 0.6 ms at width 8 in the
     review's two-CPU container. Against the 76 ms SpOC4 II root at eight
     workers this is under 1%; it was not re-measured on the eight-CPU host.
     SpOC4 I stays below the gate.
   - `Execution::new` now documents the hazard for callers of the public
     `map` and `chunks_mut` with long tasks.
   - A test runs a short task on a shared pool beside a long one. It fails
     when the long task shares the pool and passes when it is exclusive.
4. **A batch panic was an error or an unwind, depending on scheduling.**
   - 0.28 runs one share of a batch on the calling thread. A panic there
     unwound out of `solve_batch` (in Python a `PanicException`). The same
     panic on a spawned worker became `Error::Backend("batch worker
     panicked")`, as every panic did in 0.27.
   - With a panic injected into one model's solve, 200 identical runs gave
     the error 97 times and unwound 103 times.
   - The caller's share is now caught, every thread is joined before
     returning, and tests inject panics on the caller and on the workers.
   - The residual search's new one-worker path had the same defect for
     "residual worker panicked"; both now use `ordered::inline`.
5. **Text.**
   - The prose of 70 Markdown files had lost the space between a word and a
     following number or symbol ("the0.28", "at most8", "1 ×8").
   - So had five strings and comments in the code, among them two error
     messages ("must not exceed8", "maximum numerical budget is8").
   - The Python `Options.threads` docstring now says that a width above 1
     serializes the outer mechanisms.
6. **Report regeneration.**
   - `scripts/parallel028_report.py` needs the raw journals. These ship only
     in the delivery archive, because `/docs/*/data/` is ignored (as for
     earlier releases).
   - Run on the repository, the script raised `FileNotFoundError`. It now
     names the missing files and points to [../EVIDENCE.md](../EVIDENCE.md).
   - Run on the archive's `docs/parallel-028`, it regenerates `TABLES.md`,
     `data/final-scaling.csv` and `data/metrics.json` byte for byte.

## Checked and left as delivered

An independent agent's differential tests against `a1180e0` found
corrections 2–4. They also confirmed the following:
- **Bit-identical results at widths 1, 2, 4 and 8, matching `a1180e0`.** The
  compared fields were status, nodes, iterations and messages, and the bits
  of the best bound, candidate and duals. The models:
  - 48 models: dense LPs with tied costs, LPs with equality and ranged rows,
    infeasible and unbounded LPs, Devex and Dantzig pricing, MIPs up to 324k
    nonzeros and packing roots above 150k nonzeros;
  - 18 random LPs with boxed and free variables;
  - edge cases with m = 0, unbounded boxes, infeasible MIPs, and empty and
    pre-cancelled batches.

  The 28 core cases were rerun on the final tree with the same result.
- **The parallel branches ran.** In an instrumented copy they made:
  - thousands of reduced-cost and column-pricing dispatches;
  - over 160 dense Schur-update dispatches;
  - 70 packing-root iterations.

  This matters because the delivered 15,255-call audit mostly uses models
  below the dispatch gates, where every width runs the scalar code.
- **Rust batches and concurrency:**
  - Rust batches of 1 × 8, 2 × 4, 4 × 2, 8 × 1, 16 × 1 and 3 × 2 are
    identical to serial solves, in input order.
  - Six concurrent callers at mixed widths showed no mismatch or deadlock.
- **Budgets, limits and ordering:**
  - Widths 0 and 9, and batches of 2 × 8 and 5 × 2, are rejected.
  - Pre-cancelled solves and limits of zero create no pool and return the
    right status.
  - Reading the code, the summation order in pricing, the Schur update and
    the packing gradients is unchanged.

Left as delivered:
- **`effective_options` at a width above 1** reports the settings the engine
  received: the time limit minus worker setup, and component, tree and
  residual workers forced to 1. The time limit therefore varies slightly
  from run to run. The field is documented as the applied settings.
- **Repository size.** The tracked tree grows from 18.7 MB to 20.2 MB. The
  118 MB of raw journals stay in the archive.
- **History.** On GitHub the 0.28 work is one squashed commit. The
  checkpoint commits and tags `lp-parallel-01` to `lp-parallel-30` stay in
  the archive's `history/lp-parallel.bundle`. GitHub also keeps the
  commits under pull request #10.

## Verification

- **Rust:** 201 workspace tests pass in release mode (197 delivered and four
  new). Clippy with warnings denied and `rustfmt` are clean.
- **Python:** 290 tests pass (289 delivered and one new).
- **SpOC4 II root** at widths 1, 2 and 8 gives the archived objective, bound
  and point hash.
- **CI:** all jobs pass on the merged commit.
