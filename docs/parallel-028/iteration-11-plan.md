# Iteration 11: Add bounded reusable numeric execution

## Change and purpose

Introduce a small safe local execution module using locked pure-Rust Rayon, explicit widths 1..8 and weak pool sharing; never alter its global pool.

## Tests and measurement gate

Test ordered maps, disjoint slices, panic recovery, resource sharing and invalid widths. Default 1 starts no workers; measure startup separately. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
