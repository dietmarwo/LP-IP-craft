# Iteration17: Tune dense update dispatch thresholds

## Change and purpose

Use measured remaining cubic/block work to avoid late tiny parallel updates; preserve a cheap scalar path.

## Tests and measurement gate

Measure 1/2/4/8 at frozen development sizes plus predeclared heldouts; keep slower observations and revert thresholds if no reliable gain. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
