# LP/IP — iteration 1 implementation plan

> Pre-implementation plan, kept as written. What was implemented, what the
> review changed and what remains open is in
> [iteration-1-results.md](iteration-1-results.md).

Planning2027 · Idea 004 · Revision 2 · 16 October 2026 (UTC)

**Goal:** implement the checked-in Rust/Python LP/IP interface against one real open LP/MIP backend, with reliable conversion, termination reporting, duals, bounds and independently checked candidates. Keep the dummy as a contract fixture. This is the baseline for selecting a useful native Rust component later.

**Verified source:** public `dietmarwo/LP-IP-craft`, branch `main`, commit [`769e556f79ce56c97fde842a5d57c02df5662d80`](https://github.com/dietmarwo/LP-IP-craft/tree/769e556f79ce56c97fde842a5d57c02df5662d80), “Add LP/MIP interface draft: Rust contracts, PyO3 binding, dummy backend (#1)”. The clone was clean; no `AGENTS.md` was present in the repository or applicable workspace ancestors. This revision supersedes the earlier archive-based plan. Source was inspected directly from Git, not inferred from the README.

## 1. Checked-in state and review evidence

| Evidence | Verified state |
| --- | --- |
| Source tree | 31 tracked files; two workspace crates; no real solver dependency |
| Core | `lp-ip-craft` 0.1.0; dependency-free; Rust edition 2024; declared MSRV 1.85; `forbid(unsafe_code)` |
| Binding | `lp-ip-craft-python`; manifest allows PyO3 `0.30.1`; lockfile resolves 0.30.1; `abi3-py310` |
| Python distribution/import | `lp-ip-craft` / `lp_ip_craft`; Python >=3.10 |
| Licensing | MIT in the checked-in `LICENSE` and package metadata; `publish = false` |
| GitHub CI | [Run 38835476742](https://github.com/dietmarwo/LP-IP-craft/actions/runs/38835476742) reports success for this exact commit |
| Local Rust checks | `cargo fmt --all --check`, Clippy with `-D warnings`, 35 integration tests, 1 doctest and the production example passed |
| Local Python checks | 48 tests and the Python example passed against a directly compiled release extension |
| Local environment | Linux x86_64, Rust 1.99.1, CPython 3.12.14; PyO3 from the committed lockfile; pytest 9.1.1, NumPy 2.5.4 |

The local `scripts/check.sh` reached its packaging stage after passing Rust checks. The installed maturin 1.15.0 executable initially lacked execute permission and then crashed after that installation issue was corrected. The binding was then built with the documented `PYO3_BUILD_EXTENSION_MODULE=1 cargo build --release --locked -p lp-ip-craft-python` path, staged with the unchanged Python package in a separate directory, and tested there. This tested the actual native module, not a Python substitute. Local packaging and the full CI matrix were not reproduced here; the successful remote CI result is separate evidence. This is not a claim of a repository build defect.

The saved concept source is `open-rust-solvers-concept-v3.md`. The real GTOC model, reference point and target numbers remain unavailable; `docs/target-card.md` is correctly marked TBD. Passing interface tests is not evidence of optimization capability.

### What changed since the ZIP and must be retained

| Checked-in location | Existing contract | Planning consequence |
| --- | --- | --- |
| `crates/core/src/model.rs` | `Model::new(ModelData)` validates once; fields private; getters expose immutable data | Reuse one validated model across solves; do not reintroduce public mutable model fields or repeated model validation |
| `crates/core/src/solver.rs` | `Options { tolerances: Tolerances, threads }`, `WarmStart::from_primal`, non-exhaustive `Status`, `Duals`, derived `Gap` | Extend these types; retain existing names and mathematical conventions |
| `crates/core/src/check.rs` | `evaluate` and `lp_residuals` independently recompute primal/integrality and LP KKT diagnostics | Harden and reuse; do not plan a second production evaluator |
| `crates/core/src/sparse.rs` | Canonical CSR, contextual errors, bounds helpers; `violation` now asserts value length | The original missing value-length check was partly fixed; remaining raw-helper and arithmetic cases are below |
| `crates/python/src/lib.rs` | Native frozen, comparable, picklable `Model`; construction copies data once; solve/evaluate borrow immutable Rust data with the interpreter detached | Preserve that ownership design; no per-solve dataclass deep copy |
| `python/lp_ip_craft/{__init__.py,_native.pyi}` | Typed facade, native model, Python `evaluate`, `EvaluationError` | Extend the facade and existing stub together |
| `.github/workflows/ci.yml`, `scripts/check.sh` | fmt/Clippy/tests/examples; MSRV job; Python 3.10/3.14 on Linux/macOS/Windows | Extend existing CI; do not describe it as missing |

There is still no LP/MIP engine, backend selector, native basis/search code or numerical optimization acceptance suite. `DummySolver` evaluates a start (or bound-clamped zero), returns `Stub`, and supplies neither duals nor a bound. Its only cancellation checkpoint is before evaluation. The checked-in gap formula, signed dual convention, preservation of a complete candidate on future cancellation, and no-silent-repair rule are part of the baseline.

### Reproduced hardening findings

1. **Residual arithmetic can hide overflow.** With a two-row, one-column matrix `[1e308, -1e308]`, row duals `[1e308, 1e308]`, zero point, objective coefficient 1 and equality rows at zero, all inputs are finite but `A^T y` becomes NaN through overflow. `check::lp_residuals` returned stationarity 0 and all other residuals 0 because its maximum reduction discards NaN. The exact stationarity residual for those supplied duals is 1, not 0. Reject nonfinite intermediate products, accumulated values and final residuals with `Error::Evaluation` before any backend uses this checker for acceptance.
2. **The raw bounds helper still has unchecked preconditions.** `Bounds::new(vec![0.0], vec![]).violation(&[-1.0])` returned 0. The new assertion only compares `values.len()` to `lower.len()`. Validated `Model` objects prevent this malformed case on the normal solve path; a direct public call does not. Make the helper checked, or clearly restrict it to validated/private use and assert both vector lengths. Include nonfinite point handling.

These were reproduced in an external probe against the unmodified committed core. The plan records fixes; this review does not modify solver source.

## 2. Scope and implementation choice

Use **HiGHS through its existing Rust bindings** for the first real implementation of `LpIpSolver`. Keep the backend in its own workspace crate so the core remains independent of C/C++ tooling. The new Rust code owns validation, conversion, lifecycle, result semantics and tests; HiGHS owns LP/MIP search. This is an open solver workflow, not yet a native Rust LP/MIP engine.

This follows concept v3's baseline-first approach. Do not implement simplex, branch-and-bound, cuts and a backend adapter simultaneously. A later native component should address a measured gap. SCIP remains the independent MIP comparison candidate when the real workload is available; a second production adapter is not required in this iteration.

### Included

- Continuous, integer and binary variables; minimization/maximization; objective offsets.
- Two-sided linear rows, equalities, fixed variables, free bounds and zero-row models.
- Finite, full primal starts; transparent reporting of start submission and acceptance.
- Numerical termination status, optional candidate, optional best bound, defined gap calculations and backend provenance.
- Explicit resource limits, deterministic single-thread reference configuration and cooperative cancellation.
- Rust and Python use of the same adapter; no Python-only optimization fallback.
- Source-build verification and an installed-wheel smoke test on Linux.

### Deferred

Native LP/MIP algorithms, SCIP plugins, new cuts, persistent basis updates, partial MIP starts, SOS/indicator/quadratic constraints, automatic scaling, file-format import/export, a new modelling DSL, GPU/distributed search and public package publication. Exact-arithmetic optimality certificates are also outside this iteration.

## 3. Contract changes before backend integration

### 3.1 Dispatch and compatibility

Proposed Python call:

```python
result = lp_ip_craft.solve(model, backend="highs", options=options, start=start)
```

Add a keyword-only `backend` with `"dummy"` and `"highs"`. Preserve the v0.1 default `"dummy"` for this draft transition and make every real example specify `"highs"`. Add `backend` and `backend_version` to results. If the selected engine is not built, raise `NotImplementedError`; never silently use the dummy or relax integrality. Version the extended draft as 0.2.0 and document the result-field changes.

`LpIpSolver` remains the Rust replacement point. Implement `HighsSolver` in `crates/highs`; construction may carry backend configuration, but each solve initially constructs a fresh backend model. Avoid introducing mutable sessions until a workload needs repeated-model reuse.

### 3.2 Termination is separate from candidate quality

Replace the two-value status restriction with explicit outcomes. Keep `Stub` and `Cancelled`; add `Optimal`, `Infeasible`, `Unbounded`, `InfeasibleOrUnbounded`, `TimeLimit`, `NodeLimit`, `IterationLimit`, `SolutionLimit`, `ObjectiveLimit`, `NumericalFailure` and `Unknown`. Map every status from the pinned backend exhaustively, retaining the raw backend status. Do not invent a more precise outcome than the backend reports.

`Optimal` means the backend reported numerical optimality under the recorded effective tolerances and the returned candidate passed independent primal and integrality checks; for an LP with valid duals, the independently checked dual residuals must also pass the declared thresholds. It is not an exact mathematical certificate. A time/node-limited solve can have a feasible candidate. A candidate-less result can still have a valid backend bound. A fractional LP relaxation is never an IP incumbent.

Keep `candidate: Option<Vec<f64>>` and matching `evaluation: Option<Evaluation>`. Populate a candidate only when the backend declares usable primal solution data; an allocated solution buffer alone is insufficient. Recompute objective and feasibility in original model units. If a supposedly optimal result fails these checks, return `NumericalFailure` with the raw status and residuals; do not report success.

### 3.3 Preserve the committed bounds, gaps and dual conventions

Pass the original objective sense and offset to HiGHS. If the high-level binding lacks offset support, use a narrowly scoped, version-matched C API call in the backend crate. Verify positive and negative offsets without double addition, including the reported search bound.

Keep the existing `Gap` type and `SolveResult::gap(sense)` behavior:

```text
absolute = incumbent - best_bound    for minimization
absolute = best_bound - incumbent    for maximization
relative = absolute / abs(incumbent) if incumbent != 0
relative = 0 if absolute == 0, otherwise signed infinity, if incumbent == 0
```

Only derive a gap when `evaluation.feasible_candidate` and a valid `best_bound` are both present. Keep negative gaps visible as inconsistencies; do not clamp them. The earlier plan's `max(1, abs(incumbent))` denominator and roundoff clamping would contradict checked-in tests and are withdrawn. A materially reversed bound also prevents an `Optimal` acceptance result. Validate candidate/bound values before computing gaps; explicitly test finite subtraction overflow and intentional zero-objective infinity separately.

`best_bound` is reserved for a backend-supported valid search bound in the original sense, offset included. Record its provenance and the backend's floating-point validity assumptions. Never substitute a primal objective, a relaxation point, or an unchecked approximate LP dual objective. Leave LP `best_bound=None` until a reviewed valid bound route is implemented. A safe locally computed dual bound with rounding control is a separate later task, as the repository design notes require. Do not use this iteration to introduce new pruning logic or exact-arithmetic certificate claims.

Backend MIP stopping-gap options may use a different denominator. Name and document those options separately and record effective settings; they do not change the facade's gap formula or become an alternate public `gap` field.

Keep `Duals { row_duals, reduced_costs }` and `c = A^T row_duals + reduced_costs` in the original objective sense. For minimization, positive means a lower bound and negative an upper bound; reverse this activity interpretation for maximization. Equalities allow either sign. Expose valid LP duals and check them through the hardened `check::lp_residuals`; do not fabricate MIP duals. Leave MIP `duals=None` in this iteration unless a separately labelled LP-relaxation/fixed-integer solve is explicitly implemented. Add Python access to the existing LP residual checker for baseline evaluation.

### 3.4 Options and errors

Keep original-unit feasibility and integrality tolerances in `Tolerances`. Separate structural validation (`threads > 0`) from backend capability checks. `Options::validate()` currently rejects every `threads != 1`; keep this for the dummy and the initial deterministic reference configuration, and relax it for HiGHS only after its scheduler/concurrency behavior is verified.

Add optional time, node and LP-iteration limits, a seed, and an explicit supported backend-option subset. Validate values before backend allocation. Query or record the applied backend values; reject unsupported settings rather than ignoring them. Do not expose arbitrary untyped options as the initial public API.

Define `time_limit_seconds` as a cooperative solve-call budget starting after input validation and including model conversion, engine execution and result extraction; check remaining time before engine entry and report overruns. A backend interruption interval is not a hard real-time deadline. A separate process is required for hard termination. Keep wall-clock timing out of deterministic correctness assertions.

Malformed input remains `InvalidInput`/`ValueError`. Numerical termination returns a result. API setup, allocation or unsupported-operation failures return a distinct backend/configuration error, mapped consistently in Python. Preserve `Unsupported`/`NotImplementedError`. Never turn a backend API error into an infeasibility finding.

### 3.5 Validation hardening

- Keep canonical CSR rejection of duplicates and unsorted indices; convert storage once per solve without densifying.
- Check all conversions to the backend's integer index type for overflow, including `nnz` and row pointers.
- Reject finite input magnitudes that the selected backend would interpret as its infinity sentinel; map actual infinite bounds deliberately.
- Add an explicit HiGHS-adapter integer-range policy: reject unsupported finite integer-variable bounds and submitted/returned integer values outside `[-(2^53-1), 2^53-1]`. This is a proposed adapter limitation, not a silent change to the general `Model` constructor or reference evaluator. The existing documentation also warns that integrality becomes uninformative above `2^52`; the proposed range does not solve loss of fractional information or make arithmetic exact.
- Do not round submitted starts or returned solutions. Fractional integer starts can be rejected with a clear start-rejection result or documented backend submission outcome; they must not silently become feasible incumbents.
- Fix the reproduced residual-overflow and raw-bounds-helper findings in section 1. Add finite checks after transpose products, differences and complementarity products; do not accept NaN discarded by `f64::max`. Keep regression cases separate from backend tests so this safety boundary remains independent.

## 4. Backend integration design

1. Borrow the already validated immutable `Model`; validate per-solve options/start and backend conversion limits. Do not revalidate or deep-copy the full model unnecessarily.
2. Check cancellation and remaining time.
3. Construct the backend model on the same native execution thread that will solve it.
4. Transfer rows, columns, bounds, objective, sense and integrality. Binary is integer plus its validated `[0,1]` bounds.
5. Apply only supported options through fallible API methods.
6. Submit an optional full primal start; record `not_provided`, `submitted`, `rejected` or `unsupported`, distinguishing submission from actual incumbent use.
7. Register cancellation callbacks with stable owned context for the duration of the solve.
8. Solve and read status, solution validity, candidate, bound and counters before destroying the model.
9. Check diagnostics against the original model and assemble a result with requested/effective settings and timings.

The inspected binding documentation marks `highs::Model` as `!Send` and `!Sync`. Do not construct one outside `Python::detach` and move it in, or add an unsafe `Send` implementation to force this. Capture a lifetime-safe immutable Rust `&Model` (or `Arc<Model>` if required), owned per-solve settings and the cloned atomic token; construct/use/drop the native backend handle inside the detached closure. The current binding already borrows immutable Rust model data safely. Recheck ownership against the selected `highs` and committed PyO3 0.30.1 APIs; do not downgrade PyO3 to the archive version.

If raw bindings are needed for offset, information queries or callbacks, confine them to `crates/highs/src/ffi.rs`. Use the same `highs-sys` version as the safe binding; avoid linking two HiGHS copies. Document every pointer lifetime, unregister-before-free rule and error code. Callback code must not call Python, retain borrowed backend arrays, panic across FFI, or mutate a model from another thread. Poll the atomic token and return the backend's supported interruption request.

## 5. File-level work packages

Paths below are relative to the verified repository root. Existing files are extended; proposed new adapter files are identified explicitly.

| ID | Changes | Acceptance evidence |
| --- | --- | --- |
| L0 — baseline handoff | Repository inspection is complete in this plan. At implementation start confirm HEAD; record local/CI evidence and resolve the local packaging-tool limitation | `docs/baseline.md` records actual starting commit and checks; no repeated archive reconstruction |
| L1 — backend capability probe | Pin `highs` and its native dependency; demonstrate LP, MIP, offset, primal start, solution-validity query, dual bound, time/node limits and cancellation | Tiny probe executable with recorded supported operations; unsupported features are explicit |
| L2 — hardening and contracts | Fix `check.rs` arithmetic and `sparse.rs` public boundaries; extend `solver.rs`/`error.rs`, Python facade and stub; preserve `Gap`/`Duals`; expose existing LP residuals in Python | Reproduced failures become checked errors; 35 existing Rust tests plus doctest and Python suite remain green; new fields/statuses round-trip |
| L3 — real adapter | Add `crates/highs/{Cargo.toml,src/lib.rs,src/convert.rs,src/result.rs}` and narrowly scoped `ffi.rs` if required | Analytic LP/IP fixtures solve through Rust; mapping/residual tests pass |
| L4 — limits and starts | Add cancellation context, budget accounting, start handling and fallible setup | Pre-cancel, in-flight cancel, limits with/without incumbent, start rejection tests |
| L5 — Python end to end | Add backend feature to binding crate, dispatch within detach, package build options and `_native.pyi` | Installed wheel solves same fixtures as Rust; missing feature fails explicitly |
| L6 — acceptance and docs | Add independent checker, small exhaustive-IP tests, process-worker smoke test, extensions to existing CI, real examples and updated `docs/api.md`/`docs/design-notes.md` | Release checklist below, including evidence limitations |

Dependencies: L0 → L1 → L2 → L3 → L4/L5 → L6. Each package should be a reviewable commit with tests for its own contract changes.

Proposed time box: 6–8 focused working days in total, including integration/build contingency; approximately 1 day for L0/L1, 1 for L2, 2 for L3, 1–2 for L4/L5, and 1–2 for L6. This is an initial planning budget, not a performance or completion guarantee. If an item consumes twice its time box, record a smaller deliverable or stop for diagnosis; do not expand into a new solver implementation.

## 6. Acceptance fixtures and checks

| Fixture | Expected result or invariant |
| --- | --- |
| LP: maximize `3x+2y`, `x+y<=4`, `x<=2`, `y<=3`, `x,y>=0` | Objective 10 at `(2,2)`; with offset 7 report 17 |
| IP: maximize `3x+2y`, `2x+y<=4`, `x+2y<=4`, nonnegative integer x/y | IP optimum 6 at `(2,0)`; LP relaxation has value `20/3`; never report the fractional relaxation as the IP solution |
| Mixed problem: minimize `2x+3y`, `x+2y>=2.5`, `0<=x<=3`, binary y | Objective 4 at `(0.5,1)` |
| Infeasible rows: `x>=1` and `x<=0`, represented as separate rows with valid variable bounds | Backend infeasibility status, no fabricated primal candidate |
| Unbounded LP: minimize `-x`, `x>=0`, no rows | Unbounded or explicitly ambiguous backend outcome; never an optimal zero |
| Max/min with positive, negative and zero objective offsets | Correct objective/bound signs and offset; defined gap denominator |
| Fixed variables, empty rows, free rows, zero coefficients, duplicate rows | No storage corruption or invalid assumptions about matrix rank |
| Tiny bounded binary/integer models | Exhaustive enumeration agrees on optimum/infeasibility and validates reported bound direction |
| Strict limits/cancellation | Preserve any independently feasible incumbent and valid bound, but retain the actual termination reason |
| Invalid sparse structure, out-of-range index, oversized integer magnitude, nonfinite arithmetic | Clear checked error; no panic or fake feasible solution |

For the analytic fixtures, propose objective error at most `1e-8 * max(1, abs(reference))`, primal violation at most `1e-7`, and integer distance at most `1e-7`. Configure the backend accordingly and freeze these test thresholds before tuning. They are fixture requirements, not GTOC requirements.

The independent Python checker must compute row activities and residuals directly from original arrays, without importing the Rust diagnostic function. Use exhaustive enumeration only for genuinely tiny bounded integer models. Comparing the Rust wrapper with Python HiGHS tests the interface, but is not independent algorithmic evidence; label it accordingly.

Test limit/status mapping deterministically with controlled backend-result fixtures, plus at least one genuine interruption integration case. Do not assert that a trivial model must reach a wall-time limit before it solves. Property tests should include model copies, row permutations and consistent objective scaling, with appropriate bound/gap transformations.

## 7. Sixteen-core and application gates

The correctness reference is one solver thread. Use one worker-local solver per process first, with no shared mutable model and no per-node cross-worker communication. Send only model/task identifiers, start points and compact results. On a 16-physical-core machine later, compare 16×1, 8×2 and 4×4 under equal CPU/time budgets. A two-worker isolation test can run now; it is not evidence of 16-core speedup. Record backend/global scheduler limitations before offering thread-based concurrent solves.

Complete `docs/target-card.md` with the actual GTOC source revision, formulation, dimensions, integrality requirements, saved reference, tolerances, incumbent objective target, need for a valid bound/gap, CPU/memory/time budget and held-out partition. Keep application construction and solver timing separate. Missing application data blocks claims of GTOC acceptance, not small-fixture adapter development.

After the adapter is reliable, run the same unchanged formulation through HiGHS and an independent SCIP baseline. Use those measurements to choose native Rust work. A comparison involving formulations that differ must be labelled as such.

## 8. Definition of done and handoff

- [x] Repository identity and inspected commit recorded; archive differences reconciled in this plan.
- [ ] Implementation starts from a confirmed HEAD and fixes the reproduced residual-arithmetic boundary.
- [ ] Existing core contracts reused and the dummy still explicitly available.
- [ ] Real Rust and Python LP, IP and mixed-integer solves pass independent checks.
- [ ] Status, candidate validity, objective offset, bound direction and gap rules tested.
- [ ] Full primal starts, cancellation and limits behave as documented.
- [ ] Missing capabilities return explicit errors; no silent LP relaxation or dummy fallback.
- [ ] Linux wheel built and tested outside the source tree; Rust-only client also tested.
- [ ] New dependency revisions, effective backend settings, toolchains and actual checks recorded.
- [ ] Application target-card omissions remain visible; no Gurobi performance claim.
- [ ] `docs/iteration-1-results.md` identifies completed and deferred items and the next measured gap.

This delivery updates the plan only. Preserve the repository MIT licence and notices; audit the chosen backend dependency licences during integration. No source, commit or pull request is created by this review.

## 9. References and companion

- Supplied `open-rust-solvers-concept-v3.md`: baseline-first strategy, required MIP scope, target cards and track-selection rule.
- [Verified source tree](https://github.com/dietmarwo/LP-IP-craft/tree/769e556f79ce56c97fde842a5d57c02df5662d80), especially `docs/api.md`, `docs/design-notes.md`, `solver.rs`, `check.rs`, bindings and tests: authoritative current contract.
- Original `rust-lpip-interface-draft.zip` is historical comparison material; its 9 Rust / 25 Python test counts and package names no longer describe this repository.
- [HiGHS Rust Model API](https://docs.rs/highs/2.4.0/highs/struct.Model.html): fallible setup/solve methods, starts and ownership restrictions. Version 2.4.0 was shown during review; pin and compile-test the chosen version before adoption.
- [HiGHS C API](https://ergo-code.github.io/HiGHS/dev/interfaces/c_api/): objective offset and lower-level integration points.
- [HiGHS callbacks](https://ergo-code.github.io/HiGHS/dev/callbacks/) and [options](https://ergo-code.github.io/HiGHS/dev/options/definitions/): capability-probe references; development documentation must be checked against the pinned release.
- SQP iteration-1 plan (kept with the SQP-craft iteration): separate pilot and scheduling gates.

The API design, work sequence, test thresholds and time boxes above are proposals from this review. They are not statements that these changes are already committed or implemented.
