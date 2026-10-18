# Iteration 18: Parallelize dense panel row updates

## Change and purpose

After ordered pivot selection/swaps, distribute sufficiently long independent elimination rows within a panel.

## Tests and measurement gate

Require factor/permutation identity and solve residuals; test pivot swaps and small tails. Retain only useful work-size regimes. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
