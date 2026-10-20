# Iteration 2 results — conservative dual bounds

Version 0.3.0, 16 October 2026. [Plan](iteration-2-plan.md) ·
[API](../api.md) · [Validity argument](../safe-dual-bounds.md).

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

## Review of the delivered iteration

The delivery built and passed its 70 Rust tests, doctest and 88 Python tests.
The review checked validity and usefulness separately.

**Validity: no defect found.** I read the derivation and the kernel line by
line against [safe-dual-bounds.md](../safe-dual-bounds.md): the neighbour
enclosures, the exact identities, the corner minimization, the checks for
one-sided boxes and the probes. I then ran an independent rational oracle
on 210,000 random models in both senses. The models mix infinite, one-sided,
fixed and free sides with exact identities, subnormals and values near
overflow. No finite bound was on the wrong side of the exact Lagrangian. In
90,000 of the cases I also checked the reported index. It was always the
first row or variable that blocks a bound.

I also reproduced the flush-to-zero claim with a C helper that sets the
x86-64 MXCSR. FTZ, DAZ and both together return `unsupported_arithmetic`.
Under round-up and round-toward-zero the bounds stay valid, because the
argument needs only faithful rounding. The derivation now says so.

**Availability: the main limitation was understated.** The docs said that
interval width can prevent a bound on free variables. Measured with HiGHS
duals (`examples/bound_availability.py`, 100 random LPs or MIPs per family),
the limitation is much broader:

| Variable boxes | Finite with HiGHS duals | Finite with margin duals |
| --- | ---: | ---: |
| all in [0, 3] | 100 | 100 |
| `x ≥ 0`, no upper bound | 5 | **100** |
| half the columns free | 2 | 4 |
| MIP: integers in [0, 4], continuous `x ≥ 0` | 15 | **100** |

Most of these failures are not caused by arithmetic. The exact Lagrangian
bound of the HiGHS duals is itself infinite in 79 of 95, 98 of 98 and 73 of 85
cases. HiGHS's duals are dual feasible only to its tolerance, so basic columns
on `x ≥ 0` get slightly negative exact residuals. Even the textbook dual can
fail: 0.3 / 0.2 is not 1.5 in binary64, so for
`min 0.3x₀ + 0.7x₁, 0.2x₀ + 0.3x₁ ≥ 2, x ≥ 0` the dual 1.5 has an
exact bound of −∞.

The delivered Rust tutorial test worked around this with upper bounds derived
by hand. The remedy is to ask the solver for duals with a margin. Re-solve the
LP with the cost of each one-sided column shifted by `1e-6 · max(1, |c_j|)` in
the safe direction, then bound the original model with those duals. This
stays valid for any multipliers, gave a bound in every case above, and loosens
it by about `1e-6` relative. It cannot help free columns, which need finite
bounds.

Changes in the review:

- `margin_duals` in `examples/safe_bound.py`, plus a second part of the example
  that shows the stored-decimal case.
- `examples/bound_availability.py`, the measurement above. It checks every
  finite bound against the exact Lagrangian and fails on a wrong one. CI runs
  it in the HiGHS jobs and writes the table to the job summary.
- New tests:
  - 3,000 random models with infinite sides and special values, checking
    validity, the reported index and that every status occurs;
  - the stored-decimal example in Rust and Python;
  - 50 HiGHS LPs with `x ≥ 0` and the tutorial LP, bounded with margin duals
    against exact optima.
- Docs: the availability section and the faithful-rounding assumption in the
  derivation, the availability paragraph in `api.md`, the Rust and Python doc
  comments, the README, and a pruning prerequisite: node LPs must supply
  margin duals.

`safe_dual_bound`, its statuses and the HiGHS adapter are unchanged.

## Validation

Review, local: Linux x86_64, Rust 1.97.0, CPython 3.13, PyO3 0.30.1,
maturin 1.15.0, pytest 9.1.1, NumPy 2.5.4. The pull request's CI repeats the
checks on Linux, macOS and Windows with Python 3.10 and 3.14, the dummy-only
build and the Rust 1.85 check.

| Check | Result |
| --- | --- |
| Delivered archive, `bash scripts/check.sh` | Pass: 70 Rust tests + 1 doctest, 88 Python tests |
| Reviewed, `bash scripts/check.sh` from a clean build (fmt, both Clippy runs, debug tests, examples) | Pass: **71 Rust tests + 1 doctest, 94 Python tests** |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | Pass |
| `mypy --strict python/lp_ip_craft` | Pass |
| Native wheel in a fresh virtual environment (imports from `site-packages`) | 94 pass; all examples run |
| Dummy-only wheel (`--no-default-features`) in a fresh virtual environment | 71 pass, 23 HiGHS tests skipped; `safe_bound.py` runs in analytic mode |
| Independent rational oracle, 210,000 adversarial models | No finite bound on the wrong side; 90,000 index checks consistent |
| MXCSR probe (C helper, x86-64) | FTZ, DAZ, both: `unsupported_arithmetic`; round-up and round-toward-zero: finite and valid |
| `examples/bound_availability.py`, seed 1 | The table in the review section; no unsafe bound |
| `git diff --check` | Pass |

The delivery recorded the same counts with Rust 1.99.1, CPython 3.12.14 and
maturin 1.9.4, for which it set `PYO3_BUILD_EXTENSION_MODULE=1`. With
current maturin, no variable is needed:

```bash
python -m pip install maturin pytest numpy mypy
bash scripts/check.sh                       # in an activated virtual environment
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
python -m maturin build --release --locked --out dist
python -m maturin build --release --locked --no-default-features --out dist-dummy
# Install a wheel into a separate virtual environment, then from this repo:
python -m pytest -q tests
python examples/safe_bound.py
python examples/bound_availability.py --table
```

## Limitations and next input

The bound applies to the exact real model represented by the binary64 data.
It requires ordinary IEEE arithmetic and gradual underflow. A runtime probe is
not hardware verification; fast-math builds and arithmetic-environment changes
during a call are unsupported. Large interval widths, unbounded variables or
overflow can prevent a finite result even for a bounded feasible problem.
With plain solver duals, columns with an infinite bound usually do, as the
review measured. Margin duals fix this for one-sided columns, but not for free
ones. Finite bounds can be weak for poor multipliers or loose variable boxes.

The implementation does not validate primal feasibility exactly, supply a
safe pruning API, prove infeasibility, propagate variable bounds, reconstruct
rational duals, certify cuts or introduce a Rust LP/MIP engine. Independent
review of the derivation remains appropriate before integrating pruning.

The next useful input remains a representative GTOC LP/MIP instance, reference
solution/bound and explicit unscaled tolerances. Measure its bound availability,
bound tightness and phase timings before selecting basis reuse, a native
heuristic, exact primal validation or a parallel component.
