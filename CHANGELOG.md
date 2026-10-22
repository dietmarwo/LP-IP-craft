# Changelog

All notable changes to LP-IP-craft are documented here. Versions are the
workspace version in Cargo.toml (Rust crates, Python package lp-ip-craft and
the job worker share it). No version has been published to a registry yet;
dates are those of the commits that set each version.

Versions 0.4.0-0.15.0 and 0.21.0-0.23.0 were developed outside this
repository and never set on its main branch. They arrived with the imports
that set 0.16.0 and 0.24.0 and are summarized in those sections. References
such as (#5) are pull requests.

## [Unreleased]

Everything merged after the commit that set 0.28.0 (#11-#20), the
release preparation and the fixes of the agent-discovery review. The code of
the numerical crates and the Python package is unchanged since 0.28.0; the
crates gained publishing metadata, and the Python tests gained the discovery
and MPS converter tests.

### Added

- `lp-ip-job-worker`, a worker for the shared agent job API (#11). Profile
  `lp-ip.solve/1` solves SHA-256-checked sparse CSR model artifacts
  (`lp-ip.model/1`) with complete MIP starts, node and pivot budgets and
  `inner_threads` 1-8, and rechecks each result independently. A model the
  native solver refuses (for example a MIP with a free continuous column)
  completes as `termination: "unsupported"` with zero work instead of
  failing the job; `presolve: false` also clears the reductions it switches
  off, so the reported effective options are true; native settings are
  validated when the job is prepared. Runnable tutorial in
  `tutorials/job-api/`. The CI workflow `job-api.yml` checks the job layer
  with Rust 1.88; the numerical crates keep Rust 1.85.
- Result contract 0.2 in the worker (#15): `infeasible`, `unbounded` and
  `infeasible_or_unbounded` claims, `objective_bounds`, evidence with the
  recomputed checks (an LP optimum's duals are rechecked with the native KKT
  acceptance) and refutations of a native status that fails its check
  (`infeasible_point`, `false_optimality`, `false_infeasibility`). New
  profile `lp-ip.evaluate/1` evaluates given points without solving.
- `optimization-mcp`, the MCP server over one node, built by
  `cargo build -p optimization-runner` (#18). CI runs the shared MCP tests
  (`test_mcp.py`) with this worker.
- `ai-context.md`: operational context for AI agents (when the solver fits,
  problem card, modelling in Rust and Python, options, statuses, independent
  checks, parallel layouts, job profiles) (#12). Corrected in #13 (for
  example, `presolve=false` skips the whole root stage, so a MIP then needs
  finite bounds as given) and brought up to date in #20 (result contract
  0.2, optimization-jobs, the MCP server, the sibling SQP-craft and
  fcmaes-rust workers).
- Release workflows modeled on fcmaes-rust, with `RELEASING.md`:
  `python-release.yml` builds abi3 wheels (CPython 3.10+) for Linux x86_64,
  macOS x86_64 and arm64 and Windows x86_64 plus the source
  distribution, installs and tests each one, attests them and publishes
  `lp-ip-craft` to PyPI through Trusted Publishing on a `v*` tag that equals
  the workspace version (a manual run builds and tests without publishing).
  `publish-crates.yml` checks and publishes `lp-ip-craft` and
  `lp-ip-craft-native` to crates.io once the repository variable
  `CRATES_IO_TRUSTED_PUBLISHING` is `true`; until then it runs every check
  and skips the upload.
- The two library crates are publishable: crates.io metadata, a README and
  the license in each crate, and a version on the native crate's path
  dependency. The Python package and the worker stay unpublished crates.
- README section "Install from source": requirements (Rust 1.85, 1.88 for
  the job layer, no C/C++ toolchain, CMake or BLAS), the Python build, the
  job worker with its git dependency on optimization-jobs, and the crates
  as git dependencies.
- `ai-context.md` points to the AI entry point of optimization-jobs, which
  chooses between the four solver repositories, and says what the release
  workflows will publish.
- Job discovery (review finding F01): the worker's `describe` adds
  `payload_schemas` (JSON Schema draft 2020-12 of the `lp-ip.solve/1` and
  `lp-ip.evaluate/1` payloads, strict as the worker), `input_schemas` (the
  `lp-ip.model/1` artifact, with the rules across fields in `$comment`) and
  `examples` (a continuous LP, a MIP with a complete start and an evaluate
  job, each on 1/1/1 resources with its model inline), so a client without a
  checkout can build and validate one job per profile. The files are in
  `crates/lp-ip-job-worker/discovery/`. Rust tests turn every example into a
  job with hashed input files and check its outcome; `tests/test_job_discovery.py`
  validates every example and tutorial payload and model with `jsonschema`
  and, in the job API CI, that `describe` embeds the files.
- `scripts/mps_to_model.py` (review finding F13): converts fixed and free
  MPS files (also gzipped) to `lp-ip.model/1`, with OBJSENSE, RANGES, integer
  markers and the bound types UP, LO, FX, FR, MI, PL, BV, LI and UI; the
  objective constant is `-RHS` of the objective row, and a negative `UP`
  without a given lower bound frees the lower side (CPLEX's convention). It
  writes the CSR by rows in MPS order, prints the file's SHA-256 and
  optionally the row and column names; unsupported input (SC bounds,
  quadratic sections, an infinity on the wrong side, duplicates) fails with
  the line number. Tested on fixtures in `tests/fixtures/` against
  hand-computed optima.
- `AGENTS.md`: what the repository is, what to read first, build and test
  commands and conventions. The README links it and `ai-context.md`.
- `ai-context.md`: a table of what `lp-ip.solve/1` exposes, forces and leaves
  at native defaults compared with the in-process options; a recipe for many
  independent models on many cores (the batch cap of 16, one model per job,
  `component_threads=1` for callers that own the concurrency, why identical
  null starts do not scale); the discovery keys and MCP `put_input`; the MPS
  converter.
- The test extra `.[test]` adds `jsonschema>=4.18`; the job API CI installs
  `pytest` and runs the discovery test against the built worker.

### Changed

- Discovery: the README opens with the title and a short description, then
  links the shared solver-selection guide and the ecosystem overview; both
  crate READMEs link them too. `pyproject.toml` adds keywords and project
  URLs (documentation, issues, changelog, ecosystem, agent guide).
  `RELEASING.md` covers discovery after a release.
- The README opens with a banner (`docs/images/lp-ip-craft.png`). It is
  linked by absolute URL, so the PyPI description shows it too.
- Breaking: the worker speaks worker protocol `optimization.worker/0.2` and
  publishes results as `optimization.result/0.2` (shared job crates 0.2.0,
  #15). A 0.2 runner still accepts 0.1 workers; a 0.1 runner rejects 0.2
  workers at registration. The job envelope stays `optimization.job/0.1`.
  Runner errors carry codes, and JSON Schemas describe jobs and results.
- The shared job crates `optimization-job-api` and `optimization-runner`,
  imported in-tree at 0.1.0 (#11) and synchronized as 0.1.1 with fcmaes-rust
  and SQP-craft (#14; zero-work repair of a queued job whose launcher died,
  shared runner regression tests in CI), were removed from the repository
  together with the shared guide, plan, JSON Schemas and scripts (#17). The
  workspace depends on github.com/dietmarwo/optimization-jobs at a pinned git
  revision instead. `cargo build -p optimization-runner` still builds
  `optimization-run`; CI and the tutorial run the shared harness from
  Cargo's checkout (`scripts/job-api/shared_root.py`); `docs/job-api.md`
  points to the shared guide.
- The pin moved from `9feb72e` (#17) to `0ee495f` (MCP server, #18),
  `eb1c9de` (`Outcome::check` rejects contradictory bounds, #19) and
  `6a42472` (#20): worker-declared artifacts and paged artifact reads
  (`jobs artifact --offset/--limit`, MCP `get_artifact` with `offset` and
  `limit`), which also page through the `results` journal of a running job.
  Then `423cd87`: `artifact::page` streams the file and parses only the
  page's records. This worker's profiles are unchanged by the pin moves.
- optimization-jobs pin `069d87c` (its review of agent discovery):
  `optimization-mcp` gains `put_input` (stages an input file, such as a model
  or a CSV, and returns its `sha256` and `input_dir`) and `get_guide` (the
  cross-repository AI entry point); `node doctor` reports the host's usable
  CPUs and memory; `workers add` accepts up to 1 MiB of `describe` output;
  and the shared harness, which CI runs from Cargo's checkout, checks the
  worker's payload schemas and runs its examples, also over MCP.
- Shared job API docs and harness synchronized with fcmaes-rust (#16).

### Fixed

- Review LP-01 (#19): after refuting an optimum, the worker still published
  the solver's bound and a gap computed from the solver's own objective, so
  a minimization with primal 7 could report dual 8 and a zero gap. The gap
  is now recomputed from the shown point's objective (an overflowing gap is
  left out instead of failing the task). A bound beyond that objective by
  more than `BOUND_TOLERANCE` is kept only in `details.reported_best_bound`
  with a `bound_note`. An optimum without KKT confirmation whose candidate
  does not evaluate to the solver's objective is refuted
  (`objective_mismatch`).

## [0.28.0] - 2026-10-18

Thirty checkpoints toward one eight-worker CPU budget (#10, one squashed
commit), on top of the MIP iterations (#8) and the presolve default change
(#9) merged after 0.27.0. The default numerical width stays 1.

### Added

- `Options.threads` is a maximum native numerical width of 1-8 (default 1);
  before, any value other than 1 was `Unsupported`. 0 is invalid, values
  above 8 are `Unsupported`, the dummy backend accepts only 1, and the
  effective width is reported as `numeric_threads`. A width above 1
  serializes component, residual and tree workers.
- Parallel numerical phases that keep each value's operation order:
  structural reduced costs and dense column pricing over disjoint columns,
  dense-LU trailing Schur rows after the ordered panel, and the packing
  primal-dual root loop. Dispatch is gated by actual work, so small work
  runs the original scalar loops. Sparse pivot selection, triangular solves,
  certification and acceptance stay ordered.
- Mixed batches: `solve_batch(models, workers, options=Options(threads=k))`
  and `NativeSolver::solve_batch` accept outer x inner <= 8 (1x8, 2x4, 4x2,
  8x1). The serial-inner limit of 16 outer workers is unchanged.
- Opt-in MIP policies from twenty iterations on the open 0.27 issues (#8):
  a RENS sub-MIP in `root_heuristics` (`diagnostics.root_rens_nodes`,
  `root_rens_candidates`); probing on up to 2,000 binaries in the first MIP
  presolve round (`diagnostics.mip_presolve_probed`, `probe_fixed`,
  `probe_tightened`); c-MIR divisors from variable-upper-bound slopes; a
  warm-started feasibility-pump projection LP. With GMI and MIR both on, a
  family that contributes no selected cut is no longer separated, and cuts
  with more than max(50, n/4) terms are not selected while both are active.
  Rejected experiments are recorded in `docs/mip-iterations/ITERATIONS.md`.
- Benchmarks: frozen synthetic MIP development and test sets (six classes,
  four seeds each, optima proven by HiGHS 1.15.1; `run.py --synthetic`), the
  harness policies `presolve-nodual` and `legacy`, and a fail-closed scaling
  and oracle harness (`scripts/parallel028*.py`).

### Changed

- Breaking: `mip_presolve` and `coefficient_tightening` are on by default in
  Rust and Python (#9). Both keep every feasible point; `dual_presolve`, cuts
  and root heuristics stay off. Pass
  `mip_presolve=False, coefficient_tightening=False` for the 0.27 behaviour.
  No benchmark case is lost (shifted geometric mean x0.62 on the public set,
  x0.99-x1.02 elsewhere); on the frozen oracle no status changes, while 165
  node counts, 233 pivot counts and 33 objectives (within tolerance) do.
- `lp-ip-craft-native` depends on Rayon 1.12.0 (pinned) for local pools with
  an owned width; no global Rayon pool is configured. The core crate stays
  dependency-free, and no HiGHS, BLAS or C/C++ solver is linked.
- Batch, component, tree and residual execution collect results privately
  and merge them in order with bounded task claims; expensive batch
  models and components are dispatched first (components not under explicit
  node or pivot limits).
- Search-identical speedups from #8: in-place and stack-held exact sums and
  indexed multipliers for GMI, an allocation-free gcd on the default
  rational-witness path, and in-place probing (MIP presolve of a 200k-column
  cover model 1.13 s -> 0.19 s).
- On eight pinned CPUs, 16 independent dense384 solves take 379 ms at
  8 outer x 1 inner against 1,863 ms at 1x1. At width 8 instead of 1, the
  dense1024 LP takes 2,568 ms instead of 3,345 ms (with more CPU time) and
  the SpOC4 II root phase 76.1 ms instead of 102.9 ms. Small and medium
  cases can be slower (held-out dense256 38.6 ms -> 46.2 ms). An independent
  differential check against the pre-0.28 code found bit-identical results
  at widths 1, 2, 4 and 8.

### Fixed

Found in the review of the delivered 0.28 code, before the merge:

- Python `solve_batch` ran every job at width 1 and accepted `threads=0`; it
  now passes `Options.threads` and validates it as a direct solve does.
- The packing root runs on an exclusive pool above its 131,072-nonzero gate,
  so a concurrent caller's waiting worker can no longer pick up the root
  loop and miss its own deadline.
- A panic in the batch or residual share run on the calling thread becomes
  `Error::Backend`, as on spawned workers, instead of unwinding.
- The benchmark drivers run on macOS and Windows (CI failed there), and two
  error messages had lost the space before a number.

## [0.27.0] - 2026-10-17

Opt-in MIP presolve, exact GMI/MIR root cuts and checked root heuristics,
informed by ripsolve (#7). Generic search defaults are unchanged.

### Added

- MIP presolve, off by default in this version: `mip_presolve` (bounded root
  fixpoint with exact integer and outward mixed propagation; rows proven
  redundant over the box are freed), `mip_presolve_rounds` (8, 0-32),
  `coefficient_tightening` (binary big-M reduction with an exact i128 path)
  and `dual_presolve` (optimum-preserving empty and singleton objective
  fixes).
- Exact root cuts, off by default: `gmi_cuts` (GMI from advisory
  optimal-basis rows after exact reaggregation), `mir_cuts` (MIR with
  certified variable-upper-bound substitution), `mip_cut_rounds` (8, 0-50),
  `mip_cut_limit` (16 per round, 1-64) and `cut_exact_work` (4,000,000
  charged limb operations per separation call). Cut rows extend the retained
  root basis; `diagnostics.root_stages` records each checked round.
- `root_heuristics` (checked rounding, guided diving, L1 feasibility-pump
  projections; `heuristic_lp_calls`, default 16) and the branching
  experiments `branching_score` (`weighted_sum` or `product`),
  `pseudocost_min_step` and `branch_trace_limit` (`diagnostics.branch_trace`).
- Benchmark-only comparison harness `benchmarks/rust-solvers/` with ripsolve
  0.4.0 pinned and a HiGHS driver.
- p0548 with presolve, GMI and MIR proves 8691 in 1.70-1.72 s (before: 60 s
  time limit with incumbent 28397). Combined cuts regress bell5 and many easy
  held-out models, so they stay opt-in.

### Changed

From the review before the merge:

- GMI and MIR are derived in exact dyadics (`dyadic.rs`) without gcd
  normalization, and coefficient tightening's exact fallback uses the same
  path (p0548 combined 2.0 s -> 0.4 s). Advisory multipliers are rounded to
  an integer grid, fixed terms are left out, and each coefficient is rounded
  to the binary64 neighbour with the smaller box compensation, so a variable
  with one infinite bound no longer rejects a cut.
- Cut rows strictly slack at the final root optimum are removed before the
  search (`diagnostics.root_cuts_pruned`); the root bound is unchanged. bell5
  with combined cuts proves optimality in 57,144 nodes (delivered build:
  100,000-node cap without proof); rgn GMI-only 3.2 s -> 0.84 s.

### Fixed

- The checked root bound of the cut model was merged into every child
  result, so infeasible or unbounded outcomes came back with a finite
  `best_bound`; only limit, failure and optimal results take it now.
- `diagnostics.branch_trace` wrote non-finite values as `inf` and `NaN`,
  which is not JSON; they are `null` now.
- Coefficient tightening rebuilt the matrix after every rewritten row
  (quadratic in the row count on big-M models); it rebuilds once per pass,
  with unchanged results.

## [0.26.0] - 2026-10-17

Opt-in controls for the packing repair (#6, merged together with 0.25.0).
Defaults keep the 0.25 behaviour.

### Added

- `packing_combine_repairs` merges compatible improving repairs from one
  batch snapshot and keeps the best standalone repair as a fallback.
- `packing_rounds` (1-32) runs successive repair rounds with refreshed
  snapshots, `packing_neighborhood_growth` (0-1024) enlarges neighborhoods
  per round, and `packing_seed_strategy` (`incumbent`, `rejected`, `mixed`)
  adds rejected-transfer seeds ranked with root prices.
- `packing_residual_cliques` and `packing_clique_work` insert bounded local
  clique cuts inside residual repairs.
- `packing_repair_seconds` and `packing_proof_seconds` cap the repair and
  proof phases separately; `packing_price_diagnostics` exposes the best
  checked resource prices. New `diagnostics.packing_*` counters and a
  per-round trajectory.
- Eight rounds of 64 rejected-transfer neighborhoods (four workers, merging
  on) reach 32,847.565 on SpOC4 matching-I and 70,945.093 on matching-II,
  against 32,120.012 and 68,914.100 with the 0.25 policy. These are checked
  feasible incumbents, not proven optima.

### Changed

- Packing repair candidates are reconstructed and checked once in the
  workers instead of in the serial merge, and residual tasks and tree
  subtrees are taken from a shared counter. Results are still merged in task
  order and are unchanged; with one worker, matching-I 2.15 s -> 1.21 s and
  matching-II 8.14 s -> 4.14 s.
- Option validation errors name the offending field and its range.
- The local clique separator counts conflict-graph degrees once instead of
  in every sort comparison.

### Fixed

- The disjoint-frontier tree of 0.25.0 took its status from the last
  unfinished subtree, so a subtree ending in `NumericalFailure` or on the
  wall clock could be reported as `NodeLimit`. Statuses now combine by
  severity (`NumericalFailure` > `Cancelled` > `TimeLimit` >
  `IterationLimit` > `NodeLimit`), and the message names the first failing
  subtree and its error, which was dropped before.

## [0.25.0] - 2026-10-17

Opt-in packing search for binary unit-packing models, aimed at the ESA SpOC4
matching tables. The 0.25.0 source was never delivered on its own: it was
reconstructed by reversing the 0.26.0 archive's patch and merged with 0.26.0
(#6). Existing default search modes are unchanged.

### Added

- `packing_root` with `packing_root_iterations` and `packing_root_work`: a
  factorization-free projected primal-dual root pass and a greedy early
  incumbent for recognized unit-packing models. Every reported price bound
  goes through the outward-rounded core bound.
- `packing_neighborhoods`, `packing_neighborhood_size`, `packing_workers`
  (1-16) and `packing_neighborhood_nodes`: residual neighborhoods solved by
  private workers and merged in task order through the original-model check.
- `packing_cuts` and `packing_cut_work`: cross-resource triangle and
  odd-cycle (lengths 3, 5 and 7) cuts directed by the root hint.
- `partial_dual_bounds`, and `scalable_branching` with
  `branch_probe_candidates`, `branch_probe_pivots` and `branch_probe_work`:
  reliability probing without the old model-size gate, under work caps.
- `tree_threads` (1-16) and `tree_split_depth` (0-8): an exhaustive disjoint
  binary frontier solved by private subtree workers with conservative bound
  aggregation.
- The SpOC4 matching tables (ESA, GPL-3.0, with license text and provenance)
  in `benchmarks/spoc4-025/`. With the root pass and residual neighborhoods,
  a one-second budget gives 32,120.012 on matching-I and 68,914.100 on
  matching-II, 7.82% and 8.85% above the greedy baseline.

## [0.24.0] - 2026-10-17

Imports the native solver archive 0.24.0, which contains 0.21.0-0.23.0,
followed by review fixes (#5). Raw benchmark data under `docs/*/data/` is not
committed; `docs/EVIDENCE.md` names the archives and their checksums.

### Added

- 0.21: Rust `PreparedDualBound` caches the conservative dual bound for
  repeated variable-bound changes (O(log n) per change, O(1) per read);
  opposite-domain fixing uses it for its trials.
- 0.22: ordered independent-job batches, `solve_batch(models, workers=...)`
  in Python and `NativeSolver::solve_batch` in Rust, with 1-16 workers,
  private solver state, per-job limits and errors, and shared cancellation
  (32 packing jobs: 5.38x the throughput of one worker at eight).
- 0.22 controls: `singleton_presolve` (on; exact-quotient singleton
  reductions with original-row dual reconstruction), `root_propagation`
  (on), `root_cuts` (off; at most eight exact binary cover cuts before
  factorization), `factor_replay` (off), `adaptive_pricing` (on), `devex`
  (off) and `avoid_dominant_components` (on).
- 0.23: `mip_diagnostics` and `mip_trace_limit` (256, 0-4096), a bounded
  opt-in search trace in `diagnostics.mip_search`; six public MIPs and six
  planted packing holdouts in the benchmark corpus.
- 0.24 MIP search: `rational_certification` (on), exact rational witnesses
  for incumbents on the supplied binary64 data; `reliability_branching` with
  `reliability_threshold` and `reliability_max_probes`; a deterministic
  serial Feasibility Jump prototype (`feasibility_jump`,
  `feasibility_jump_steps`); LP-directed root covers with exact sequential
  lifting (`root_separation`, `root_cut_rounds`, `cover_lifting`); reversible
  mixed-row node propagation (`mixed_propagation`). All but rational
  certification are off by default.

### Changed

- Root inference (0.22) admits a MIP whose box is not finite when the
  constraints prove finite bounds; unresolved infinities and magnitudes
  above 2^53-1 still return `Unsupported`. egout and flugpl now solve; bell5
  reaches the 100,000-node cap.
- LP kernel: four accumulators in dense-tail forward solves (0.21; wide1024
  kernel 170 ms -> 84 ms), sparse support through forward solves and eta
  updates, broader guarded Forrest-Tomlin eligibility and row/column pricing
  selection (0.22), reuse of captured Forrest-Tomlin output support (0.23).
- With default rational certification, egout needs 12,602 nodes instead of
  66,553, and rgn solves to numerical optimality in 7,249 nodes instead of
  reaching the 100,000-node cap. `diagnostics.certification_solves` counts
  actual LP repair calls only.
- Mixed propagation reuses its buffers and inlines its outward-rounded
  helpers (review): bit-identical search, bell5 with `mixed_propagation`
  19.2 s -> 7.0 s.

### Fixed

- 25fv47 no longer ends in `numerical_failure` (0.21): only consecutive
  restarts without pivot progress are bounded, and perturbed costs are
  cleaned before a one-sided dual infeasibility is classified, so a
  perturbed auxiliary optimum no longer implies unboundedness. All seven
  public LPs pass.

Found in the review of the delivered 0.24.0 code, before the merge:

- Rational witnesses were limited to 250 ms of wall time, so certification
  and pruning depended on machine speed. Each attempt is now capped at
  8,000,000 and each solve at 64,000,000 limb operations
  (`diagnostics.rational_witness_work`), with word-level division (egout
  witness time 151 ms -> 8 ms).
- Sequential cover lifting stopped at the first item heavier than the row
  capacity; such items are skipped and lifting continues.
- Feasibility Jump clamped integer columns to fractional bounds; they now
  move within `ceil(lower)..floor(upper)`.
- Legacy root strong branching (`strong_branching=true`) again probes only
  when there is a real choice, as before 0.24.
- Root-separation counters are reported as `diagnostics.*` keys with a
  number or `none` instead of Rust `Debug` strings.

## [0.20.0] - 2026-10-17

### Added

- `reduced_cost_fixing` (default true): certified opposite-domain fixing.
  Numerical reduced costs only shortlist candidates; each inherited integer
  fixing is justified by a conservative core dual bound against a certified
  incumbent.
- `strong_branching` (default false): bounded root probes (at most four
  32-pivot solves) charged to the global limits; off after negative
  measurements.
- Public corpus (seven LPs and three MIPs with their original infinite
  bounds) and seeded MIP coverage and holdout fixtures.

### Changed

- Cached reciprocal LU and Forrest-Tomlin diagonals, zero-aware forward
  triangular arithmetic, and exact steepest-edge winners cached in blocks of
  64.
- Exact i128 integer-row activities with an incidence-driven bounded
  propagation queue, now also for long rows; smoothed pseudocost estimates
  and a balanced degradation score.
- Against 0.19: packing-4096 263.7 ms -> 216.3 ms, network-2048 88.7 ms ->
  73.1 ms, nodecopy 105 -> 42 nodes. Regressions are kept in the report
  (long-row-mip 112 2.08x slower, adlittle +17%). 25fv47 still fails
  numerically, and egout, flugpl and bell5 are rejected for infinite bounds.

## [0.19.0] - 2026-10-17

### Added

- `factor_ordering` (`auto`, `natural`, `min_degree`; Rust
  `FactorOrdering`), `factor_pivot_search` (1-64, default 4) and
  `adaptive_refactor` (default true).
- Optional `basis-replay` feature: capture of actual bases with a validated
  binary replay reader, and a frozen corpus of 21 bases in
  `docs/irregular-lu/corpus/`.

### Changed

- Dense LU tails of at least 64 rows are factorized as row-major blocks with
  partial pivoting (panel width 32) and kept for both triangular solves.
  Small tails, singular fallback and tight entry caps keep the old path.
- Sparse elimination uses cross-indexed entries with checked 32-bit links
  and reusable workspaces; dimensions above `u32::MAX` are rejected before
  allocation.
- Bounded approximate minimum-degree ordering advice, used automatically
  only for m >= 2048 after a dense tail above m/8.
- Work-based rebuilds for large dense bases: after at least 32 updates, when
  eta work exceeds a quarter of the estimated factor work. The default
  `refactor_interval` changes from 64 to 256 and is now a maximum; other
  bases keep an internal maximum of 64.
- irregular-4096 19.36 s -> 7.34 s (HiGHS 9.45 s), irregular-1024 217 ms ->
  159 ms; some sub-millisecond cases are up to 26.5% slower.

## [0.18.0] - 2026-10-17

### Added

- Forrest-Tomlin basis updates with a checked eta fallback, selected by
  `basis_update` (`auto` default, `eta`, `forrest_tomlin`; Rust
  `BasisUpdate`). `auto` starts with eta updates and enables Forrest-Tomlin
  at a refactorization only for bases of dimension >= 128 with a sparse L
  and enough observed update work.
- Optional `kernel-profile` feature for exclusive kernel timings, compiled
  out of normal builds.

### Changed

- Sparse support is carried from the triangular solves into pricing and the
  entering column; L is skipped when it is the identity.
- packing-4096 478 ms -> 286 ms, packing-2048 121 ms -> 73 ms;
  irregular-1024 unchanged; a few small cases are slower (pivot-0 +15.5%).

## [0.17.0] - 2026-10-17

Ten measured performance iterations on 0.16 (`docs/performance-ten/`).
0.17.0-0.20.0 reached main as history patches with the 0.24.0 import (#5).

### Changed

- Integral row bounds are rounded inward before the MIP relaxation when all
  coefficients and variables are integral: selection-64 needs 1 node
  instead of 9,311.
- Row-free continuous models are solved directly with the same independent
  checks, and component discovery is skipped for empty matrices (box-lp
  16.5 ms -> 4.6 ms).
- Sparse kernels: reachability-based forward and transpose triangular solves
  with dense fallback, reuse of the entering column's support, sparse unit
  support through eta updates, and exact cancellations removed from the
  Markowitz lists.
- Exact i128 propagation of short (at most eight entries) all-integer rows
  during MIP reoptimization.
- Large bases are refactorized when eta fill exceeds eight times the base
  factor after at least 16 updates; `refactor_interval` stays the hard
  maximum. packing-4096 556.9 ms -> 489.4 ms; network-2048 is about 2%
  slower.

## [0.16.0] - 2026-10-16

The project's own LP/MIP solver replaces the HiGHS adapter (#4). The pull
request imported the native archive 0.15.0, which carries 0.4.0-0.15.0, and
reworked its kernels into 0.16.0.

### Added

- Native backend: crate `lp-ip-craft-native` with `NativeSolver` and
  `NativeOptions`, selected in Python by
  `solve(..., backend="native", backend_options=NativeOptions())`. The
  numerical crates have no third-party dependencies; the default backend
  stays `dummy`.
- One kernel for every LP and node relaxation: a bounded-variable dual
  revised simplex on a sparse LU factorization. Variable bounds stay bounds,
  so a branch changes a bound in place and keeps the factorized basis.
  Power-of-two scaling, Markowitz LU (threshold 0.1) with a dense tail and
  eta updates, dual steepest edge (`steepest_edge`), a bound-flipping ratio
  test with Harris tolerances, cost perturbation, an auxiliary dual phase 1
  and a Farkas re-check of infeasibility claims.
- Integer search: best-bound node selection with diving and pseudocost
  branching. Pruning uses only certified incumbents; an
  incumbent-certification LP makes them available on mixed models.
- From 0.5.0-0.14.0 (`docs/native/`): presolve of fixed columns, free and
  empty rows and conservatively redundant rows; checked incumbents with
  conservative pruning and integral-objective bounds; independent component
  execution with up to 16 workers and reserved shares of the global budgets
  (`decompose`, `component_threads`).
- Rust `check::lp_residuals_with_bounds` and `safe_dual_bound_with_bounds`
  for replacement variable bounds.
- `examples/validate_random.py`, a randomized differential check against
  SciPy (5,500 LP/MIP cases). CI fails if `highs`, `bindgen`, `cmake`,
  `blas` or `lapack` enter the native dependency tree.

### Changed

- Breaking: the HiGHS backend is removed (crate `lp-ip-craft-highs`,
  `backend="highs"`, Python `HighsOptions`); use `backend="native"`. In the
  archive, 0.4.0 still loaded models through HiGHS's bulk CSR API, and 0.5.0
  replaced HiGHS with the first native simplex.
- Breaking: MIPs need finite bounds on all variables, continuous ones
  included, of magnitude at most 2^53-1. LP starts are rejected
  (`start_status="rejected"`); MIP starts are evaluated as incumbent
  candidates.
- Breaking, against the 0.15.0 archive: `NativeOptions` drops `sparse`,
  `max_tableau_entries`, `fast_pivots` and `dantzig_pricing`, and renames
  `max_sparse_entries` -> `max_factor_entries` and
  `sparse_refactor_interval` -> `refactor_interval`. Unknown option names
  are rejected.
- Against the 0.15.0 build: LP benchmark families 2.6-30x faster than the
  better 0.15 kernel, network-2048 now solves, selection-20 needs 93 nodes
  instead of 1,505, and randomized MIPs need 20-29x fewer nodes.

## [0.3.0] - 2026-10-16

### Added

- `safe_dual_bound` in the Rust core and in Python, with `SafeDualBound` and
  `SafeBoundStatus`: a conservative Lagrangian bound in the original sense
  from approximate row duals, using residual correction over the variable
  bounds and outward-rounded binary64 arithmetic, or the reason (with the
  first blocking row or variable index) why no finite bound exists. The
  validity argument is in `docs/safe-dual-bounds.md`; an exact rational
  oracle on 210,000 adversarial models found no bound on the wrong side.
- `examples/safe_bound.py`, including `margin_duals`: duals of the LP with
  one-sided costs shifted by 1e-6 max(1, |c_j|) in the safe direction gave a
  bound in 100 of 100 random LPs and MIP relaxations.
  `examples/bound_availability.py` measures availability.

## [0.2.0] - 2026-10-16

### Added

- HiGHS backend (crate `lp-ip-craft-highs`; HiGHS 1.15.0 built from source
  through `highs` 2.4.0 and `highs-sys` 1.15.0):
  `solve(..., backend="highs", backend_options=HighsOptions(...))` with
  time, node and iteration limits, cancellation through the interrupt
  callback, complete primal starts and independent acceptance checks.
  `iteration_limit` on a MIP and `node_limit` on an LP are `Unsupported`.
- Statuses `optimal`, `infeasible`, `unbounded`, `infeasible_or_unbounded`,
  `time_limit`, `node_limit`, `iteration_limit`, `solution_limit`,
  `objective_limit`, `memory_limit`, `numerical_failure` and `unknown`, each
  mapped explicitly from HiGHS.
- LP `optimal` requires the relative duality gap
  |P - D| / (1 + |P| + |D|) <= `dual_tolerance`, and stationarity and
  dual-sign residuals <= `dual_tolerance` * (1 + max|c|);
  `LpResiduals.duality_gap`.
- `BackendReport` (Python `result.backend`), `StartStatus`, `BACKENDS`,
  `BackendError` and Python `lp_residuals`.

### Changed

- Breaking: `SolveResult::gap` returns `Result<Option<Gap>>` and fails on
  overflow, instead of `Option<Gap>`.

## [0.1.0] - 2026-10-15

Interface draft without an optimizer (#1).

### Added

- Core crate `lp-ip-craft` without dependencies: an immutable `Model`
  validated once by `Model::new(ModelData)` (canonical zero-based CSR,
  duplicates rejected, errors naming field and entry), the `LpIpSolver`
  trait, `Options` with `Tolerances` and `threads` (only 1), `WarmStart`
  with a primal point, and `CancellationToken`.
- `best_bound` as a proven bound in the original sense and one
  crate-derived signed `Gap` for a feasible candidate; `Duals` with
  `c = A^T y + z` in the original sense; independent `check::evaluate` and
  `check::lp_residuals`.
- `DummySolver`, which evaluates one candidate (the start, or zero clamped
  to the bounds) and returns status `stub`.
- Python package `lp_ip_craft` (PyO3 0.30.1, abi3 for CPython 3.10+): a
  keyword-only, frozen, picklable `Model`, `solve()` with the interpreter
  released, `evaluate()` for any solver's point, and `EvaluationError`.
- CI on GitHub Actions: fmt, Clippy, tests, MSRV 1.85, and Python 3.10 and
  3.14 on Linux, macOS and Windows.

Repository: https://github.com/dietmarwo/LP-IP-craft
