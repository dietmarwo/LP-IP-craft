# Iteration 13: Parallelize fresh reduced costs

## Change and purpose

Partition independent structural reduced-cost outputs for sufficiently large matrices; retain each column's scalar subtraction order.

## Tests and measurement gate

Compare bitwise outputs for 1/2/4/8 with basic/logical, zero and irregular columns; test complete solves and changed costs. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
