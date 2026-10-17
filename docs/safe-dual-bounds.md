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
zero. No near-zero tolerance can substitute for this test. Interval
overestimation can also make a bound unavailable when the exact residual has
the safe sign or is zero. This is a deliberate loss of strength, not validity.
With solver duals, however, the exact residual itself usually has the wrong
sign somewhere; see [Availability with solver duals](#availability-with-solver-duals).

## Arithmetic kernel and assumptions

For a finite rounded addition or product, the immediately preceding and
following representable binary64 values enclose the exact result under
ordinary IEEE arithmetic with gradual underflow. The argument needs only that
each operation is faithful (error below one unit in the last place), which
every IEEE rounding direction provides; a directed-rounding overflow returns
`±MAX`, whose outer neighbour is infinite and is rejected. A local probe that
set the x86-64 MXCSR from C confirmed valid bounds under round-up and
round-toward-zero (Rust itself treats changing the environment as undefined
behavior, so this is not a supported mode). The code obtains adjacent
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

## Availability with solver duals

A finite bound needs an exact residual of the safe sign on every column with
an infinite side: $r_j\ge0$ when $u_j=\infty$, $r_j\le0$ when $l_j=-\infty$
(oriented to minimization), and $r_j=0$ when the column is free. An LP
solver's duals are dual feasible only to its tolerance (HiGHS: `1e-7`, on its
scaled model), and its basic columns have $r_j\approx0$ of either sign. On a
column without an upper bound, a negative exact residual makes the exact
Lagrangian bound of those duals $-\infty$. No arithmetic can recover that.

The binary64 data alone can cause this. Minimize $0.3x_0+0.7x_1$ subject to
$0.2x_0+0.3x_1\ge2$ and $x\ge0$. The textbook dual is $1.5$, but with the
stored values of 0.3 and 0.2 its exact residual is
$0.3-0.2\cdot1.5=-2.8\cdot10^{-17}$. HiGHS returns $1.4999999999999998$,
whose exact residual of $+1.7\cdot10^{-17}$ is smaller than the enclosure
width. Neither gives a bound.

`examples/bound_availability.py` measures this with HiGHS on random feasible,
bounded LPs (n = 3–24). In the run below there are 100 models per family, with
seed 1. Looseness is the gap to the HiGHS objective, relative to
$\max(1,|\text{optimum}|)$.

| Variable boxes | HiGHS duals: finite | of the rest, exact bound infinite | Median looseness | Margin duals: finite | Median looseness |
| --- | ---: | ---: | ---: | ---: | ---: |
| all in [0, 3] | 100 | – | 2.5e-15 | 100 | 2.5e-15 |
| x ≥ 0, no upper bound | 5 | 79 of 95 | 1.4e-16 | **100** | 1.4e-06 |
| half the columns free | 2 | 98 of 98 | 1.5e-16 | 4 | 1.3e-06 |
| MIP: integers in [0, 4], continuous x ≥ 0 | 15 | 73 of 85 | 7.2e-16 | **100** | 8.2e-02 |

For the MIP family, the looseness is mostly the integrality gap of the LP
relaxation. For most of the HiGHS duals that give no bound, the exact
Lagrangian bound of those duals is itself infinite. The rest fail only because
of the interval width.

The remedy is to ask the solver for duals with some room. Solve the LP again
with the cost of each one-sided column shifted by $\varepsilon\max(1,|c_j|)$
in the safe direction: lower when $u_j=\infty$ and minimizing. Take
$\varepsilon=10^{-6}$, well above the dual tolerance. The duals of the shifted
LP satisfy the original sign conditions with a margin of about $\varepsilon$.
Applied to the *original* model, `safe_dual_bound` is valid with these duals
as with any others. The bound is weaker by about
$\varepsilon\sum_j|c_jx_j|$. `margin_duals` in `examples/safe_bound.py`
implements this, and for a MIP it solves the LP relaxation.

A shift cannot give a free column an exactly zero residual. Free columns need
finite bounds, either from the application or from bound propagation, which
is not implemented. The delivered tutorial test in Rust adds upper bounds
derived by hand for this reason. With margin duals, the Python test bounds the
same LP with `x ≥ 0` as given.

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
  box cases; invalid multipliers and concurrency; frozen/picklable Python results;
- 3,000 models with infinite, one-sided, fixed and free variable and row
  sides, mixing exact identities, subnormals and near-overflow values. Each
  status must occur at least 100 times, and the reported row or variable
  must be the first one that blocks a bound;
- the stored-decimal example above in both senses, in Rust and Python;
- 50 HiGHS LPs with `x ≥ 0` and no upper bounds, plus the tutorial
  production LP, bounded with `margin_duals` against exact optima.

These are deterministic regression checks, not a substitute for the validity
argument or external review. The existing 61 Rust tests plus one doctest and
69 Python tests are retained unchanged. Results and reproduced commands are
recorded in [iteration-2-results.md](iteration-2-results.md).
`examples/bound_availability.py` also checks every finite bound it computes
against the exact Lagrangian of its multipliers.

## What must precede rigorous pruning

A conservative dual bound is only one side of a safe pruning comparison.
One also needs an exactly validated feasible incumbent and a conservatively
enclosed incumbent objective, with the correct comparison direction. A point
accepted within `1e-7` can still lie outside the exact feasible set, and an
ordinary objective dot product can round the wrong way. This iteration adds
no pruning API and never derives a certificate from a small numerical gap.

The node LPs of a pruning search must also supply usable multipliers. Columns
with an infinite side need duals with a margin, as above, or an equivalent dual
correction. Free columns need finite bounds. Otherwise the bound is usually
unavailable on exactly the models where pruning would need it.

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

## Prepared bounds (Rust, 0.21)

`PreparedDualBound::new(model, bounds, row_duals)` owns residual intervals and
the downward-rounded row constant. `set_bound(j, lower, upper)` replaces a
variable interval; `bound()` reads the current report. Rebuild for new model
coefficients or multipliers. The floating-point environment must remain
unchanged throughout its lifetime. This API is currently Rust-only.

Each leaf of its balanced sum tree is the lower enclosure of a variable's
rectangle infimum. Each internal node downward-adds its children's enclosures;
by induction the root is a lower enclosure of the sum of all infima. Adding the
cached row constant downwards preserves the Lagrangian inequality. Sign reversal
then gives the appropriate bound in the original objective sense. Updates
recompute exactly the changed leaf and its ancestors, including failed nodes, so
infinite/overflowing variable terms can recover after a valid domain change.
The immutable row preparation cannot recover from failure without rebuilding.

Preparation is O(nnz+n+m) time and O(n) space; changes are O(log n), reads O(1).
Summation order differs from the fresh API: last bits, availability near overflow
and failure precedence can differ. Both APIs retain conservative rounding.
The fresh API's operation order is unchanged. Certified reduced-cost fixing uses
prepared bounds when two or more domain trials share the same multipliers.
