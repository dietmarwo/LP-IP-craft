# Job API validation

Tested on Linux x86_64 with Rust 1.99.1 (2026-10-13). The API layer declares
Rust 1.88 as its minimum; that exact toolchain was not separately tested.
All commands use the delivered lockfiles. Solver cores are unchanged from the
base commits recorded in `SOURCE-REVISION`.

| Gate | Observed result |
| --- | --- |
| Shared API/runner and native adapters | Release builds passed in all three independent workspaces |
| Job layer format / Clippy | Passed; Clippy used all targets and `-D warnings` |
| Shared contract/durability tests | 18 passed, including legacy validation and float round-trip regression |
| fcmaes selected Rust tests, release | 30 passed (includes shared contracts, facade and native worker tests) |
| Legacy fcmaes runner/worker tests, debug | 11 passed, including 6 real CLI lifecycle tests |
| Original fcmaes Python integration suite | 14 passed unchanged |
| Mixed integration with all three native workers | 10 passed, none skipped |
| LP-IP local integration | 8 passed; 2 tests for other solvers intentionally skipped |
| SQP local integration | 8 passed; 2 tests for other solvers intentionally skipped |
| LP-IP core/native/adapter Rust selection | 196 passed |
| SQP core/solver/QP/parallel/adapter Rust selection | 197 passed; 1 optional documentation census ignored |
| Tutorial executions | 9 examples completed; numerical/termination assertions passed |
| systemd detached acceptance | Not verified: no systemd user manager; explicit skip (exit 77) |

Counts above describe test invocations and overlap; they are not a claim of
unique test cases. Python extension suites were not re-run for unchanged
bindings. Build targets, toolchains and node executables are not bundled.

## Numerical and lifecycle coverage

The mixed harness uses deterministic retry seeds, independent objective/bound
checks, known LP/MIP optima, infeasible and unbounded LPs, analytic derivative
checks and independent SQP KKT acceptance for both QP backends. It compares
serial and eight-task layouts; LP/SQP also run with one task and eight inner
threads. These are correctness/resource checks, not performance benchmarks.

Lifecycle cases include strict schema rejection, unchanged-key idempotency,
conflicting keys, over-budget resource rejection, model checksum failure,
artifact escape, duplicate journals, queued cancellation under a shared
capacity of eight, forced timeout with unknown active-work totals, killed
launcher with a live worker, conservative slot retention and repair without
solver resume. Native snapshots and checked journals are retained for all
nine tutorial examples under `validation/job-api/tutorial-runs/` in their
respective repository archives.

## Fixes found during acceptance

The first LP assertion incorrectly required bit-exact equality between a
conservative bound and objective 7. It now verifies tolerance agreement and
that the bound remains on the conservative side; no solver values were changed.

The first fcmaes tutorial revealed that the default JSON parser could shift
small f64 values by one bit on a repeated parse, causing manifest validation to
reject a completed journal. The shared crate now enables `float_roundtrip`.
The regression test covers small/large exponents, stable spec digests and the
complete journal-to-manifest-to-runner path. All tutorial examples were rerun.

Extracting the legacy runner also required retaining its original test-only
dependencies. All-target compilation and the unchanged CLI tests verified that
compatibility surface, including the original live-worker repair diagnostic.

## Reproduce

This section reproduces the 0.1.0 delivery archives. For the current tree,
whose shared crates and harness come from optimization-jobs, use
[Running the shared tests here](../job-api.md#running-the-shared-tests-here).
For the archives, use the commands in `docs/job-api.md` and the repository's
`tutorials/job-api/README.md`. Each ZIP builds independently. To reproduce the
mixed test, build each repository's worker and pass all three absolute paths to
`scripts/job-api/test_integration.py`. The exact native test selections are:

```sh
# fcmaes-rust
cargo test --release --locked -p optimization-job-api -p optimization-runner \
  -p fcmaes-job-worker -p fcmaes-native-worker
cargo test --locked -p fcmaes-runner -p fcmaes-native-worker
python3 scripts/runner/test_integration.py

# LP-IP-craft
cargo test --release --locked -p lp-ip-craft -p lp-ip-craft-native -p lp-ip-job-worker

# SQP-craft
cargo test --release --locked -p sqp-craft -p sqp-craft-solver \
  -p sqp-craft-activeset -p sqp-craft-clarabel -p sqp-craft-parallel -p sqp-job-worker
```

`validation/job-api/` in the delivery archive (not committed) contains raw
logs, tutorial results, test environment and binary hashes in the recorded
attempt provenance. Those paths refer to the
validation host and are evidence, not portable execution configuration.
`docs/job-api/SHARED-SOURCES.sha256` verifies identical shared implementation
files across the three archives. `MANIFEST.sha256` covers every delivered file
except itself and can be checked with `sha256sum -c MANIFEST.sha256` after unzip.

## Review and merge (main 166bda7)

After the corrections in [REVIEW.md](REVIEW.md), on Linux x86_64 with Rust
1.97.0:

| Gate | Observed result |
| --- | --- |
| `cargo fmt --check`, Clippy `--workspace --all-targets -D warnings` | Passed |
| Workspace tests, debug | 226 passed, including the job crates |
| Job crates, release (`-p optimization-job-api -p optimization-runner -p lp-ip-job-worker`) | 25 passed: 18 shared, 7 new LP adapter tests |
| `scripts/job-api/test_integration.py` with the LP worker | 8 passed; the fcmaes and SQP cases skipped |
| `tutorials/job-api/run.py` | 3 examples × 8 tasks; objective 7 and the infeasible termination checked |
| Unsupported MIP through the runner | Job `completed`, both tasks `unsupported`, zero work |
| Numerical crates and `cargo tree -p lp-ip-craft-native` | Unchanged against main |
| MSRV: 1.85 for the numerical crates, 1.88 for the job crates | Passed in CI (GitHub Actions); the toolchains could not be installed on the review host |

The delivery's raw logs under `validation/job-api/` were lost with the
truncated archive, and the review's own logs are not committed.

## Synchronization as 0.1.1 (19 October 2026)

The shared files were replaced by the 0.1.1 set that fcmaes-rust, LP-IP-craft
and SQP-craft now share byte for byte (20 files, `SHARED-SOURCES.sha256`).
Until then this repository had the delivered runner. 0.1.1 brings in
SQP-craft's review fixes and fcmaes-rust's zero-work repair of a queued job
whose launcher died ([job API guide](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#results-and-durable-recovery)),
the fcmaes set and archive claims, and the shared runner regression tests
`scripts/job-api/test_review.py`. CI now runs them and checks the manifest. The
LP worker, the numerical crates and `lp-ip.solve/1` are unchanged.

On Linux x86_64 with Rust 1.97:

| Gate | Observed result |
| --- | --- |
| `cargo fmt --check`, Clippy `-D warnings` for the job crates | Passed |
| Job crates, release | 27 passed: 20 shared (5 legacy spec, 7 contracts, 8 durability), 7 LP adapter |
| `scripts/job-api/test_integration.py` with the LP worker | 9 passed; the fcmaes and SQP cases skipped |
| `scripts/job-api/test_review.py` with the LP worker | 7 passed |
| `tutorials/job-api/run.py` | 3 examples × 8 tasks passed |
| `sha256sum -c docs/job-api/SHARED-SOURCES.sha256` | 20 files OK |
| Mixed node: `test_integration.py` with all three workers, under each repository's runner build | 12 passed, none skipped, three times |


## Result contract 0.2 (19 October 2026)

The shared crates moved to 0.2.0 with the result contract 0.2: five new
claims, evidence (levels, checks, refutations, certificate slot), objective
bounds, worker protocol 0.2 with 0.1 still accepted, coded runner errors,
`already_terminal` on cancelling a finished job, and JSON Schemas for the job
envelope and the result manifest. The shared set grew to 23 files with the
two schemas and `scripts/job-api/outcome_cases.json`. CI installs `jsonschema`
and sets `JOB_API_REQUIRE_SCHEMA=1`, so the schema checks cannot be skipped
there.

An independent review before the merge reported 13 defects; all were fixed
and are covered by tests (see the [plan](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/PLAN.md#6-result-contract-02-october-2026)).
After the fixes, 1.2 million generated outcomes found no disagreement between
the JSON Schema and `Outcome::check`, and every case of `outcome_cases.json`
behaves the same as a journal record through the runner.

In this repository the LP worker reports `infeasible`, `unbounded` and
`infeasible_or_unbounded` as claims, adds `objective_bounds`, rechecks an LP
optimum's duals with native's KKT acceptance, refutes a native status that
fails its check (`infeasible_point`, `false_optimality`,
`false_infeasibility` from a feasible candidate or start), and adds
`lp-ip.evaluate/1`. Checked with Rust 1.97 on Linux x86_64:

| Gate | Observed result |
| --- | --- |
| `cargo fmt --check`, Clippy `-D warnings` for the workspace | Passed |
| Workspace tests (`cargo test --workspace --locked`) | 236 passed, including 26 shared (5 legacy spec, 13 contracts, 8 durability) and 9 LP adapter tests |
| `scripts/job-api/test_integration.py` with the LP worker | 13 passed, 3 skipped (fcmaes and SQP cases) |
| `scripts/job-api/test_review.py` (`JOB_API_REQUIRE_SCHEMA=1`) | 15 passed |
| `tutorials/job-api/run.py` | 4 examples; jobs and results validated against the schemas |
| Mixed node with all three workers, under each repository's runner build | 13 passed, none skipped, three times |
| `sha256sum -c docs/job-api/SHARED-SOURCES.sha256` | 23 files OK |
| Numerical crates | Unchanged |

## The shared crates move to optimization-jobs (19 October 2026)

`optimization-job-api`, `optimization-runner`, the shared guide, plan,
schemas and scripts moved to
[optimization-jobs](https://github.com/dietmarwo/optimization-jobs)
([plan](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/PLAN.md#8-one-shared-repository-october-2026)).
This repository depends on the crates at a pinned git revision; the copies and
their checksum manifest are gone, and `docs/job-api.md` points to the shared
guide. The 26 tests of the shared crates now run there. The worker keeps the
runner as a dependency, so `cargo build -p optimization-runner` still builds
`optimization-run` here. On the git dependency:

| Gate | Observed result |
| --- | --- |
| `cargo fmt --check`, Clippy `-D warnings` for the worker | Passed |
| Workspace tests (`cargo test --workspace --locked`) | 210 passed (236 before, less the 26 shared tests) |
| Shared harness from Cargo's checkout, with the LP worker | 14 run: 10 passed, 4 skipped (fcmaes and SQP cases) |
| Shared review tests (`JOB_API_REQUIRE_SCHEMA=1`) | 15 passed |
| `tutorials/job-api/run.py` | 4 examples; jobs and results validated against the schemas of the checkout |
| Numerical crates | Unchanged |

Earlier sections mention `scripts/job-api/` and the checksum manifest as they
were at the time.

## The MCP server (19 October 2026)

This repository moved to optimization-jobs revision `0ee495f`, which adds
`optimization-mcp`, the MCP server over one node
([guide](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api.md#agents-and-mcp),
[plan](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/PLAN.md#9-mcp-server-october-2026)). `cargo build -p optimization-runner -p lp-ip-job-worker` builds it here.
Nothing in this repository's code changed. On Linux x86_64 with Rust 1.97:

| Check | Observed result |
| --- | --- |
| `cargo fmt --check`, Clippy `-D warnings` | Passed |
| Workspace tests (`cargo test --workspace --locked`) | 210 passed |
| Shared harness from Cargo's checkout | 14 run: 10 passed, 4 skipped (fcmaes and SQP cases) |
| Shared review tests (`JOB_API_REQUIRE_SCHEMA=1`) | 15 passed |
| `test_mcp.py` with this repository's binaries | 25 passed, including the real-worker test (`lp-ip.solve/1`, claims and evidence) |
| `tutorials/job-api/run.py` | 4 examples; jobs and results validated against the schemas of the checkout |

## Review fixes (20 October 2026)

The review of 20 October 2026 found that a refuted optimum could still carry
the solver's contradictory bound and a gap computed from the solver's own
objective (LP-01, reproduced with an injected result; not seen in a normal
solve). The worker now publishes bounds and the gap for the point the outcome
shows: the gap is recomputed from that point's objective, a bound beyond it
(by more than the shared `BOUND_TOLERANCE`) is kept only as
`details.reported_best_bound` with a `bound_note`, an overflowing gap is left
out instead of failing the task, and a MIP optimum whose candidate does not
evaluate to the solver's own objective is refuted (`objective_mismatch`). The
regression test covers minimization, maximization, the offset, a stale
evaluation, a contradictory bound, warm-start substitution and an overflowing
gap.

This repository moved to optimization-jobs revision
[`eb1c9de`](https://github.com/dietmarwo/optimization-jobs/commit/eb1c9de66e677097a7ea22d9a4b2f6357c18751d),
whose `Outcome::check` now rejects contradictory bounds itself, and which
fixes the cancellation race, bounds the MCP server's requests and tests the
shared SDK inside this worker before a repin. On Linux x86_64 with Rust 1.97:

| Check | Observed result |
| --- | --- |
| `cargo fmt --check`, Clippy `-D warnings` for the worker | Passed |
| Workspace tests (`cargo test --workspace --locked`) | 211 passed |
| Shared harness from Cargo's checkout | 14 run: 10 passed, 4 skipped (fcmaes and SQP cases) |
| Shared review tests (`JOB_API_REQUIRE_SCHEMA=1`) | 17 passed |
| `test_mcp.py` with this repository's binaries | 28 passed, including the real-worker test |
| `tutorials/job-api/run.py` | 4 examples; jobs and results validated against the schemas of the checkout |
