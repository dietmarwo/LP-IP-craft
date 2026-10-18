# Iteration 28: Run application and held-out matrix

## Change and purpose

Exercise public MIPs and available real SpOC4 workloads with identical budgets, original checks and setup-inclusive timings.

## Tests and measurement gate

Publish all optimal/limited/error outcomes separately from throughput; report supported scope and actual application gains or losses. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.

Before measuring applications: use the original checked public/held-out JSON and both GPL-attributed SpOC4 matching tables. SpOC4 uses 64 fixed primal-dual steps with a sufficient root-work cap and no proof phase, optionally 16 fixed64-column residual tasks. Compare whole API time, construction-inclusive workflow and reported root phase separately; validate original triples/resources and preserve all limited/error outcomes. Compare exact current upstream scalar and existing outer 8 to new scalar/kernel8/outer8 where supported. No claim that equal time limits imply equal completed work.
