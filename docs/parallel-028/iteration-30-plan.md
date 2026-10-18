# Iteration30: Freeze0.28 and complete release gates

## Change and purpose

Bump version0.28.0; run full Rust/Python/lint/doc, independent audits and rotated repeated final timing, then package clean source/history/patches with hashes.

## Tests and measurement gate

Verify every checkpoint's tests and failures, exact patch application and all archive bytes. Report measured improvements and regressions, never thirty monotone speedups. Run the full regular release Rust suite and build the release harness before paired timing. Every timed solve must pass its frozen known-objective and original-model residual checks. Record all samples, CPU time, hashes and failures. Compilation and numerical stress must not overlap timings.

## Decision

Accept only a sound capability or a measured reduction of useful work; otherwise revert the prototype and preserve its evidence. Cheap fallbacks and the one-thread default remain. Final results will distinguish noisy/negative observations from retained wins.

## Final measured adaptations before freeze

Three-repeat29 confirmation: SpOC4II root104.87ms upstream/119.59ms candidate1/63.13ms candidate8, API232.60/232.44/181.44ms; instanceI has no reliable eight-worker benefit. Keep an exact original scalar root function outside the pool closure, and use it for root workloads below131,072 nonzeros. Skip idle pools for such packing-only proof-disabled requests. Large roots retain the fused, once-entered pool loop. Restore the direct one-thread dispatcher instead of cloning all native settings for every default/batch job. These are final fallback/setup reductions, tested by the full release gates and independent three-way audit before measurement.

Final evidence storage must preserve every actual application point without multiplying repeated92,103-column vectors in a large JSON file. Dehydrate successfully checked point vectors to SHA-verified little-endian float64 files; every solve still runs, and records keep point hash/length/reference. Failed child output remains intact. Final application audit requires identical fixed-work SpOC4 points, objectives, bounds and node/pivot counts; public limits/errors stay separate from successful latency.

## Full-matrix correction before final acceptance

The initial complete30 campaign found substantial existing-fixture regressions: packing128×64 serial34.50→64.37ms, seven-repeat packing128×128/outer8 15.75→24.37ms, and skewed components/outer8 8.62→13.60ms against upstream. All solves/points/work counts were unchanged. The column-pricing fallback was doing a full basic-column clear plus a second touch-gather pass instead of the old single basic-aware loop. Preserve the exact original reduced-cost and column-pricing loops whenever actual nonbasic work is below the parallel gate or no numerical pool exists. Keep the independent-output parallel branch only for substantial work. Retain the full initial campaign/audit and source tag30-initial, then rerun all final gates and matrix on the corrected source. No source or slower sample is discarded.

## Build-provenance correction

The second complete matrix still showed inflated small Rust timings. The release runner copied the example alias after forced all-target/all-feature tests had rebuilt it with kernel-profile instrumentation (which measures every kernel entry). This confounds those Rust before/after comparisons; neither large timing gap is attributed solely to the scalar-loop change. Retain both instrumented campaigns and exact feature fingerprints, exclude them from production claims, rebuild the intended parallel-bench-only examples *after* all gates, and explicitly report/reject kernel-profile in the protocol. Add a real profiled-binary negative test and rerun all Rust timing datasets. The numerical source, nonprofiled Python extension/application results and full independent audits are unchanged. Separately rebuild the pre-scalar-fix source without profiling to isolate that work change under matched features.
