# Irregular LP factorization work

Baseline: native 0.18.0, commit 1d6463dbcad0574c314e8f52aa7cba83108c7802.
Every step records its plan before implementation; results follow after testing.
The production core/native remain pure Rust without external numerical dependencies.

## Step 1 — plan

Capture selected actual basis matrices before refactorization, with recent pricing
and primal/dual work vectors plus deterministic sparse/dense controls. Add an
optional, dependency-free binary capture/replay API and a JSON benchmark driver.
Capture is disabled in normal builds. Split factorization into assembly, pivot
search, pivot extraction, sparse elimination, dense elimination and final packing.
Freeze early/middle/late irregular bases at 1024 and 4096 rows and sparse controls.
Verify both forward and transpose residuals on identical saved inputs; retain
baseline factor-entry counts, pivot fingerprints and timing distributions.

Test: Rust workspace/all-feature tests; replay all captured bases; independent
LP acceptance while capturing. Reject malformed replay files safely. Do not use
instrumented measurements as final production speed claims.
