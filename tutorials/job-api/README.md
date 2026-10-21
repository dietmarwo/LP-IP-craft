# Tutorial: LP and MIP jobs from checked model artifacts

This tutorial minimizes `2*x0 + 3*x1 + 5` with `x0+x1=1` and `0<=x<=1`, first
with binary variables and then continuous ones. Both have objective 7 at
`[1,0]`. A third example asks for `x0+x1=3` and is infeasible. A fourth
evaluates three given points of the binary model without solving.

## Build and execute

From the repository root, on Linux with Rust 1.88+ and Python 3:

```sh
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
python3 tutorials/job-api/run.py --root /tmp/lp-ip-job-tutorial
```

The script creates an eight-slot node and runs all four examples, with eight
independent tasks per solve job. It looks for the binaries in `target/release`, or in
`$CARGO_TARGET_DIR/release` when that is set; `--bin-dir` overrides both. It
checks objective 7, the evidence and the objective bounds for the feasible
models, the infeasibility claim for the third, and the three evaluated points.
If the `jsonschema` package is installed, it also validates every job and
result against the published
[job](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/schemas/optimization.job-0.1.schema.json) and
[result](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/schemas/optimization.result-0.2.schema.json)
schemas, from Cargo's checkout of optimization-jobs at the pinned revision. The final
JSON includes job IDs and directories; `tutorial-report.json` is saved under
the node.

An infeasible model still produces lifecycle `completed` when the solver
returned normally. Its outcome has `termination: "infeasible"`, `claim:
"infeasible"` and `evidence.level: "claimed"`: the native solver checks its
claim with a Farkas row but does not export a certificate. The feasible
examples have `claim: "numerical_optimum"` with `evidence.level: "rechecked"`:
the worker re-evaluated the candidate (`objective`, `bounds`, `constraints`,
and `integrality` for the MIP or `kkt`, the recomputed dual residuals, for the
LP). This is a numerical statement under the solver's tolerances, not an exact
certificate. `objective_bounds` holds the candidate's objective (`primal`) and
the proven bound (`dual`).

## One job, step by step

```sh
./target/release/optimization-run --root /tmp/lp-ip-manual node init --capacity 8
./target/release/optimization-run --root /tmp/lp-ip-manual workers add \
  --id lp-ip --executable ./target/release/lp-ip-job-worker
./target/release/optimization-run --root /tmp/lp-ip-manual problems describe lp-ip
./target/release/optimization-run --root /tmp/lp-ip-manual validate \
  --spec tutorials/job-api/assignment-mip.json
./target/release/optimization-run --root /tmp/lp-ip-manual run \
  --spec tutorials/job-api/assignment-mip.json
```

Use `optimization-run result JOB_DIRECTORY` to read the result. Every task
reports nodes and simplex iterations separately. Details include the incumbent,
original objective sense, best bound, absolute/relative gap, independent primal
and integrality violations, and LP dual residuals when available. Conservative
bound rounding can differ from the objective by a few floating-point ulps.

If an independent check contradicts the native status, the worker reports
what survives and names the rejected status in `evidence.refuted`: an optimal
status with an infeasible candidate (`infeasible_point`), LP duals failing the
KKT acceptance (`false_optimality`), a MIP optimum whose candidate does not
evaluate to the solver's own objective (`objective_mismatch`), or an
infeasibility claim although the returned point or the task's start is
feasible (`false_infeasibility`; `x` and the details then describe that
point). Bounds and the gap always describe the point the outcome shows; a
best bound that contradicts it is not published as `objective_bounds.dual`
but kept as `details.reported_best_bound`.

## Through an agent (MCP)

The runner's build also makes `optimization-mcp`, an MCP server over the same
node. With the node of the previous section, connect an MCP client, for
example Claude Code:

```sh
claude mcp add lp-ip-jobs -- "$PWD/target/release/optimization-mcp" --root /tmp/lp-ip-manual
```

The agent sees the worker with `describe_worker` (worker `lp-ip`), which also
lists JSON Schemas of both payloads (`payload_schemas`) and of the model
artifact (`input_schemas`) and runnable examples (`examples`) to build a
first job from. It can pass
any job of this directory to `submit_job` as `spec_path` (its input files are
read next to it), follows it with `wait_job` and reads each task's claim,
evidence and objective bounds with `get_result`; `cancel_job` stops it gracefully. With
the MCP Tasks extension, `submit_job` returns a task that completes with the
job's result. See [Agents and MCP](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#agents-and-mcp).

## Evaluate given points

`assignment-evaluate.json` uses `payload_schema: "lp-ip.evaluate/1"` to check
three points against the binary model without solving:

```json
"payload": {"model_input": "model", "points": [[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            "feasibility_tolerance": 1e-07, "integrality_tolerance": 1e-07}
```

Each point is one task with `termination: "evaluated"`: `[1,0]` and `[0,1]`
are `feasible` (objectives 7 and 8), `[1,1]` violates the row and is `none`.
Points may lie outside the bounds; they are then reported infeasible. A point
whose objective or row activity overflows is `none` without an objective,
with the reason in `details.message`. Points are inline: keep the job well
below the runner's 4 MiB validation output. Use `inner_threads` 1.

## Supply your own LP/MIP

Convert an MPS file (it prints the SHA-256 of the written model):

```sh
python3 scripts/mps_to_model.py model.mps -o my-model.json --names names.json
```

or copy `assignment-mip-model.json` and edit the `lp-ip.model/1` artifact:

| Field | Meaning |
| --- | --- |
| `sense` | `minimize` or `maximize`; output values retain that sense |
| `objective`, `objective_offset` | Coefficients and constant |
| `variable_lower`, `variable_upper` | One bound per column; null means unbounded on that side |
| `kinds` | `continuous`, `integer` or `binary` per column |
| `matrix` | Zero-based CSR `indptr`, sorted unique `indices`, finite `values` |
| `row_lower`, `row_upper` | One bound per row; equal finite values express an equality |

The number of columns is the objective length; the row count is the row-bound
length. `indptr` has `rows+1` entries, begins at zero and ends at the nonzero
count. A zero-row model uses `[0]` and empty index/value arrays. JSON NaN and
Infinity are invalid. A null lower bound means negative infinity; a null upper
bound means positive infinity. Binary bounds must lie in `[0,1]`. Existing native
restrictions, including MIP domain/range checks, still apply.

Copy the job JSON, set a new request key, reference the new model using a
relative path, and update its checksum:

```sh
python3 -c 'import hashlib,pathlib; print(hashlib.sha256(pathlib.Path("my-model.json").read_bytes()).hexdigest())'
```

Put this exact value in `inputs[0].sha256`. A content change without a new hash
is rejected. `payload.model_input` selects the input by its `name`, not by a
second filename. The artifact is staged and rechecked at execution, then the
model is shared by the worker's tasks.

`starts` defines the tasks. Use `[null]` for one solve, or multiple null entries
for independent repeats. Starts are inline in the job file and are echoed back
by worker validation, whose output is capped at 4 MiB, so all starts together
must stay well below that size. A MIP entry can be a complete finite primal vector;
it is considered as a candidate, without rounding or silently repairing it.
Continuous LP warm starts are currently unsupported and rejected. Node and
iteration limits are positive per-task budgets. `presolve` is native's master
presolve switch. With `true`, the native defaults apply, including MIP presolve
and coefficient tightening. With `false`, every reduction is off: LP and
singleton presolve, MIP presolve, coefficient tightening, and root and node
bound propagation; the reported `effective_options` say so. Other native tuning
options are outside this first profile and are rejected.

Native solves a MIP only when every variable bound is finite after root
propagation. A model outside that domain, such as a MIP with a free continuous
column, does not fail the job: each task completes with `termination:
"unsupported"`, `claim: "none"` and the native message in `details.message`.

## Choose the parallel layout

The tutorial uses `cpu_slots=8, outer_workers=8, inner_threads=1`. For one larger
model use `starts:[null]`, `outer_workers=1, inner_threads=8`, keeping eight slots.
The worker disables nested component, frontier and residual task pools; only
the selected numerical kernel budget runs inside each task. This is an upper
bound, not a guarantee that a small model uses every thread.

LP/MIP cancellation uses the native token. A returned candidate can survive a
node, pivot or time limit; inspect both termination and feasibility. Forced
process termination retains completed tasks but cannot reconstruct the active
branch-and-bound tree or its uncommitted work.

See the [shared API guide](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md) for mixed-worker nodes, commands and
recovery, and [shared job API](../../docs/job-api.md) for running the shared
acceptance harness against this adapter. The native solver API remains documented in
[docs/api.md](../../docs/api.md).
