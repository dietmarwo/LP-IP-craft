# Native iteration 9 — independent component solves

Version 0.13.0. Detect connected variable components through nonzero entries
of restricted rows; free rows and explicit zero entries must not connect
blocks. Solve independent components separately, initially in serial, with
normal native algorithms and no recursive decomposition. Copy sparse entries
only once during splitting. Isolated contradictory rows must stay on the
ordinary solver path; an unbounded component cannot imply global unboundedness
until other components are known feasible.

Reassemble primal values and LP multipliers in original order, recompute
global feasibility/KKT residuals, count the objective offset exactly once, and
aggregate conservative MIP bounds with outward sums. Enforce shared sequential
node, pivot and whole-call time budgets. On incomplete solves retain a valid
original MIP start and avoid presenting a partial vector as a candidate.

Expose decomposition and minimum-size controls; default to at least 32
variables and skip all-singleton partitions to avoid tiny-model overhead.
Test separable LP/IP/MIP cases, offsets/senses, free and zero rows, infeasible
and unbounded combinations, starts and global budgets. Compare against an
unsplit solve and the 600-case independent oracle. Benchmark the 8-block,
512-variable LP with identical objectives and unchanged acceptance thresholds.
