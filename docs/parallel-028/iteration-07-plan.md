# Iteration 07: Collect component results privately

## Change and purpose

Replace the shared result mutex with worker-local indexed outputs and a post-join linear gather.

## Tests and measurement gate

Check partial results and stop/error behavior with existing budget/cancellation tests and extra uneven-component cases. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
