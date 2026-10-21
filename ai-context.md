# AI context for solving LP and MIP problems with LP-IP-craft

Last reviewed 2026-10-20 against `main` at `f9ba356` (workspace 0.28.0). This
state includes the result contract 0.2 (claims with evidence,
`lp-ip.evaluate/1`, error codes, JSON Schemas; #15, docs and harness synced in
#16), the shared job crates 0.2.0 of
[optimization-jobs](https://github.com/dietmarwo/optimization-jobs) at
revision `eb1c9de` instead of in-repo copies (#17), the `optimization-mcp`
server (#18), and bounds and gaps published only for the point an outcome
shows (#19); #20 moved the pin to `6a42472` and the release preparation to
`423cd87`, which add worker-declared artifacts and paged artifact reads
(`get_artifact` with `offset` and `limit` also pages through the `results`
journal of a running job); the pin is now `43ef3ae`, which adds the MCP tools
`put_input` and `get_guide` (with the corrected Pareto guidance) and a host
report in `node doctor`. The numerical crates and the Python package did not
change in these pull requests. Since then (CHANGELOG `[Unreleased]`), the
worker's `describe` lists JSON Schemas and runnable examples
([discovery](#discovery-schemas-and-examples)), and `scripts/mps_to_model.py`
converts MPS files ([Import an MPS model](#import-an-mps-model)). In another
checkout, check `Cargo.toml`,
`python/lp_ip_craft/__init__.py`, the worker's discovery output and the linked
guides before using a command or claiming a capability.

This file is operational context for an AI that must model and solve a user's
linear program (LP) or mixed-integer linear program (MIP) with this
repository, or decide that another tool fits better. It is not a performance
claim. Every solve status is a numerical finding under declared tolerances,
not an exact certificate; report results that way.

If the solver is not chosen yet, start with the
[entry point](https://github.com/dietmarwo/optimization-jobs/blob/main/ai-context.md)
in optimization-jobs. It routes between fast-cma-es, fcmaes-rust,
LP-IP-craft and SQP-craft and covers installation, the job API and the MCP
server.

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
| `optimization-job-api`, `optimization-runner` | git dependency on [optimization-jobs](https://github.com/dietmarwo/optimization-jobs) | Shared job protocol, the `optimization-run` runner and the `optimization-mcp` server |
| `lp-ip-job-worker` | `crates/lp-ip-job-worker` | Job worker for profiles `lp-ip.solve/1` and `lp-ip.evaluate/1` |

Nothing is published yet: neither crates.io nor PyPI has an `lp-ip-craft`
package at this review. The two library crates and the Python package are
prepared for both registries ([RELEASING.md](RELEASING.md)); the worker
stays source-only. Use a checkout
of https://github.com/dietmarwo/LP-IP-craft. The workspace version is 0.28.0;
the shared job crates are 0.2.0. Profile names such as `lp-ip.solve/1` are protocol
identifiers, independent of either.

Toolchains:

- The numerical crates need Rust 1.85 or newer; CI checks them with 1.85. The
  job worker and the shared job crates need Rust 1.88; CI checks the worker
  with 1.88. Plain `cargo build` builds only `crates/core` and
  `crates/native` (the default members).
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
| Does every variable have a finite lower and upper bound? | A MIP requires finite bounds on **all** variables, continuous ones included, of magnitude at most 2^53−1 after root inference, which runs only with `presolve` on. |
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
  nonsmooth, multimodal or multi-objective problems, quality-diversity
  archives or expensive simulations, see
  [fcmaes-rust](https://github.com/dietmarwo/fcmaes-rust). Both run on the
  same job node ([Other solvers on the same node](#other-solvers-on-the-same-node));
- the objective is quadratic: there is no QP or MIQP support;
- peak speed on hard instances decides: in the pinned HiGHS 1.15 comparisons
  of earlier releases, HiGHS stayed faster on public LPs such as 25fv47 and
  on some wide LPs, and proved more of the public MIPs
  ([0.21](docs/robustness-wide-cert/RESULTS.md#remaining-deficiencies),
  [0.27](docs/ripsolve-027/RESULTS.md#timings-and-limits)); it varies by
  model, so measure on the user's models;
- the application needs exact rational optimality or infeasibility
  certificates: statuses are numerical, and the bounded rational witness
  covers only incumbent feasibility;
- the model is a large, hard industrial MIP for which a mature solver is
  expected and permitted: compare with one under the same independent check
  before choosing;
- a MIP has variables, integer or continuous, that cannot be given finite
  bounds and whose bounds root inference cannot derive (it runs only with
  `presolve` on): native returns `Unsupported` (LPs may have infinite
  bounds);
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

Python's `solve` defaults to `backend="dummy"`, which only evaluates one point
(the start, or else the origin clamped to the bounds) and returns `stub`.
Always pass `backend="native"` to optimize.

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
are picklable. The packages read no model files. For an MPS file,
`scripts/mps_to_model.py` writes the JSON model artifact of the job API
([Import an MPS model](#import-an-mps-model)), which also gives the Python
arrays. There is no LP-format reader; otherwise build the CSR arrays from the
user's data.

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
| `presolve` | true | Master switch: with false, the whole root stage is skipped (reductions, bound inference, cuts and separation, root heuristics) and nodes skip domain propagation; a MIP then needs finite bounds as given |
| `mip_presolve`, `coefficient_tightening` | true | Default since the change after 0.27; MIP presolve keeps every feasible point, coefficient tightening every integer-feasible point. Set both false for the earlier behaviour |
| `reduced_cost_fixing`, `incumbent_heuristic`, `rational_certification`, `objective_lattice` | true | Each keeps the acceptance checks; disable only for matched ablations |
| `decompose`, `component_threads` | true, 16 | Independent components may run on up to 16 workers when `threads=1` |
| `strong_branching`, `reliability_branching`, `root_cuts`, `root_separation`, `gmi_cuts`, `mir_cuts`, `root_heuristics`, `feasibility_jump`, `dual_presolve` | false | Opt-in; effects vary by model |
| `packing_root`, `packing_neighborhoods`, `packing_cuts`, `scalable_branching`, `tree_threads` | off (false, 0 or 1) | Experimental packing and parallel-search modes |

Change one option at a time and compare on held-out models under the same
independent check. The README's 0.20–0.28 sections and
[docs/api.md](docs/api.md) describe each mechanism and its measured gains and
regressions. The "Deliberate limits" paragraph of docs/api.md predates several
of these options (cuts, strong branching, MIP presolve, tree workers); its
remaining limits, such as no basis import or export and no MIP relative-gap
stop, still hold.

### In process or as a job

The `lp-ip.solve/1` profile exposes a few of these controls, forces some and
leaves the rest at the native defaults:

| Control | In process (`Options`, `NativeOptions`) | `lp-ip.solve/1` job |
|---|---|---|
| Tolerances (feasibility, integrality) | `Options`, 1e-7 each by default | Required payload fields |
| `threads` | `Options`, 1–8 | `resources.inner_threads`, 1–8 |
| `node_limit`, `iteration_limit` | 100000 and 1000000; `None` removes a limit | Required, positive, per task; no unlimited value |
| `time_limit_seconds` | `None` | Forced: the job's remaining wall time |
| `presolve` | Master switch; its sub-switches stay settable | Payload field; `false` also clears `singleton_presolve`, `root_propagation`, `mip_presolve` and `coefficient_tightening` |
| `component_threads`, `packing_workers`, `tree_threads` | 1–16 (defaults 16, 1, 1) | Forced to 1 |
| Every other field: cuts, branching, heuristics, packing modes, `decompose`, `dual_tolerance`, factorization | Settable | Native default; unknown payload fields are rejected |
| Starts | One complete point per solve; LP starts rejected | `starts`, one task each; non-null only for a MIP |
| Many models | `solve_batch`, 1–16 workers, no starts | One model per job ([recipe](#many-independent-models-on-many-cores)) |
| Cancellation | `Cancellation` (Python), `CancellationToken` (Rust) | `jobs cancel` sets the native token |
| LP duals and reduced costs | `duals` of the result | Not exported: `details.lp_residuals` and the `kkt` check only |

Only in process: every option outside the profile, unlimited budgets, LP
duals, batches, and `evaluate`, `lp_residuals` and `safe_dual_bound` on
points and duals from any source (`lp-ip.evaluate/1` covers `evaluate`).

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
- `backend.effective_options` lists the configured value of every control,
  after native's own overrides (for example `component_threads=1` at width
  above 1), and the `diagnostics.*` counters. A listed control did not
  necessarily run: with `presolve=false`, sub-switches such as `mip_presolve`
  or `root_cuts` are still shown as set. `backend.start_status` says what
  happened to a start (`rejected` for every LP start).

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

### Many independent models on many cores

- One process: `solve_batch(models, workers=W)` (Rust
  `NativeSolver::solve_batch`) takes 1–16 workers; with `Options(threads=1)`
  that is W cores, and with `threads > 1`, `W * threads <= 8`. The batch
  forces component, packing and tree workers to 1, so W is its whole budget.
  The cap is 16 per batch: for more cores in process, run several batches at
  once (from threads or processes) and keep the sum of their workers within
  the cores.
- Jobs: a job holds one model, so many models are many jobs on one node. On a
  64-slot node (`node init --capacity 64`), submit 64 one-slot jobs
  (`1 / 1 / 1`) for many small or medium models, or 8 eight-slot jobs
  (`8 / 1 / 8`) for large LPs, or mix them. The runner never reserves more
  than the capacity; a job waits (`waiting`) until its slots are free. Each
  job reserves its `cpu_slots` for the whole attempt, so give a job more
  slots only when its own tasks use them.
- A caller that owns the concurrency (its own threads, processes or
  scheduler around plain `solve` calls) sets `component_threads=1`:
  `component_threads` defaults to 16 at `threads=1`, so each solve of a model
  that splits into large independent components (average size at least
  `component_parallel_min_entries`, 2048) may start up to 16 workers. Batches
  and the job worker set it to 1 themselves.
- Repeating identical `null` starts is not a scaling strategy: those tasks
  solve the same model the same way and, under node and iteration limits,
  return the same result. They suit timing and resource checks only. Use
  distinct complete MIP starts as tasks of one job, and different models as
  different jobs or batch entries.

## Shared agent job API

The envelope, result contract, runner and MCP server live in
[optimization-jobs](https://github.com/dietmarwo/optimization-jobs). Start
with its [guide](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md),
then [docs/job-api.md](docs/job-api.md) (this worker) and
[the LP/MIP tutorial](tutorials/job-api/README.md). The job layer gives an
agent durable, validated, CPU-admitted solves with journals, cancellation and
result manifests. Its profile is deliberately smaller than the in-process API.

```sh
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
python3 tutorials/job-api/run.py --root /tmp/lp-ip-job-tutorial
./target/release/optimization-run --root /tmp/lp-ip-job-tutorial \
  problems describe lp-ip
```

The tutorial creates an eight-slot node, registers the worker as `lp-ip`,
runs a binary MIP, the same model as an LP (both objective 7 at `[1, 0]`) and
an infeasible variant, eight tasks each, and evaluates three given points
with `lp-ip.evaluate/1`. `--bin-dir` selects the binaries (default
`target/release`). Commands for a single job, with
`validate`, `run`, `submit`, `jobs get|events|wait|cancel|repair` and
`result`, are in the guide.

Agents can drive the same jobs through `optimization-mcp`, the MCP server
that the runner's build also makes. Its thirteen tools describe the node and
its workers; validate, submit, wait for, cancel, kill and repair jobs; and
read results, events and artifacts. With the MCP Tasks extension, a submitted
job is a task that completes with its result. Initialize the node and
register the worker with `optimization-run` first: the server cannot
register workers or change the node. Then connect a client, for example
Claude Code:

```sh
claude mcp add lp-ip-jobs -- "$PWD/target/release/optimization-mcp" --root /abs/node
```

With the default `--launch process`, jobs outlive the server but not a
reboot. The tool table, launch modes and limits are in
[Agents and MCP](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#agents-and-mcp).
`describe_worker` also returns the payload and model schemas and runnable
examples ([discovery](#discovery-schemas-and-examples)), enough to build a
first job without a checkout.

### Other solvers on the same node

One runner and one MCP server can serve the workers of all three repositories
against one CPU budget. Send problems that are not LP/MIP to them:

| Problem | Repository, worker | Profile |
|---|---|---|
| Smooth constrained NLP with derivatives, at most 256 variables | [SQP-craft](https://github.com/dietmarwo/SQP-craft), `sqp-job-worker` | `sqp.solve/1` |
| Black-box, nonsmooth or multimodal scalar objective | [fcmaes-rust](https://github.com/dietmarwo/fcmaes-rust), `fcmaes-job-worker` | `fcmaes.retry/1` |
| Several objectives (Pareto front) | fcmaes-rust | `fcmaes.mode/1` |
| Quality-diversity archive | fcmaes-rust | `fcmaes.map-elites/1` |
| Expensive deterministic scalar simulation, 1–32 continuous variables, at most 256 evaluations | fcmaes-rust (experimental) | `fcmaes.surrogate/1` |

Both workers also check given points (`sqp.evaluate/1`, `fcmaes.evaluate/1`).
Unlike `lp-ip.solve/1`, which takes a model artifact, their profiles solve
models registered in Rust inside a worker (SQP-craft's registered models and
extension seam, fcmaes application workers); a job never carries code. Build
each worker in its own repository, register it with `workers add`, and read
that repository's `ai-context.md` first. Never add work counters across
profiles.

### Shared crates

The shared crates, the shared guide, the JSON Schemas, the integration
harness and the runner regression tests (`scripts/job-api/test_review.py`
with `fake_worker.py`) live in
[optimization-jobs](https://github.com/dietmarwo/optimization-jobs). This
repository depends on them at the git revision its root `Cargo.toml` pins;
`cargo build -p optimization-runner` still builds `optimization-run` and
`optimization-mcp` here,
and `scripts/job-api/shared_root.py` prints the path of Cargo's checkout, from
which CI runs the harness. Until October 2026 the three repositories carried
byte-identical copies. Version 0.1.1 synchronized them, after they had
drifted following the 0.1.0 delivery, and carries the review fixes of all
three:

- `jobs repair` seals a job whose launcher died while it was queued as
  `interrupted`, with zero work and complete accounting; detached attempts that
  their unit may still start stay unknown.
- Validation stages the worker in a `.staging-*` directory under the node root,
  so a `noexec` `TMPDIR` works; the node root itself must allow execution.
- A worker that writes an invalid journal record ends its job `failed` with
  `runner.journal_error` instead of leaving it `running`; `jobs list` reports
  the error for that job only.
- Appends end a torn journal tail first, a signal during validation stops the
  validator, finished jobs are not flagged after a reboot, `./model.json` is a
  valid input path, and detached jobs run in `optimization-job-ID.service`.
- The result contract also knows the fcmaes `non_dominated_set` and
  `elite_archive` claims; LP results never use them.

Version 0.2.0 adds the result contract `optimization.result/0.2` and worker
protocol `optimization.worker/0.2`: the claims `infeasible`, `unbounded`,
`infeasible_or_unbounded`, `locally_infeasible` and `certified_optimum`;
`evidence` with checks, refutations and a certificate slot;
`objective_bounds`; coded runner errors; and JSON Schemas
(`docs/job-api/schemas/` in optimization-jobs). The job envelope stays `optimization.job/0.1`. A 0.2
runner still accepts 0.1 workers, and a 0.1 runner rejects 0.2 workers at
registration. `describe` reports the shared version as
`build.job_api_version`. Run a node root through one runner build.

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
| `presolve` | Native's master switch; `false` skips the whole root stage, including bound inference, so a MIP then needs finite bounds as given. The worker clears the sub-switches, so the job's `effective_options` say what ran |

The model artifact is JSON with `schema_version: "lp-ip.model/1"`, `sense`
(`minimize` or `maximize`), `objective`, `objective_offset`,
`variable_lower`, `variable_upper`, `kinds`, `matrix` (`indptr`, `indices`,
`values`) and `row_lower`, `row_upper`. In bounds, `null` means infinite on
that side; JSON has no NaN or Infinity. Its JSON Schema is
`input_schemas["lp-ip.model/1"]` of `describe`, whose `$comment` lists the
rules across fields (lengths, the CSR layout by rows, bounds, the MIP
domain). Convert an MPS file ([Import an MPS model](#import-an-mps-model)),
or copy `tutorials/job-api/assignment-mip-model.json`, then recompute its
hash:

```sh
python3 -c 'import hashlib,pathlib; print(hashlib.sha256(pathlib.Path("my-model.json").read_bytes()).hexdigest())'
```

An LP with a non-null start, a start of the wrong length, or a non-finite
value is rejected at validation. Starts are inline and validation output is
capped at 4 MiB, so keep all starts together well below that size. Other
native options are not part of the profile. The worker uses the native
defaults, the payload's limits and tolerances, the remaining wall time as the
time limit, and forces `component_threads`, `packing_workers` and
`tree_threads` to 1, so `inner_threads` (1–8) maps only to `Options::threads`
([In process or as a job](#in-process-or-as-a-job)).

Each job holds one model; its tasks differ only by start. Null starts repeat
the same solve, which is useful for timing or the resource checks of the
tutorial but not as independent evidence. For many different
models, submit one job per model, each with its own slots, or use
`solve_batch` in process ([recipe](#many-independent-models-on-many-cores)).
Use `8 / 1 / 8` (`cpu_slots / outer_workers / inner_threads`) for one large
model and `8 / 8 / 1` for eight MIP starts. The
runner requires `outer_workers * inner_threads <= cpu_slots <= 1024` and
reserves `cpu_slots` for the whole attempt.

#### Import an MPS model

```sh
python3 scripts/mps_to_model.py model.mps -o model.json --names names.json
```

The script (Python standard library only; `.mps.gz` works too) reads fixed
and free MPS (`--format auto`, the default, tries free, then fixed) and
writes `lp-ip.model/1` with rows and columns in MPS order. It prints the
file's `sha256` for the job's `inputs`. `--names` writes the row and column
names in model order, to map `x` and the rows back. Its docstring documents
the conventions:

- The first `N` row is the objective; further `N` rows are dropped. An RHS
  `b` on the objective row is the constant: `objective_offset = -b`.
  `OBJSENSE` with `MAX` or `MAXIMIZE`, on the next line or the header line,
  sets `sense`; coefficients keep their signs.
- RANGES follow the MPS standard (on an `E` row, the sign of `R` picks the
  side). Default bounds are `[0, +inf)`, for integer marker columns too.
  `BV` makes a `binary` column, `MI` keeps the upper bound, and `UP` or `UI`
  with a negative value on a column without a given lower bound also sets
  the lower bound to -infinity, with a warning (the convention documented
  for CPLEX).
- Values of magnitude 1e30 or more (`--infinity`) in bounds, RHS and RANGES
  are infinite.
- Errors name the line: `SC` bounds, quadratic and other unsupported
  sections, an infinity on the wrong side, duplicates, unknown names, a
  second RHS, RANGES or BOUNDS set, a missing `ENDATA`.

The column-wise COLUMNS entries are transposed into the CSR by rows of
`lp-ip.model/1`. A converted MIP still needs finite bounds after root
inference: an integer marker column without bounds stays `[0, +inf)` and is
`unsupported` unless presolve derives a bound from the rows. For an
in-process solve, map `null` to an infinity:

```python
import json
from lp_ip_craft import Model
d, inf = json.load(open("model.json")), float("inf")
side = lambda v, s: [s if b is None else b for b in v]
m = Model(objective=d["objective"], objective_offset=d["objective_offset"], sense=d["sense"],
          lower=side(d["variable_lower"], -inf), upper=side(d["variable_upper"], inf),
          kinds=d["kinds"], row_lower=side(d["row_lower"], -inf),
          row_upper=side(d["row_upper"], inf), **d["matrix"])
```

### Discovery: schemas and examples

The worker's `describe` (`problems describe lp-ip`, MCP `describe_worker`)
lists JSON Schemas (draft 2020-12) of each payload (`payload_schemas`) and of
the model artifact (`input_schemas`, key `lp-ip.model/1`), and runnable
examples (`examples`): a continuous LP, a MIP with a complete start and an
`lp-ip.evaluate/1` job, each on `1 / 1 / 1` resources with its model inline.
Rules a schema cannot express (vector lengths, the CSR layout, MIP-only
starts, the `inner_threads` limits) are in `$comment`. The same files are in
`crates/lp-ip-job-worker/discovery/`; tests check that every example and
tutorial payload and model validates against them and that every example
passes the worker's validation.

To turn an example into a job, add `schema_version: "optimization.job/0.1"`,
a new `request_key`, `worker` (the registered ID) and `payload_schema` (the
example's `profile`). Write each input's `content` to its `file` with any JSON
serializer, put the SHA-256 of the written bytes in `sha256`, and keep only
`name`, `file` and `sha256` in the `inputs` entry: the envelope rejects other
fields. An MCP-only client that cannot write files next to the job uses the MCP
tool `put_input`: it uploads the model JSON and returns its `file`, `sha256`
and an `input_dir`, with which the job is then validated and submitted.

### Job results

Inspect the job `state` first: `completed` only means every task returned. Then
read each task's outcome with `optimization-run result JOB_DIRECTORY`
(schema `optimization.result/0.2`, ordered by task):

| Outcome field | LP/MIP meaning |
|---|---|
| `termination` | The native status, or `unsupported` when native refuses the model; `details.message` says why |
| `claim` | `numerical_optimum` (native `optimal` that passed the independent checks), `feasible` (checked candidate, no optimality), `infeasible`, `unbounded`, `infeasible_or_unbounded` (the native claims), or `none` |
| `evidence` | `rechecked` with `checks` `objective`, `bounds`, `constraints`, plus `integrality` (MIP) or `kkt` (LP with duals); `claimed` for the infeasibility and unboundedness claims, which the native solver does not certify for export, and for `none` (its `checks` list what was still recomputed); `refuted` when a check rejected the native status |
| `objective_bounds` | `primal` (objective of the checked feasible point the outcome shows) and `dual` (the proven best bound), in the model's `sense`; a bound beyond `primal` contradicts the point and is left out |
| `feasible`, `objective`, `x` | From the independent evaluation of the candidate |
| `details` | `sense`, `is_mip`, `best_bound` (the published `dual`), `gap` (of the shown point against it), `primal_violation`, `integrality_violation`, `lp_residuals` (LP), `raw_status`, `warm_start`, `engine_version`, `effective_options`, `message`, `exact_certificate: false`; `reported_best_bound` and `bound_note` for a contradictory bound, `reported_objective` when native's objective of its candidate differs from the recomputed one |

Non-finite numbers in details are the strings `"+infinity"` and `"-infinity"`.
An infeasible model is a `completed` job with `termination: "infeasible"` and
`claim: "infeasible"`. A native status that fails its independent check is not
reported as such: `evidence.refuted` names it, with verdict
`infeasible_point` (optimal, but the candidate violates the model; claim
`none`), `false_optimality` (an LP optimum whose duals fail the KKT
acceptance or overflow; claim `feasible`), `objective_mismatch` (an optimum
without KKT confirmation, such as a MIP's, whose candidate does not evaluate to
native's own objective; claim `feasible`) or `false_infeasibility`
(infeasible, but the returned point or the task's start is feasible; claim
`feasible`, and `x` and the details then describe that point). An optimal
status without a candidate has nothing to check: claim `none`, `claimed`, and
`details.note` says so. Bounds and gaps always describe the point the
outcome shows: the gap is recomputed from its objective, and a native bound
that contradicts it (beyond it by more than the shared `BOUND_TOLERANCE`,
relative 1e-6) is not published. A
refutation indicates a solver defect or a tolerance mismatch; report it. The
KKT acceptance is native's: primal residual within the feasibility tolerance,
stationarity and dual signs within `dual_tolerance * (1 + max|c|)`, duality gap
within `dual_tolerance`. Work counters are `nodes`,
`simplex_iterations` and `validation_evaluations`; never add them to another
profile's counters. If a worker is killed, `work` is null,
`runner.accounting_complete` is false and only committed tasks count.
Graceful cancellation sets the native token; `jobs cancel --now` terminates
the process group after at most one more second. Repair never resumes a
branch-and-bound tree; use a new request key for a fresh solve.

### Evaluate given points

`lp-ip.evaluate/1` checks points against the model without solving, for
example candidates an agent built or a solution from another solver. Payload:
`model_input`, `points` (finite vectors of the model's dimension, one task
each; they may violate bounds) and the two tolerances; `inner_threads` must be 1.
Points are inline: the resolved job must stay below the runner's 4 MiB
validation output. Each outcome has `termination: "evaluated"`, claim
`feasible` within the tolerances or `none`, the objective, `primal_violation`
and `integrality_violation` in details, `objective_bounds.primal` for a
feasible point, and `rechecked` evidence. A point whose objective or row
activity overflows gets claim `none`, no objective, unknown feasibility,
`claimed` evidence and the reason in `details.message`. The work counter is `evaluations`.
See `tutorials/job-api/assignment-evaluate.json`.

### Extend the job profile

The adapter is `crates/lp-ip-job-worker/src/main.rs`. Expose another native
option only with a new, strictly typed payload field, validate it in
`prepare`, record it in `effective_options`, add it to the payload schema in
`crates/lp-ip-job-worker/discovery/`, add tests, and bump the profile
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
| `NotImplementedError` / `Unsupported` for a MIP | An infinite or huge variable bound that root inference could not tighten (or `presolve` is off); add justified finite bounds |
| Job task `termination: "unsupported"` | The same refusal inside a job; the message is in `details.message` |
| LP start rejected | Basis import is not implemented; pass no start (`null` in a job) |
| `time_limit` or `node_limit` with a candidate | Report it as feasible with its gap; raise the budget or try opt-in search options |
| `numerical_failure` or `unknown` | Rescale the model, check for near-parallel rows or huge coefficients, compare with a reference solver |
| `Unsupported` naming `max_factor_entries` | The guard (4,000,000 entries) on one factorization; heavy LU fill is a known limit. It is not a whole-process memory cap |
| A parallel component solve stops earlier than serial | Reserved component budgets; set `component_threads=1` |
| `TypeError` naming an option | An unknown or removed `NativeOptions` field |
| A queued job never finishes after its launcher died | `jobs repair JOB_ID` seals it `interrupted` with zero work; see [Shared crates](#shared-crates) |

## Test commands

The main CI commands, from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p lp-ip-craft-native --release --example benchmark --locked -- 1
cargo test --release --locked -p lp-ip-job-worker
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
SHARED=$(python3 scripts/job-api/shared_root.py)
export JOB_API_TARGET="$PWD/target/release" JOB_API_TUTORIALS="$PWD/tutorials/job-api"
python3 "$SHARED/scripts/job-api/test_integration.py"
python3 "$SHARED/scripts/job-api/test_review.py"
python3 "$SHARED/scripts/job-api/test_mcp.py"
LP_IP_JOB_WORKER="$PWD/target/release/lp-ip-job-worker" python3 -m pytest -q tests/test_job_discovery.py
python3 tutorials/job-api/run.py --root /tmp/job-api-tutorial
python -m pip install ".[test]" && python -m pytest -q
python examples/solve_native.py
python examples/safe_bound.py
```

The test extra includes `jsonschema`, so `pytest` also checks the discovery
schemas and examples (against the binary only when `LP_IP_JOB_WORKER` is set)
and the MPS converter. CI also checks the numerical crates with Rust 1.85
(excluding the job worker) and the job worker with Rust 1.88, and runs family
benchmark smoke runs, the dependency audit and the SciPy oracle validations
(`examples/validate_*.py`, with `scipy` installed). Pass `--fcmaes`, `--sqp`
(and `--lp-ip`) executables to `test_integration.py` to test a mixed node;
optimization-jobs' CI does that with all three workers.

## Repository references

- [README.md](README.md): release notes, build commands, guarantees and limits
- [docs/api.md](docs/api.md): native API, options, kernel, search, results
- [docs/safe-dual-bounds.md](docs/safe-dual-bounds.md): conservative bounds
- [docs/simplex/README.md](docs/simplex/README.md): kernel and search design and validation
- [docs/parallel-028](docs/parallel-028/VALIDITY.md): execution contract for 1–8 workers
- [docs/job-api.md](docs/job-api.md) (this worker, with links into the shared
  guide), [tutorials/job-api](tutorials/job-api/README.md),
  [docs/job-api/VALIDATION.md](docs/job-api/VALIDATION.md),
  [docs/job-api/REVIEW.md](docs/job-api/REVIEW.md): shared job API and this worker
- [optimization-jobs](https://github.com/dietmarwo/optimization-jobs) and its
  [guide](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md):
  envelope, result contract, schemas, runner and MCP server
- [docs/target-card.md](docs/target-card.md): the open application contract
- `crates/core/src/check.rs`, `crates/core/src/bound.rs`: independent checks
- `crates/lp-ip-job-worker/src/main.rs`: the `lp-ip.solve/1` and `lp-ip.evaluate/1` adapter
- `crates/lp-ip-job-worker/discovery/`: the payload and model JSON Schemas and
  the examples that `describe` embeds
- `scripts/mps_to_model.py`: MPS to `lp-ip.model/1`, with its conventions in
  the docstring
- [AGENTS.md](AGENTS.md): short orientation and contributor commands
- `python/lp_ip_craft/__init__.py`: Python API and defaults
