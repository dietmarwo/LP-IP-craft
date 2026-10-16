# Native iteration 10 results

Version 0.14.0 adds up to 16 scoped standard-library component workers.
A configurable estimated-dictionary threshold keeps small components serial.
Global time/cancellation checks are shared; deterministic per-component node
and pivot reservations keep total work inside the requested limits. Unused
reservations are not redistributed. Worker numerical state remains private.

All 68 Rust tests, one doctest and 134 Python tests pass. Independent objective
checks pass 600 base cases plus 180 component cases in serial and again in
parallel (780 distinct models, 960 solver/oracle comparisons). A regression
launches all sixteen workers. Worker errors, low budgets, cancellation,
partial-result suppression, offsets, both senses and free/upper-only columns
are covered. Parent diagnostics are unique in Rust as well as Python.

Testing also fixed a pre-existing status problem: a numerically infeasible
root relaxation could leave a tolerance-feasible MIP start marked optimal.
That contradiction now reports numerical_failure with the candidate quality
kept separate. A direct regression preserves this behavior.

Final-version parallel speedup on the eight-block LP is 2.17x
(2.921 ms serial, 1.347 ms with eight workers).
Relative to v5 the block fixture improves about 64x and the redundant-row
fixture about 4x on this host. Other fixture medians show about 1.09–1.27x speedups.
Use performance-6-10.md and raw JSON for exact values and intermediate
regressions. These are development fixtures, not GTOC performance results.

No third-party numerical or HiGHS dependency was added. Rust 1.85, Clippy,
rustdoc, typing, final wheel and extracted-source checks are documented in
validation-6-10.md. The per-component numerical engine remains dense.
