# Native iteration 10 — bounded parallel components

Version 0.14.0. Use scoped standard-library Rust threads for independent
components, with at most 16 workers and no shared mutable dictionary. Only
work dispatch and one result per component cross threads. Keep small blocks
serial via a configurable dictionary-entry threshold. Default maximum workers
is 16; explicit component_threads=1 retains sequential execution.

Use a single whole-call deadline and the existing external cancellation token.
For parallel runs reserve node and pivot quotas per component: divide the
requested global budget deterministically, assigning remainder units by
component index. Unused quota is not redistributed. Sum of performed work
must never exceed the global node/pivot budget. This can stop earlier than
serial shared-budget execution; document the tradeoff. Memory cap is still
per dictionary, so worker count also controls peak memory.

Collect results by original component index, preserve full-vector/offset/dual
checks and conservative bounds, handle worker errors without treating an
unfinished component as feasible, and stop claiming new work after an error
or limit. In-flight solves keep their cooperative external cancellation and
deadline checks. Phase timings become sums of worker durations; total_seconds
remains wall time.

Test serial/parallel equality, both senses and LP/IP/MIP, low global budgets,
starts, cancellation, worker-count validation and memory controls. Run all
Rust/Python tests, 600 base and 180 component oracle cases in both modes,
Rust 1.85, Clippy, rustdoc, typing, clean wheel install and archive smoke tests.
Repeat the six-version benchmarks in rotating order without compilation;
report neutral/negative results and retain all raw timings. No HiGHS source,
FFI, C++ solver or third-party numerical dependency may reappear.
