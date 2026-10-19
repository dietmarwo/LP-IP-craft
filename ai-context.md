# AI context for solving LP and MIP problems with LP-IP-craft

Last reviewed 2026-10-19 against `main` at `aa9eafd` (workspace 0.28.0,
including the shared agent job API 0.1, #11). In another checkout, check
`Cargo.toml`, `python/lp_ip_craft/__init__.py`, the worker's discovery output
and the linked guides before using a command or claiming a capability.

This file is operational context for an AI that must model and solve a user's
linear program (LP) or mixed-integer linear program (MIP) with this
repository, or decide that another tool fits better. It is not a performance
claim. Every solve status is a numerical finding under declared tolerances,
not an exact certificate; report results that way.

## What the repository is

LP-IP-craft is a native Rust LP/MIP solver. One kernel, a bounded-variable dual
revised simplex on a sparse LU factorization, solves every LP and every node
relaxation. Integer search is best-bound branch and bound with diving and
pseudocost branching, plus presolve, propagation and optional cuts and
heuristics. The engine is owned by the project: no HiGHS, C/C++ solver, BLAS,
CMake or libclang is built or linked. Rayon supplies thread scheduling only.
CI fails if `highs`, `bindgen`, `cmake`, `blas` or `lapack` enter the native
dependency tree.

| Package (crate or distribution) | Path | Use as |
|---|---|---|
| `lp-ip-craft` | `crates/core` | `lp_ip_craft`: immutable CSR model, options, results, independent checks, conservative bounds, dummy backend; no dependencies |
| `lp-ip-craft-native` | `crates/native` | `lp_ip_craft_native`: `NativeSolver`, `NativeOptions`, batches |
| `lp-ip-craft-python` / `lp-ip-craft` | `crates/python`, `python/lp_ip_craft` | `import lp_ip_craft`: optional PyO3 package |
| `optimization-job-api`, `optimization-runner` | `crates/` | Shared job protocol and the `optimization-run` runner |
| `lp-ip-job-worker` | `crates/lp-ip-job-worker` | Job worker for profile `lp-ip.solve/1` |

Nothing is published. The workspace sets `publish = false`, and neither
crates.io nor PyPI has an `lp-ip-craft` package at this review. Use a checkout
of https://github.com/dietmarwo/LP-IP-craft. The workspace version is 0.28.0;
the job crates are 0.1.0. Profile names such as `lp-ip.solve/1` are protocol
identifiers, independent of either.

Toolchains:

- The numerical crates need Rust 1.85 or newer; CI checks them with 1.85. The
  job crates need Rust 1.88, and CI checks them with 1.88. Plain `cargo build`
  builds only `crates/core` and `crates/native` (the default members).
- The Python package needs CPython 3.10 or newer and builds from source with
  Rust: `python -m pip install ".[test]"`. CI tests 3.10 and 3.14 on Linux,
  macOS and Windows.
- The job layer is Linux only.

The [application target card](docs/target-card.md) is intentionally
incomplete: it awaits a GTOC-based integer application. Do not claim
application-level performance, and do not fill the card with toy examples.
The README's dated version banners describe what each release added; the
option tables in [docs/api.md](docs/api.md) and `NativeOptions` itself are the
current reference.

## Required workflow for the AI

Before modelling, write a problem card with these facts. Inspect the user's
code and data where possible. Ask only for facts that cannot be inferred and
would change the solution.

| Question | Why it matters |
|---|---|
| Are the objective and every constraint linear in the decision variables? | This solver accepts only `objective_offset + cᵀx` and linear row intervals. |
| How many variables, rows and nonzeros? Which variables are integer or binary? | Size and integrality decide LP versus MIP effort and the parallel layout. |
| Does every variable have a finite lower and upper bound? | A MIP requires finite bounds on **all** variables, continuous ones included, of magnitude at most 2^53−1 after root inference. |
| Minimize or maximize? Is there a constant term? | Both senses are native; results keep the original sense and include the offset. |
| What are the units and coefficient ranges? | Badly scaled rows weaken numerical acceptance; tolerances are absolute in original units. |
| Which tolerances does the application accept? | Defaults are 1e-7 for feasibility and integrality. Freeze them before tuning. |
| Is a feasible incumbent enough, or is a proven optimum, bound or gap needed? | Finding a good incumbent and proving a gap are different goals with different stopping rules. |
| Are LP duals or reduced costs needed? | They exist only for LPs, never for a MIP. |
| Is a feasible MIP solution known? | Complete primal MIP starts are evaluated as incumbent candidates. LP warm starts are rejected. |
| What time, node, iteration, core and memory budget applies? | Limits are cooperative and retain the incumbent; parallel components split budgets. |
| Is there a reference solver or known optimum? | Validate against it with the independent `evaluate` check, not the solver's own report. |
| One model, many independent models, or a durable agent experiment? | This selects the in-process API, a batch, or the job API. |

For every completed solution, state the problem card, the model dimensions,
the backend and every non-default option, the thread layout, the budgets, the
termination status, the independently evaluated objective and violations, the
best bound and gap where they exist, and how the result was validated.

## First decide whether LP-IP-craft is appropriate

Use it when the problem is a linear or mixed-integer linear model of small to
moderate size, when a dependency-free Rust engine is valuable, or when
independent result checks matter more than peak speed. It also suits many
independent models solved in one batch.

Prefer another tool when:

- the objective or constraints are nonlinear: for smooth constrained problems
  with derivatives and at most 256 variables, see
  [SQP-craft](https://github.com/dietmarwo/SQP-craft); for black-box,
  nonsmooth or multimodal objectives, see
  [fcmaes-rust](https://github.com/dietmarwo/fcmaes-rust);
- the objective is quadratic: there is no QP or MIQP support;
- the application needs exact rational optimality or infeasibility
  certificates: statuses are numerical, and the bounded rational witness
  covers only incumbent feasibility;
- the model is a large, hard industrial MIP for which a mature solver is
  expected and permitted: compare with one under the same independent check
  before choosing;
- a MIP has variables, integer or continuous, that cannot be given finite
  bounds and whose bounds propagation cannot infer: native returns
  `Unsupported` (LPs may have infinite bounds);
- the user needs a relative-gap stopping rule: there is no MIP gap stop; use
  node, iteration or time limits and report the gap.

## Choose the integration surface

| Need | Entry point | Scope |
|---|---|---|
| Rust application | `lp_ip_craft` + `lp_ip_craft_native::NativeSolver` | Full native option set, starts, cancellation |
| Python application | `lp_ip_craft.solve(..., backend="native")` | Same engine; the GIL is released during native work |
| Many independent models | `solve_batch` (Python) or `NativeSolver::solve_batch` (Rust) | Ordered results; per-model limits; no starts |
| Durable, CPU-budgeted agent experiments | `optimization-run` + `lp-ip-job-worker` | One hashed model artifact per job, a smaller option profile; see below |
| Check a point or a bound from any solver | `evaluate`, `lp_residuals`, `safe_dual_bound` | Independent of every solver, including commercial ones |

Python's `solve` defaults to `backend="dummy"`, which only evaluates the start
and returns `stub`. Always pass `backend="native"` to optimize.

## Build the model

The model is

```text
minimize or maximize   objective_offset + cᵀx
subject to             row_lower <= A x <= row_upper
                       lower     <=  x  <= upper
                       x_j continuous, integer or binary
```

`A` is zero-based CSR by rows: `indptr` has `m + 1` entries from 0 to `nnz`;
each row's `indices` are strictly increasing and below `n`; explicit zeros are
allowed. `n` is the objective length and `m` the row-bound length. A zero-row
model uses `indptr = [0]`. Equal finite row bounds express an equality;
infinite sides express one-sided or free rows and columns. Binary bounds must
lie in `[0, 1]`. The model is validated once and is immutable; Python models
are picklable.

Python:

```python
from lp_ip_craft import Model, NativeOptions, solve

inf = float("inf")
m = Model(objective=[3., 2.], sense="maximize",
          lower=[0., 0.], upper=[10., 10.],
          row_lower=[-inf, -inf], row_upper=[4., 5.],
          indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[1., 1., 2., 1.])
r = solve(m, backend="native", backend_options=NativeOptions())
assert r.status == "optimal" and r.evaluation.feasible_candidate
print(r.evaluation.objective, r.candidate, r.best_bound, r.duals)
```

`kinds=["integer", ...]` or `"binary"` turns the same model into a MIP.
Rust (from `crates/native/examples/solve.rs`):

```rust,ignore
use lp_ip_craft::*;
use lp_ip_craft_native::*;

let model = Model::new(ModelData {
    sense: Sense::Maximize,
    objective: vec![7., 11., 5., 6.],
    objective_offset: 0.,
    variables: Bounds::new(vec![0.; 4], vec![1.; 4]),
    kinds: vec![VariableKind::Binary; 4],
    matrix: CsrMatrix::from_dense(1, 4, &[3., 5., 2., 4.])?,
    rows: Bounds::new(vec![f64::NEG_INFINITY], vec![8.5]),
})?;
let r = NativeSolver::default().solve(
    &model, &Options::default(), None, &CancellationToken::default())?;
assert_eq!(r.status, Status::Optimal);
```

`CsrMatrix::new(rows, cols, indptr, indices, values)` takes CSR arrays
directly. A MIP start is `Some(&WarmStart::from_primal(x))`: a complete point,
used as given, never rounded or repaired.

Modelling rules:

- Give every MIP variable the tightest bounds the application justifies,
  continuous ones included. The kernel keeps bounds as bounds, not rows, and
  branching changes them in place.
- Keep coefficients and right-hand sides in sensible units. Native scales
  internally by powers of two, but acceptance is measured in original units.
- Prefer exact integral data when the objective could be integral: objective
  lattice tightening needs all-integer variables, integral objective
  coefficients, zero offset and a finite box.
- Do not encode a nonlinear relation by a fine discretization without telling
  the user what approximation error it adds.

## Options

`Options` (Rust and Python) holds the original-unit tolerances, both 1e-7 by
default, and `threads`, the maximum numerical width, 1–8 (default 1). Widths
above 8 are `Unsupported` (`NotImplementedError` in Python).

`NativeOptions` lists only controls the engine honours. Unknown names are
rejected (`TypeError` in Python), so stale settings fail loudly. The defaults
that matter most:

| Field | Default | Note |
|---|---:|---|
| `time_limit_seconds` | `None` | Cooperative whole-call budget |
| `node_limit` | 100000 | MIP relaxations; `None` removes it |
| `iteration_limit` | 1000000 | Simplex iterations across all nodes, phase 1, certification LPs and retries |
| `presolve` | true | Master switch: with false, every root reduction and node propagation is off |
| `mip_presolve`, `coefficient_tightening` | true | Default since the change after 0.27; both keep every feasible point. Set both false for the earlier behaviour |
| `reduced_cost_fixing`, `incumbent_heuristic`, `rational_certification`, `objective_lattice` | true | Each keeps the acceptance checks; disable only for matched ablations |
| `decompose`, `component_threads` | true, 16 | Independent components may run on up to 16 workers when `threads=1` |
| `strong_branching`, `reliability_branching`, `root_cuts`, `root_separation`, `gmi_cuts`, `mir_cuts`, `root_heuristics`, `feasibility_jump`, `dual_presolve` | false | Opt-in; effects vary by model |
| `packing_root`, `packing_neighborhoods`, `packing_cuts`, `scalable_branching`, `tree_threads` | off (0 or 1) | Experimental packing and parallel-search modes |

Change one option at a time and compare on held-out models under the same
independent check. The README's 0.20–0.28 sections and
[docs/api.md](docs/api.md) describe each mechanism and its measured gains and
regressions. The "Deliberate limits" paragraph of docs/api.md predates several
of these options (cuts, strong branching, MIP presolve, tree workers); its
remaining limits, such as no basis import or export and no MIP relative-gap
stop, still hold.

## Interpret results

`status` is why the solve ended, independent of candidate quality. Native
reports `optimal`, `infeasible`, `unbounded`, `time_limit`, `node_limit`,
`iteration_limit`, `cancelled`, `numerical_failure` and `unknown`; the dummy
reports `stub`. The shared `Status` type also names
`infeasible_or_unbounded`, `solution_limit`, `objective_limit` and
`memory_limit`, which native does not currently return.

- `candidate` and `evaluation` appear together. `evaluation` is the core's
  independent check: objective with offset in the original sense, row
  activities, largest bound and integrality violations, and
  `feasible_candidate`.
- LP `optimal` requires independent primal feasibility, stationarity, dual
  signs and a relative duality gap within tolerance, all in original units.
  MIP incumbents pass the independent primal and integrality check.
- `best_bound` is a proven, outward-rounded bound in the original sense
  (lower when minimizing, upper when maximizing), never an estimate. It can be
  absent, and it can differ from an optimal objective by a few ulps.
  `gap` is present only with a bound and a feasible candidate; its sign is
  preserved.
- `duals` exist only for LPs. In the original sense,
  `c = Aᵀ row_duals + reduced_costs`. When minimizing, positive multipliers
  select lower bounds and negative ones upper bounds; maximization reverses
  the signs.
- A limit status keeps the incumbent when one exists. Report it as a feasible
  solution with its gap, not as optimal.
- `infeasible` and `unbounded` are numerical findings. An unbounded MIP
  relaxation returns `unknown`, never integer unboundedness.
- A model outside the native domain raises `Unsupported`
  (`NotImplementedError` in Python): a MIP with an infinite or huge bound,
  a bound out of range after scaling, or a basis factorization above
  `max_factor_entries`.
- `backend.effective_options` records every control that ran and the
  `diagnostics.*` counters; `backend.start_status` says what happened to a
  start (`rejected` for every LP start).

Python errors: `ValueError` for malformed input, `TypeError` for wrong types or
unknown options, `NotImplementedError` for unsupported settings or models,
`EvaluationError` for floating-point overflow, `BackendError` for engine
failures.

### Independent checks and bounds

`evaluate(model, x)` checks any point against the model, without rounding or
repair. `lp_residuals(model, x, duals)` checks an LP primal–dual pair.
`safe_dual_bound(model, row_duals)` turns any finite row duals into a valid
bound for every feasible LP or MIP point of the stored binary64 model, with
outward-rounded arithmetic; its status says why no finite bound exists
(`unbounded_variable`, for instance). Use these to validate results from
LP-IP-craft or any other solver. They do not certify a search tree. They need
IEEE binary64 arithmetic with gradual underflow; fast-math is unsupported. See
[docs/safe-dual-bounds.md](docs/safe-dual-bounds.md) and
`examples/safe_bound.py`.

## Parallelism

- One model: `threads=1` is the default. For one large LP or packing root,
  `Options(threads=8)` lets the kernels use up to eight workers; small work
  stays scalar.
- Numerical width above 1 serializes component, tree and residual workers.
  With width 1, `component_threads` (default 16), `packing_workers` and
  `tree_threads` control those outer mechanisms. Count them in the CPU budget
  of an in-process solve.
- Many independent models: `solve_batch(models, workers=8)` with
  `Options(threads=1)`. Mixed layouts need `workers * threads <= 8`
  (1×8, 2×4, 4×2, 8×1); with `threads=1`, up to 16 workers remain allowed.
  Nested component, residual and tree workers are disabled inside a batch.
  Results keep input order, and a failed model returns its exception value
  without discarding the others.
- Never multiply eight outer workers by eight numerical workers. Separate
  callers must manage their combined budget; no global Rayon pool is
  configured.
- Parallel components reserve fixed shares of the node and iteration limits
  (`component_budget_mode = parallel_reserved`) and do not redistribute
  unused shares, so they can stop earlier than a serial solve. Set
  `component_threads=1` for one shared budget.
- The 0.28 kernels keep scalar operation order and ordered merges; tests
  require identical factors and packing-root outputs at widths 1, 2, 4 and 8.
  Outcomes under wall-time limits or cancellation still depend on
  scheduling. Use node and iteration limits when results must reproduce.
- Python solves from several threads run in parallel because the interpreter
  is released. A width is a maximum, not a speedup promise; measure on the
  real models ([docs/parallel-028](docs/parallel-028/RESULTS.md)).

## Shared agent job API

Start with [the shared guide](docs/job-api.md) and
[the LP/MIP tutorial](tutorials/job-api/README.md). The job layer gives an
agent durable, validated, CPU-admitted solves with journals, cancellation and
result manifests. Its profile is deliberately smaller than the in-process API.

```sh
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
python3 tutorials/job-api/run.py --root /tmp/lp-ip-job-tutorial
./target/release/optimization-run --root /tmp/lp-ip-job-tutorial \
  problems describe lp-ip
```

The tutorial creates an eight-slot node, registers the worker as `lp-ip`, and
runs a binary MIP, the same model as an LP (both objective 7 at `[1, 0]`) and
an infeasible variant, eight tasks each. Commands for a single job, with
`validate`, `run`, `submit`, `jobs get|events|wait|cancel|repair` and
`result`, are in the guide.

### Shared copies

The shared crates and `docs/job-api.md` were delivered as identical copies in
fcmaes-rust, LP-IP-craft and SQP-craft. They now differ. fcmaes-rust
(`bf68a16`) and SQP-craft (`2578c11`) each fixed, in different ways, the repair
of a job whose launcher died while the job was still queued; SQP-craft also
changed other runner details. This repository has the delivered runner, so
here `jobs repair` refuses such a job ("attempt processes still alive or
liveness unknown; reservation retained"), the job stays `waiting`, and its
request key stays taken: `run` with that key returns the same unfinished job.
Use a new request key, or run `jobs repair` once with an
`optimization-run` built from fcmaes-rust or SQP-craft `main` on the same node
root; both seal the job as `interrupted` (checked 2026-10-19). Otherwise run a
node root through one runner build; mixing builds concurrently is untested.

SQP-craft's [job API review](https://github.com/dietmarwo/SQP-craft/blob/main/docs/job-api/REVIEW.md)
lists further defects of the delivered shared code, which this copy still
has. The ones an operator meets: `validate` and `run` stage the worker under
`TMPDIR`, so a `noexec` `/tmp` makes them fail with "Permission denied"
(point `TMPDIR` at an executable directory); an invalid record in a worker's `results.jsonl` leaves the job
`running` and makes `jobs list` fail for the whole node; and after a reboot,
finished jobs are also reported `interrupted_after_reboot`.

The worker protocol is unchanged across the copies. The LP worker ran the
tutorial jobs under the fcmaes-rust runner, on one node that also served the
`fcmaes` and `sqp` workers. `sha256sum -c docs/job-api/SHARED-SOURCES.sha256`
checks this repository's copies, not the siblings'.

### Job specification

`schema_version` is `optimization.job/0.1`, `worker` the registered ID,
`payload_schema` `lp-ip.solve/1`. Exactly one entry in `inputs` holds the
model; its `name` must equal `payload.model_input`, its `file` is relative to
the job file, and its `sha256` must match the content. All payload fields are
required, and unknown fields are rejected:

| Payload field | Meaning |
|---|---|
| `model_input` | Name of the model input |
| `starts` | One entry per task, 1 to 1,000,000: `null`, or for a MIP a complete finite primal vector |
| `node_limit`, `iteration_limit` | Positive per-task budgets |
| `feasibility_tolerance`, `integrality_tolerance` | Original-unit tolerances, as in `Options` |
| `presolve` | Native's master switch; `false` turns off every reduction, and `effective_options` says so |

The model artifact is JSON with `schema_version: "lp-ip.model/1"`, `sense`
(`minimize` or `maximize`), `objective`, `objective_offset`,
`variable_lower`, `variable_upper`, `kinds`, `matrix` (`indptr`, `indices`,
`values`) and `row_lower`, `row_upper`. In bounds, `null` means infinite on
that side; JSON has no NaN or Infinity. Copy
`tutorials/job-api/assignment-mip-model.json`, then recompute its hash:

```sh
python3 -c 'import hashlib,pathlib; print(hashlib.sha256(pathlib.Path("my-model.json").read_bytes()).hexdigest())'
```

An LP with a non-null start, a start of the wrong length, or a non-finite
value is rejected at validation. Starts are inline and validation output is
capped at 4 MiB, so keep all starts together well below that size. Other
native options are not part of the profile. The worker uses the native
defaults, the payload's limits and tolerances, the remaining wall time as the
time limit, and forces `component_threads`, `packing_workers` and
`tree_threads` to 1, so `inner_threads` (1–8) maps only to `Options::threads`.

Each job holds one model; its tasks differ only by start. Null starts repeat
the same solve, which is useful for timing or the resource checks of the
tutorial but not as independent evidence. For many different
models, submit one job per model, each with its own slots, or use
`solve_batch` in process. Use `8 / 1 / 8` (`cpu_slots / outer_workers /
inner_threads`) for one large model and `8 / 8 / 1` for eight MIP starts. The
runner requires `outer_workers * inner_threads <= cpu_slots <= 1024` and
reserves `cpu_slots` for the whole attempt.

### Job results

Inspect the job `state` first: `completed` only means every task returned. Then
read each task's outcome with `optimization-run result JOB_DIRECTORY`
(schema `optimization.result/0.1`, ordered by task):

| Outcome field | LP/MIP meaning |
|---|---|
| `termination` | The native status, or `unsupported` when native refuses the model; `details.message` says why |
| `claim` | `numerical_optimum` (native `optimal` plus the independent check), `feasible` (checked candidate, no optimality), or `none` |
| `feasible`, `objective`, `x` | From the independent evaluation of the candidate |
| `details` | `sense`, `is_mip`, `best_bound`, `gap`, `primal_violation`, `integrality_violation`, `lp_residuals` (LP), `raw_status`, `warm_start`, `engine_version`, `effective_options`, `message`, `exact_certificate: false` |

Non-finite numbers in details are the strings `"+infinity"` and `"-infinity"`.
An infeasible model is a `completed` job with `termination: "infeasible"` and
`claim: "none"`. A native `optimal` that fails the independent check fails the
job instead of being reported. Work counters are `nodes`,
`simplex_iterations` and `validation_evaluations`; never add them to another
profile's counters. If a worker is killed, `work` is null,
`runner.accounting_complete` is false and only committed tasks count.
Graceful cancellation sets the native token; `jobs cancel --now` terminates
the process group after at most one more second. Repair never resumes a
branch-and-bound tree; use a new request key for a fresh solve.

### Extend the job profile

The adapter is `crates/lp-ip-job-worker/src/main.rs`. Expose another native
option only with a new, strictly typed payload field, validate it in
`prepare`, record it in `effective_options`, add tests, and bump the profile
version if the mathematical meaning of inputs or results changes. Keep
solver-specific logic out of the shared runner.

## Validation and reporting checklist

1. Recompute the objective and every violation with `evaluate` on the
   original model, or with the user's own evaluator, never from the solver's
   internal values.
2. Compare with a reference: a known optimum, SciPy (`examples/validate_native.py`
   and `examples/validate_random.py` show the oracle protocol), or the user's
   reference solver under the same check.
3. Report the termination status, `claim` or `feasible_candidate`, the best
   bound and gap, nodes and iterations, wall time, thread layout and all
   non-default options.
4. Say "numerically optimal under tolerance 1e-7", not "proven optimal".
   Infeasibility is a numerical finding too.
5. For tuning, freeze tolerances and held-out models first, change one option
   at a time, and keep regressions in the report.
6. For a MIP stopped by a limit, report the incumbent and the gap; do not
   round or repair the candidate.

## Failure diagnosis

| Symptom | Likely cause and action |
|---|---|
| `NotImplementedError` / `Unsupported` for a MIP | An infinite or huge variable bound; add justified finite bounds |
| Job task `termination: "unsupported"` | The same refusal inside a job; the message is in `details.message` |
| LP start rejected | Basis import is not implemented; pass no start (`null` in a job) |
| `time_limit` or `node_limit` with a candidate | Report it as feasible with its gap; raise the budget or try opt-in search options |
| `numerical_failure` or `unknown` | Rescale the model, check for near-parallel rows or huge coefficients, compare with a reference solver |
| `Unsupported` naming `max_factor_entries` | The guard (4,000,000 entries) on one factorization; heavy LU fill is a known limit. It is not a whole-process memory cap |
| A parallel component solve stops earlier than serial | Reserved component budgets; set `component_threads=1` |
| `TypeError` naming an option | An unknown or removed `NativeOptions` field |
| A queued job never finishes after its launcher died | See [Shared copies](#shared-copies) |

## Test commands

The CI set, from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p lp-ip-craft-native --release --example benchmark --locked -- 1
cargo test --release --locked -p optimization-job-api -p optimization-runner -p lp-ip-job-worker
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
python3 scripts/job-api/test_integration.py
python3 tutorials/job-api/run.py --root /tmp/job-api-tutorial
python -m pip install ".[test]" && python -m pytest -q
python examples/solve_native.py
python examples/safe_bound.py
```

CI also runs family benchmark smoke runs, the dependency audit and the SciPy
oracle validations (`examples/validate_*.py`, with `scipy` installed). Pass
`--fcmaes`, `--sqp` (and `--lp-ip`) executables to `test_integration.py` to
test a mixed node.

## Repository references

- [README.md](README.md): release notes, build commands, guarantees and limits
- [docs/api.md](docs/api.md): native API, options, kernel, search, results
- [docs/safe-dual-bounds.md](docs/safe-dual-bounds.md): conservative bounds
- [docs/simplex/README.md](docs/simplex/README.md): kernel and search design and validation
- [docs/parallel-028](docs/parallel-028/VALIDITY.md): execution contract for 1–8 workers
- [docs/job-api.md](docs/job-api.md), [tutorials/job-api](tutorials/job-api/README.md),
  [docs/job-api/REVIEW.md](docs/job-api/REVIEW.md): shared job API and this worker
- [docs/target-card.md](docs/target-card.md): the open application contract
- `crates/core/src/check.rs`, `crates/core/src/bound.rs`: independent checks
- `crates/lp-ip-job-worker/src/main.rs`: the `lp-ip.solve/1` adapter
- `python/lp_ip_craft/__init__.py`: Python API and defaults
