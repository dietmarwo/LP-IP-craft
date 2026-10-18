# Iteration 06: Schedule expensive components first

## Change and purpose

Stable descending structural cost for parallel independent parts; component quotas remain keyed to original index.

## Tests and measurement gate

Retain original merge, starts, conservative bounds and exact per-part budget reservations; compare skewed and balanced blocks. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
