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

Step 1 outcome: 98 Rust/all-feature checks pass. Captured 21 actual bases across
irregular-1024/4096, packing-4096 and network-2048. Every saved RHS in both
orientations passes independent backward-error checks in three retained rounds.
Capture solves pass original-model LP checks. Detailed timing and fingerprints
are retained in the replay evidence. A build-time integer type inference issue
was fixed before capture. Normal builds have no capture filesystem code.

## Step 2 — plan

Replace sparse row/column linear searches with cross-indexed entries: each
column entry records its row-list slot, and each row entry records its column
slot. Swap-removal updates both back-pointers. Keep the same pivot/candidate
order and arithmetic order, and cache column maxima with invalidation on edits.
Reuse the active matrix and scatter/pivot scratch buffers between refactors.

Test: all frozen bases must retain pivot fingerprints and factor entry counts,
with forward/transpose residuals passing; test insertion/removal/cancellation
metadata directly, workspace reuse across dimensions, full Rust tests, and paired
replay timing. Measure memory as well as time because indices add storage.
