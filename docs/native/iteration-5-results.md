# Native iteration 5 results

Implemented version 0.9.0. Rounding and at most 64 objective-ranked unit
moves run at the root and while no conservative incumbent exists. Candidate
reporting and conservative pruning incumbents are separate; no heuristic
point becomes an incumbent without the ordinary independent check.
Pruning additionally requires conservative original-row feasibility and an
objective upper enclosure. Integral-objective bounds use explicit sufficient
conditions and can be disabled independently.

The mixed-variable oracle caught an early reporting bug: giving a certified
incumbent blanket priority could hide a better tolerance-feasible point.
The final engine keeps both separately, and handles a contradiction between
a tolerance-only candidate and a conservative bound without reporting an
inconsistent optimum. A near-integer unsafe-start regression is included.

An initial mixed-variable comparison also exposed the external solver's
feasibility-tolerance effect: it returned -8.000001 for a model with exact
optimum -8. This was not a native-bound error. The final oracle enumerates
integer assignments and continuous vertices with exact integer/rational
arithmetic instead of weakening the bound assertion.

Final independent validation passes 600 cases: 200 continuous LPs checked
against SciPy, 200 integer models by exhaustive enumeration, and 200 mixed
models by exact rational vertex enumeration. All 63 Rust tests, one doctest
and 98 compiled Python tests pass. Clippy, rustdoc, strict Python typing and
Rust 1.85 compatibility pass. See validation.md for scope and commands.

The final alternating comparison reduces knapsack from 33 to 15 LP nodes
and partition from 55 to 47. Full raw timings and the other iterations are
in performance.md and comparison.json. Final timings supersede the early
iteration-5-benchmarks.json probe. No general application-performance claim.
