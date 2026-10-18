# Iteration 24: Stress concurrent solves and isolation

## Change and purpose

Add simultaneous independent callers with total budget 8, immutable shared models, changing dimensions/options and cancellation/errors followed by recovery.

## Tests and measurement gate

Require status/objective/bound agreement, valid candidates and no shared mutable numerical workspace; execute compiled Python callers too. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
