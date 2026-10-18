# Iteration03: Bound batch task claims

## Change and purpose

Amortize atomic task claims in small bounded chunks for large homogeneous batches; retain single claims when there are few jobs.

## Tests and measurement gate

Check every index exactly once and retain mixed-cost coverage; measure many cheap jobs as well as expensive solves. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
