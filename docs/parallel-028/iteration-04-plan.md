# Iteration 04: Schedule large batch models first

## Change and purpose

Use a stable descending matrix-work estimate for heterogeneous large batches and gather by original input index.

## Tests and measurement gate

Verify independent status/objective and error order; compare a declared skewed fixture, including queue preparation. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
