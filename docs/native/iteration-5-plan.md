# Native iteration 5 — incumbents and conservative integer pruning

Version 0.9.0. Add bounded deterministic rounding heuristics (nearest,
floor, ceil) at the root and while no conservative incumbent exists, independently checked before becoming an
incumbent. Improve candidates with at most one pass of feasible unit moves,
ranked by objective gain (at most 64 unit moves per candidate), keeping the heuristic cooperative and bounded.

Use objective-lattice tightening only when every variable is integral,
objective coefficients are integral, the offset is zero, and the finite box
keeps every objective sum within the exact binary64 integer range. Tighten an
outward-rounded dual bound upward (minimize) or downward (maximize) to the
integer lattice. Do not use this rule for mixed-continuous models or
noninteger coefficients. Expose opt-outs and counters for differential tests.

Prune against the incumbent only when its original-model constraints are
certified feasible by conservative arithmetic, and use a conservatively
rounded objective upper endpoint in minimization orientation. Numeric-only
incumbents remain reportable but cannot justify conservative bound pruning.
Preserve closed-node bounds when reporting global bounds at limits.

Run all existing suites and the 400-case oracle. Add fractional coefficient,
nonzero-offset, mixed-variable, unsafe rounding and disabled-heuristic tests.
Measure against iteration 4 and rerun all five retained benchmark executables
in alternating order without compilation. Record limitations and regressions.
