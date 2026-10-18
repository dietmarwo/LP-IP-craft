# Iteration21: Add kernel scaling and reuse measurements

## Change and purpose

Expand the harness with actual dense factor+solve work, fresh matrices, cold-resource and reused-resource timing, and full CPU accounting.

## Tests and measurement gate

Require analytical forward/transpose checks and actual finite timing fields; distinguish standalone kernel gains from full solver/application performance. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
