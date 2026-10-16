# Iteration 2 results — conservative dual bounds

Version 0.3.0, 16 October 2026. [Plan](iteration-2-plan.md) ·
[API](api.md) · [Validity argument](safe-dual-bounds.md).

## Baseline and review

Work starts from public `main` at
`cc2aa680240f011f5cd205e13bd34762146669e5`, which merged the reviewed iteration-1
HiGHS backend. The earlier archive was not used as the base. The merged fixes
to LP acceptance, termination reporting, limit validation, status mapping and
Python result types are preserved. Locally reproduced baseline:
**61 Rust tests plus one doctest and 69 compiled Python tests pass**.

The GTOC model and reference target card remain unavailable. The selected next
step comes directly from the reviewed next-task list: a safe bound from
approximate duals before any future pruning implementation uses LP objectives.
This iteration implements that component and its validation, not a benchmark
substitute for the missing application data.

## Delivered

- Dependency-free native Rust `safe_dual_bound`, exported from `lp_ip_craft`.
  It applies residual correction over explicit variable bounds and outward
  binary64 arithmetic to an elementary Lagrangian inequality.
- Typed Rust/Python `SafeDualBound` and `SafeBoundStatus`: a finite original-sense
  bound including offset, or a precise unavailable outcome. Row/variable
  failures include the relevant zero-based index.
- A detached PyO3 binding and frozen, typed Python facade with no native-solver
  requirement. MIP input is handled by continuous relaxation without rounding
  bounds or changing the model.
- A private rounding kernel with subnormal and signed-zero handling, finite
  enclosure checks and runtime probes for flush-to-zero arithmetic. No new
  Rust dependencies, unsafe code or global rounding-mode changes.
- A HiGHS/analytic example, exact rational test oracles, documentation of the
  inequality and arithmetic assumptions, and CI/local-script coverage.

The existing `solve` result, `best_bound`, gap, status and tolerance semantics
are unchanged. In particular, this new calculation does not certify HiGHS's
MIP bound or its search tree. Package versions are bumped to 0.3.0; all external
dependency versions in Cargo.lock are unchanged.

## Measured behavior

For `f(x)=2*x0+3*x1+5`, `x0+x1=1`, `0<=x<=1`, exact optima are 7 for
minimization and 8 for maximization. The example computes an independent bound
from actual HiGHS duals, then uses a controlled perturbation of the analytic
dual to illustrate the unsafe naive calculation.

| Objective sense | Exact optimum | Bound from HiGHS duals | Naive perturbed value | Corrected perturbed bound |
| --- | ---: | ---: | ---: | ---: |
| Minimize | 7 | 6.9999999999999982 | 7.0000000999999994 | 6.9999999999999973 |
| Maximize | 8 | 8.0000000000000018 | 7.9999999000000006 | 8.0000000000000036 |

Both naive values are on the invalid side of the exact optimum. All corrected
values are on the valid side; these checks use no epsilon. HiGHS can choose
different duals for this degenerate model, so the last two columns deliberately
use known analytic multipliers plus/minus `1e-7` for reproducibility.

The tests also check:

| Family | Coverage |
| --- | --- |
| Exact Lagrangian oracle | 1,000 wide-exponent model/sense cases; every finite result must respect Python `Fraction` evaluation of the exact binary64 data; overflow must remain unavailable |
| Exact LP optima | 160 two-variable model/sense cases, rational vertex enumeration |
| Exact integer optima | 60 four-variable model/sense cases, exhaustive feasible integer enumeration |
| Actual HiGHS duals | 40 two-variable LP solves; original and two perturbed dual vectors give 120 exact optimum comparisons |
| Numerical edge cases | Large cancellation, subnormals, signed zero, overflow, fixed/free/one-sided variables, free/empty rows and unavailable sign choices |
| Binding contract | Invalid input, iterable/NumPy inputs, immutable and pickled results, independent thread calls |

The deterministic randomized families are regressions, not formal verification.
No optimization speedup or GTOC acceptance claim follows from them.

## Validation

Local Linux x86_64, Rust 1.99.1, CPython 3.12.14, PyO3 0.30.1,
maturin 1.9.4. Native tests use an installed release wheel in a separate
environment; imports resolve to `site-packages`, not an editable checkout.

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| Workspace Clippy, all targets, warnings denied | Pass |
| Dummy-only binding Clippy, warnings denied | Pass |
| Debug Rust tests (overflow checks enabled) | **70 tests + 1 doctest pass** |
| Rust 1.85 workspace/all-targets compatibility | Pass |
| Rustdoc with warnings denied | Pass |
| Installed native wheel Python suite | **88 pass** |
| Dummy-only installed wheel Python suite | **67 pass, 21 native-only tests skipped** |
| `mypy --strict python/lp_ip_craft` | Pass |
| HiGHS and analytic modes of `examples/safe_bound.py` | Pass |
| Linux x86_64 runtime arithmetic probe | FTZ, DAZ and combined modes each return `unsupported_arithmetic` with no bound; restored normal mode returns a finite bound |
| `git diff --check` | Pass |

The arithmetic-mode probe used a small local C shim to set/restore the thread's
MXCSR register and call the installed Python API. That platform-specific probe
is supplementary; it adds no C dependency to the core checker. CI runs the
portable exact-oracle tests on the existing Linux/macOS/Windows and Python
3.10/3.14 matrix, and the new example in both native and dummy-only jobs.
Those remote CI jobs were not run for these local changes.

Reproduction with a suitable C++/CMake/libclang toolchain and an activated
virtual environment:

```bash
python -m pip install 'maturin>=1.9.4,<2' pytest numpy mypy
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p lp-ip-craft-python --no-default-features --locked -- -D warnings
cargo test --workspace --locked
cargo +1.85.0 check --workspace --all-targets --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
PYO3_BUILD_EXTENSION_MODULE=1 python -m maturin build --release --locked --out dist
# Install the wheel into a separate virtual environment, then run from this repo:
python -m pip install dist/*.whl pytest numpy
python -m pytest -q tests
python examples/safe_bound.py
```

The explicit PyO3 environment variable supports the locally used older maturin;
it emitted a legacy extension-feature warning, but the resulting wheel was
installed and exercised. The native wheel targeted Linux `manylinux_2_38` with
the `cp310-abi3` ABI. The source ZIP includes no platform-specific binary, build
directory or transient validation log. For a dummy-only wheel use a separate
output directory and add `--no-default-features` to the build command. Run the
same test suite; native-backend cases are explicitly skipped.

## Limitations and next input

The bound applies to the exact real model represented by the binary64 data.
It requires ordinary IEEE arithmetic and gradual underflow. A runtime probe is
not hardware verification; fast-math builds and arithmetic-environment changes
during a call are unsupported. Large interval widths, unbounded variables or
overflow can prevent a finite result even for a bounded feasible problem.
Finite bounds can be weak for poor multipliers or loose variable boxes.

The implementation does not validate primal feasibility exactly, supply a
safe pruning API, prove infeasibility, propagate variable bounds, reconstruct
rational duals, certify cuts or introduce a Rust LP/MIP engine. Independent
review of the derivation remains appropriate before integrating pruning.

The next useful input remains a representative GTOC LP/MIP instance, reference
solution/bound and explicit unscaled tolerances. Measure its bound availability,
bound tightness and phase timings before selecting basis reuse, a native
heuristic, exact primal validation or a parallel component.
