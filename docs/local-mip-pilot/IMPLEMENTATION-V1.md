# LP-IP-craft LocalMIP and FPR implementation log

26 November 2026 · CI-scale pilot

## Decision

**Archive this LocalMIP variant as an opt-in experiment.** The additive arm's held-out primal-integral ratio was **0.9972** and the matched replacement ratio was **1.0878**, with lower better. No complete A/B pair gained a checked incumbent by the horizon. LocalMIP improved the incumbent at the root in 11 of 45 additive dispatches and found the first incumbent in 1. Those improvements did not meet the plan's end-to-end threshold.

The FPR evidence supports keeping the integration notes and closing this pilot. The fixing and propagation replay ran out of effort on both held-out cases where local search stalled and the solver still had no incumbent. The larger campaign stays scoped to the home machines after January 2027.

## Implementation

The patch applies to LP-IP-craft `2468f091855a00085ad39e3c6f7922732f22fda4`. The public Rust control is `NativeOptions::local_mip_effort`, default 0. A positive value enables a single attempt after Feasibility Jump at the first fractional root reached by the top-level general serial MIP search. Named diagnostic fields expose the local effort, time and stopping reason. The solver checks each proposal on the search and original models before handing it to normal incumbent admission.

The worker samples up to eight violated rows and visits at most 64 row–column pairs per move. It tries individual tight moves towards a violated row bound, using the two adjacent integer values for an integer column. The score uses weighted row transitions and violation progress. Objective change and a seed resolve ties. A three-move reverse tabu and bounded row weights diversify the search. The attempt ends with its first checked feasible point or a limit.

The common runner charges coefficient visits in setup, refresh, move generation, scoring and update. It tracks bookkeeping and checking separately. It uses the solver's cancellation and time limits, a 250 ms stage deadline and a patience budget equal to one quarter of effort. A new drop of at least 10% in violated rows renews patience. Local search uses zero LP pivots and zero LP nodes. Existing certification of its candidates can spend LP work.

The benchmark stream begins at solve entry. It records improvements at their original-model check, including the RENS return before that point becomes a warm start. It records proof-bound improvements separately. The recorder is exposed by the benchmark feature. The benchmark driver stores the seed and selects matched FJ or LocalMIP at the main hook. [Source base](https://github.com/dietmarwo/LP-IP-craft/tree/2468f091855a00085ad39e3c6f7922732f22fda4) · [LocalMIP paper](https://doi.org/10.4230/LIPIcs.CP.2024.19) · [Upstream budget and move reference](https://github.com/spoorendonk/mip-heuristics/tree/4e3bf0021c178189b26cce19ca65cbe78470d5e3)

## Frozen inputs and protocol

The measured matrix comprised 19 models, three seeds (11, 29, 47) and 174 fresh processes. A and B ran on nine existing public MIPs and ten mipfeas MIPs. C and D ran on the ten mipfeas MIPs. The two development inputs were `assign1-5-8` and `markshare2`. The other eight external inputs were held out. Calibration kept the planned ceilings.

| Arm | Search at the main root |
| --- | --- |
| A | Existing diving, L1 pump, RENS and ordinary FJ |
| B | A plus one tight-move attempt |
| C | Same root stack, with metered FJ at the main hook |
| D | Same root stack, with LocalMIP at the main hook |

The external sample met the plan's 10–20 range. The native MPS conversion left limits as supplied. HiGHS 1.16.0 served as the independent reader. The roster and reference objective file were frozen at `4e3bf00`. Source hashes, conversion checks and exclusions are recorded in the manifest.

| Input that failed screening | Finding | Replacement |
| --- | --- | --- |
| `qap10` | Importers disagreed on the implicit upper bound of marker columns | Empty binary development slot |
| `gen-ip054` | Native finite-bound admission failed | `gen-ip002` also failed finite-bound admission. The development slot stayed empty |
| `graphdraw-domain` | Native finite-bound admission failed | `beasleyC3` |
| `csched008` | Native finite-bound admission failed | `binkar10_1` |

Each solve had 30 seconds, 5,000 nodes and 250,000 pivots. The local cap was `min(2,000,000, 64 × max(nnz, 1))` visits with 250 ms of local wall time. The rerun used six independently pinned physical cores on one AMD EPYC 9V74 host, one solver thread per process and 1,280 MiB of address space per process. Pairs ran on the same core in counterbalanced order. The watchdog was 40 seconds. The measured matrix used 17.6 solver-minutes in total. Peak resident memory was 275.3 MiB. Rust 1.100.0, a release build and the same lockfile were used across arms.

The initial campaign exposed a control error when the replacement policy disabled FJ in RENS children. The driver now replaces FJ only at the main root hook. The entire 174-run matrix was rerun. 99 comparisons of observed prefixes through the accepted search-root LP matched on event order, checked objectives, safe bounds, nodes and pivots. The tables use the corrected rerun.

## Primal integral and coverage

The penalty is 2 before feasibility, 1 for an incumbent with the opposite sign to the reference, and the nonnegative, six-decimal published reference gap otherwise. Scores integrate this penalty on the common horizon. Each ratio is the geometric mean of paired log differences at shift 0.001, with seeds averaged within each case. Intervals resample whole cases. The public references are the repository's frozen objectives. [mipfeas scoring implementation](https://github.com/spoorendonk/mip-heuristics/blob/4e3bf0021c178189b26cce19ca65cbe78470d5e3/bench/parse_highs_log.py)

| Cohort | Pair | Complete cases / seed pairs | Ratio | 95% bootstrap interval | Better / tied / worse | Worst-tail sensitivity |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Held-out | B/A | 7 / 21 | 0.9972 (-0.28%) | 0.9922–1.0003 | 2/4/1 | 0.9975 |
| Held-out | D/C | 7 / 21 | 1.0878 (+8.78%) | 1.0007–1.2832 | 0/4/3 | 1.0764 |
| Public | B/A | 9 / 27 | 1.0865 (+8.65%) | 0.9900–1.2375 | 3/0/6 | 1.0865 |
| Development | B/A | 2 / 6 | 0.9916 (-0.84%) | 0.9833–1.0000 | 2/0/0 | 0.9916 |
| Development | D/C | 2 / 6 | 1.1341 (+13.41%) | 1.0000–1.2862 | 1/0/1 | 1.1341 |
| External | B/A | 9 / 27 | 0.9959 (-0.41%) | 0.9908–1.0001 | 4/4/1 | 0.9963 |
| External | D/C | 9 / 27 | 1.0979 (+9.79%) | 1.0011–1.2484 | 1/4/4 | 1.0877 |

All 174 processes returned a record. The stock context and all 12 `binkar10_1` measurements returned a numerical failure. The held-out complete statistic therefore covers seven of eight instances. The sensitivity column keeps all scheduled pairs and charges the failed run a penalty of 2 after its last observed event. There were 126 trajectories with a checked incumbent inside the horizon. Every returned candidate passed the independent Python check. No trace reported a rejected LocalMIP admission or a checked objective beyond the fixed reference by the audit threshold.

| Group | Arm | Runs | Checked at root | By 1 s | By 5 s | By 30 s | Certified by 30 s | Median capped time to first |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Public | A | 27 | 18 | 27 | 27 | 27 | 24 | 0.0040 s |
| Public | B | 27 | 19 | 27 | 27 | 27 | 24 | 0.0034 s |
| Held-out | A | 24 | 9 | 9 | 12 | 12 | 9 | 16.9035 s |
| Held-out | B | 24 | 9 | 9 | 12 | 12 | 9 | 16.8621 s |
| Held-out | C | 24 | 9 | 9 | 12 | 12 | 9 | 16.8958 s |
| Held-out | D | 24 | 9 | 9 | 12 | 12 | 9 | 16.8926 s |
| Development | A | 6 | 6 | 6 | 6 | 6 | 6 | 0.0167 s |
| Development | B | 6 | 6 | 6 | 6 | 6 | 6 | 0.0159 s |
| Development | C | 6 | 6 | 6 | 6 | 6 | 6 | 0.0165 s |
| Development | D | 6 | 6 | 6 | 6 | 6 | 6 | 0.0167 s |

Coverage counts include checked candidates from incomplete solves. The median time to first treats a censored run as 30 seconds. The archive includes the exact first event, node and pivot coordinates, objective at each checkpoint, final safe bound when available and terminal status for every run.

| Arm | Median whole-solve nodes / pivots | Local dispatches | Median local visits | Median local time |
| --- | ---: | ---: | ---: | ---: |
| A | 5,000 / 12,802 | 0 | — | — |
| B | 5,000 / 12,502 | 45 | 22,236 | 0.417 ms |
| C | 5,000 / 25,314 | 24 | 58,515 | 1.069 ms |
| D | 5,000 / 25,305 | 24 | 41,884 | 1.071 ms |

The median LocalMIP cost was below 1 ms in the complete matrix. In B the stage stopped on 23 feasible, 19 stalled, 3 effort exhausted dispatches. The matched held-out ratio after removing `50v-10` was **1.0019**. This one model accounts for most of the replacement loss. `cod105` and `b1c1s1` timed out before the hook in all 24 primary and matched runs. `cod105` spent almost the whole window in the first root LP. `b1c1s1` spent about 29.8 seconds in the first pump projection in the seed-11 A run. The stock context found a checked incumbent on `b1c1s1` within the same limits.

## Root-to-tree examples

| Case | Observed result |
| --- | --- |
| `gt2` | B improved all three root objectives from 169,047 to 80,768–93,596. At 5,000 nodes A reached 27,433 in all three runs. B reached 27,433, 36,467 and 35,299. The two weaker finishes repeated on the diagnostic CPU with the same node cap. |
| `bell5` | B seed 29 certified 9,873,951 at the root. With a certified incumbent already in hand, the tree's rounding fallback did not supply the 9,216,166 point that A found at total node 22. B had zero successful rounding/unit-move candidates on this run. The loss repeated. |
| `50v-10` | A and C reached 3,687.60 from FJ after a 7,482.06 RENS incumbent. D reached 5,096–5,106 with its root move and ended at 4,067.77 in each seed. At the same node cap D used fewer LP pivots. The quality loss repeated. |
| `lseu` and `assign1-5-8` | LocalMIP improved the root objective in all three additive seeds on both cases. All runs on each case reached the same final objective at the 5,000-node cap. |
| `binkar10_1` | A and C entered search with a checked 59,412.90 RENS point. Seed 47 reached 7,987.35 with LocalMIP. The solver hit numerical failure in all arms. None of the checked points produced a certified pruning incumbent. |
| `p0548` | No primary arm found an incumbent at the root. The historical combined-cuts replay reproduced a RENS root discovery at 8,691. This is sensitive to the root configuration. The same replay found 21,166 on `gt2` and 1,148 on `lseu`. |

The case-level table gives the primary and matched ratios and median final objectives across seeds. Absolute variation on very fast cases is sensitive to host timing. The raw archive includes independent replays directed at the largest changes.

| Case | Group | B/A primal integral | D/C primal integral | A final | B final | C final | D final |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bell5` | public | 1.4890 | — | 9.21617e+06 | 9.21617e+06 | — | — |
| `egout` | public | 1.0191 | — | 568.101 | 568.101 | — | — |
| `flugpl` | public | 0.9574 | — | 1.2015e+06 | 1.2015e+06 | — | — |
| `gt2` | public | 1.4724 | — | 27,433 | 35,299 | — | — |
| `lseu` | public | 0.9729 | — | 1,128 | 1,128 | — | — |
| `p01` | public | 1.0123 | — | 263 | 263 | — | — |
| `p0548` | public | 0.9964 | — | 26,667 | 26,667 | — | — |
| `rgn` | public | 1.0033 | — | 82.2 | 82.2 | — | — |
| `small_mip` | public | 1.0016 | — | 3.23684 | 3.23684 | — | — |
| `assign1-5-8` | development | 0.9833 | 1.2862 | 212 | 212 | 212 | 212 |
| `markshare2` | development | 1.0000 | 1.0000 | 57 | 57 | 57 | 57 |
| `50v-10` | heldout | 0.9833 | 1.7825 | 3,687.6 | 3,687.6 | 3,687.6 | 4,067.77 |
| `b1c1s1` | heldout | 1.0000 | 1.0000 | — | — | — | — |
| `beasleyC3` | heldout | 1.0011 | 1.0051 | 937 | 937 | 937 | 937 |
| `binkar10_1` | heldout | — | — | 59,412.9 | 59,412.9 | 59,412.9 | 59,412.9 |
| `cod105` | heldout | 1.0000 | 1.0000 | — | — | — | — |
| `enlight_hard` | heldout | 1.0000 | 1.0000 | — | — | — | — |
| `neos-3024952-loue` | heldout | 0.9961 | 1.0061 | 36,886 | 36,886 | 36,886 | 36,886 |
| `timtab1` | heldout | 1.0000 | 1.0000 | — | — | — | — |

## FPR feasibility investigation

FPR is Fix, Propagate, and Repair. Its Class 1 presolve direction is the relevant idea here. The upstream DFS stores a variable and value choice together with trail checkpoints for domains, the partial solution, activity and the queue. On conflict it can try the sibling or repair the failing domain assignment. Leaf repairs operate on a completed solution. [FPR paper](https://doi.org/10.1007/s12532-024-00269-5) · [Reference implementation](https://github.com/spoorendonk/mip-heuristics/tree/4e3bf0021c178189b26cce19ca65cbe78470d5e3/src)

| FPR stage | Reusable native machinery | Work and state still needed |
| --- | --- | --- |
| Fix and propagate | `propagate::Domain` has exact `i128` row min/max updates for bounded integer-only rows with supported integer coefficients and endpoints. It detects an impossible row interval or an empty integer box. | Count incidence scans as well as the current term and queue counters. Introduce an explicit local save/restore contract for nested attempts before trying sibling choices. |
| Propagate mixed rows | `mixed::Domain` uses outward prefix/suffix activity intervals on finite boxes outside the exact integer path. It records the row and previous/new endpoints of each deduction. | A work-limited return leaves partial deductions. It must be distinguishable from a completed propagation pass. Restore the queue and deduced bounds alongside the exact state. |
| Repair and return | The native checker accepts complete vectors and the proof path already works separately. | `repair_search.cpp` may widen or shift a current domain within the structural box and synchronises a second engine. Native propagation tightens bounds and has no equivalent repair operation. `walksat.cpp` repairs completed points and would need a separate bounded kernel. |

`NodeBox` restores branch-local bounds and the propagators resynchronise the columns reported as changed. A standalone FPR worker would need to own that mutable state and its snapshots. Using the tree's mutable bound state directly would mix experimental re-fixings with proof search.

The bounded diagnostic walk tried the lower endpoint first on the narrowest incident integer variable and retained the alternate interval for backtracking. It used exact rational activities on the original arrays and a limit of 64 coefficient visits per original nonzero. It implemented fixing and interval propagation only. Its result was partial tightening on all three cases.

| Case | Charged coefficient visits | Fixing branches | Bound tightenings | Maximum fixed columns | Checked point |
| --- | ---: | ---: | ---: | ---: | --- |
| `enlight_hard` | 35,838 / 35,840 | 10 | 127 | 13 | none |
| `timtab1` | 53,055 / 53,056 | 7 | 65 | 26 | none |
| `p0548` | 109,491 / 109,504 | 12 | 25 | 36 | none |

The evidence did not clear the planned gate for a separate full FPR port. The replay left the degree of benefit from domain-shift repair and completed-vector repair unresolved.

## Validation and reproduction

- A fresh checkout at the pinned base accepted the ZIP's unified patch. The patched checkout passed 221 locked Rust workspace tests, clippy and formatting. The benchmark build and the native dependency audit passed. The dependency tree contains the native Rust solver and its Rayon dependencies.
- The focused tests cover bounded incremental moves, original-model rejection, both objective senses, fractional integer bounds, signed rows, fixed columns, simultaneous patience and effort limits, cancellation during a checked scan and preservation of an incumbent when work is already exhausted.
- Effort-zero matched the stock binary on thirteen deterministic controls, with identical final vector, objective, safe bound, status, nodes, LP pivots and un-timed incumbent event order.
- The Python test suite reported 383 passed and one skipped. Native oracle validation passed 600 cases. The randomized SciPy comparison reported zero failures on 550 cases, including one unclassified reference result.
- The scoring arithmetic passed its analytic trajectory and censoring fixtures. The corrected rerun recorded zero process kills, no exceeded deterministic limits and zero independent final-vector failures. The twelve terminal numerical failures are the `binkar10_1` runs described above.

The archive includes the patch, source and result manifests, all final per-run records, scored results, root replays, the bounded FPR diagnostic and validation output. It lists the application and reproduction commands. The MIPLIB data can be fetched and verified using the hashes in the input manifest.
