# Iteration29: Document eight-thread API and reproducibility

## Change and purpose

Update Rust/Python docs, dependency notices, architecture and practical budget selection; retain all30 plans/results and raw evidence.

## Tests and measurement gate

Build documentation, check links/format/lints and reproduce a checkpoint/baseline build; ensure defaults and numerical caveats are explicit. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
