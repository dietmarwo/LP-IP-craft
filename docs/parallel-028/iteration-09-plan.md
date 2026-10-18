# Iteration09: Collect tree outputs in linear slots

## Change and purpose

Replace frontier-result sorting with indexed linear collection while preserving original subtree merge order and quotas.

## Tests and measurement gate

Exercise fixed-work 1/2/4/8 tree results, infeasible branches, cancellation, and limits; compare fixed frontier work. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
