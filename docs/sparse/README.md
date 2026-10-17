# Sparse revised-simplex pilot, version 0.15.0

> **Superseded in 0.16.0.** Both 0.15 kernels were replaced by the bounded
> dual revised simplex described in [../simplex](../simplex/README.md). This
> directory is kept as the record of the pilot; its commands, option names
> and the `sparse_benchmark` example no longer exist.

We can develop sparse numerical infrastructure without access to the real GTOC
application. This release implements and tests the first three milestones in
[plan.md](plan.md). Synthetic tests establish specific numerical behavior and
scaling mechanisms. They do not establish GTOC suitability or commercial-solver
parity. The application target card stays incomplete.

## Implemented

S1: sparse row-pivoted LU, forward/transpose solves, product-form column
updates, bordered-row updates, bounded fill and periodic refactorization.
Explicit basis-matrix residual tests cover row permutations, fill, transpose
orientation, multiple updates and singular/cancelled/resource-limited cases.

S2: sparse bound conversion, phase I/II primal revised simplex, stable primal
ratios with an anti-stall Bland path, basis residual checks/refinement, original
primal/dual postsolve and checked MIP child reoptimization by dual revised
simplex. The child keeps the parent's factorization through a bordered-row
update; no dense tableau or dense basis inverse is allocated. Existing cold
retry rules remain sparse. The dense kernel is an explicit comparison.

S3: deterministic connected benchmark generators, independently known optima,
isolated-process timing and Linux peak RSS, plus warm/cold child comparisons.
Raw measurements and interpretation are in [results.md](results.md).

## Reproduce

```sh
cargo test --workspace --locked
cargo build --release --locked -p lp-ip-craft-native --example sparse_benchmark
python3 scripts/benchmark_sparse.py
# Optional Python package and external oracle (SciPy separately installed):
python -m pip install ".[test]"
python -m pytest -q
python examples/validate_native.py --sparse
python examples/validate_native.py --sparse --seed 91027
```

Python selects the new kernel with `NativeOptions(sparse=True)`. Rust selects
`NativeOptions { sparse: true, ..Default::default() }`. Both use backend
`native`. The default remains dense while the sparse path matures. All prior
core model/checking interfaces remain available. HiGHS is not a dependency.
An optional SciPy LP oracle may use its own solver; that process is separate
from the implementation and is never required to build or use this package.

## Benchmark families

| Family | Mechanism exercised | Independent answer |
| --- | --- | --- |
| Ring packing | Connected sparse LP, active degenerate rows, periodic refactors | Planted x=1 and positive row multipliers give equal primal/dual objectives |
| Irregular packing | Fixed-seed long-range sparsity and LU fill; not separable blocks | Same independently derived primal/dual certificate; generator seed 51921 |
| Network flow | Equality rows, redundant balances, phase I, degeneracy | Consecutive-arc unit flow and node potentials; skip arcs have nonnegative reduced cost |
| Binary selection | Cycle conflicts plus global resource constraint, many related node LPs | Integer dynamic program over first bit, previous bit and capacity |
| Numerical regressions | Beale cycling, nearly parallel rows, row scaling, free/fixed/upper-only/ranged variables | Analytic answers and original-unit residual checks |
| Small LP/IP/MIP oracle | Mixed signs, equality/ranged rows, integrality and safe-bound direction | Independent SciPy LP solutions; exhaustive integer and exact rational mixed-integer vertices |

Packing generation derives the objective from a feasible dual witness; it
does not supply the witness or primal solution to the solver. Network inputs
are built directly as CSR. All performance comparisons disable decomposition
and use one numerical thread so connected sparse improvements are visible.
The binary-selection DP uses integrality to interpret each pair limit 1.5 as
at most one selected endpoint and the resource limit as its integer floor.

All cases used while developing this release are development/regression
information. Different seeds alone do not establish application generalization.
Future GTOC validation should reserve scientific instance variants before
tuning on them.

## Remaining limits and next work

Sparse matrices can acquire dense factors. This pilot uses threshold row
pivoting with a sparse-row preference, natural column order, dense row-sized
work vectors and full sparse-column pricing. It has no global Markowitz/AMD
ordering, hyper-sparse triangular solves, Forrest–Tomlin updates, steepest-edge
pricing or bound-flipping ratio test. Product-form chains and frequent LU
rebuilds can be slower than a small dense tableau.

`max_sparse_entries` limits tracked numerical matrix/factor/update storage and
an overlap estimate during refactorization. It excludes index structures,
work vectors, allocation staging and other pending nodes/components, so it is
not an RSS limit. Peak RSS is measured separately. Component counters sum
per-component storage peaks, which need not occur simultaneously. The sparse
cap and actual memory must both be watched on larger unstructured cases.

MIPs still need explicit finite bounds on every variable, including continuous
auxiliaries. Search remains branch-and-bound with the earlier checked
heuristics; no cutting-plane engine was added. Public basis import/export and
repeated LP objective/RHS-update sessions are future API work; reuse currently
occurs internally between MIP parent and child relaxations. Numerical statuses
are not exact infeasibility or complete search-tree certificates.

Next infrastructure work can continue on fill-reducing ordering, sparse solve
workspaces, pricing and branch-state allocation using these tests. In January,
capture the real model's dimensions, sparsity, bounds, reference solution and
time/memory budget; profile it and adjust priorities from that evidence.
