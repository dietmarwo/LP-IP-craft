# Iteration14: Parallelize dense column pricing

## Change and purpose

Partition independent nonbasic pivot-row column dot products and gather touched indices in original order after the join.

## Tests and measurement gate

Retain DROP, exact-zero, logical-column and basic-column rules. Compare scalar/parallel alpha, supports and solver paths. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
