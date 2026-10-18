# Twenty iterations on the open 0.27 issues

Starting point: main `3f928fa` (0.27 with review fixes, PR #7). The open
issues are listed in [../ripsolve-027/REVIEW.md](../ripsolve-027/REVIEW.md):

- combined cuts slower than the better single family;
- rgn slower with cuts than without;
- milliseconds of root work on small models;
- bell5's root target;
- the root incumbent on p0548, and incumbent finding on gt2;
- issue-2290's unsupported domain;
- sp150x300d unsolved;
- defaults unchanged.

## Protocol

Fixed before the first iteration:

- **Evaluation.** `benchmarks/rust-solvers/run.py`, native only, policies
  default, gmi, mir, combined and heuristics, on four sets:
  - the 9 public development cases, 10 s cap;
  - the new synthetic development set (24 models), 5 s;
  - the 11 held-out public MIPs, 3 s;
  - the 10 held-out seeded families, 3 s.
  Every run is a single fresh process, serial, on a 2-core host. The
  summary is the shifted geometric mean (shift 0.01 s, unproved charged the
  cap).
- **Synthetic set.** `benchmarks/rust-solvers/synthetic.py` makes six
  standard classes × 4 seeds: correlated multidimensional knapsack, set
  cover, capacitated facility location (VUB rows), generalized assignment,
  capacitated lot sizing and general-integer covering. A second group of
  24 (`test`, seeds 101–104) is evaluated only at the end. HiGHS 1.15.1
  proved all 48 optimal; those optima are frozen in the inputs, so a wrong
  optimality claim is flagged.
- **Acceptance.** An iteration is accepted only if all of these hold:
  - no solved case is lost;
  - no set's mean gets more than 3% worse for any policy (the noise level
    seen with unchanged code);
  - the targeted measure improves;
  - default results are unchanged, unless the iteration is about defaults.
  Rejected iterations are reverted and recorded here.
- **Correctness.** Unit tests in each iteration. For accepted iterations,
  also the frozen 6,330-case oracle under default and combined settings.
  The full gates run again at the end.

Iteration 0 (main) on this host:

| Set | default | gmi | mir | combined | heuristics |
|---|---|---|---|---|---|
| public | 7/9, 0.522 | 9/9, 0.166 | 7/9, 0.241 | 9/9, 0.178 | 9/9, 0.199 |
| synthetic | 24/24, 0.0449 | 24/24, 0.0399 | 24/24, 0.0320 | 24/24, 0.0469 | 24/24, 0.0527 |
| held-out | 9/11, 0.0334 | 9/11, 0.0361 | 9/11, 0.0353 | 9/11, 0.0380 | 9/11, 0.0432 |
| families | 10/10, 0.0004 | 10/10, 0.0011 | 10/10, 0.0011 | 10/10, 0.0025 | 10/10, 0.0025 |

## Iterations

### 1. In-place exact accumulation for GMI — accepted

- **Plan.** Callgrind on synthetic gap-2 (combined) put 80% of
  instructions in GMI. Almost half was its exact accumulator, whose every
  addition realigned and renormalized heap integers. Sum the aggregation
  and back-substitution products in place instead, as two's-complement
  fixed-point integers of 1536 bits at the smallest exponent involved
  (`dyadic::Wide`). An addend that does not fit makes that sum repeat in
  the existing dyadic code. Both are exact, so cuts must not change.
- **Test.** A new unit test checks in-place sums against dyadic sums on
  random multipliers up to 300 bits and binary64 values over a 1500-bit
  exponent spread, including the fallback.
- **Result.** All 270 records are search-identical to iteration 0 (status,
  objective, nodes, pivots). Means improve by up to 13% (synthetic GMI
  0.0399 → 0.0349 s, public MIR 0.241 → 0.224 s). Examples: gap-2
  combined 0.256 → 0.131 s, lseu GMI 0.422 → 0.274 s. Held-out and family
  means moved within noise (default moved by as much).

### 2. Stop separating a family that stops contributing — accepted

- **Plan.** On facility location and lot sizing, combined selected exactly
  GMI-only's cuts (zero MIR cuts in any round), yet ran MIR separation on
  every row each round. That tripled root time: cfl-1 took 0.025 s,
  against 0.007 s for GMI alone. Rule: a family with no selected cut in a
  round is not separated again; the remaining family gets the whole work
  budget, as it would alone. Combined only; single-family policies cannot
  change.
- **Result.** Combined means improve on all four sets:

  | Set | Before | After |
  |---|---:|---:|
  | public | 0.173 s | 0.163 s |
  | synthetic | 0.0420 s | 0.0375 s |
  | held-out | 0.0392 s | 0.0362 s |
  | families | 0.0026 s | 0.0014 s |

  bell5 drops from 4.40 to 3.58 s and rgn from 0.70 to 0.57 s. Nothing is
  lost or slower beyond the flag threshold, and only combined/heuristics
  records change. Oracle gates: 0 failures, default and combined.

### 3. No heap traffic for inline-size sums — accepted

- **Plan.** After iterations 1–2, gap-2 combined still spent 44% of its
  instructions in the in-place sums, mostly allocating: a vector for each
  addend's limbs and another for each column read back. Use stack arrays
  when the multiplier and the column value fit in 128 bits. Pure
  efficiency, so records must not change.
- **Result.** All records are search-identical. Wall-clock means moved
  within noise (±4%, as did the untouched default), so the deterministic
  instruction counts decide:

  | Case (combined) | Iteration 2 | Iteration 3 | Change |
  |---|---:|---:|---:|
  | gap-2 | 1.288 G | 1.159 G | −10% |
  | p0548 | 2.760 G | 2.641 G | −4% |
  | lseu | 3.462 G | 3.321 G | −4% |

### 4. Also prune tight cuts with a zero root dual — rejected

- **Plan.** Rows with a zero dual at the final root optimum can be removed
  with the root value unchanged, whether slack or tight (degenerate).
  Hypothesis: fewer cut rows speed up node LPs (rgn, gt2, gap).
- **Result.** Large regressions. gt2 GMI-only went from 1,529 to 30,062
  nodes (0.21 → 2.12 s), combined from 6,164 to 60,882 nodes (0.71 →
  3.84 s), and p0548 GMI from 0.60 to 1.29 s. Degenerate cuts at the root
  become binding after branching. Reverted; only strictly slack cuts are
  removed.

### 5. RENS in the root heuristics — accepted

- **Plan.** A gt2 trace with combined cuts shows the incumbent improving
  only through integral leaves: 27206 at node 57, then 25282, 24106,
  23876, and the optimum 21166 at node 3,430. The proof follows at node
  6,154. The root dives (one fixing per LP, at most 8 LPs) never reach an
  integral point; an exploratory run with 200 LPs and least-fractional
  rounding also failed. Add RENS after the cut loop when `root_heuristics`
  is on. It fixes every integer column that is integral at the final root
  optimum and restricts the fractional ones to their two neighbours. The
  resulting sub-MIP is solved by a child solver with a deterministic budget
  of 50 nodes per heuristic LP call (800 by default), and those nodes and
  pivots count against the call's limits. Only its incumbents are used, as
  the search start; its status and bound are discarded.
- **First version.** The heuristics policy improved on public, but got
  worse on synthetic and held-out. issue-2173, for example, went from 0.30
  to 0.53 s: with few integers fixed, the sub-MIP is nearly the whole
  problem. A rerun of the same code moved those means by 6–9%, so this host
  was noisy here.
- **Final version.** Run RENS only when at least half the integer columns
  are fixed, like SCIP's minimum fixing rate.
- **Result.** Heuristics policy:

  | Set | Before | After |
  |---|---:|---:|
  | public | 0.184 s | 0.157 s |
  | synthetic | 0.0448 s | 0.0454 s |
  | held-out | 0.0393 s | 0.0384 s |
  | families | 0.0017 s | 0.0016 s |

  RENS now finds root incumbents. gt2 gets its optimum 21166 at the root
  (0.72 → 0.39 s, 6,181 → 2,756 nodes), and p0548 gets 8698, the root
  incumbent the delivered heuristics never found. lseu gets 1165, rgn its
  optimum. No case is flagged. Other policies are untouched. Oracle gates:
  0 failures.

### 6. Allocation-free gcd for the rational witness (default path) — accepted

- **Plan.** issue-2173 (15 columns, 192 rows) costs 4 ms per node even
  under default. 96% of its instructions are in `witness::upper`, the exact
  rational incumbent witness. Each of its 3 attempts uses up its 8M-operation
  budget and fails. Half of that is allocation inside the long division
  that Euclid's gcd calls at every step. Compute gcd remainders in place
  with reused buffers. The quotient-free remainder shares the division core
  and charges exactly the same work. When the gcd is one, the two divisions
  are skipped but still charged. Every witness outcome, and hence the
  search, must stay identical.
- **Test.** A new unit test checks the in-place remainder and gcd against
  the old division and Euclid recurrence on 5,000 random pairs: same
  values and the same charged work.
- **Result.** All 270 evaluation records and all 6,330 oracle records
  (both policies) are identical to iteration 5. issue-2173 default halves
  its instructions (4.01 G → 2.05 G; 0.244 → 0.207 s), and 2171 drops from
  0.011 to 0.006 s. Held-out means drop 4–6% for every policy, including
  default.

### 7. Divide cuts by their coefficient gcd before conversion — rejected

- **Plan.** On assignment models, combined root time is mostly GMI derived
  through earlier cut rows. Those rows carry full 53-bit binary64
  coefficients, so later derivations work with long integers. Divide each
  exact cut by the gcd of its integer-scaled coefficients: the same
  inequality, often stored exactly as small integers, with only the RHS
  rounded outward.
- **Result.** Chaotic, as any change to the stored cuts is on these
  instances. bell5 lost its proof under gmi, combined and heuristics, and
  gt2 GMI-only went from 0.19 to 5.45 s. Against that, p0548 MIR-only
  became solved (4.3 s) and gt2 combined went from 0.74 to 0.24 s.
  Rejected by rule (lost cases). The tests caught a wrong RHS exponent in
  the first draft.

### 8. issue-2290's unsupported domain — investigated, no change

- **Plan.** Native refuses any MIP with an infinite variable bound, including
  on continuous columns, unless root propagation (`infer_bounds`, 4 sweeps)
  can bound them. issue-2290 has two binaries; all its infinite bounds are
  on continuous columns, four of them free. Hypothesis: longer propagation
  chains need more sweeps.
- **Result.** No admission with 8, 16, 64 or 256 sweeps. Some columns
  cannot be bounded by row reasoning at all. c23, for example, has zero
  cost and appears only in a ≥ row with a positive coefficient, so it is
  unbounded even in the LP relaxation. LP bound tightening cannot certify
  bounds through the free columns either: the safe Lagrangian bound needs
  their reduced costs exactly zero, which numerical duals do not give.
  Supporting this domain needs exact dual certificates (an exact basis
  solve) or optimum-preserving domination bounds. That is a feature, not
  an iteration; no code changed.

### 9. Stop the root loop at the first stagnant round — rejected

- **Plan.** The loop stops after three consecutive rounds each improving
  the bound by under 0.1%. On facility location, set cover and the
  families, those trailing rounds cost GMI about 2× default with little
  tree benefit. (Profiling first showed no search-identical win in the
  tree itself: the LP kernel dominates, the per-node acceptance check is
  linear in nonzeros, and propagation is small.) Stop at the first
  stagnant round.
- **Result.** The late, low-gain rounds matter on hard instances. bell5 is
  lost under gmi, combined and heuristics. With GMI, gt2 goes from 0.19 to
  2.20 s, p0548 from 0.61 to 4.19 s and flugpl from 0.37 to 0.58 s. Small
  cases gain only a few milliseconds. Reverted; patience stays at three.

### 10. Probing on binaries in MIP presolve — accepted

- **Plan.** Target the plan's missed W1 goal, a presolved p0548 root of at
  least 4,500 (it was 3,126). Probing is the gap to ripsolve's presolve the
  0.27 plan noted. For each unfixed binary (at most 2,000 per call, first
  presolve round only), propagate both values with the existing exact
  integer propagation. A value that conflicts cannot occur in any feasible
  point, so the binary is fixed to the other value. Otherwise every feasible
  point lies in one of the two propagated boxes, so each column's bounds
  shrink to the hull of its two intervals. Both reductions keep every
  feasible point. The propagation domain gains a reset that undoes a trial
  propagation. Counters: `diagnostics.mip_presolve_probed`,
  `mip_presolve_probe_fixed`, `mip_presolve_probe_tightened`.
- **Test.** A new exhaustive unit test runs 300 random binary/integer
  models. It checks that every feasible point stays inside the probed
  bounds, and that fixings and tightenings actually occur.
- **Result.** On p0548, probing tightens 4 bounds. Its presolved root rises
  from 3,126 to 4,571, meeting the W1 target. MIR-only now solves p0548
  (time limit → 0.52 s; public MIR 7/9 → 8/9, mean 0.228 → 0.159 s).
  Combined p0548 drops from 0.30 to 0.18 s (504 → 84 nodes). No other
  record's search changed. Some means looked up to 9% slower, but
  instruction counts on five search-identical cases changed by −0.4% to
  +3.5%, so those differences are timing noise. Oracle gates: 0 failures.

### 11. Mixed-row propagation in probing — rejected

- **Plan.** Probing used only the exact integer propagation, so it cannot
  act through rows with continuous columns (bell5, egout, facility
  location, lot sizing). Alternate it with the outward mixed propagation,
  as presolve itself does. The mixed domain gains a reset.
- **Result.** It found reductions only on bell5: 5 fixings and 12
  tightenings. These lift bell5's presolved root from 8,608,418 to
  8,908,552, and the GMI root from 8,931,303 to 8,935,255 (the 8.94M target
  is still not met). But the tree got much worse: bell5 GMI-only needs
  327,797 nodes at the 1M cap, against 52,495 before, and is lost at the
  100k cap under gmi, combined and heuristics. Rejected; probing stays
  exact-only.

### 12. Cap cut density — rejected, but informative

- **Plan.** rgn's nodes cost 102 µs with cuts, against 62 µs without: about
  30 kept cut rows on a 24-row model. Candidate cuts are mostly short
  (median 10–30 terms) with a dense tail (up to 162 terms on rgn and 250
  on p0548). Discard cuts with more than max(20, n/4) terms, like SCIP's
  relative support limit for GMI. A second variant used max(50, n/4).
- **Result.** Strong but uneven:
  - Combined and heuristics improve a lot. Synthetic means drop 37–47%
    (combined 0.0386 → 0.0224 s). Assignment combined drops from 0.13–0.15 s
    to 0.010–0.037 s, so combined is no longer slower than MIR alone there.
    gt2 combined goes from 0.70 to 0.17 s and heuristics from 0.40 to 0.05 s.
  - GMI-only gets worse: p0548 goes from 0.43 to 3.6 s, because its dense
    cuts are essential.
  - bell5 is lost at the 100k cap under all three GMI policies. At the 1M
    cap it needs 139k–161k nodes against 52k, even when the 50-term floor
    keeps nearly all its own cuts: one removed cut is enough to send its
    tree elsewhere.
  Rejected by rule. Density control for combined separation is the most
  promising open direction from these iterations.

### 13. Index GMI multipliers instead of copying them — accepted

- **Plan.** After iteration 12, gap-2 combined still spent about a quarter
  of its instructions in the allocator. The largest source was copying big
  integer multipliers: the back-substitution copied each one once per row
  entry it scales (290,659 copies on gap-2). Each addition also extracted
  the multiplier's limbs again into a fresh vector. Store each multiplier
  once, with its limbs prepared, and reference it by index. Form products
  of large multipliers in a stack buffer. A magnitude that cannot fit is
  refused before the product, exactly as the width check refused it
  before; refused additions charge no work. The sums are unchanged, so the
  search must be identical.
- **Result.** All 270 evaluation records are search-identical to
  iteration 10. Wall-clock means moved within noise. Instruction counts
  (combined):

  | Case | Iteration 10 | Iteration 13 | Change |
  |---|---:|---:|---:|
  | gap-2 | 1.160 G | 0.902 G | −22% |
  | p0548 | 1.453 G | 1.357 G | −7% |
  | lseu | 3.316 G | 2.990 G | −10% |

### 14. Warm-started feasibility pump — accepted

- **Plan.** On small models the heuristics policy cost several times the
  combined policy's root time. setcover-2 spent 911 root pivots against
  64, almost all in the L1 feasibility pump: every round built a new
  projection LP (n + |I| columns, m + 2|I| rows) and solved it from the
  slack basis. Successive rounds differ only in the rounded target, which
  enters the projection's row bounds and auxiliary column bounds, never its
  costs. So build the projection LP once and move the bounds; the dual
  simplex continues from the previous optimal basis, which stays dual
  feasible. The LP gains `set_row_bounds`. Heuristics policy only.
- **Result.** Heuristics policy:

  | Set | Before | After |
  |---|---:|---:|
  | public | 0.155 s | 0.141 s |
  | synthetic | 0.0470 s | 0.0414 s |
  | held-out | 0.0388 s | 0.0358 s |
  | families | 0.0018 s | 0.0015 s |

  Root pivots drop, for example on p0548 from 4,085 to 1,040 (0.25 →
  0.17 s), on gt2 from 2,009 to 671, and on setcover-2 from 911 to 244.
  Searches are unchanged except for ±2 nodes on 10 records (the pump's own
  LP calls), with the same results. Other policies are untouched.

### 15. MIR divisors from VUB slopes — accepted

- **Plan.** MIR-only lot sizing searched as many nodes as default (7,739
  against 7,738 on lotsize-1), and sp150x300d's MIR root stopped after one
  round with no cut. Their balance rows only have coefficients ±1, and MIR
  tried only divisors taken from the row's own coefficients. After a VUB
  substitution x = u y − t, though, the binary carries coefficient u, so
  every integer coefficient is a multiple of the divisor 1 and the fraction
  is zero. The c-MIR procedure of Marchand and Wolsey also divides by the
  coefficients of fractional integer variables in the substituted row.
  Here those are the substituted binaries' coefficients a·u/c. Add up to
  four of them for binaries fractional at the LP point. These divisions
  give flow-cover-type cuts. Rows without a substituted VUB keep exactly
  their old candidates, so models without VUB rows (bell5, gt2, p0548,
  lseu, rgn, flugpl) cannot change.
- **Test.** A new unit test uses a fixed-charge node: demand d fed by two
  arcs x ≤ 10 y. For five demands, it checks that a violated cut is found,
  which needs the slope divisor, and that every grid-feasible point
  satisfies all cuts. The test fails without the change.
- **Result.** MIR-only lot sizing:

  | Case | Before | After |
  |---|---:|---:|
  | lotsize-1 | 7,739 nodes, 0.189 s | 605 nodes, 0.022 s |
  | lotsize-2 | 2,155 nodes, 0.052 s | 172 nodes, 0.009 s |
  | lotsize-3 | 1,190 nodes, 0.044 s | 81 nodes, 0.007 s |
  | lotsize-4 | 1,880 nodes, 0.071 s | 40 nodes, 0.003 s |

  The synthetic MIR mean drops from 0.0335 to 0.0241 s (−28%). Combined
  and heuristics also need fewer nodes on lot sizing and on the big-M
  families. Only VUB models changed: lot sizing, egout, the big-M
  families, and sp150x300d. egout's MIR tree shrinks (163 → 114 nodes),
  but its root costs up to 4 ms more. Public MIR and heuristics means rose
  by 4–5%, mostly timing noise on unchanged records (search-identical GMI
  moved by 6%). sp150x300d's MIR bound improves only slightly, 37.8 → 38.6
  (GMI reaches 53.8; the optimum is 69). Its flow structure needs
  multi-row aggregation, which single-row MIR does not do.

### 16. Sign-extension-aware in-place sums — rejected (too little gain)

- **Plan.** In `Wide`, a carry or borrow that changes a column's sign walks
  all 24 limbs. Track each column's used limbs instead, treat the limbs
  above as the sign extension, and read values of at most 128 bits
  without touching the full width. Search-identical by construction.
- **Result.** Correct (unit tests pass) but small: instruction counts fell
  0.4–4.6% (gap-2 combined −1.2%, p0548 −2.8%, lseu −0.4%, setcover-2
  −4.6%). A line-level profile shows why. Sign flips are rare, and each
  addition costs about 95 instructions of fixed overhead. Most of GMI's
  remaining cost is spread over the exact formula itself: common-exponent
  conversion, Euclidean division and allocation of long integers.
  Reverted, as not worth the added complexity in an exactness-critical
  component.

### 17. Probing without copying the bounds — accepted

- **Plan.** Iteration 10's probing copied every column's bounds for each
  trial: two copies of n values per probe, and up to 2,000 probes. That is
  negligible on the evaluation models, but quadratic-looking in practice
  on large ones. It matters for any later promotion of MIP presolve to the
  default. Run the trials in place and undo them from a copy kept between
  probes, over the columns the trial changed only. The same propagation
  calls on the same values, so the search must be identical.
- **Result.** All 270 evaluation records are search-identical to
  iteration 15. Means moved within noise; one profiling run overlapped the
  evaluation. On seeded set-cover models with 2,000 probes each:

  | Columns | MIP presolve before | After | Presolve policy total before → after |
  |---:|---:|---:|---:|
  | 40,000 | 0.147 s | 0.047 s | 0.31 → 0.19 s |
  | 200,000 | 1.133 s | 0.186 s | 2.09 → 1.05 s (default: 1.10 s) |
