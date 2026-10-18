# Iteration30: Freeze0.28 and complete release gates

## Change and purpose

Bump version0.28.0; run full Rust/Python/lint/doc, independent audits and rotated repeated final timing, then package clean source/history/patches with hashes.

## Tests and measurement gate

Verify every checkpoint's tests and failures, exact patch application and all archive bytes. Report measured improvements and regressions, never thirty monotone speedups. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
