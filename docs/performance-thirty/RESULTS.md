# LP-IP-craft 30-iteration implementation log

27 November 2026 · v0.31.0 base `2d6c431d968623877894f350456289ef102f4791` · Validated patch revision `fc525ef2221204427ac782983ce6ec2501c7ddf2`

Four trials earned a place in the patch. The first two add an opt-in route for MIPs whose original variable domains are partly infinite. The other two queue rows for bound propagation and index long binary probe trails.

On the 1,024-variable implication chain, the final code reduced median root preprocessing from **419.1 to 165.7 ms** and median solve time from **420.4 to 166.7 ms** over seven pinned pairs. Optimum, nodes and pivots matched. That model deliberately has long implication trails. The 35-model MIP score stayed close to parity.

## Retained changes

| Trial | Implementation | Evidence |
|---:|---|---|
| 1 | Use a verified supplied incumbent to derive outward bounds on improving points. | The exact 18,100 start implied 112 finite sides on `pp08a`. Both signed objective-sense tests and the near-feasible start rejection passed. |
| 2 | Try one bounded fixed-setup LP when no start was supplied. | `pp08a` and `set1ch` returned checked incumbents in all three two-second runs. Starter LPs used 59 and 197 pivots. The main solve accounts for those pivots and one node. |
| 3 | Queue incident rows when inferring missing bounds. | Both reversed 128-step chains changed from unsupported to optimal at −1.75, one node and 255 pivots. Both forward-order controls retained their work and solution. |
| 17 | Index the last updates in long binary probe trails. | The five-pair probe screen gave a root-time ratio of 0.428 [0.312, 0.746]. All four tested cases matched work and outcome. |

Set `objective_cutoff_bounds` in Rust `NativeOptions`, Python `NativeOptions(objective_cutoff_bounds=True)` or the `lp-ip.solve/1` job payload. It defaults to false and requires root presolve and propagation. The domain reduction uses conservative incumbent certification and outward arithmetic, including the objective offset and sense. It keeps the incumbent and each point with an objective at least as good. The bootstrap LP uses at most 512 pivots under the caller's remaining budget. Any required side still unresolved produces `unsupported`.

## Hard MIP coverage

One 60-second observation per arm. Every listed model was unsupported with v0.31.0 defaults. The patched cutoff route used no supplied start. Gurobi received the original model.

| Case | Implied sides | Native status | Checked incumbent | Native lower bound | Gurobi objective and status |
|---|---:|---|---:|---:|---|
| pp08a | 112 | time_limit | 8,020.00 | 5,344.64 | 7,350.00 (optimal) |
| set1ch | 12 | time_limit | 107,691.50 | 39,256.43 | 54,537.75 (optimal) |
| pp08aCUTS | 112 | numerical_failure | 7,700.00 | — | 7,350.00 (optimal) |
| danoint | — | unsupported | — | — | 65.67 (time_limit) |

The `pp08aCUTS` search ended in numerical failure and withheld its lower bound. Its 7,700 incumbent passed the independent original-model check. All three newly admitted cases still have open proofs. On `noswot`, both builds preserved the existing adaptive-plus-equality retry outcome at 150,000 nodes, 864,337 pivots, one alternate-pricing retry, incumbent −41 and lower bound −43.

## Integrated timings

The ten-second scores use the geometric mean of per-model median PAR-2 times shifted by 10 ms. An incomplete proof by the cap receives twice the cap. Each arm ran three repetitions. Brackets are 95% intervals over cases for the patch/baseline score ratio.

| Cohort and settings | v0.31.0 solved | Patch solved | Gurobi solved | Score ratio |
|---|---:|---:|---:|---:|
| 35 historical models, defaults | 29/35 | 29/35 | 34/35 | 1.013 [0.936, 1.088] |
| 35 historical models, adaptive plus equality | 29/35 | 29/35 | — | 1.018 [0.952, 1.089] |
| 24 fresh generator cases, defaults | 24/24 | 24/24 | 24/24 | 1.000 [0.904, 1.114] |
| 6 additional public MIPs, defaults | 4/6 | 4/6 | 6/6 | 0.972 [0.923, 1.020] |
| 3 public LP relaxations, defaults | 3/3 | 3/3 | 3/3 | 0.995 [0.985, 1.002] |

The two-second diagnostic gave **6/10** solved for v0.31.0, **6/10** for the patch default, **6/10** for the cutoff profile and **10/10** for Gurobi. Cutoff also returned a checked limited-run incumbent on `pp08a`. At fixed work, **94/96** cases matched status, candidate vector, objective, bound, nodes and pivots. The remaining two were the reversed-chain admissions.

### LPs

Seven pinned, interleaved pairs per fixture. Every process warmed up once. All measured solves passed the original-model objective, feasibility and KKT checks and matched baseline pivot counts.

| Case | v0.31.0 median ms | Patch median ms | Median paired time ratio |
|---|---:|---:|---:|
| packing:1024 | 72.15 | 73.80 | 1.006 [0.867, 1.132] |
| packing:4096 | 227.80 | 247.35 | 1.091 [1.066, 1.480] |
| network:2048 | 72.45 | 78.41 | 1.020 [1.010, 1.418] |
| irregular:1024 | 187.36 | 157.26 | 0.998 [0.839, 1.479] |
| packing:1536 | 31.65 | 31.35 | 0.988 [0.833, 0.995] |
| network:3072 | 212.30 | 208.32 | 0.989 [0.824, 1.157] |
| irregular:1536 | 440.13 | 362.87 | 0.857 [0.797, 1.129] |
| irregular:4096 | 7,334.59 | 7,046.94 | 0.996 [0.675, 1.092] |

Packing 4,096 took longer on the initial comparison. A bit-identical baseline rebuild and a nine-pair replay on another CPU gave a wall-time ratio of **1.190** [0.982, 1.367] and a whole-process CPU-time ratio of **1.247**. Pivots, refactorizations, FT updates and peak factor entries matched. The follow-up stored the raw target row in each FT column entry to remove an indirection. It gave a wall-time ratio of 0.775 [0.505, 1.130] and a CPU-time ratio of 1.041 against the patch. It failed the retention gate. Large packing LP runtime remains the adverse result in this delivery.

### SpOC4

The first 15-second comparison hit the clock during Matching II at one worker. With a 90-second wall guard, both builds completed the fixed repair workload of 64 tasks over eight rounds with zero proof-phase seconds. The checked vectors, objective scores, bounds, nodes and pivots agreed for each pair.

| Matching table | Workers | Score | Upper bound | Nodes | Pivots |
|---|---:|---:|---:|---:|---:|
| I | 1 | 32,847.565 | 34,153.324 | 510 | 14,505 |
| I | 4 | 32,847.565 | 34,153.324 | 510 | 14,505 |
| II | 1 | 70,945.093 | 76,765.388 | 521 | 13,144 |
| II | 4 | 70,945.093 | 76,765.388 | 521 | 13,144 |

## 30 iteration decisions

Ratios below use the declared primary target for each experiment. Later development trials (11–16 and 21–30) used 16 release codegen units. Each pair used common flags. The integrated builds used the repository setting of one. LP, MIP and parallel trials ran in separate worktrees to overlap compilation. Each trial archive identifies the measured parent and preserves both arms, including the rejected variants.

| Trial | Candidate | Decision and evidence |
|---:|---|---|
| 1 | Certified supplied cutoff | Kept opt-in. 112 missing sides established on pp08a. |
| 2 | Fixed-setup starter LP | Kept opt-in. Checked feasible points on pp08a and set1ch in all short repetitions. |
| 3 | Incident-row queue | Kept. Both reversed chains solved in one node. |
| 4 | Prune with rounded mixed-objective bound | Reverted. No proof or bound gain in seven cases. noswot increased from 87,073 to 90,023 pivots. |
| 5 | Block dense FTRAN | Reverted. irregular:1024, 522.6 → 508.9 ms, paired ratio 1.000 [0.589, 1.203]. |
| 6 | Tile dense BTRAN | Reverted. irregular:1024, 446.5 → 468.4 ms, paired ratio 1.085 [0.958, 1.185]. |
| 7 | Pack a Schur panel | Reverted. irregular:1024, 703.2 → 581.6 ms, paired ratio 0.827 [0.738, 1.581]. |
| 8 | Narrow flat eta indices to u32 | Reverted. irregular:1024:eta:64, 606.7 → 716.6 ms, paired ratio 1.072 [0.814, 1.190]. |
| 9 | Predict eta work for refactoring | Reverted. irregular:2048, 3,617.9 → 3,465.3 ms, paired ratio 0.963 [0.841, 1.157]. |
| 10 | Reuse pivot bucket scratch | Reverted. irregular:1024, 165.9 → 134.1 ms, paired ratio 0.916 [0.809, 1.414]. |
| 11 | Pack stable FT columns | Reverted. packing:4096:ft:64, 294.0 → 280.2 ms, paired ratio 0.844 [0.704, 1.322]. |
| 12 | Clear sparse FT output by preceding support | Reverted. packing:4096:ft:64, 281.1 → 310.2 ms, paired ratio 1.057 [1.023, 1.733]. |
| 13 | Capture spike with lower reach | Reverted. packing:4096:ft:64, 253.4 → 267.9 ms, paired ratio 1.052 [0.886, 1.117]. |
| 14 | Cross-index FT row and column entries | Reverted. packing:4096:ft:64, 269.9 → 236.9 ms, paired ratio 0.873 [0.616, 1.106]. |
| 15 | Price degree-two rows directly | Reverted. network:2048, 97.3 → 132.0 ms, paired ratio 1.356 [0.974, 2.201]. |
| 16 | Maintain pricing block winners in a tree | Reverted. packing:4096, 280.9 → 318.1 ms, paired ratio 0.999 [0.931, 1.423]. |
| 17 | Stamp long probe trails | Kept. Root-time ratio 0.428 [0.312, 0.746]. |
| 18 | Index exact cut duplicates | Reverted. gap-241 root time ratio 0.851 [0.747, 1.307]. |
| 19 | Stream sparse GMI tableau weights | Reverted. lotsize-233 root time ratio 0.851 [0.156, 3.176]. |
| 20 | Drop expensive cut family after weak bound progress | Reverted. All six cases matched cut work and search outcomes. |
| 21 | Reuse scaled CSR prefix on cut append | Reverted. gap-241 root time ratio 1.007 [0.907, 1.527]. |
| 22 | Move node bounds via common ancestor | Reverted. bell5 solve time ratio 0.972 [0.874, 1.165]. |
| 23 | Pack nonbasic snapshot state | Reverted. mod008 solve time ratio 1.029 [0.736, 1.079]. |
| 24 | Cache scalable branching scores | Reverted. gap-241 solve time ratio 0.977 [0.780, 1.939]. |
| 25 | Probe less-trained directions first | Reverted. GAP incumbent improved from 450 to 386 at 1,000 nodes. Integer covering needed 762 nodes versus 635 to prove, and bell5's bound weakened. |
| 26 | Batch four initial mixed dive fixings | Reverted. VPM incumbents and bounds matched the existing dive on both cases at 1,000 nodes. |
| 27 | Best-estimate bursts after stagnation | Reverted. GAP saved 36 nodes. noswot used 18,244 versus 16,544 pivots at 4,000 nodes with the same incumbent and bound. Longer fixed-work replay is in the archive. |
| 28 | Reuse packing repair scratch | Reverted. ii:1, 5,385.6 → 5,406.9 ms, paired ratio 0.998 [0.938, 1.091]. |
| 29 | Size Schur chunks by estimated work | Reverted. 512:4, 30.4 → 29.5 ms, paired ratio 1.093 [0.926, 1.151]. |
| 30 | Schedule batches using integer structure | Reverted. mixed:4, 417.2 → 378.2 ms, paired ratio 0.985 [0.833, 1.184]. |

Trial 8 adapted the plan to the release's existing flat eta storage. Trial 15 needed a borrow-checker correction before timing. Trials 5, 6, 7 and 10 each rebuilt eight frozen bases and checked forward and transpose residuals. The narrower FT checks for trials 11–14 passed. The archive keeps the source diffs, raw observations and correctness checks.

The scratch-reuse candidate for Matching I at one worker had a 0.821 ratio. The primary Matching II result was 0.998. The 4- and 8-worker Matching II pairs gave no reliable advantage. The VPM control showed that existing `root_heuristics` found feasible points of 22 and 16.25 at 1,000 nodes. With a common checked optimal start, it raised the two lower bounds to 16.4333 and 10.2690 from zero. Neither VPM proof closed in that work budget.

## Verification and replay

- **234 Rust tests** passed with the locked workspace and all features. Formatting and all-target, all-feature Clippy passed. Cargo reported the pre-existing manifest warning about the runner dependency.
- **418 Python and harness tests** passed with the release extension and the built job worker.
- Both profiles passed **600 exact cases each**. SciPy classified 549 of 550 per profile with zero disagreements on the classified cases. One SciPy status per profile was unclassified.
- **800 cutoff-oracle solves** over 400 enumerated mixed models covered both senses, signed costs, offsets and two work caps. All checked optimum values and reported bounds agreed with enumeration.
- Four schema-checked job tutorial runs passed. A separate cutoff-enabled `lp-ip.solve/1` job returned a rechecked optimum of 1. `lp-ip.evaluate/1` verified that point on the original input.
- The cumulative patch applied to a clean checkout at the pinned base. The staged tree hash matched the validated patch revision. The ZIP includes its checksum and the checked replay evidence.

The timing host was a shared AMD EPYC 9V74. Rust 1.100.0, CPython 3.12.14 and Gurobi 14.0.0 built or ran the headline arms. HiGHS 1.16.0 cross-checked the new MPS conversions. Independent models ran concurrently in the ten-second campaigns. Per-solve time was cooperative and could exceed the configured cap slightly. The chosen cases support a presolve gain on long probe trails and greater MIP coverage. Overall MIP time and hard proof gaps still need separate evidence on larger sets. The untouched confirmation set was frozen before edits and comprised four new seeds for each of the six generator families, six public MIPs and three public LP relaxations.
