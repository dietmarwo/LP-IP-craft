# Iteration27: Tune measured outer scheduling

## Change and purpose

Use all preceding evidence to choose bounded claims/order and small-task fallbacks; remove unhelpful scheduling prototypes.

## Tests and measurement gate

Run paired scalar/eight, batch/components/tree/residual and cold/warm cases; no adjacent-ratio multiplication or cherry-picked replacement. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
