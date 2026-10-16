# Conservative dual bounds: derivation and implementation

Version 0.3.0. Implementation: `crates/core/src/bound.rs`.
Public API: [api.md](api.md). Executable example:
[`examples/safe_bound.py`](../examples/safe_bound.py).

## What the result means

A finite result bounds the objective of **every exactly feasible point** of
the real LP represented by the stored binary64 coefficients and bounds. For
minimization it is a lower bound; for maximization it is an upper bound. The
offset is included. Since a MIP's feasible points are a subset of its continuous
relaxation, the same bound holds for its integer feasible points.

No assertion is made about the decimal data before conversion to binary64,
or feasibility of a returned solver candidate. Ordinary `evaluate` and
`lp_residuals` still use numerical checks and tolerances. This routine is not
a certificate for the solver's search tree, cuts or `mip_dual_bound`.

## Exact-arithmetic inequality

Write the input model as

$$
f(x)=c_0+c^Tx,\qquad L\le Ax\le U,\qquad l\le x\le u.
$$

Let $s=1$ for minimization and $s=-1$ for maximization. Given any finite row
multiplier vector $y$ in the existing original-sense convention, set
$v=sy$, $d=sc$, and $r=d-A^Tv$. For every feasible $x$ the following is an
algebraic identity, regardless of dual feasibility:

$$
s f(x)=s c_0+v^TAx+r^Tx.
$$

Define scalar infima over the explicit model intervals:

$$
q_i=\inf_{a\in[L_i,U_i]}v_i a,\qquad
t_j=\inf_{z\in[l_j,u_j]}r_j z.
$$

Then $sf(x)\ge sc_0+\sum_i q_i+\sum_j t_j$. For a positive multiplier,
$q_i=v_iL_i$; for a negative one, $q_i=v_iU_i$; for a zero multiplier it is
zero even on a free row. A nonzero multiplier selecting an infinite side has
infimum $-\infty$. The API reports an unavailable result in that case; it
does not silently replace the multiplier or assert model unboundedness.

This formula needs no reduced costs: the full residual is recomputed. A dual
vector that would otherwise overestimate the optimum is corrected by the
infimum of its residual over the variable box.

## Incorporating floating-point error

1. Orient coefficients and multipliers by sign changes, which are exact.
2. Initialize each residual interval to the point interval $[d_j,d_j]$.
   Traverse canonical CSR entries. Compute an enclosing interval of $A_{ij}v_i$
   and subtract it outwards from residual $j$. Inductively the final interval
   $R_j=[r_j^-,r_j^+]$ contains the exact real $r_j$.
3. Compute a downward enclosure of each selected row product $q_i$.
4. Compute a downward enclosure of
   $\inf\{az:a\in R_j,\ z\in[l_j,u_j]\}$. This infimum is no larger than
   $t_j$, since the exact residual belongs to $R_j$.
5. Add the offset and these lower endpoints with downward enclosures. Call
   the resulting finite float $B$. Induction over the additions gives
   $B\le sc_0+\sum q_i+\sum t_j\le sf(x)$.
6. Return $B$ for minimization or $-B$ for maximization. Negation is exact, so
   the returned maximization value is at least $f(x)$ for all feasible $x$.

For finite variable bounds the bilinear minimum in step 4 is attained at one
of the four corners of the residual/variable rectangle. Negative and fixed
bounds are therefore handled directly, without an absolute-value shortcut.
For one-sided boxes the dangerous directions are checked first:

- $l_j=-\infty$ and $r_j^+>0$ imply an infimum of $-\infty$;
- $u_j=+\infty$ and $r_j^-<0$ imply an infimum of $-\infty$.

Otherwise the finite endpoint corners suffice. A wholly free variable is
accepted only when the residual enclosure is exactly $[0,0]$, and contributes
zero. No near-zero tolerance can substitute for this test. In particular,
interval overestimation can make a bound unavailable even when the exact
residual happens to be zero. This is a deliberate loss of strength, not validity.

## Arithmetic kernel and assumptions

For a finite rounded addition or product, the immediately preceding and
following representable binary64 values enclose the exact result under
ordinary IEEE arithmetic with gradual underflow. The code obtains adjacent
values through `to_bits`/`from_bits`, retaining the Rust 1.85 minimum. It does
not change a process/thread rounding mode, call an interval library or use
unsafe code. The default supported build uses Rust's standard floating-point
semantics; fast-math/reassociation or a changing arithmetic environment is
outside this contract.

Only elementary exact identities avoid widening: addition with zero, addition
of exact opposites, multiplication by zero or ±1. This permits some exact-zero
residuals and avoids meaningless `0 * inf`. In other cases, even an apparently
exact product is widened. A product rounded to zero is enclosed by the negative
and positive smallest subnormal; underflow is not silently replaced by zero.

If a rounded operation or either outward endpoint becomes nonfinite, the
calculation returns `arithmetic_overflow` without a bound. It does not recover
by clamping or dropping a term. This can reject a calculation whose eventual
exact sum would be finite after cancellation. The implementation prioritizes
a short, auditable argument over aggressive recovery or tightness.

Each invocation probes a normal-to-subnormal product and subnormal addition
and multiplication through `black_box`, so common flush-to-zero and
denormals-are-zero modes are rejected as `unsupported_arithmetic`. These are
environment checks, not formal hardware verification. The environment must
remain unchanged for the duration of the call.

The loop visits each nonzero once, each row once and each variable once:
O(nnz + n + m) time, O(n) temporary residual storage. No solver state, global
rounding changes, tolerances or additional Rust dependencies are involved.

## Why a naive dual value is unsafe

Consider minimizing $2x_0+3x_1+5$ with $x_0+x_1=1$ and $0\le x\le1$.
The exact optimum is 7. A row multiplier just above 2 gives a naive value
$5+y>7$, which is an invalid lower bound. The first residual coefficient
is $2-y<0$, and its box infimum subtracts the excess. Our outward arithmetic
keeps the corrected result at or below 7.

The maximizing variant has optimum 8. A multiplier just below 3 gives an
invalid naive upper bound below 8, while the oriented residual correction
keeps the result at or above 8. `examples/safe_bound.py` obtains multipliers
from HiGHS (or uses analytic ones in a dummy-only build), perturbs them and
checks these inequalities without an epsilon.

## Validation strategy

The Python tests use `fractions.Fraction(float)` to represent **exactly** each
binary64 input. Their independent implementation evaluates the Lagrangian
formula in rational arithmetic; every finite native result must be on its
safe side with no tolerance. Test families include:

- 1,000 models with wide exponent ranges, including subnormals and overflow;
- 160 bounded 2-variable LPs with exact enumeration of boundary intersections;
- 60 small integer models with exhaustive feasible-point enumeration;
- 40 HiGHS LP solves, each checked with its original duals and two perturbations
  against the exact vertex optimum (120 bound checks);
- explicit cancellation, signed-zero, fixed-variable, row-sign and unbounded
  box cases; invalid multipliers and concurrency; frozen/picklable Python results.

These are deterministic regression checks, not a substitute for the validity
argument or external review. The existing 61 Rust tests plus one doctest and
69 Python tests are retained unchanged. Results and reproduced commands are
recorded in [iteration-2-results.md](iteration-2-results.md).

## What must precede rigorous pruning

A conservative dual bound is only one side of a safe pruning comparison.
One also needs an exactly validated feasible incumbent and a conservatively
enclosed incumbent objective, with the correct comparison direction. A point
accepted within `1e-7` can still lie outside the exact feasible set, and an
ordinary objective dot product can round the wrong way. This iteration adds
no pruning API and never derives a certificate from a small numerical gap.

Further scope would require separately reviewed safe variable propagation,
exact primal checks, proof logging or rational reconstruction as appropriate.
Poorly scaled models and very large boxes can yield weak bounds; missing finite
bounds may require propagation that is not implemented here. The actual GTOC
target model is still needed to measure usefulness and choose performance work.

## References

- A. Neumaier and O. Shcherbina, *Safe bounds in linear and mixed-integer linear
  programming*, Mathematical Programming 99 (2004), 283–296.
  [Author preprint](https://arnold-neumaier.at/ms/mip.pdf),
  [publication record](https://ucrisportal.univie.ac.at/en/publications/safe-bounds-in-linear-and-mixed-integer-linear-programming/).
  The general motivation is conservative correction of approximate duals;
  this document derives the specific elementary implementation independently.
- [Rust `f64` documentation](https://doc.rust-lang.org/std/primitive.f64.html),
  binary64 representation, operations and adjacent floats. Standard
  `next_down`/`next_up` methods were stabilized after this project's MSRV.
