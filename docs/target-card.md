# Application target card (LP/MIP track) — intentionally incomplete

Status: awaiting the GTOC-based integer linear application. Toy examples must
not fill these fields. See Idea 004 concept v3, "Target contracts and
protection against benchmark overfitting".

| Field | Value |
| --- | --- |
| Application source and data revision | TBD |
| Formulation, dimensions `n`, `m`, `nnz`, counts of integer and binary variables | TBD |
| Integer structure (routing/sequencing, assignment, scheduling, other) | TBD |
| Units, coefficient ranges, variable and row scaling | TBD |
| Reference solver, version, settings, thread count and seeds | TBD |
| Saved reference solution with independently recomputed residuals (`evaluate`) | TBD |
| Feasibility and integrality tolerances for the independent check | TBD |
| Required incumbent quality | TBD |
| Whether a proven bound or gap is required, and which gap target | TBD |
| Warm-start sources (domain heuristics, previous solutions) | TBD |
| Time, core and memory budget; determinism requirements | TBD |
| End-to-end versus solver-only timing boundary (model build, presolve, solve) | TBD |
| Development, checkpoint and held-out partitions | TBD |
| Performance variability: seeds per case | TBD |
| Failure rate and adverse cases | TBD |
| Licence terms on publishing comparisons with the reference solver | TBD |
| First experiment hypothesis, time box and stop rule | TBD |

Freeze tolerances and held-out cases before adaptive tuning. Feasibility and
objective quality must be recomputed outside the solver, in unscaled units.
Finding a good incumbent and proving a gap are separate objectives; record
which one the application needs.
