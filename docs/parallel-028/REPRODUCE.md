# Reproduce the 0.28 eight-worker series

Base: upstream `a1180e06a85685fa82ca977d521ef33f285f8f51`. Tags `lp-parallel-01`…`lp-parallel-30` preserve all 30 iterations; original 24–27 refs also preserve an explicitly recorded test-staging correction. Those corrections add the exact executed Python tests, changing no production or measured harness bytes. Full source history and an exact incremental patch ship with the archive. The checkpoint history and tags were not pushed; they stay in the archive bundle, and GitHub keeps the commits under pull request #10. The review's corrections are listed in [REVIEW.md](REVIEW.md).

## Build and validate

```bash
python3 -m venv .venv
. .venv/bin/activate
python -m pip install maturin pytest numpy scipy black
cargo fmt --all --check
cargo test --workspace --locked
cargo test --release --locked -p lp-ip-craft -p lp-ip-craft-native --features parallel-bench
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
cargo build --release --locked --examples --features parallel-bench
maturin develop --release --locked
OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 python -m pytest -q
```

Executed environment: Linux x86_64, Rust 1.99.1, CPython 3.12.14. Minimum Rust 1.85 and other platforms were not separately run. On this managed runtime only, forced PyO3 all-target tests need a private linker path to the real versioned libpython because its unversioned system link points at a removed temporary directory. Ordinary extension installation/testing does not need that workaround:

```bash
mkdir -p /tmp/lpip-python-link
# Point at the real versioned library of your Python build.
ln -s /absolute/python/lib/libpython3.12.so.1.0 /tmp/lpip-python-link/libpython3.12.so
RUSTFLAGS='-L native=/tmp/lpip-python-link' LD_LIBRARY_PATH=/absolute/python/lib \
  cargo test --workspace --release --all-targets --all-features --locked
```

## Recover baseline and checkpoints

```bash
git clone history/lp-parallel.bundle /tmp/lp-series
git -C /tmp/lp-series worktree add --detach /tmp/lp-base a1180e0
# Common known-answer harness with unchanged upstream numerical implementation:
git -C /tmp/lp-series worktree add --detach /tmp/lp-common lp-parallel-01
CARGO_TARGET_DIR=/tmp/lp-common-target cargo build --release --locked \
  --manifest-path /tmp/lp-common/Cargo.toml --example performance_parallel
cd /tmp/lp-base
CARGO_TARGET_DIR=/tmp/lp-base-target maturin build --release --locked --out /tmp/lp-base-wheels
python -m pip install --no-deps --target /tmp/lp-base-python /tmp/lp-base-wheels/*.whl
```

Pass the preserved Python package's **parent** as baseline-python. Each worker is a fresh interpreter; never overwrite an extension loaded by a running interpreter. Measured executable/extension hashes identify actual runs; rebuild hashes can differ with compiler/path. Source-only delivery excludes machine-specific binaries. Native default remains numerical width 1, with existing MIP presolve/coefficient tightening on.

## Measure after all builds and stress finish

The matrix driver records all 1/2/4/8 cases, raw samples, hashes, CPU quota/affinity, complete job counts and failures. Child processes run serially and each has a total numerical budget at most 8. Every known-answer child independently checks its original model and reference; failed/limited/nonfinite results cannot become solver speedups. One warmup is excluded; final samples use at least three repetitions, standalone factor and selected confirmations seven.

```bash
python scripts/parallel028.py docs/parallel-028/data/final-matrix.json \
  final=target/release/examples/performance_parallel --full --numeric --repeats 3
python scripts/parallel028_validate.py --baseline-python /tmp/lp-base-python \
  --output docs/parallel-028/data/audit-full
python scripts/parallel028_applications.py --baseline-python /tmp/lp-base-python \
  --output docs/parallel-028/data/applications.json --repeats 3
```

The complete release also saves explicit common-upstream, predeclared held-out, dense-factor and mixed-budget campaigns. `parallel028_matrix.py` reproduces their exact case lists. The public/SpOC4 driver keeps unsupported/errors/limits visible; fixed64 root steps and optional 16 residual tasks permit meaningful progress comparisons. Root-phase time excludes parsing/model construction; whole workflow includes them. Public MIPs have equal explicit time/node/pivot budgets and original-source/reference checks, so limited cases are not successful solver latency measurements.

`factor` uses the delivered dense factor/forward/transpose implementation on three fresh changed matrices per sample, with independently constructed points and a factor-bit checksum; status is `checked`, not LP optimality. Numeric/kernel time, resource setup, workflow and CPU time remain separate. Larger standalone factor gains cannot be presented as whole-solver gains. Checkpoint 21's initial auxiliary factor JSON was incomplete; its exact saved executable received a complete rerun at 25, explicitly recorded with both files.

`--quick` on validation uses a smaller development audit; full mode uses three seeds, three matched policies, unchanged randomized and exact generators, actual-model fingerprints, independent NumPy checks and additional separately planted large LPs. All actual calls/results are journaled, including nonoptimal outcomes. Source/extension hashes, expected call counts and matching mode keys are required. Python tests inject missing records, wrong inputs/objectives/statuses and inconsistent extensions.

## Use the budget

For independent small LP/MIPs: `solve_batch(models, workers=8, options=Options(threads=1))`. For one large numeric/root workload: `solve(..., options=Options(threads=8), ...)`. For existing packing residual parallelism: numerical width 1 plus packing_workers8. Mixed native batches support 1×8/2×4/4×2/8×1; serial-inner outer 16 remains compatible. Compatible live owners share pools and can use less concurrency than the maximum product. Per-job limits apply independently, while batch resource startup belongs to the full batch workflow. No global Rayon pool is changed.

Results, numerical boundaries, all 30 detailed plans/results and archive checksums accompany the package. Original scalar defaults, limits, tolerances and independent acceptance remain in force; experimental losses and public failures are not hidden.

## Build-mode guard

Forced all-target/all-feature tests can overwrite the example alias with a kernel-profile build. Always rebuild `cargo build --release --locked --examples --features parallel-bench` after every correctness/lint/doc gate and before copying/timing the executable. Modern benchmark JSON explicitly reports `kernel_profile`; the driver rejects true, and a real instrumented-binary negative test is retained. Final Rust measurements require false. Baseline checkpoint 01 was built without profiling. Instrumented initial 30 campaigns are retained separately and excluded from all production claims. The nonprofiled Python extension and its application/oracle results were unaffected; extension hashes prove this.

Run `python scripts/parallel028_report.py --plot` after all timing finishes to regenerate tables, CSV, compact metrics and exported PNG/SVG (matplotlib is optional). All candidate vectors in application evidence are retained as SHA-checked little-endian float64 files; repeated storage is deduplicated only after each solve/check completes.

The matrix can be run as one command after building both native executables and the preserved baseline extension:

```bash
python scripts/parallel028_matrix.py --binary target/release/examples/performance_parallel \
  --baseline /tmp/lp-common-target/release/examples/performance_parallel \
  --baseline-python /tmp/lp-base-python --output docs/parallel-028/data
```

`--rust-only` repeats only native campaigns. A separate three-build production isolation in `data/scalar-isolation-production.json` compares upstream, `lp-parallel-30-initial`, and final numerical `lp-parallel-30`; build each without profiling before measurement. `lp-parallel-release-0.28.0` includes the final evidence/guard/documentation additions; numerical implementation is identical to checkpoint 30. JSON journal writes use synced atomic replacement and the matrix driver independently verifies completion/counts after each child exits. An incomplete 127/128 factor journal is retained separately; only its complete fresh rerun enters final tables.

The standalone native comparison lockfile was updated to 0.28 and Rayon. The legacy ripsolve comparison lockfile was also resolved, but offline metadata could not download its uncached `bitflags`; that comparison binary was not built in this series. `benchmarks/reference/harness` still needs its original `native-0.16.0` bootstrap. Neither historical optional harness supplies the new measurements or affects the production workspace.
