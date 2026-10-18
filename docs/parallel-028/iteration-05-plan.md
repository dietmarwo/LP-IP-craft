# Iteration 05: Let the batch caller participate

## Change and purpose

For multiple workers, spawn at most workers-1 helpers and solve one worker loop on the caller.

## Tests and measurement gate

Count concurrent work at or below the requested width and preserve join/panic/error handling; compare small-batch startup. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
