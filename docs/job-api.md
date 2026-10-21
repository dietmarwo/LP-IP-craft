# Shared job API

The job envelope `optimization.job/0.1`, the result contract
`optimization.result/0.2`, the worker SDK and the `optimization-run` runner
live in [optimization-jobs](https://github.com/dietmarwo/optimization-jobs),
shared with the other worker repositories
([fcmaes-rust](https://github.com/dietmarwo/fcmaes-rust),
[LP-IP-craft](https://github.com/dietmarwo/LP-IP-craft),
[SQP-craft](https://github.com/dietmarwo/SQP-craft)). This repository depends
on its crates `optimization-job-api` and `optimization-runner` at the revision
pinned in the root `Cargo.toml`, and contributes `lp-ip-job-worker`.

## The shared guide

| Topic | Section of the guide |
| --- | --- |
| Envelope fields, inputs and validation | [Envelope](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#envelope) |
| `cpu_slots`, `outer_workers` and `inner_threads` | [Eight CPU slots](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#eight-cpu-slots) |
| Commands, lifecycle states and cancellation | [Run, inspect and stop](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#run-inspect-and-stop) |
| Error codes | [Errors](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#errors) |
| Claims, evidence and refutations | [Termination, claim and evidence](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#termination-claim-and-evidence) |
| Journals, accounting and recovery | [Accounting and recovery](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#accounting-and-recovery) |
| Agents and MCP | [Agents and MCP](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#agents-and-mcp) |
| JSON Schemas of jobs and results | [`docs/job-api/schemas`](https://github.com/dietmarwo/optimization-jobs/tree/main/docs/job-api/schemas) |
| Implementation history and gates | [Plan](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/PLAN.md) |

## `lp-ip-job-worker`

Profiles `lp-ip.solve/1` (sparse LP/MIP artifacts, candidate starts, node and pivot budgets, numerical checks) and `lp-ip.evaluate/1` (given points); see the [tutorial](../tutorials/job-api/README.md) and
the [validation report](job-api/VALIDATION.md); the [review](job-api/REVIEW.md) records the corrections made before the 0.1 merge.
`cargo build --release --locked -p optimization-runner -p lp-ip-job-worker` builds the
runner, the MCP server and the worker; Cargo fetches the shared crates.
The worker's `describe` lists JSON Schemas of both payloads and of the model
artifact, and runnable examples (`payload_schemas`, `input_schemas`,
`examples`, from [`crates/lp-ip-job-worker/discovery`](../crates/lp-ip-job-worker/discovery));
[`scripts/mps_to_model.py`](../scripts/mps_to_model.py) converts MPS files to the model artifact.

## Agents: the MCP server

The same build makes `optimization-mcp`, an MCP server over one node
([Agents and MCP](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#agents-and-mcp)). On a node with this
worker registered (as `lp-ip`), connect an MCP client, for example Claude Code:

```sh
claude mcp add lp-ip-jobs -- "$PWD/target/release/optimization-mcp" --root /abs/node
```

Its tools describe the node and the worker, validate and submit jobs, wait for
them, read their claims and evidence, and cancel or repair them; with the MCP
Tasks extension, a submitted job is a task that completes with its result.
The [tutorial](../tutorials/job-api/README.md#through-an-agent-mcp) connects
it to the tutorial's node.

## Running the shared tests here

The shared harness runs from Cargo's checkout of optimization-jobs at the
pinned revision, against this repository's binaries:

```sh
cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
SHARED=$(python3 scripts/job-api/shared_root.py)
export JOB_API_TARGET="$PWD/target/release" JOB_API_TUTORIALS="$PWD/tutorials/job-api"
python3 "$SHARED/scripts/job-api/test_integration.py"
python3 "$SHARED/scripts/job-api/test_review.py"
python3 "$SHARED/scripts/job-api/test_mcp.py"
```

A change to the shared crates lands in optimization-jobs first. Then change
`rev` in the root `Cargo.toml` and run
`cargo update -p optimization-job-api -p optimization-runner`. To develop both
at once, add a `[patch."https://github.com/dietmarwo/optimization-jobs"]`
section with path dependencies to a local checkout, as the
[optimization-jobs README](https://github.com/dietmarwo/optimization-jobs#depending-on-this-repository)
shows.
