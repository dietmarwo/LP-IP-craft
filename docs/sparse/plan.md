# Sparse native engine — development before application access

The GTOC formulation is unavailable until application capture. Synthetic cases
establish algorithm correctness, controlled scaling and regressions; they do
not establish GTOC performance or replace the unfilled application target card.

## Milestone S1: sparse basis algebra

Implement sparse row-pivoted LU, forward and transpose solves, sparse product
form column updates, and bordered-row updates for branch constraints. Retain
fill (no numerical dropping), bound stored entries, periodically refactor, and
test residuals against explicitly constructed matrices. No external numerical
dependency and no dense m-by-n or m-by-m allocation.

## Milestone S2: revised simplex

Stream existing bound substitutions into sparse columns. Implement two-phase
primal revised simplex and dual revised simplex reoptimization. Keep the dense
kernel as an explicitly selected comparison, never a hidden sparse fallback.
Accept results through existing original-model primal/dual checks. Exercise
free, upper-only, fixed, ranged, equality, infeasible, unbounded, scaled and
degenerate cases, including independent exact small MIP oracles.

## Milestone S3: scaling and reuse

Use deterministic connected packing and network-flow families, planted primal
and dual certificates, resource-constrained binary selection, ill-conditioned
and cycling fixtures. Record seed, n/m/nnz, pivots, stored matrix/factor/update
entries, wall time and process peak RSS separately. Compare identical models,
single-threaded, with decomposition off. Include dimensions exceeding the
dense guard. Measure warm versus cold MIP child solves; report regressions.
Keep generator controls and fixed validation seeds reproducible. Application
instances and held-out scientific variants are added later without discarding
adverse synthetic fixtures or weakening correctness checks.

## References and boundaries

Huangfu and Hall, *Novel update techniques for the revised simplex method*
(2015), https://www.research.ed.ac.uk/en/publications/novel-update-techniques-for-the-revised-simplex-method/.
This pilot implements the classical product form, not their advanced updates
or a claim of commercial robustness. Sparse fill, dense work vectors, full
pricing and branch-and-bound size remain possible bottlenecks. Finite-box MIP
restrictions remain independent of the sparse LP work.
