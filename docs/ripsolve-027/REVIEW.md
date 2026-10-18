# Review of the 0.27.0 delivery

0.27.0 was imported as delivered, from a truncated archive whose source is
nevertheless complete; see [../EVIDENCE.md](../EVIDENCE.md). The delivered
`RESULTS.md` and `TABLES.md` describe the delivered build and are left
unchanged. This page records what the review changed, what it measured and
what it tried and left out. All new controls stay opt-in; the default path
is untouched (see Verification).

## Changes

1. **Exact cut arithmetic was the bottleneck.** Callgrind on p0548 with the
   combined profile put 96% of all instructions in cut separation (MIR 66%,
   GMI 30%) and 2.4% in the tree search. The derivation used gcd-normalized
   rationals, allocated on every operation (each `Rat::zero` allocates) and
   divided by the pivot coefficient first, so numerators and denominators
   multiplied up to the 8192-bit limit; MIR also allocated and scanned a
   dense column vector per candidate. The derivation is now carried out in
   exact dyadics (`dyadic.rs`). Aggregation, anchoring and substitution only
   add and multiply binary64 data. The GMI and MIR functions are applied to
   the equation scaled to integers at a common exponent, where frac(a/p) is
   the remainder (a mod p)/p, and the cut is multiplied through by its
   positive denominators. No gcd is needed. Integers below 2^127 are held
   inline; larger ones fall back to the existing limbs and long division.
   The argument is in [VALIDITY.md](VALIDITY.md). The same equation gives
   the same cut: a test keeps the delivered rational code as a reference and
   checks that both cuts are proportional on random models.

   The derivation also changed in four valid ways:
   - Advisory multipliers are rounded to integers on a 53-bit grid relative
     to the largest. Any multipliers aggregate validly; the grid keeps the
     integers short and drops BTRAN noise on free rows, which used to reject
     the whole candidate.
   - Terms whose variable or row activity is fixed are omitted, since they
     are zero on every feasible point.
   - Each coefficient is rounded to whichever binary64 neighbour needs the
     smaller exact box compensation, instead of always upward. A variable
     with one infinite bound therefore no longer rejects the cut, and
     rounding toward a zero bound costs nothing.
   - VUB rows whose ratio is not dyadic are handled by scaling the base row
     by the least common multiple of the coefficients' odd parts.

   p0548 combined drops from 24.0 to 3.3 billion instructions. The work
   budget (`cut_exact_work`, 4 million charged operations per round) now
   goes much further. The delivered GMI pass spent 0.4 to 1.1 million per
   candidate and stopped after a few per round: 47 candidates on bell5 and
   36 on gt2. It now derives every tableau candidate: 217 and 117. That is
   the main source of the stronger GMI roots below. Coefficient tightening's rational fallback uses the same exact
   path.

2. **Cuts slack at the end of the root stayed in the tree.** All selected
   cuts were kept for the whole search. On rgn, 152 GMI cuts grew the 24-row
   LP to 176 dense rows, and node throughput collapsed. On bell5, combined
   cuts proved nothing within 100,000 or (delivered RESULTS) 1,000,000
   nodes. Cut rows strictly slack at the final accepted
   root optimum are now removed before branching. Their duals are zero
   there, so the root bound is unchanged.

3. **Smaller fixes.**
   - The checked root bound was merged into every child result, so an
     infeasible or unbounded outcome came back with a finite `best_bound`.
   - `diagnostics.branch_trace` wrote non-finite values as `inf`/`NaN`,
     which is not JSON; they are now null.
   - Coefficient tightening rebuilt the whole matrix after each rewritten
     row, which is quadratic on big-M models. It now rebuilds once per
     pass, with the same result.
   - `Lp::tableau_candidates` was inserted between `Lp::solve` and its doc
     comment and took it over.
   - The 14 new `NativeOptions` fields (and two older root separation
     fields) had no doc comments.

## Results

The benchmark machine is a 2-core Xeon. The harness is the delivered one
(`benchmarks/rust-solvers`), run with the delivered protocol. Every run is
a single run in a fresh process, run serially. The comparisons are
delivered build against this branch, both on this host. The shift is
0.01 s, and an unproved case is charged the cap. The baseline is the
delivered 0.26.0 Python package.

**Public, 10 s, 100,000 nodes:**

| Policy | Delivered | This branch |
|---|---|---|
| default | 7/9, 0.514 s | 7/9, 0.519 s |
| presolve | 7/9, 0.304 s | 7/9, 0.299 s |
| gmi | 7/9, 0.560 s | **9/9, 0.161 s** |
| mir | 7/9, 0.298 s | 7/9, 0.236 s |
| combined | 8/9, 0.466 s | **9/9, 0.177 s** |
| heuristics | 8/9, 0.483 s | 9/9, 0.199 s |

ripsolve 0.4.0 proves 8/9 in 0.193 s; HiGHS 1.15.1 proves 9/9 in 0.092 s.

**Public, 60 s, 1,000,000 nodes, 10,000,000 pivots:**

| Solver/policy | Delivered | This branch |
|---|---|---|
| native gmi | 8/9, 0.801 s | 9/9, 0.161 s |
| native combined | 8/9, 0.597 s | 9/9, 0.182 s |
| baseline 0.26 | 8/9, 0.697 s | — |
| ripsolve | 8/9, 0.240 s | — |
| HiGHS | 9/9, 0.094 s | — |

Combined, per case at 10 s (seconds):

| Case | Default | Delivered | This branch |
|---|---:|---:|---:|
| bell5 | node limit | node limit | 4.03 |
| egout | 0.588 | 0.422 | 0.009 |
| flugpl | 0.353 | 0.674 | 0.322 |
| gt2 | 3.791 | 1.598 | 0.751 |
| lseu | 0.788 | 1.527 | 0.539 |
| p0548 | time limit | 2.033 | 0.332 |
| rgn | 0.449 | 0.689 | 0.698 |

Root LP objectives after the cut rounds:

| Case | GMI, delivered | GMI, this branch | Combined, delivered | Combined, this branch | Optimum |
|---|---:|---:|---:|---:|---:|
| bell5 | 8,677,483 | 8,931,303 | 8,680,995 | 8,931,215 | 8,966,406 |
| gt2 | 19,497 | 21,164 | 20,007 | 21,160 | 21,166 |
| p0548 | 6,741 | 8,436 | 8,677 | 8,690 | 8,691 |
| lseu | 1,015.6 | 1,016.3 | 1,043.1 | 1,052.0 | 1,120 |

**Held-out (11 public MIPs) and seeded families (10), 3 s:**

| Policy | Held-out, delivered | Held-out, this branch | Families, delivered | Families, this branch |
|---|---:|---:|---:|---:|
| default | 9/11, 0.0333 s | 9/11, 0.0339 s | 10/10, 0.0004 s | 10/10, 0.0004 s |
| gmi | 9/11, 0.0601 s | 9/11, 0.0354 s | 10/10, 0.0177 s | 10/10, 0.0011 s |
| mir | 9/11, 0.0449 s | 9/11, 0.0349 s | 10/10, 0.0061 s | 10/10, 0.0011 s |
| combined | 9/11, 0.0792 s | 9/11, 0.0375 s | 10/10, 0.0218 s | 10/10, 0.0025 s |

The largest held-out overheads of combined cuts drop from tenths of a
second to milliseconds. For example, knapsack-111 goes from 0.640 s to
0.022 s (default 0.001 s), infeasible-mip0 from 0.191 s to 0.004 s, and 2171
from 0.241 s to 0.018 s (default 0.011 s).

Against the plan's targets, measured with combined or GMI cuts:

- Met: p0548 in under one second, 9/9 development proofs within 60 s, a
  shifted geometric mean at or below ripsolve's, and within 3× of HiGHS
  (1.9× combined, 1.7× GMI at 60 s).
- Still missed: bell5's root of 8.94 million (8.931 million now), no
  regression against the default on every held-out case, and a root
  incumbent on p0548.

## Tried and not adopted

- **MIR from the model's own rows only**, not earlier cuts. This cut
  knapsack-111's 3,487 MIR candidates, but gt2 MIR-only went from 0.26 s to
  5.1 s: rank-2 MIR closes most of its gap. All rows stay.
- **Multipliers rounded to 40 instead of 53 bits.** This keeps more
  arithmetic inline. Combined got faster on p0548, lseu, gt2 and rgn, but
  bell5 fell back to the node limit and GMI-only got slower on all five
  cases tried. 53 bits stay.
- **Root heuristics on the final cut LP**, and dives up to the full LP
  budget. Neither found a better first incumbent on gt2, p0548, lseu or
  bell5; times were neutral to worse. On gt2 the tree's cost is finding the
  incumbent: the GMI root bound rounds up to 21,165 against the optimum
  21,166, yet the first incumbent is 33,406. Better primal heuristics remain
  open; the delivered heuristics are unchanged.

## Verification

- `cargo fmt --check` passes, and so does clippy with `-D warnings`, both
  default and `--all-features`. 178 Rust tests pass (6 new), also with
  `--all-targets --all-features`, and pytest passes 257. The CI oracle
  scripts, benchmark smoke runs and dependency audit pass.
- **Frozen oracle.** All 6,330 cases were rerun under the default and
  combined settings, and the frozen-adjudication audit gives 0 failures.
  The 19 raw pre-existing oracle disagreements are unchanged. Under the
  default settings, every record matches the delivered one in status,
  objective, nodes and pivots. Under the combined settings, statuses all
  match; 423 objectives (within the tolerance), 758 node counts and 1,307
  pivot counts change.
- **400-model bound audit.** 0 unsafe bounds under either policy; rows
  identical to the delivered records.
- **SpOC4.** Both full repairs (8 rounds of 64 rejected seeds, 4 workers)
  give solution vectors byte-identical to the 0.26 package's, with the same
  objectives (32,847.565 and 70,945.093) and bounds.
- **New tests.**
  - Dyadic arithmetic against i128, and the inline/limb boundary.
  - Exhaustive feasible-point checks of the cuts on 399 random mixed models.
    These have fractional coefficients, negative bounds, equality and ranged
    rows, fixed columns and VUB rows with non-dyadic ratios.
  - The differential comparison with the rational reference.
  - Tightening of two rows of opposite sense in one pass.

## Still open

- Combined cuts still lose on rgn (0.45 s default, 0.70 s), and on gt2 and
  lseu combined is slower than either family alone (gt2: 0.75 s against
  0.21 s with GMI only).
- Small held-out models pay a few milliseconds of root work.
- No defaults change. GMI-only has the best development mean (0.161 s), but
  promoting it would be a separate decision on more evidence than these 30
  models.

Follow-up: twenty iterations on these issues, with their measurements, are
in [../mip-iterations/ITERATIONS.md](../mip-iterations/ITERATIONS.md).
