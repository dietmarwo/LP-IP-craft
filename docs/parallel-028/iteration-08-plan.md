# Iteration08: Use a serial residual fast path

## Change and purpose

Avoid spawning an OS worker when a repair round has only one effective residual worker; preserve ordered snapshots and checks.

## Tests and measurement gate

Keep fixed-work packing incumbents and proof bounds unchanged; run existing residual/repair tests and compare single-worker overhead. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
