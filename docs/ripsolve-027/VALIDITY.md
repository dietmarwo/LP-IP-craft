# Validity arguments for native 0.27

## The certificate boundary

Numerical LP solutions, BTRAN multipliers, root heuristics, candidate scores and efficacy calculations propose actions. They do not establish a valid cut. Accepted cuts start from exact equalities or original inequalities and are derived in exact dyadic arithmetic (`dyadic.rs`, since the review; 0.27.0 used the bounded Rat implementation), which interprets binary64 data exactly. Arithmetic overflow/bit/work limits reject the proposal. The final branch-and-bound solver retains its original incumbent admission, exact witness and outward dual-bound rules.

## Exact arithmetic without division (review change)

Every binary64 value is a dyadic rational m·2^e. Sums and products of dyadics are dyadic, so aggregation, anchoring, substitution and the box compensation below are exact without any gcd. The only nondyadic step is the rounding function itself. For the equation (or inequality) at hand, all coefficients, the right-hand side and the pivot coefficient (GMI) or divisor (MIR) are scaled by the same power of two to integers. Then frac(a/p) = (a mod p)/p with the Euclidean remainder, and multiplying the GMI or MIR inequality by its positive common denominator (f0·(p-f0) for GMI, p-f0 for MIR, with f0 = b mod p) gives integer coefficients:

- GMI, `Σ g z >= f0 (p-f0)`: integer z with r = a mod p gets `r (p-f0)` if r <= f0, else `(p-r) f0`; continuous z gets `a (p-f0)` for a > 0, else `-a f0`.
- MIR, `Σ h z <= floor(b/p) (p-f0)`: integer z gets `floor(a/p) (p-f0) + max(0, r-f0)`; continuous z gets a for a < 0 and 0 otherwise.

These are the standard inequalities of the next sections multiplied by a positive number. Integers below 2^127 are held inline; larger ones use the existing limb arithmetic and long division, with the same 8192-bit limit and work counter. The tests keep the 0.27.0 rational derivation as a reference and check that both give proportional cuts.

Three further changes are all justified by the arguments below. Multipliers are rounded to integers on a grid relative to the largest one before aggregation: any multipliers give an exact identity, so this changes only the residuals. A term whose variable or row activity is fixed over the box (z = 0 on every feasible point) is left out of the cut, which leaves every feasible point's left-hand side unchanged. For a VUB coefficient c whose odd part does not divide the row coefficient, the base row is first multiplied by the least common multiple of those odd parts, which keeps a/c dyadic; MIR of a positively scaled row with the equally scaled divisor is the same cut.

## Floating coefficient conversion

Suppose an exact rational inequality is `Σ c_j x_j <= b` on the actual root box `[l,u]`. For any selected finite binary64 coefficients q (including q=0 for discarded tiny terms), define

`B = b + Σ max((q_j-c_j) l_j, (q_j-c_j) u_j)`.

Then every point satisfying the exact inequality also satisfies `Σ q_j x_j <= B`. The maximizers are chosen by the exact sign of the coefficient error, and the sum is computed exactly. The emitted RHS is an enclosing binary64 upper value of B. Since the review, the row is first scaled by a power of two so that its largest coefficient lies in [1,2), and each q is the binary64 neighbour below or above c (or zero for |c| < 2^-37) with the smaller exact compensation among those whose required endpoint is finite. A coefficient with no admissible choice rejects the cut. Exactly representable coefficients need no compensation. Density/dynamism/efficacy filters can discard a valid cut but cannot make an invalid one valid.

## Binary coefficient tightening

Negate a lower-only row to obtain `a x + r <= b`, where x is binary in [0,1]. Let M be the exact maximum row activity over the box, and let d=M-b. When `0<d<|a|`:

- For a>0 replace a by d and b by M-a. At x=1 the residual constraint remains `r<=b-a`. At x=0 both old and new constraints are automatic because the maximum residual is M-a<b.
- For a<0 replace a by -d and keep b. At x=0 the residual constraint is unchanged. At x=1 both constraints are automatic because the maximum residual is M=b+d.

All other variables may be continuous. The reasoning requires a binary x, not an arbitrary integer column. Ranged/equality rows are skipped. The exact changed row is converted with the preceding box compensation; a skipped or slightly weakened converted row remains safe. Existing outward activity tests prove row redundancy, and existing exact/outward propagation proves bound tightening. Repeated reductions therefore retain every feasible integer point, up to safe relaxation when coefficients are rounded.

Objective-directed empty/singleton fixing has a different contract: move the variable to a finite, objective-preferred bound only when this movement cannot violate any active row side. This preserves some optimum, not all feasible points. Globally valid cuts do not invalidate that argument, because they also hold at the retained optimal integer point. An external lazy-constraint callback with unseen rows would invalidate the argument; native exposes none. A better original-feasible start outside an optimum-preserving reduced box invokes the existing original-model fallback.

## Probing (iterations)

For a binary x and the box B after presolve's propagation, every feasible point lies in B ∩ {x = 0} or B ∩ {x = 1}. The exact integer propagation is applied to each. It only removes values that no feasible point of that half can take (exact i128 row activities over integral data). If one half becomes infeasible, every feasible point has the other value, so x is fixed to it. Otherwise each feasible point lies in one of the two propagated boxes, and every column's bounds may shrink to the hull of its two intervals. Both reductions keep every feasible point, so the reduction preserves the feasible set, unlike dual fixing. Trials run on the bounds in place and are undone exactly over the columns they changed. An exhaustive unit test checks the invariant on 300 random binary and integer models.

## Exact tableau reconstruction

For a numerically chosen basic structural integer column k, obtain advisory multipliers rho from BTRAN and undo power-of-two row scaling. Treat each finite resulting binary64 multiplier as an exact dyadic. Reaggregate the original equations `A x - r = 0` in Rat arithmetic. This is an exact identity for all original feasible points, regardless of BTRAN error.

Normalize by the exact nonzero coefficient of x_k. Keep **every** remaining structural coefficient, including small residuals on other basic columns, and every nonzero logical-row coefficient. Shift/complement these variables on valid finite bounds to nonnegative z. Integer variables have integral anchors; row activities are declared integral only if their original row contains exclusively integral variables with exact integral coefficients. A nonzero free logical term without a finite anchor rejects the derivation.

The result is the exact equation `x_k + Σ a_j z_j = b`, with integer x_k. With f=frac(b), the standard GMI inequality is

`Σ_integer min(frac(a_j)/f, (1-frac(a_j))/(1-f)) z_j + Σ_continuous [a_j/f if a_j>=0, -a_j/(1-f) otherwise] z_j >= 1`.

This follows from the integer split on x_k and the integer part of the integral z terms. Tiny/endpoint f values are skipped for stability. Affine z expressions are substituted back exactly; logical activities become their original structural row expressions. Only then is the cut converted with the box compensation above. Thus a numerical BTRAN residual may weaken or eliminate a candidate, but it is never silently assumed zero in a proof.

## MIR and implied upper bounds

A recognized exact two-term row `a x + c y <= 0`, a>0>c, implies `x <= (-c/a)y`. y must be binary and x continuous; d=-c/a remains rational. Substituting `x=d y-z` introduces a certified nonnegative continuous residual z. Other variables are shifted/complemented on valid bounds. This gives an original inequality over nonnegative integer/continuous terms.

After dividing by positive d and writing f=frac(b/d), MIR maps an integer coefficient a to `floor(a)+max(0,(frac(a)-f)/(1-f))`, a continuous coefficient to `min(0,a)/(1-f)`, and RHS to floor(b/d). This standard mixed-integer rounding inequality holds for every feasible integer assignment and continuous slice. All arithmetic and substitutions remain exact before conversion. Numerical hints only choose anchors/divisors and rank the resulting valid rows.

Any positive divisor gives a valid MIR inequality; the divisor only affects strength. Since the iterations, divisors also include the coefficient a·u/c that a substituted VUB gives its binary, for binaries fractional at the LP point. They are chosen numerically and enter the derivation as exact binary64 values like the row's own coefficients.

## Selection and root heuristics (iterations)

Two later rules only choose among valid rows. A family with no selected cut in a round is not separated again. While both families are active after the first round, a cut with more than max(50, n/4) terms is not selected. Discarding a valid cut never invalidates anything.

The warm-started feasibility pump and RENS only propose points. A pump projection LP solution is rounded and checked in original coordinates like before; its objective is never a bound. RENS solves a restriction of the presolved model with its root cuts: the root box with some integer columns fixed and the others narrowed. Its incumbent is a candidate. Like every other root heuristic proposal, it is checked against the presolved model before it becomes the search's start, and the final solution is checked in original coordinates as before. A restriction's status and bound say nothing about the full model, so both are discarded. The child solver's nodes and pivots are charged to the parent's limits.

## Basis reuse, bounds and limits

Appending root rows preserves old combinatorial basis columns/nonbasic states and adds logical basic columns. Since the review, cut rows that are strictly slack at the final accepted root optimum are removed before the search. Such a row has a zero dual there, so that primal/dual pair remains optimal for the smaller LP and the recorded root bound is unchanged; removing valid inequalities removes no feasible point. Row mappings are checked, and the enlarged numerical factorization is rebuilt; a stale LU is never used for the new matrix. The first solve and every resolved root round pass native's existing primal/dual acceptance checks. Checked bounds survive interruption and can tighten the final global bound because reductions preserve the integer optimum and cuts retain all integer feasible points.

All root LP calls and heuristic projections consume parent node/pivot/time budgets. L1-pump objectives and dive statuses never supply a global bound. Every heuristic proposal is checked in original coordinates; the final search applies normal certification before pruning. Bounded traces and pseudocost estimates affect branching choices only. No new optimality termination shortcut is introduced.

## Verification limits

Exhaustive small-model tests validate the implementation, not every future input by enumeration. Production cut validity relies on the exact constructions above and fail-closed arithmetic. The unchanged 6,330-case oracle and 400-model exact Lagrangian audits are additional regression evidence. All optimality/status claims retain native's documented numerical-search scope; this release does not introduce a complete exact search-tree proof log.
