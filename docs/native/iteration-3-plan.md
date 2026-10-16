# Native iteration 3 — pivot kernel and pricing

Version 0.7.0. Replace the allocating reference pivot with an in-place
Gauss–Jordan update. Normalize the pivot row once and skip only exactly zero
row factors; never drop a small coefficient based on magnitude. Keep the
reference kernel selectable for numerical differential tests.

Use most-negative-reduced-cost pricing to reduce expensive pivots, with
Bland fallback after repeated objective stalls. Keep deterministic tie
breaking, pivot/time/cancellation budgets and original-unit acceptance checks.
Expose both switches in Rust and Python for reproducibility.

Compare fast/reference pivot kernels and pricing modes on bounded random
LPs and exhaustive binary fixtures; run all Rust/Python tests and the external
400-case oracle. Measure the fixed release fixtures against iteration 2,
recording pivot counts as well as time. No wall-time CI assertion.
