# Iteration29: Reduce packing synchronization and document APIs

## Change and purpose

Update Rust/Python docs, dependency notices, architecture and practical budget selection; retain all30 plans/results and raw evidence.

## Tests and measurement gate

Build documentation, check links/format/lints and reproduce a checkpoint/baseline build; ensure defaults and numerical caveats are explicit. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.

## Application-driven adaptation before implementation

The first fixed-work SpOC4 campaign reports a real loss, not a gain: instanceI root phase24.99ms scalar versus48.28ms at8, instanceII115.76 versus133.94ms (one measured sample; exploratory). Per-iteration calls enter a pool from an external thread three times and materialize a gradient vector. Run the complete root iteration loop inside its owned pool context once. Fuse independent column gradients with their primal coordinate projection, retaining each original row-ordered sum; join before row projections. Preserve the exact original scalar loop when no pool is selected. This removes one gradient-buffer pass and one synchronization phase per iteration, and avoids repeated external pool entry. Require unchanged35-step hints/prices/bounds/work at1/2/4/8 and rerun actual SpOC4 before release. Preserve all slower observations.
