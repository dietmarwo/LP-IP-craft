# Iteration 01: Freeze eight-worker performance harness

## Change and purpose

Add a native common benchmark with frozen known-objective LP/MIP fixtures, ordered batches and balanced/skewed components. Record all outcomes, residuals, nodes and pivots; timed scope includes orchestration. Add isolated pinned Python timing with warmup, repeats and process CPU time.

## Tests and measurement gate

Require exact known objectives, original-unit feasibility and LP KKT gates. Exercise baseline scalar and existing eight-worker paths, keeping the upstream numerical implementation untouched. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.
