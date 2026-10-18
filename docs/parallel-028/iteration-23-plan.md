# Iteration23: Balance irregular column dispatch

## Change and purpose

Evaluate nonzero-weighted column partitions for reduced-cost/pricing work, limiting task creation and keeping scalar order.

## Tests and measurement gate

Test empty/skewed columns and compare uniform/irregular costs; retain only if scheduling/allocation costs are justified. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.

Before implementation: retain a compatible batch-owned numeric resource across mixed-budget jobs. Its cold setup is part of total batch workflow, while deadlines remain per individual solve. This completes the reuse path for iteration22, which already added budget validation.
