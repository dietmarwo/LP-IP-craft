# Iteration12: Expose native numerical thread budgets

## Change and purpose

Allow Options.threads1..8; dummy still rejects>1. Propagate budget into LP controls, hold compatible resources for large solves, and serialize nested inner work in existing outer executors.

## Tests and measurement gate

Test Rust/Python contract and nesting; validate input before resources; keep all mathematical options/defaults and native16-worker outer APIs. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
