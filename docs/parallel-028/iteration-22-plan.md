# Iteration22: Add bounded mixed outer/inner batches

## Change and purpose

Support explicit outer*inner<=8 native batch budgets while suppressing other nested mechanisms; preserve previous16-worker serial-inner API.

## Tests and measurement gate

Test allowed1x8/2x4/4x2/8x1 and rejected oversubscription, including Python validation and ordered errors; measure fixed total work. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
