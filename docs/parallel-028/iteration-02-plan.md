# Iteration 02: Remove batch result mutex

## Change and purpose

Keep dynamically claimed jobs and private solver instances, return indexed results in worker-local vectors, then gather into linear slots after join.

## Tests and measurement gate

Preserve per-job errors, original ordering, cancellation and all 1/2/4/8/16 existing widths; compare tiny and substantive batches. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
