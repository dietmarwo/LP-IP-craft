# Iteration20: Avoid idle kernel resources

## Change and purpose

Create pools only when matrix/factor estimates can reach a retained parallel kernel, and reuse owners across certification/root LP phases.

## Tests and measurement gate

Measure tiny and sparse LPs, batches and large dense cases; exercise deadline/pre-cancel behavior and repeated independent callers. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
