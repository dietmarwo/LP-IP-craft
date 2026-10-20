# Review of the job API 0.1.0 delivery

18 October 2026. The delivery (`LP-IP-craft-job-api-0.1.0.zip`, change
record `PATCH-job-api.diff`) was reviewed, tested and corrected before it was
merged into `main`. This page records what the review found and changed. The
delivered [PLAN.md](https://github.com/dietmarwo/optimization-jobs/blob/main/docs/job-api/PLAN.md) (now in optimization-jobs) and [VALIDATION.md](VALIDATION.md) stay as
delivered; the validation of the merged code is appended to VALIDATION.md.

## Import

- **The archive was truncated.** It had no central directory. 540 of its 541
  entries were recovered from their local headers. Every crate, tutorial and
  script file matches the delivered `MANIFEST.sha256` and
  `SHARED-SOURCES.sha256`. The 39 raw validation logs (`validation/job-api/`)
  and one data file of `docs/parallel-028` were lost. The raw logs stay out
  of the repository, as for earlier deliveries; the review's own runs are
  recorded in [VALIDATION.md](VALIDATION.md#review-and-merge-main-166bda7).
- **New base.** The delivery was built on `af9a9a4`, the unreviewed 0.28
  branch. The patch was applied to `main` (`166bda7`, the reviewed 0.28).
  Only README.md and docs/api.md conflicted, both in prose. The numerical
  crates (`crates/core`, `crates/native`, `crates/python`), the Python
  package and its tests are unchanged, and so is `cargo tree -p
  lp-ip-craft-native`.

## Corrections

1. **CI would have failed on the first push.** The job crates declare Rust
   1.88; they use let chains. The existing MSRV job ran `cargo check
   --workspace` with Rust 1.85, and Cargo refuses to build a package whose
   `rust-version` is newer than the compiler. The 1.85 job now checks the
   numerical crates only. `job-api.yml` gains a Rust 1.88 check of the three
   job crates, which the delivery had not tested ("that exact toolchain was
   not separately tested"). It also gets the same action versions, cache and
   read-only permissions as `ci.yml`.
2. **`presolve: false` did more than documented and reported less.**
   - The tutorial said it maps to the LP presolve option and that the MIP
     presolve defaults stay in effect.
   - In fact `presolve` is native's master switch. With `false`, the root
     skips every reduction: singleton and MIP presolve, coefficient
     tightening and root propagation. Nodes also skip domain propagation.
   - The task's `effective_options` still listed `mip_presolve`,
     `coefficient_tightening`, `singleton_presolve` and `root_propagation` as
     on.
   - The adapter now clears those switches too, which changes nothing that
     runs but makes the reported options true. The tutorial describes the
     switch correctly.
3. **A model outside the native domain failed the whole job.** For example,
   a MIP with a free continuous column makes native refuse the model before
   any node. The first task's `Unsupported` error stopped every other task.
   The job ended `failed`, and the only explanation was in `stderr.log`. The
   refusal is a property of the model, like infeasibility. Each task now
   completes with `termination: "unsupported"`, `claim: "none"`, zero nodes
   and iterations, and the native message in `details.message`. Other solver
   errors still fail the job. `describe` lists the termination values.
4. **Native settings were validated in every task, not at validation.** The
   adapter built `NativeOptions` inside each solve. It now builds them once,
   while preparing the job, and validates them there, together with the
   wall-time limit. A setting native would refuse is reported by `validate`.
5. **Hard-coded engine version.** `describe` reported `"0.28.0"` as a
   literal. The worker crate now takes the workspace version, which is the
   version of the linked `lp-ip-craft-native`, so the two cannot drift.
6. **The LP adapter had no Rust tests.** Seven unit tests now cover:
   - the tutorial optimum for MIP and LP, at widths 1 and 8;
   - the conservative bound side;
   - infeasible and unbounded models;
   - the unsupported outcome;
   - the presolve switch and its reported options;
   - MIP starts, and the rejection of LP starts;
   - nine invalid requests and two invalid models, rejected before any
     solve;
   - the description.
7. **Documentation.**
   - `docs/dependencies.md` repeated the API pointer from docs/api.md
     instead of naming the new dependencies. It now lists them: serde,
     serde_json, sha2, libc, fs2, clap and tempfile. They are confined to the
     job crates and do not reach the numerical crates.
   - The shared guide and VALIDATION.md pointed to `validation/job-api/` as
     if it were in the tree. It ships only in the delivery archive.
   - The tutorial now explains that MIP starts are echoed by worker
     validation, whose output is capped at 4 MiB, and how unsupported models
     are reported.
   - `tutorials/job-api/run.py` was the shared template, with branches for
     the fcmaes and SQP workers. It now covers this worker only, checks that
     the binaries exist, honours `CARGO_TARGET_DIR`, and accepts `--bin-dir`.
   - `.gitignore` covers the runner's default node directories and
     `validation/`.

## Shared files to port

The shared crates and the integration script are unchanged. One sentence of
the shared guide `docs/job-api.md` changed. Apply the same edit in
fcmaes-rust and SQP-craft to keep the copies identical. The new SHA-256 is in
`SHARED-SOURCES.sha256`, which the move to optimization-jobs retired.

```text
- source identities are in `validation/job-api/`.
+ source identities are in `validation/job-api/` of each delivery archive; they
+ are not committed to the repositories.
```

## Not changed, for a later version

- Detached jobs use the systemd unit name `fcmaes-<id>.service` for every
  worker. Renaming it belongs in the shared runner, together with fcmaes'
  legacy tests.
- The LP profile takes starts inline. A start file input would lift the
  4 MiB validation cap for large MIPs.
- `systemd` detached execution is still not verified on a service host, as
  the delivery states.
