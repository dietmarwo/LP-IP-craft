# Iteration 10: Collect residual outputs in linear slots

## Change and purpose

Replace per-round residual sort with bounded indexed slots, preserving omitted/skipped tasks and deterministic repair merge.

## Tests and measurement gate

Check identical fixed-work accepted points, task counters and bounds for workers 1/8; compare residual tasks and cheap overhead. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
