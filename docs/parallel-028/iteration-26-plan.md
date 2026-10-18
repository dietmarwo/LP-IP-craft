# Iteration26: Independent serial/eight numerical audit

## Change and purpose

Run a frozen generated LP/MIP corpus with independent SciPy/oracle checks under upstream, new1 and new8, plus public cases and conservative bound checks.

## Tests and measurement gate

Compare all statuses, acceptance, objectives and bounds with explicit tolerances; preserve known failures/limits and no false success counts. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
