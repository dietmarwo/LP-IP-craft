# Iteration19: Improve dense scalar tile locality

## Change and purpose

Evaluate row-tile traversal that reduces repeated panel reads while preserving each output's operation order.

## Tests and measurement gate

Measure scalar and eight-worker paths on multiple sizes; reject the candidate if it loses consistently or changes factor values. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
