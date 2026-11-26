# LP-IP-craft LocalMIP and FPR review

26 November 2026 · Revision 2 · Native Rust experimental patch

> This is the delivered report of the pilot (`REVIEW.md` in the delivery
> archive), kept as delivered apart from this note and the two links in the
> last paragraph. It describes the delivered build. [REVIEW.md](REVIEW.md)
> records what the review of that delivery changed and measured,
> [PLAN.md](PLAN.md) is the plan, [IMPLEMENTATION-V1.md](IMPLEMENTATION-V1.md)
> the first edition's log, and [../EVIDENCE.md](../EVIDENCE.md) names the
> archive with the raw records.

**Keep LocalMIP opt-in and retain ordinary Feasibility Jump.** The revision fixes a reproducible loss on `bell5`. Across seven complete held-out models, adding LocalMIP yielded a primal-integral ratio of 0.9999 and replacing FJ yielded 1.0861. All four arms found a checked incumbent on the same 12 of 24 held-out runs by 30 seconds. The original plan set an advancement threshold of 0.95 or repeatable additional coverage on two held-out models.

## What changed

| Review finding | Revision and evidence |
| --- | --- |
| An early LocalMIP incumbent switched off tree rounding | When a metered root worker supplies the first checked point, at most eight later fractional-node rounding calls remain available. The original rounding option and global budgets still govern these calls. The allowance ends on successful rounding or an accepted integral leaf. Both matched workers get the same rule. On `bell5` seed 29 the root kept its new point and rounding again found the better point at node 22. |
| Matched FJ tested progress at the start of the next iteration | The metered worker checks every completed move and restart, including the last. A regression test ends a one-move search on the exact effort cap and tests one visit below it. Both workers report observed initial and best violation counts. An unobserved count is omitted. FJ also reports its own scoring ceiling and actual proposal count. |
| The experiment's seed was available only to the observer | Rust callers can now set `NativeOptions::local_mip_seed`, default 0. The benchmark passes the seed to the normal solver settings. Plain and instrumented calls with equal seeds agree on the returned vector, objective, bound, status, nodes and pivots. The two tested seeds produced different root incumbents on `gt2`. |
| The runner could silently reuse a directory for a different selection or score a partial campaign | Each run directory now records its exact expected observations and locks writes. Resuming requires the same configuration. The scorer rejects missing, extra and mislabeled files. Plain mode rejects the matched policies, which require the observed root hook. The LP smoke control now emits its checked point and the scorer handles the infeasible control's missing reference. |
| MPS verification could accept a bound with the wrong infinity sign | The independent import comparison now checks the side of an unbounded interval. A reproduction can require the original hashes, conversions, references, exclusions and ordered roster with `prepare.py --verify-against`. Rebuilding from the cached public MPS files reproduced the frozen input identities. |
| The FPR diagnostic could accept a fractionally fixed integer or fail to serialize a successful rational result | The replay rounds integer domains inward, rejects empty ones, verifies any completed point against the original column domains and rows, and writes exact rational values as strings. Tests cover an integer box with one valid value, an empty integer box and an objective of 7/2. |

Eight follow-up rounding attempts is an engineering choice introduced after inspecting the first pilot. It adds bounded tree work to the opt-in policy. The revised measurements reuse the original cases and seeds. They support an engineering replay on this corpus. An independent generalization claim would require fresh cases.

## Paired results

A uses the existing root stack with ordinary FJ. B adds LocalMIP after FJ. C and D select metered FJ and LocalMIP respectively at the main fractional root. RENS children keep their ordinary FJ configuration. The common input roster contains nine existing public MIPs and ten mipfeas models, two development and eight held out. A/B ran everywhere. C/D ran on the external models. Each pair used the same assigned CPU, deterministic budgets and seed.

| Cohort and comparison | Complete cases / pairs | First edition | Revision 2 | Revision 2 case-bootstrap 95% interval | Revision 2 including failed solves |
| --- | ---: | ---: | ---: | ---: | ---: |
| Held-out B/A | 7 / 21 | 0.9972 | **0.9999** | 0.9940–1.0053 | 0.9998 |
| Held-out D/C | 7 / 21 | 1.0878 | **1.0861** | 0.9991–1.2818 | 1.0749 |
| Public B/A | 9 / 27 | 1.0865 | **1.0468** | 0.9980–1.1427 | 1.0468 |
| Development B/A | 2 / 6 | 0.9916 | 0.9563 | 0.9144–1.0000 | 0.9563 |
| Development D/C | 2 / 6 | 1.1341 | 0.9861 | 0.9725–1.0000 | 0.9861 |

The metric integrates the original-model checked incumbent's normalized reference gap over 30 seconds. Time without a point costs 2. Ratios below 1 favor the experimental arm. Paired log differences use shift 0.001, average seeds within each case and weight cases equally. Intervals resample whole cases. The last column charges unobserved time after a failed solve at 2. Timing-heavy ratios vary between campaigns, especially on small instances. The independent unit for the held-out interval is the model, giving seven complete observations.

| Held-out arm | Checked at root | Checked by 30 s | Certified by 30 s | Median capped time to first checked point |
| --- | ---: | ---: | ---: | ---: |
| A, existing root heuristics | 9/24 | 12/24 | 9/24 | 16.879 s |
| B, additive LocalMIP | 9/24 | 12/24 | 9/24 | 16.844 s |
| C, metered FJ | 9/24 | 12/24 | 9/24 | 16.729 s |
| D, metered LocalMIP | 9/24 | 12/24 | 9/24 | 16.767 s |

All 174 solver processes returned. The 126 returned vectors passed the independent original-array check. There were zero watchdog kills, deterministic budget overruns or LocalMIP admission failures. The solver returned numerical failure on all 12 `binkar10_1` runs again. Their checked incumbents contribute to the coverage table and worst-tail sensitivity. All 99 comparable event prefixes matched through the shared root hook.

| Arm over its full scheduled cohort | Solves | Metered dispatches | Median stage wall time | Median whole-solve nodes / LP pivots |
| --- | ---: | ---: | ---: | ---: |
| A | 57 | 0 | — | 5,000 / 12,802 |
| B | 57 | 45 | 0.447 ms | 5,000 / 12,502 |
| C | 30 | 24 | 0.992 ms | 5,000 / 25,593 |
| D | 30 | 24 | 0.830 ms | 5,000 / 25,558.5 |

B improved the root incumbent in 11 of 45 dispatches and supplied the first checked point once. It reached a feasible point in 23 dispatches, stalled in 19 and used its full coefficient budget in 3. Root search spends coefficient visits. Certification, preprocessing and tree search can spend LP pivots and nodes. Each fresh process used one solver thread. Limits were 5,000 nodes, 250,000 pivots, 30 seconds, 1,280 MiB of address space and a 40-second watchdog. The main campaign used six pinned logical CPUs on the AMD EPYC 9V74 host and 17.44 solver-minutes. Its peak resident memory was 275.3 MiB.

## What the paired traces show

- **`bell5` recovered the lost solution.** At seed 29, B checked and certified 9,873,951.19 at total node 17, then checked and certified 9,216,166.14 at node 22. The first edition finished at 9,873,951.19. All three revision-2 B runs finished at the A objective of 9,216,166.14.
- **`gt2` still loses final quality on two seeds.** A finished at 27,433 each time. B improved all three root incumbents from 169,047 to 80,768–93,596. Its terminal objectives were 27,433, 36,467 and 35,299. FJ already held a certified root incumbent on this case, so the follow-up rounding allowance did not activate.
- **`50v-10` accounts for most of the matched penalty.** C finished at 3,687.60 in every seed and D at 4,067.77. Removing this one case moves the held-out D/C ratio to 0.9999. For these three diagnostic cases, 24 fresh repeats reproduced the root and final objectives, bound, status, nodes and pivots.
- **Two held-out models spent their allowance before local search.** `cod105` and `b1c1s1` again produced no checked incumbent in the campaign. The earlier stage diagnosis identified the initial LP and L1 pump respectively. `binkar10_1` still produced a checked 7,987.35 at seed 47 in the LocalMIP arms before its terminal numerical failure.

## FPR disposition

The review replayed the bounded Fix, Propagate, and Repair investigation on `enlight_hard`, `timtab1` and `p0548`. The corrected diagnostic reproduced the earlier output byte for byte. It exhausted its coefficient budget after 10, 7 and 12 fixing branches respectively. It produced no checked point. The evidence supports retaining a reproducible fixing and propagation diagnostic. A production FPR worker would still need bounded repair, reversible local state and evidence that these steps solve stalls on relevant inputs. The full 233-model, 600-second campaign remains deferred.

## Verification and files

- Both patch routes applied with Git and produced the same source tree, `64ea895645d1f7d1ef285cff021c93af970525a7`. The native source and benchmark driver passed formatting and locked Clippy checks. Cargo emitted its existing unused worker-dependency manifest warning.
- The locked Rust workspace passed 222 tests. The native crate passed 155 tests with the benchmark feature enabled. These counts overlap. The rebuilt Python package passed 383 tests with one skipped.
- All four review regressions passed. Thirteen effort-zero runs matched the stock solver on final vector, objective, safe bound, status, nodes, pivots and untimed native event sequence.
- An exhaustive oracle independently enumerated 96 small signed integer MIPs, split across both objective senses, with LocalMIP enabled. All final candidates and objectives matched the enumerated optima. LocalMIP dispatched on 58. The repository's native oracle passed 600 cases. The SciPy comparison recorded zero failures in 550 cases and one unclassified reference.
- The archive includes 174 main, 30 smoke and 24 repeat records, the parity observations, scored summaries and the frozen input manifest. `evidence/source.json` identifies the exact patches, build and base. [IMPLEMENTATION-V1.md](IMPLEMENTATION-V1.md) contains the first edition's findings. See the [pilot recipe](../../benchmarks/local-mip/README.md) for the replay commands.
