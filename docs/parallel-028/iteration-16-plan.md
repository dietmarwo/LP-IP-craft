# Iteration16: Parallelize blocked Schur updates

## Change and purpose

Partition disjoint trailing dense rows, sharing the read-only solved panel. Keep each cell's original panel accumulation order and scalar pivot choice.

## Tests and measurement gate

Compare full factor bytes/permutations and forward/transpose residuals, singular fallback and entry caps; measure standalone factor and LP time. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
