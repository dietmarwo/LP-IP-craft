# Iteration15: Pass execution through basis factorization

## Change and purpose

Thread the explicit reusable execution reference through LU dense-tail dispatch without moving sparse pivot selection or changing its arithmetic.

## Tests and measurement gate

Run full factor/update/singularity/cap regression suite and verify no dense resource is needed for small or sparse factors. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
