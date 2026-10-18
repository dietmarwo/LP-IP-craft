# Iteration 20: Avoid idle kernel resources

## Change and purpose

Create pools only when matrix/factor estimates can reach a retained parallel kernel, and reuse owners across certification/root LP phases.

## Tests and measurement gate

Measure tiny and sparse LPs, batches and large dense cases; exercise deadline/pre-cancel behavior and repeated independent callers. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.

Adaptation: global numerical dispatch now requires at least 65,536 coefficient operations after iteration 17. Match pool eligibility to that retained threshold, so 192-square pricing cannot create unused resources; sparse models keep scalar fallbacks.

Measured adaptation before implementation: panel prototype 18 lost about 8% on dense384 eight-worker workflow, and row-local scalar traversal 19 showed no reliable scalar gain while eight-worker time also lost. Restore the original panel and scalar tile traversal, keeping useful disjoint-row Schur parallelism. Tags 18/19 and slower samples remain. Also skip resource acquisition when the deadline has already expired.
