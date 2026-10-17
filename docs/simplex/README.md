# Native dual simplex and search, version 0.16.0

Version 0.16.0 replaces both linear kernels of 0.15.0 (the dense tableau and
the opt-in sparse revised-simplex pilot) with one bounded-variable dual
revised simplex on a sparse LU factorization, and replaces depth-first
branch and bound with best-bound search, diving and pseudocost branching.
Nothing is delegated to HiGHS or any other solver; the numerical crates still
have no dependencies. All independent checks of 0.15 are unchanged.

## Review of 0.15.0: where the time went

| Finding | Effect |
|---|---|
| Every finite variable bound was an explicit row (`x ≤ u` as a slack row) | Up to twice the rows; boxed variables could not be handled by bound flips |
| Branching appended a row to a copy of the parent tableau/basis | Node LPs grew with depth; dense children cloned an `O(m·n)` dictionary |
| Dense kernel: `O(m·n)` work per pivot, 8M-entry cap | `dense_guard` or time limit beyond a few thousand rows/columns |
| Sparse pilot: row-pivoted LU without fill-reducing order, per-iteration residual refresh | 2–20× slower than needed on connected models; network-2048 failed numerically |
| Sparse warm children: bordered-row update and a Bland-like dual leaving rule | selection-20: 91 → 1,505 nodes and 359 cold fallbacks (warm slower than cold) |
| Pruning only with interval-certified incumbents, which LP-derived points with continuous values on tight rows never pass | Mixed MIPs pruned nothing; random 25-column MIPs exceeded 100,000 nodes |

## LP kernel (`crates/native/src/lp.rs`, `lu.rs`)

**Internal form.** `Ã x̃ − r̃ = 0` with bounds on structural columns `x̃` and
on one logical `r̃_i` per kept row, whose basis column is `−e_i`. The slack
basis is `−I`. Rows are kept unless presolve proves them free, empty and
feasible, or (with `redundant_rows`) implied by the root box; root-fixed
columns are substituted into the row bounds. Branching only changes bounds.

**Scaling.** Four passes of geometric row/column scaling followed by column
equilibration, rounded to powers of two and skipped when every coefficient
already lies in [1/32, 32]. `Ã = R A S`, `x̃ = S⁻¹ x`, `r̃ = R A x`, costs
`σ c S / C`. Powers of two keep bounds and nonbasic values exact, so every
nonbasic variable sits exactly on an original bound. Primal and dual
tolerances are per variable (`0.1 ×` the original-unit target divided by the
relevant scale, clamped to [1e-12, 1e-7]), so that the independent
original-unit acceptance check holds at a declared optimum.

**Factorization.** Markowitz pivoting with relative threshold 0.1 over
doubly linked row/column count buckets, examining four candidates once one is
acceptable; exact cancellations stay as structural zeros, so row and column
lists never need rescans. When the active submatrix reaches 30% density the
remainder is eliminated densely with partial pivoting. L and U are stored per
pivot and transposed, so FTRAN and BTRAN both push from nonzeros only.
Column replacements are product-form eta updates; the LU is rebuilt after
`refactor_interval` updates or when the entries exceed `max_factor_entries`
(a fresh factorization above the cap is `Unsupported`). Uncovered rows of a
singular basis are repaired by their logical columns.

**Iteration.** Pricing chooses the basic variable maximizing
`infeasibility² / w_r` with dual steepest-edge weights `w_r = ‖e_rᵀB⁻¹‖²`
(updated with `τ = B⁻¹ρ`; refreshed exactly from `ρ` when a row is chosen).
The pivot row `α_r = ρᵀ[Ã −I]` is accumulated row-wise over the nonzeros of
`ρ = B⁻ᵀe_r`. The bound-flipping ratio test passes breakpoints of boxed
variables while the leaving variable stays infeasible by more than its
primal tolerance (one extra FTRAN applies all flips), and Harris' two-pass
rule chooses the largest pivot among near-ties; without a possible flip it
needs no sort. The pivot is cross-checked between row and column; a mismatch
refactorizes. After 25 consecutive zero-step iterations the nonbasic costs
are perturbed (5e-7·(1+|c|), random, sign-preserving) until optimality.

**Phase 1 and classification.** Boxed variables are made dual feasible by
choosing their bound. If a column without a finite box is dual infeasible,
an auxiliary problem with bounds `{0}`, `[0, 1]`, `[−1, 0]` or `[−1, 1]` is
solved by the same dual simplex; its optimum is dual feasible for the real
bounds unless the LP is dual infeasible, in which case a zero-cost solve
decides `unbounded` (primal feasible) or `infeasible`.

**Optimality protocol.** The perturbation is removed, primal and dual values
are recomputed from scratch (cold solves refactorize first) and must be
feasible within tolerance; otherwise iterations continue on a fresh
factorization. Basic logicals get exactly zero price, and a row price whose
sign would select an infinite row bound is reported as zero; duals map back
as `y = σ C R π` and reduced costs are recomputed in original units.

**Infeasibility re-check.** An infeasibility claim (no blocking candidate on
a fresh factorization) stores `ρ`. `infeasibility_verified` recomputes the
Farkas combination `ρᵀ(Ã x̃ − r̃)` from the matrix, independent of the
factorization, and requires its range over the box to exclude zero with a
rounding margin (coefficients below 1e-12 of the largest are ignored).

## Search (`crates/native/src/lib.rs`)

**Node selection.** The open node with the smallest conservative bound is
chosen (deeper, then newer, on ties) whenever a dive ends. A dive always
continues into the child nearer to the LP value, which reuses the factorized
parent basis untouched. Open nodes store their box as an `Arc` chain of bound
changes (`O(1)` per node) and the parent basis as a snapshot while snapshots
stay below 256 MiB; beyond that a queued node starts from whatever basis is
current (dual feasible because MIP columns are boxed). A snapshot within 16
positions of the current basis is reached by eta updates, not a
refactorization. Warm node optima skip the final refactorization; they are
checked independently anyway.

**Branching.** Pseudocosts record the LP objective degradation per unit of
bound change of every solved child; the score is the product of the down and
up estimates (variable average, else the global average). Before any child is
solved the most fractional variable is chosen.

**Certified pruning, made effective.** Pruning still requires an incumbent
whose original rows and objective are certified by outward-rounded
arithmetic; tolerance-only incumbents never prune (see
`tests/test_native.py::test_uncertified_start_cannot_prune`). New: when the
best tolerance-feasible incumbent does not certify, its integers are fixed
and its continuous part is re-solved on a second LP whose finite row bounds
and continuous column bounds are shrunk inward by sixteen primal tolerances.
The interior point usually certifies. At most 64 such solves run per call
(`diagnostics.certification_solves`, `certified_incumbents`).

**Checks and retries.** Warm results are accepted only after the original-
unit LP acceptance (optimum) or the Farkas re-check (infeasibility); either
failure or a numerical failure gets one slack-basis retry. Cold results that
fail acceptance stop the solve with `numerical_failure`, as before.

## Validation

All commands are in the repository; the oracle scripts need SciPy, which is
not a package dependency. Results of this release (Linux x86-64):

| Check | Cases | Result |
|---|---:|---|
| Rust unit, integration and doc tests (`cargo test --workspace`) | 88 (37 native) | pass |
| Python tests (`pytest`) | 155 | pass |
| `examples/validate_native.py` (SciPy LP, exhaustive IP, exact rational MIP), seeds 59103 and 91027, with default pricing, `--dantzig` and `--refactor-interval 1` | 5 × 600 | all pass |
| `examples/validate_random.py`, seeds 1–8 (400 LPs + 150 MIPs each; mixed column/row types, row scales 1e-3…1e3, planted infeasible rows), plus seed 2 with `--refactor-interval 8` and seed 9 with `--dantzig` | 5,500 | all pass |
| `examples/validate_components.py` with 1 and 4 workers | 2 × 180 | all pass |
| Family benchmark known optima (14 cases below, 4 runs each) | 56 | all optimal at the known value |

The randomized oracle found two HiGHS behaviours that would otherwise be
reported as native errors: with presolve, HiGHS reports some feasible,
unbounded LPs as infeasible, and without presolve its costed solve reports
some feasible unbounded LPs as infeasible. The oracle therefore decides
feasibility with a separate zero-cost solve without presolve and maps a
costed "infeasible" on a feasible LP to unbounded. Every disputed case was
also checked by an independently evaluated native zero-cost solve.

## Performance

All timings are medians on one shared Linux x86-64 cloud machine (no CPU
pinning), single thread, release builds; "0.15" is the delivered archive
built from source. Raw records: [benchmarks.json](benchmarks.json) and
[validation.json](validation.json).

### Families with known optima (`scripts/benchmark_families.py`)

Isolated processes, rotated and reversed order, 3 measured repeats after a
warm-up round, 30 s solver limit, no decomposition. Every optimal record
matches the family's independently known optimum.

| Case (n / m) | 0.16 ms | 0.15 sparse ms | 0.15 dense ms | Speedup vs best 0.15 |
|---|---:|---:|---:|---:|
| packing 256 (256 / 256) | 2.80 | 9.53 | numerical failure | 3.4× |
| packing 1024 | 39.8 | 104.3 | time limit | 2.6× |
| packing 4096 | 606 | 2,082 | dense guard | 3.4× |
| network 128 (253 / 128) | 0.71 | 15.5 | 24.6 | 22× |
| network 512 | 7.74 | 232 | 1,758 | 30× |
| network 2048 (4,093 / 2,048) | 124 | numerical failure | dense guard | only 0.16 solves |
| irregular 256 (256 / 256) | 7.66 | 55.3 | 34.6 | 4.5× |
| irregular 1024 | 243 | 7,496 | 2,174 | 8.9× |
| selection 20, warm | 1.23 (93 nodes) | 62.3 (1,505) | 1.14 (91) | 0.9×; 51× vs sparse |
| selection 20, cold | 2.38 (94) | 2.37 (91) | 1.19 (91) | 0.5× |
| selection 32, warm | 4.07 (232) | 321 (5,096) | 4.83 (271) | 1.2× |
| selection 32, cold | 6.03 (275) | 10.3 (281) | 5.96 (281) | 1.0× |
| selection 64, warm | 212 (9,311) | iteration limit | 239 (7,090) | 1.1× |
| selection 64, cold | 413 (7,411) | iteration limit | iteration limit | only 0.16 solves |

The LP families gain 2.6–30× over the best 0.15 kernel and solve the cases
0.15 could not. The selection MIPs have at most 65 rows, where the dense
tableau is cheap; there the new engine is on par with dense 0.15 when warm
(the default) and removes the sparse pilot's warm regression (51–79×).
`cold` disables node reoptimization (every node from the slack basis), an
ablation mode in which tiny bases pay the factorization overhead. On
selection-64 warm, pseudocost branching needs more nodes than most-fractional
(9,311 vs 7,683 in an ablation), while it needs 5–25% fewer nodes on the
randomized MIPs below, so it stays the default.

### Fixed fixtures (`examples/benchmark.rs`, the CI benchmark)

Three alternating runs of 9 repeats each; medians of the run medians.

| Fixture | 0.15 ms | 0.16 ms | 0.15 / 0.16 nodes, iterations |
|---|---:|---:|---|
| fixed (220 columns, 180 fixed) | 0.215 | 0.096 | LP: 40 → 0 pivots |
| pivot (110 / 65, dense) | 1.405 | 0.501 | LP: 68 → 26 |
| knapsack | 0.081 | 0.071 | 15 / 36 → 17 / 17 |
| partition | 0.177 | 0.174 | 47 / 88 → 53 / 40 |
| nodecopy | 1.842 | 1.816 | 143 / 838 → 131 / 325 |
| redundant (row presolve) | 2.223 | 1.229 | LP: 120 → 0 |
| blocks (8 components) | 1.961 | 1.704 | LP: 336 → 195 |

### Randomized models (`examples/validate_random.py`, 400 LPs + 150 MIPs)

The same generated models and oracle for all three builds; native solve time
only. Failures are status or objective disagreements with SciPy.

| Seed | Build | Failures | LP s | MIP s | MIP nodes |
|---|---|---:|---:|---:|---:|
| 1 | 0.15 dense / sparse | 0 / 1 | 0.210 / 0.398 | 0.753 / 3.47 | 49,744 / 63,156 |
| 1 | 0.16 | 0 | 0.142 | 0.056 | 2,461 |
| 2 | 0.15 dense / sparse | 1 / 2 | 0.230 / 0.440 | 1.169 / 4.79 | 78,662 / 85,148 |
| 2 | 0.16 | 0 | 0.149 | 0.063 | 2,886 |
| 3 | 0.15 dense / sparse | 2 / 2 | 0.202 / 0.358 | 1.763 / 4.48 | 98,849 / 80,310 |
| 3 | 0.16 | 0 | 0.143 | 0.065 | 3,360 |

The 0.15 failures are iteration limits and one numerical failure. MIP nodes
fall 20–29× against dense 0.15 and MIP time 13–27×. The search changes
contribute in stages (seeds 1–4, total MIP nodes): plain DFS on the new
kernel 26.9k / 88.5k / 71.1k / 234k (one case at the 100,000-node limit);
with best-bound diving 23.1k / 26.8k / 27.4k / 132k; with pseudocosts
20.4k / 20.5k / 27.4k / 122k; with the certification LP 2.3k / 2.9k / 3.3k /
2.9k (the node-limit case drops from >100,000 to 289 nodes). LP models here
are small (at most 90 columns), so their time is mostly per-call overhead.

## Limits and next steps

- Triangular solves use dense work vectors of the row dimension and loop over
  all pivots; there are no hypersparse solves. Network-like LPs spend most of
  their time in these `O(m)` passes.
- Product-form updates store dense eta columns when `B⁻¹` is dense (ring
  packing, random sparse bases); a Forrest–Tomlin update would keep U sparse.
- Bases with heavy inherent LU fill (the random `irregular` family) are
  solve-bound: irregular-4096 still exceeds the 30 s benchmark limit.
- No cuts, strong branching, presolve beyond fixed columns and redundant
  rows, or parallel tree search. The pseudocost rule has no reliability
  initialization.
- MIPs require finite bounds on all columns. Infeasibility and unboundedness
  are numerical findings; the Farkas re-check is a floating-point check, not
  an exact certificate.
