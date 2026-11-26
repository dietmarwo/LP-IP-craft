# Review of the LocalMIP pilot delivery

26 November 2026. The delivery is `LP-IP-craft-LocalMIP-review-v2.zip`
(see [../EVIDENCE.md](../EVIDENCE.md)). Its full patch applied unchanged to
`2468f09` and gave the tree the archive records (`64ea895`); that is the
first commit of #52. [RESULTS.md](RESULTS.md) describes the delivered build
and is left as delivered. This page records what the review changed, what it
measured and what it left open.

**The delivered conclusion stands: keep LocalMIP opt-in, keep ordinary
Feasibility Jump.** The review changed no search decision. With the reviewed
build, every untimed output of the 54 public-corpus campaign runs equals the
archived record, apart from the two reporting changes below.

## Changes

1. **Two different stops were both reported as `stalled`.** The search
   returned `stalled` when patience ran out and also when no sampled move was
   admissible (every candidate fixed or tabu). The second is now
   `no_admissible_move`. On the public corpus this relabels the three `p0548`
   runs; see the first open point. The plan asks for the stop reasons to be
   recorded separately.

2. **A caller could not see why the search did not run, or whether its point
   was used.** `diagnostics.root_local_search.skip` now reports
   `ineligible_path` or `global_stop_before_dispatch` when the search was
   requested at a fractional root and not started; before, only the benchmark
   trace had the reason. `improved_incumbent` is 1 only when the point became
   the incumbent. The plan counts a feasible point that is worse than the
   incumbent as feasible but non-improving, and `stop = feasible` did not
   distinguish the two.

3. **The scorer's output depended on the process.** `analyze.py` iterated a
   set of split names, so the key order of the comparisons changed with
   Python's string hash seed. Rescoring the archived campaign gave the
   archived values in another order. The splits are now sorted. A test scores
   one synthetic campaign under four hash seeds and checks a ratio computed
   by hand; it fails on the delivered scorer.

4. **Checks that existed only in the archive now run in CI.**
   - `enumerated_signed_models_keep_their_optimum_and_admit_only_feasible_points`
     ports the archive's `check-small.py`: 96 six-variable signed integer
     models with ranged, upper and lower rows, both senses and an offset. The
     solver must return the enumerated optimum with the search enabled, with
     and without Feasibility Jump ahead of it, and replay under the same
     seed. The search alone, started from the box centre, may return only
     points that pass an exact check. The search runs in 64 of the 96 solves
     and repairs 92 of the 96 centre points. With the row test loosened and
     the original-model check disabled, the test fails.
   - A plateau test: a tight move of score 0 raises the sampled row's weight
     and is taken, the reverse move stays tabu for three moves, and the
     attempt ends with `no_admissible_move`.
   - Zero effort adds no metadata; two numeric threads report the skip;
     component, tree, packing and RENS child settings carry zero effort.
   - A new CI job runs clippy and the native tests under `local-mip-bench`,
     lints and builds the pilot driver with and without its feature, and runs
     `test_pilot.py`. CI did not build the benchmark workspace before; its
     lockfile had gone stale at 0.29.0.

5. **Documents.** The plan, the delivered report, the first edition's log and
   the frozen input manifest are committed here; the pilot README pointed
   `--verify-against` at a file that existed only in the archive.
   `docs/api.md` states the `threads == 1` condition, the stop reasons and
   that the setting is Rust only. `benchmarks/target/` is git-ignored.

## What the review measured

Host: 2 cores, rustc 1.97.0; the delivery used rustc 1.100.0 on an AMD EPYC
9V74.

| Check | Result |
| --- | --- |
| Archive checksums | 289 of 289 match |
| Rescoring `evidence/campaign` and `evidence/repeats` | every value equals `scored.json` and `repeats-scored.json` |
| Revision-2 figures of RESULTS.md against `scored.json` | ratios, intervals, coverage, dispatch counts, stops, memory and solver time agree |
| 24 archived repeats against the campaign | objective, bound, status, nodes and pivots agree |
| Effort zero against a stock driver built here from `2468f09` | 11 of 11 controls agree in status, vector, objective, bound, nodes, pivots, events and all untimed metadata; the same 11 agree with the archive's stock records |
| Public-corpus campaign rerun with the reviewed build (9 models, A and B, 3 seeds) | 54 of 54 agree with the archive in every untimed output, apart from the three relabelled stops and the new field |
| Reference repository | `4e3bf00` is its head; the roster has 233 instances |

The rerun also shows how much of the public comparison is timing. Its
trajectories are the archived ones, yet it scores B/A at 1.0386 with six
models better and three worse, where the archive has 1.0468 with one better
and eight worse. Only `gt2` (1.47 in both) is an effect of the search; the
other eight ratios lie within 0.977 to 1.002 here and move with the host.

Not rerun here: the 120 records of the ten external models. MIPLIB was not
reachable from the review host, so they rest on the archive. The two
controls of the 13 that use external models were not rerun for the same
reason.

## Open points

These would change the measured arms, so they are recorded, not changed.

1. **`p0548` stops one row short of a feasible point.** Its three runs end
   with `no_admissible_move` after 143 to 187 moves and about a quarter of
   the allowance; their best state had one of 22 rows violated. In a scratch build
   that retries an all-tabu step without the tabu, all three seeds reach a
   checked point at the root (33,778, 35,259 and 32,366 after 162 to 204
   moves) at about 0.04 s. Arm A has no root incumbent and finds its only
   point, 26,667, by rounding at node 3,059 after 0.6 s here. The scratch arm
   then keeps its root point to the 5,000-node limit: sooner feasible, worse
   at the end. This is one model, measured after the holdout was opened.

2. **An early certified incumbent ends tree rounding.** The solver rounds at
   fractional nodes only until an incumbent is certified. The revision's
   eight further calls recover `bell5` (node 22) and cannot reach `p0548`'s
   node 3,059; 255 calls in the scratch build did not either. Any root heuristic that certifies a
   poor point early pays this price, so its value cannot be judged apart
   from that rule.

3. **Activity refreshes take most of the allowance on the larger models.**
   The search recomputes all row activities every 32 moves and charges the
   nonzeros. In the additive arm that is 14% of the effort at the median and
   70% on `neos-3024952-loue`, 67% on `beasleyC3` and 56 to 60% on
   `binkar10_1`. On `neos-3024952-loue` the runs end as stalled or out of
   effort after 1,058 to 1,438 moves with 35 to 49 of 146 rows still
   violated. Metered Feasibility Jump pays the same refresh, so the matched
   comparison is fair, but both workers get few moves there.

4. **The held-out comparison is thin.** Of the eight held-out models,
   `cod105` and `b1c1s1` never reach the hook, `enlight_hard` and `timtab1`
   reach it and get no incumbent in any arm, and `binkar10_1` fails
   numerically in every arm. The B/A ratio of 0.9999 is four exact ties, two
   small losses and one small gain.

5. **Python.** The Python options do not expose the setting.
