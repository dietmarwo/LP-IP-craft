# Iteration25: Make benchmark failures impossible to hide

## Change and purpose

Extend harness regression tests for nonfinite timing/certificates, incomplete job counts, missing records, child errors and oversubscription.

## Tests and measurement gate

Fault-inject each protocol failure and require persisted failure evidence and a nonzero driver exit, never a speedup. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
