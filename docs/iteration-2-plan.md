# Iteration 2 plan — conservative dual bounds in native Rust

16 October 2026. Baseline: public `main` at
`cc2aa680240f011f5cd205e13bd34762146669e5`, the reviewed HiGHS iteration.

Implementation completed. See `docs/iteration-2-results.md` in the source ZIP
for measured outcomes and `docs/safe-dual-bounds.md` for the validity argument.

## Repository review and selection

The current code has a real HiGHS LP/MIP adapter, independent primal/dual
checks, typed Rust/Python contracts, limits, starts, cancellation and native
build CI. The merged review fixed scale-dependent LP acceptance, termination
reporting and unsupported limits. Preserve those fixes and the original-unit
absolute feasibility contract. The earlier delivered archive is not the base.

The next application benchmark in `iteration-1-results.md` cannot be completed
without the GTOC target model and reference data. Do not invent them. The same
review identifies a concrete prerequisite for any future native pruning:
approximate duals must be converted into conservative bounds, including roundoff.
This is a useful dependency-free Rust component that can be tested independently
of solver performance, and is the selected iteration.

## Deliverable and contract

Add `safe_dual_bound(model, row_duals)` to the Rust core and Python wrapper.
It accepts arbitrary finite row multipliers in the existing original-sense
dual convention. No primal solution, solver reduced costs or tolerance is used.
Return a typed `SafeDualBound` with `bound`, `status` and optional `index`:

| Status | Meaning |
| --- | --- |
| `finite` | A finite lower bound for minimization, upper bound for maximization |
| `unbounded_row` | A nonzero multiplier selects an absent row bound; index identifies the row |
| `unbounded_variable` | Residual enclosure and an infinite variable side prevent a finite bound; index identifies the variable |
| `arithmetic_overflow` | No finite intermediate enclosure could be maintained |
| `unsupported_arithmetic` | Runtime checks detect missing gradual underflow |

All non-finite outcomes have `bound=None`. They describe this calculation,
not the feasibility or boundedness of the optimization problem. Invalid lengths
or nonfinite multipliers remain input errors. No clipping of multipliers or
rounding of integer bounds. The exact mathematical model is the stored binary64
data, not a decimal/rational model before conversion to floats.

Expose the function explicitly. Keep `solve`, `best_bound`, gap computation,
statuses and all HiGHS acceptance rules unchanged: the MIP engine's bound is
not certified by this function, and tolerance-feasible incumbents are not exact
primal certificates. An example combines a HiGHS LP solve with independent
bound computation. The checker also works without HiGHS and on MIP models,
where it bounds their continuous relaxation and therefore their integer subset.

## Algorithm and validity obligation

Orient to minimization with `s=+1` for minimization and `s=-1` otherwise.
Let `d=s*c`, `v=s*y`, `r=d-A^T v`. For every feasible point,

```
s*f(x) = s*c0 + v^T A*x + r^T x
       >= s*c0 + sum_i inf_{L_i <= a <= U_i} v_i*a
                + sum_j inf_{l_j <= x <= u_j} r_j*x.
```

Select the appropriate row endpoint by multiplier sign. Enclose each residual
coefficient with sparse outward-rounded arithmetic. Minimize the product of
that interval and the variable interval, including negative, fixed, free and
one-sided boxes. Sum lower endpoints outwards, include the objective offset,
and negate the result for maximization. Document each inequality and the
floating-point assumptions in `safe-dual-bounds.md`.

Use a small private interval implementation: no process-wide rounding-mode
changes, unsafe code, extra Rust dependencies or generic interval framework.
Expand rounded additions/products by adjacent binary64 values. Keep only
obviously exact identities (zero, multiplication by ±1, exact cancellation)
without expansion. Support subnormals and signed zero; any nonfinite enclosure
returns an unavailable bound. Use an MSRV-compatible bit implementation of
adjacent floats. Do not infer exact zero from a tolerance. A loose interval
on a free variable may prevent a bound even when the true residual is zero.
Complexity must remain O(nnz + n + m), with O(n) temporary memory.

## Implementation sequence

1. Reproduce the reviewed Rust and compiled Python baseline in isolation.
2. Write the derivation and implement the private rounding kernel, typed
   outcomes and sparse accumulation in `crates/core/src/bound.rs`.
3. Export the native function through a detached PyO3 call, frozen Python
   result dataclass, status literal and native type stub. Bump packages to 0.3.0.
4. Add an executable example using actual HiGHS duals; retain all existing
   examples and check it in CI/local scripts. Add no native-backend requirement
   to the core checker or its Python tests.
5. Test validity using exact arithmetic, not a floating-point epsilon against
   another approximate optimum. The Python standard-library `Fraction` oracle
   evaluates the formula exactly for the binary64 inputs. Enumerate exact
   vertices of small LPs and feasible integer points to check bounds against
   optima, in both objective senses. Include perturbed solver duals.
6. Run debug Rust tests, rustfmt/Clippy, Rust 1.85, rustdoc, clean-wheel Python
   tests and dummy-only builds. Include randomized/adversarial checks for
   cancellation, underflow, overflow and infinite bounds, and actual HiGHS
   integration regressions. No tolerance weakening or silent skipped failures.
7. Deliver source ZIP, this plan, mathematical/API documentation and measured
   results. Preserve upstream's decision to keep build logs outside the repo.

## Acceptance criteria

- Every finite result is on the safe side of the exact Lagrangian bound in
  the rational oracle; small exact LP/IP optima respect it with no slack.
- Analytic examples return useful tight bounds, in both senses with offsets.
- Nonzero residuals on unbounded sides and unsafe arithmetic never produce a
  finite assertion; malformed duals raise contextual errors.
- The same API works in native and dummy-only wheels, and standard Rust core
  remains dependency-free and forbids unsafe code.
- Existing HiGHS behavior, tolerance policy, limitations and tests remain intact.

## Scope limits and follow-up

This is a documented bound routine, not a formally verified optimizer or a
proof of HiGHS's branch-and-bound tree, cuts or incumbent feasibility. There is
no new branch-and-bound, exact primal validator, safe pruning API, interval
input-model support, rational reconstruction, SCIP adapter, basis reuse or
parallel solver. Bounds may be weak for poor multipliers and unavailable for
unbounded boxes. No GTOC, Gurobi parity or 16-core speed claim is possible yet.
The application target card remains the next input needed to select performance
work; independent review of the bound derivation should precede pruning use.

## References

- A. Neumaier and O. Shcherbina, *Safe bounds in linear and mixed-integer
  linear programming*, Mathematical Programming 99 (2004), 283–296.
  [Author's preprint](https://arnold-neumaier.at/ms/mip.pdf),
  [publication record](https://ucrisportal.univie.ac.at/en/publications/safe-bounds-in-linear-and-mixed-integer-linear-programming/).
  Motivates correction of approximate duals with variable bounds and outward
  arithmetic. This iteration implements the elementary Lagrangian inequality
  above, not the full preprocessing/branch-and-cut framework of that paper.
- [Rust `f64` documentation](https://doc.rust-lang.org/std/primitive.f64.html):
  binary64 operations and adjacent representable values. The local bit helper
  retains compatibility with the declared Rust 1.85 minimum.
