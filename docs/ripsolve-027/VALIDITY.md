# Validity arguments for native 0.27

## The certificate boundary

Numerical LP solutions, BTRAN multipliers, root heuristics, candidate scores and efficacy calculations propose actions. They do not establish a valid cut. Accepted cuts start from exact equalities or original inequalities and use the existing bounded Rat implementation, which interprets binary64 data exactly. Arithmetic overflow/bit/work limits reject the proposal. The final branch-and-bound solver retains its original incumbent admission, exact witness and outward dual-bound rules.

## Floating coefficient conversion

Suppose an exact rational inequality is `Σ c_j x_j <= b` on the actual root box `[l,u]`. For any selected finite binary64 coefficients q (including q=0 for discarded tiny terms), define

`B = b + Σ max((q_j-c_j) l_j, (q_j-c_j) u_j)`.

Then every point satisfying the exact inequality also satisfies `Σ q_j x_j <= B`. The maximizers are chosen by the exact sign of the coefficient error, and the sum is computed in exact rationals. The emitted RHS is an enclosing binary64 upper value of B. A nonzero error with an infinite required endpoint rejects the cut. Exactly representable coefficients need no compensation. Density/dynamism/efficacy filters can discard a valid cut but cannot make an invalid one valid.

## Binary coefficient tightening

Negate a lower-only row to obtain `a x + r <= b`, where x is binary in [0,1]. Let M be the exact maximum row activity over the box, and let d=M-b. When `0<d<|a|`:

- For a>0 replace a by d and b by M-a. At x=1 the residual constraint remains `r<=b-a`. At x=0 both old and new constraints are automatic because the maximum residual is M-a<b.
- For a<0 replace a by -d and keep b. At x=0 the residual constraint is unchanged. At x=1 both constraints are automatic because the maximum residual is M=b+d.

All other variables may be continuous. The reasoning requires a binary x, not an arbitrary integer column. Ranged/equality rows are skipped. The exact changed row is converted with the preceding box compensation; a skipped or slightly weakened converted row remains safe. Existing outward activity tests prove row redundancy, and existing exact/outward propagation proves bound tightening. Repeated reductions therefore retain every feasible integer point, up to safe relaxation when coefficients are rounded.

Objective-directed empty/singleton fixing has a different contract: move the variable to a finite, objective-preferred bound only when this movement cannot violate any active row side. This preserves some optimum, not all feasible points. Globally valid cuts do not invalidate that argument, because they also hold at the retained optimal integer point. An external lazy-constraint callback with unseen rows would invalidate the argument; native exposes none. A better original-feasible start outside an optimum-preserving reduced box invokes the existing original-model fallback.

## Exact tableau reconstruction

For a numerically chosen basic structural integer column k, obtain advisory multipliers rho from BTRAN and undo power-of-two row scaling. Treat each finite resulting binary64 multiplier as an exact dyadic. Reaggregate the original equations `A x - r = 0` in Rat arithmetic. This is an exact identity for all original feasible points, regardless of BTRAN error.

Normalize by the exact nonzero coefficient of x_k. Keep **every** remaining structural coefficient, including small residuals on other basic columns, and every nonzero logical-row coefficient. Shift/complement these variables on valid finite bounds to nonnegative z. Integer variables have integral anchors; row activities are declared integral only if their original row contains exclusively integral variables with exact integral coefficients. A nonzero free logical term without a finite anchor rejects the derivation.

The result is the exact equation `x_k + Σ a_j z_j = b`, with integer x_k. With f=frac(b), the standard GMI inequality is

`Σ_integer min(frac(a_j)/f, (1-frac(a_j))/(1-f)) z_j + Σ_continuous [a_j/f if a_j>=0, -a_j/(1-f) otherwise] z_j >= 1`.

This follows from the integer split on x_k and the integer part of the integral z terms. Tiny/endpoint f values are skipped for stability. Affine z expressions are substituted back exactly; logical activities become their original structural row expressions. Only then is the cut converted with the box compensation above. Thus a numerical BTRAN residual may weaken or eliminate a candidate, but it is never silently assumed zero in a proof.

## MIR and implied upper bounds

A recognized exact two-term row `a x + c y <= 0`, a>0>c, implies `x <= (-c/a)y`. y must be binary and x continuous; d=-c/a remains rational. Substituting `x=d y-z` introduces a certified nonnegative continuous residual z. Other variables are shifted/complemented on valid bounds. This gives an original inequality over nonnegative integer/continuous terms.

After dividing by positive d and writing f=frac(b/d), MIR maps an integer coefficient a to `floor(a)+max(0,(frac(a)-f)/(1-f))`, a continuous coefficient to `min(0,a)/(1-f)`, and RHS to floor(b/d). This standard mixed-integer rounding inequality holds for every feasible integer assignment and continuous slice. All arithmetic and substitutions remain exact before conversion. Numerical hints only choose anchors/divisors and rank the resulting valid rows.

## Basis reuse, bounds and limits

Appending root rows preserves old combinatorial basis columns/nonbasic states and adds logical basic columns. Row mappings are checked, and the enlarged numerical factorization is rebuilt; a stale LU is never used for the new matrix. The first solve and every resolved root round pass native's existing primal/dual acceptance checks. Checked bounds survive interruption and can tighten the final global bound because reductions preserve the integer optimum and cuts retain all integer feasible points.

All root LP calls and heuristic projections consume parent node/pivot/time budgets. L1-pump objectives and dive statuses never supply a global bound. Every heuristic proposal is checked in original coordinates; the final search applies normal certification before pruning. Bounded traces and pseudocost estimates affect branching choices only. No new optimality termination shortcut is introduced.

## Verification limits

Exhaustive small-model tests validate the implementation, not every future input by enumeration. Production cut validity relies on the exact constructions above and fail-closed arithmetic. The unchanged 6,330-case oracle and 400-model exact Lagrangian audits are additional regression evidence. All optimality/status claims retain native's documented numerical-search scope; this release does not introduce a complete exact search-tree proof log.
