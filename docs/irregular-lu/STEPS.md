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

Step 2 outcome: all Rust tests pass. All 21 frozen cases retain exact pivot
fingerprints and factor-entry counts in the first paired replay. Search and
extraction improve, but extra metadata regressed sparse elimination and the
unchanged dense tail dominates overall time. Replaced native-sized links with
checked 32-bit links, restoring the old active-entry footprint; retained the
first attempt as evidence. The compact variant is replayed separately. Workspace
reuse is tested across dimensions while prior factors remain independently usable.

## Step 3 — plan

For dense tails of at least 64 rows, factor a compact row-major block with
partial row pivoting and blocked panel/trailing updates. Keep the block for
forward/transpose solves instead of packing every numerical entry into sparse
lists. Preserve sparse prefix interactions and conventional eta updates. Expand
the block into sparse factors only when forced Forrest–Tomlin needs it.
Keep the old dense path for small tails, singular fallback, and entry-cap cases
where dense storage would exceed the configured cap. No BLAS/native dependency.

Test: direct dense/mixed-prefix residuals, singular and entry-cap behavior,
eta/FT updates and reoptimization, all frozen RHSs, full Rust/Python/oracle gates.
Compare factor and triangular-solve times on the frozen matrices and full LPs.

Step 3 outcome: frozen replay residuals all pass. The latest irregular-4096
basis drops from 1151 ms to 329 ms factorization and 38 ms to 29 ms for its RHS
set in the paired development run. All Rust tests pass, including retained-tail
eta and forced-FT updates. A full irregular-4096 LP is independently accepted in
7.54 seconds (one retained development solve; final paired results still pending).
The 32-bit-link step-2 variant still did not improve overall replay time; its
regressions are retained, and the dense kernel is responsible for the net gain.

## Step 4 — plan

Add bounded approximate minimum-degree column ordering using merged symbolic
row elements (implicit cliques), ignoring structurally dense rows and bounding
symbolic work. Use it as a candidate priority while preserving Markowitz costs
and numerical thresholds. Cache ordering across small changes in basis pattern.
Expose natural/auto/min-degree and candidate-search budget controls for ablation.
Compare search budgets and ordering on identical frozen bases and full LPs;
select defaults from end-to-end results, not factor counts alone.

Test: valid deterministic permutations, dense-row/work-budget behavior, changed
patterns and cache reuse, residuals on all saved matrices, full solver acceptance,
Rust tests and Python controls. Retain slower policies as explicit options and
record rejected tuning trials. No weakening of pivot or final checks.

Step 4 status: minimum-degree advice and configurable search compile and pass
all Rust tests and frozen residual checks. The ordering has setup cost and is
not uniformly beneficial: the late 1024-row replay becomes slower, while the
late 4096-row replay is roughly flat. Full policy sweeps determine defaults;
explicit controls retain the implementation for application-specific use.

## Step 5 — plan

Measure fixed rebuild intervals (32/64/128/256) and compare a work-based policy.
Count actual eta arithmetic visited by FTRAN/BTRAN, and estimate rebuild work
from sparse Schur products plus the dense-tail cubic term. For sufficiently
large dense tails, rebuild when accumulated update work consumes a calibrated
fraction of rebuild work; retain minimum update spacing, entry cap, numerical
fallbacks and the configured hard maximum. Keep short intervals for sparse
bases whose updates are already cheap. Expose an ablation switch and select
public defaults using the full solver benchmarks.

Test: deterministic trigger/cap tests, frequent-refactor and warm reoptimization
oracles, forced FT and natural/min-degree controls, independent LP KKT checks,
conservative-bound fixtures, Rust/Python/Clippy gates. Final comparisons use fresh
uninstrumented 0.18/new/HiGHS processes, including the large irregular fixture.
Package complete source, plans, raw evidence and reproducible benchmark tools.

Step 4 outcome: all ordered replays and full policy solves pass their checks.
Search 8/16 and forced minimum-degree advice do not give a robust overall win.
Keep search=4. Auto ordering is restricted to m>=2048 after a prior dense tail
larger than m/8; smaller cases retain natural order. Forced ordering remains
available and is covered by a complete 6330-model oracle pass.

Step 5 outcome: fixed intervals 32/64/128/256 and two work budgets were measured.
The first adaptive policy regressed the smaller irregular fixture; corrected by
restricting work-based rebuilding to m>=2048 and dense tail >=m/8. Final policy
requires at least 32 updates and rebuilds when visited eta work exceeds one
quarter of estimated factor work. The hard maximum is 256; other bases retain
an internal maximum of 64. Existing entry/fill limits and numerical fallbacks
still override these estimates. The corrected large development case uses 60
rather than 67 rebuilds and runs in 6.78–7.01 s (final fresh comparisons in RESULTS).

Final gates: 104 Rust/all-feature tests, 169 Python tests, strict Clippy,
four complete 6330-model oracle passes (default/FT/ordering/fixed128), 1200 exact
frequent-refactor forced-FT cases, 180 parallel-component cases, and 400 exact
conservative-bound safety checks. The 19 known oracle disagreements in each
full pass are the identical previously adjudicated cases; no new discrepancy.
Twelve medium dense LP solves with planted primal/dual certificates exercise
retained dense tails, ordering and forced FT through the full solver.

A final focused test verifies the blocked-tail singularity fallback and the
tight-cap path where sparse legacy storage fits but a retained dense block does
not. Both pass; the final complete Rust gate contains 105 checks.
