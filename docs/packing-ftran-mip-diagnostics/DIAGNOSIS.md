# What the new MIP observations reveal

These are observations of the existing search, not new search algorithms.
Use `data/mip-diagnostics.json` for complete repeated traces, overhead and
existing-policy probes. Decomposition is explicitly off in that study.

## Accepted candidates do not always yield pruning certificates

On egout, the first original-model tolerance-feasible candidate arrives at the
root, around objective 680.35085. The observed search makes 42282 accepted
candidate considerations, finds 13 improving candidates, and attempts the
existing certification LP 13 times. It never obtains a certified incumbent
bound and performs zero incumbent-bound prunes. It eventually solves after
66553 node LPs, including 8523 integral leaves.

Flugpl similarly finds an accepted point (objective 1201500) at node 4449,
but obtains no certified incumbent bound. It performs zero incumbent-bound
prunes and completes 23947 node LPs. These are not trace truncation artifacts:
first milestones and totals are independent of the event cap, and existing
engine counters independently agree. Original-check rejections are zero.

Thus finding more tolerance-feasible candidates alone cannot unlock bound
pruning on these cases. The next investigation should inspect why fixed-integer
continuous certification fails, including equality rows, boundary solutions,
inward shrinking and the difference between a binary64 point and a rigorously
enclosed feasible witness. A rigorous primal-witness enclosure or suitable
exact arithmetic for supported subproblems may help; this is a hypothesis to
test, not an implemented or proven solution. Do not relax acceptance or
pruning thresholds to make a certificate appear.

Bell5 illustrates a different pattern: accepted candidates arrive early (node
6 in the retained observation), but its first certified bound arrives only
at node 7008. It later makes 5090 solved-bound prunes, yet still reaches the
unchanged 100000-node cap. Certification, incumbent quality, root relaxation
strength and branching can therefore contribute differently across models.

## Propagation coverage is a separate issue

The existing exact integer-row node propagator admits 0/98 search rows on
egout and 0/17 on flugpl. Root mixed-domain inference remains active and is
what makes these models admissible; it is not a general mixed-row node
propagator. This is a documented capability boundary, not evidence that a
new propagation rule was added by diagnostics.

Lseu has complete integer propagation coverage (28/28), yet its root LP value
is about 834.68235 (conservative lattice bound 835) versus optimum 1120, and it
requires 19747 nodes. GT2's root bound is 13461 versus optimum 21166; its first
certified candidate is poor (140108), and it needs 72182 nodes. These gaps
support investigating relaxation strength and incumbent quality as well as
branching. Node count alone cannot identify the cause of HiGHS's advantage.

The added models separate the failure modes further. Rgn finds an accepted
candidate matching the numerical reference at node 45, but never obtains an
incumbent certificate. It still hits 100000 nodes. Its reported bound and
tolerance-feasible candidate are very close; this does not permit inventing
an exact primal certificate or silently changing the stopping/pruning contract.
Only 4/24 of its search rows are eligible for exact integer node propagation.

P0548 finds no accepted incumbent within 100000 nodes. Its root LP value is
about 429.68290 (lattice bound 430), versus reference optimum 8691, and its
terminal conservative bound is 4033. Its 174 search rows are all eligible for
the existing exact propagator, so this is not simply missing integer-row
coverage. It is a strong target for feasibility repair and relaxation-strength
experiments. The current restricted covers do not constitute a general
cutting-plane loop.

## How to interpret the existing-policy probes

The trace study probes heuristic-off, presolve-off, optional root strong
branching and optional root covers. They are labelled single-process probes,
not repeated performance comparisons. Read effective options and actual probe/
cut counters: setting strong branching does not bypass its existing eligibility
guard. Presolve-off changes several reductions and may lose finite-bound
admission; it is not an isolated node-propagation ablation.

The evidence makes incumbent certification an explicit next-work priority for
mixed MIPs. Feasibility repair, reliability branching and stronger cuts remain
useful candidates, but their order should depend on these distinct failure
modes. This release deliberately implements none of them.
