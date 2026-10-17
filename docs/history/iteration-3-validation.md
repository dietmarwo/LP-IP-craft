# Iteration 3 validation

Version 0.4.0. Base: `a7a04074747a99048810204efe0421b4093c9fb4`.
Local platform: Linux x86_64, CPython 3.12.14, Rust 1.99.1, maturin 1.9.4,
PyO3 0.30.1, HiGHS 1.15.0 / `highs` 2.4.0. Rust 1.85.0 is checked separately.
The source changes do not update any external dependency.

| Gate | Local result |
| --- | --- |
| Reviewed baseline Rust suite | 71 tests + 1 doctest pass |
| Reviewed baseline release wheel, original Python suite | 94 pass |
| New Rust debug suite | 74 tests + 1 doctest pass |
| New release wheel in a fresh virtual environment | 102 Python tests pass |
| Feature-disabled wheel in an isolated import directory | 71 pass, 31 HiGHS-only cases skipped; dispatch rejects the unavailable backend |
| Rust formatting and `git diff --check` | Pass |
| Clippy, native and feature-disabled, warnings denied | Pass |
| Rustdoc, warnings denied | Pass |
| Strict mypy on the Python package | Pass |
| Rust 1.85.0 workspace/all-target check | Pass |
| Both Rust examples and all four Python examples | Pass |
| Bound availability, seed 1, 100 models per family | Reviewed counts preserved; no finite bound fails the exact rational oracle |
| Alternating release-wheel efficiency comparison | 7 runs per version/case; all 56 measured solves accepted; objectives and iteration/node counts match |

New Rust tests exercise tiny supported coefficients in both senses and LP/MIP
modes; explicit zeros, empty/free/ranged rows, fixed/free/unused variables;
objective offsets, input immutability and native nonzero-count mismatch rejection.
New Python cases exercise the small-coefficient fix, the unchanged rejection
threshold, and an all-zero matrix with empty rows. Existing randomized LP/MIP
oracles, conservative-bound exact arithmetic, starts, limits, cancellation,
parallel workers and binding-contract tests remain in the suite.

The small-coefficient regression was run on the reviewed base before changing
the adapter: it returned `Infeasible` for the feasible minimization LP. After
implementation, all four sense/domain combinations return their analytic optimum.

## Reproduction

Install a C++ compiler, CMake and libclang. With Rust >=1.85 and Python >=3.10:

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install 'maturin>=1.9.4,<2' pytest numpy mypy
bash scripts/check.sh
mypy --strict python/lp_ip_craft
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo +1.85.0 check --workspace --all-targets --locked
python examples/benchmark_efficiency.py --repeats 7 > efficiency.json
```

The local validation installed a built release wheel in a fresh environment
instead of the script's editable `maturin develop` step. With maturin 1.9.4,
`PYO3_BUILD_EXTENSION_MODULE=1` was supplied during wheel builds. The dummy-only
wheel was built separately with `--no-default-features` and installed in an
isolated directory, never on top of the normal wheel.

For a baseline comparison, install version 0.3.0 from the base commit in a
separate environment, keep the new version active, and run:

```bash
python examples/benchmark_efficiency.py --repeats 7 \
    --baseline-python /path/to/baseline-venv/bin/python > comparison.json
```

The script uses shell-free child processes, alternating baseline/new order.
Each child constructs the same fixture, warms one solve and measures one solve.
Process launch and Python model construction are excluded from solve wall time;
model construction is recorded separately. The existing backend phase timers
exclude backend input validation and Python result conversion. Full Python
`solve` wall time includes both. Every case checks status, objective and primal
feasibility; LP cases also recompute the independent dual residuals.

CI adds a benchmark acceptance smoke check without a timing threshold. The
existing Linux/macOS/Windows and Python 3.10/3.14 matrix remains configured;
these remote jobs were not run as part of this local task. No GTOC application,
16-core throughput, peak-memory campaign or basis reuse was measured.

Detailed [results](iteration-3-results.md), [raw timings](iteration-3-efficiency-results.json)
and [bound availability](iteration-3-bound-availability.md) accompany this record.
