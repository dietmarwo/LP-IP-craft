# Native iteration 1 — own the optimization engine

Base: public main a7a04074747a99048810204efe0421b4093c9fb4 plus the delivered
iteration-3 Rust core (unchanged public contracts). Version 0.5.0.

Implement a dependency-free `lp-ip-craft-native` crate. Convert general bound
LPs to nonnegative variables and <= rows; use a dense two-phase primal simplex
with Bland pricing and a deliberately simple, copying reference pivot.
Reconstruct original-sense row multipliers and reduced costs and run the
existing independent primal/dual checks before accepting an optimum.

Add deterministic depth-first branch-and-bound for finite-box MIPs. Rebuild
the LP at each node initially. Keep incumbent, node bounds, cancellation,
whole-call time and pivot budgets separate. Reject unimplemented settings
rather than silently forwarding them. An unbounded LP relaxation is not a
proof that a MIP is unbounded. Use outward-rounded Lagrangian bounds where
available; do not claim exact feasibility or a certified search tree.

Remove the HiGHS crate, C API, build dependencies, Cargo features and Python
dispatch. Expose `NativeOptions` and `backend="native"`; preserve the dummy
default for the existing contract tests. No alias that still calls HiGHS.

Validate analytic LPs in both senses, offsets, ranged rows, free/upper-only
variables, phase-I infeasibility, unboundedness, limits, starts, small binary
enumeration and Python boundary behavior. Record a release-mode native
baseline on fixed LP/MIP fixtures, excluding model construction. Future
iterations compare to this actual implementation, without CI timing gates.

Scope: dense pilot with a checked tableau memory cap; no commercial-solver
performance claim, sparse revised simplex, cuts or parallel node search.
