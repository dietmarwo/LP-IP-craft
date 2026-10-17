# LP-IP-craft 0.24 — results

Implemented the requested order: exact incumbent witnesses → reliability branching → serial Feasibility Jump prototype → LP-directed covers and lifting → reversible mixed-row propagation. 139 Rust checks, 209 Python checks, 83,670 frozen-oracle runs and the 400-model bound audit pass under the unchanged adjudications. The final native unit suite was rerun and passes 47 checks after exact zero/integer fast paths were added. Clippy passes with warnings denied.

## Defaults and capability limits

Exact rational certification is enabled by default. Reliability branching, Feasibility Jump, directed root separation and mixed-row node propagation are independently selectable and remain opt-in: their gains and costs vary by model. This release adds no third-party production dependency and does not add parallel tree search.

A rational certificate proves existence of a feasible rational point for the supplied binary64 data, with an outward objective endpoint. Its binary64 display approximation is separately checked. This is a primal-witness mechanism; the solver still reports its existing numerical optimality contract. Failed reconstruction does not authorize pruning.

The FJ implementation is a bounded deterministic feasibility-only prototype inspired by the published weighted-violation coordinate-jump method. It uses cached sparse coordinate scores, breakpoint jumps, row-weight increases and deterministic restarts. It does not implement the full published heuristic or objective-weight adaptation.

Directed covers are heuristic proposals from the current root LP, for positive integral binary-knapsack rows with 3..128 stored entries and coefficients/capacity up to the documented 31-bit range. Integer validity is checked before insertion. Sequential lifting and final validation use an exact profit DP capped at 8192 profit states and 2000000 work units per separation call. Up to eight cuts per round and four configured rounds are supported. General MIR/GMI cuts and live-tree insertion remain outside scope.

Mixed propagation recomputes outward-rounded prefix/suffix activities for finite-box rows, derives conservative quotient bounds, rounds integral targets, records local row reasons and restores implications through NodeBox. It alternates at most twice with the existing exact integer propagator. Continuous changes below a relative 1e-12 threshold are ignored, safely retaining wider boxes. No approximate row activity can itself prove infeasibility.

## Certification diagnosis

The independent Python Fraction inspection establishes why flugpl cannot receive an exact certificate for these numerical candidates. Its supplied model fixes x0=60 and requires binary64(0.9)*x0+x1−x3=0 with integer x1 and x3. Exact binary64(0.9)*60 differs from integer 54 by 1.3322676295501878e-15. The equality is incompatible in exact real arithmetic on those data. Numerical tolerance acceptance remains unchanged; the certificate is correctly refused. No coefficients or tolerances were altered.

Egout initially exceeded the first reconstruction cap: its final point has 41 free continuous columns. The accepted cap is 64, with 8192-bit arithmetic, a 250 ms per-attempt guard and the whole-call cancellation/time budget. Python Fraction independently reconstructs and checks its boundary point. All unsupported, oversized, inconsistent or failed reconstructions retain the existing LP-shrinking fallback.

## Public policy observations

Each row below is the final measured solve after one warmup and one earlier measured solve in its own fresh process. Both measured solves are retained in the raw data. Public cases use one numerical thread, decomposition off, 30 seconds, 100000 nodes and 1000000 pivots. This table contains single-process observations, not stable timing estimates. The repeat table below provides the selected timing comparison. References are numerical reference values, not an exact-decimal reformulation.

| Case | Policy | Status | Nodes | Pivots | Objective | Bound | ms |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| bell5 | baseline-023 | node_limit | 100,000 | 105,524 | 8,966,406 | 8,963,502 | 3476.450 |
| bell5 | legacy-policy | node_limit | 100,000 | 105,524 | 8,966,406 | 8,963,502 | 3494.257 |
| bell5 | certification | node_limit | 100,000 | 105,544 | 8,966,414 | 8,963,032 | 3316.483 |
| bell5 | reliability | node_limit | 100,000 | 111,223 | 8,966,406 | 8,961,823 | 3343.325 |
| bell5 | jump | node_limit | 100,000 | 105,544 | 8,966,414 | 8,963,032 | 3321.959 |
| bell5 | directed-covers | node_limit | 100,000 | 105,612 | 8,966,414 | 8,963,032 | 3257.316 |
| bell5 | lifted-covers | node_limit | 100,000 | 105,612 | 8,966,414 | 8,963,032 | 3089.697 |
| bell5 | mixed-propagation | optimal | 43,946 | 54,191 | 8,966,406 | 8,966,406 | 13488.272 |
| bell5 | combined | optimal | 26,351 | 41,737 | 8,966,406 | 8,966,406 | 8082.310 |
| egout | baseline-023 | optimal | 66,553 | 55,398 | 568.1007 | 568.1007 | 3018.696 |
| egout | legacy-policy | optimal | 66,553 | 55,398 | 568.1007 | 568.1007 | 3181.267 |
| egout | certification | optimal | 12,602 | 14,718 | 568.1007 | 568.1007 | 516.156 |
| egout | reliability | optimal | 13,353 | 15,658 | 568.1007 | 568.1007 | 461.285 |
| egout | jump | optimal | 12,602 | 14,718 | 568.1007 | 568.1007 | 572.107 |
| egout | directed-covers | optimal | 12,603 | 14,760 | 568.1007 | 568.1007 | 576.498 |
| egout | lifted-covers | optimal | 12,603 | 14,760 | 568.1007 | 568.1007 | 534.820 |
| egout | mixed-propagation | optimal | 3,094 | 4,949 | 568.1007 | 568.1007 | 903.745 |
| egout | combined | optimal | 2,980 | 5,349 | 568.1007 | 568.1007 | 883.697 |
| flugpl | baseline-023 | optimal | 23,947 | 15,622 | 1,201,500 | 1,201,500 | 247.916 |
| flugpl | legacy-policy | optimal | 23,947 | 15,622 | 1,201,500 | 1,201,500 | 265.453 |
| flugpl | certification | optimal | 23,947 | 15,622 | 1,201,500 | 1,201,500 | 246.214 |
| flugpl | reliability | optimal | 25,967 | 16,612 | 1,201,500 | 1,201,500 | 252.092 |
| flugpl | jump | optimal | 23,947 | 15,622 | 1,201,500 | 1,201,500 | 253.374 |
| flugpl | directed-covers | optimal | 23,948 | 15,634 | 1,201,500 | 1,201,500 | 233.367 |
| flugpl | lifted-covers | optimal | 23,948 | 15,634 | 1,201,500 | 1,201,500 | 262.403 |
| flugpl | mixed-propagation | optimal | 425 | 502 | 1,201,500 | 1,201,500 | 51.150 |
| flugpl | combined | optimal | 295 | 330 | 1,201,500 | 1,201,500 | 27.899 |
| gt2 | baseline-023 | optimal | 72,182 | 147,116 | 21,166 | 21,166 | 2809.906 |
| gt2 | legacy-policy | optimal | 72,182 | 147,116 | 21,166 | 21,166 | 2686.238 |
| gt2 | certification | optimal | 72,182 | 147,116 | 21,166 | 21,166 | 2652.163 |
| gt2 | reliability | optimal | 16,056 | 49,131 | 21,166 | 21,166 | 664.085 |
| gt2 | jump | node_limit | 100,000 | 190,011 | 22,521 | 21,166 | 3910.328 |
| gt2 | directed-covers | optimal | 72,183 | 147,151 | 21,166 | 21,166 | 2762.166 |
| gt2 | lifted-covers | optimal | 72,183 | 147,151 | 21,166 | 21,166 | 2857.624 |
| gt2 | mixed-propagation | node_limit | 100,000 | 176,882 | 22,342 | 21,166 | 5476.861 |
| gt2 | combined | node_limit | 100,000 | 174,354 | 22,521 | 21,166 | 5245.375 |
| lseu | baseline-023 | optimal | 19,747 | 47,935 | 1,120 | 1,120 | 538.016 |
| lseu | legacy-policy | optimal | 19,747 | 47,935 | 1,120 | 1,120 | 528.928 |
| lseu | certification | optimal | 19,747 | 47,935 | 1,120 | 1,120 | 581.541 |
| lseu | reliability | optimal | 13,898 | 33,674 | 1,120 | 1,120 | 376.026 |
| lseu | jump | optimal | 15,277 | 36,364 | 1,120 | 1,120 | 428.399 |
| lseu | directed-covers | optimal | 21,807 | 51,794 | 1,120 | 1,120 | 649.598 |
| lseu | lifted-covers | optimal | 17,806 | 43,493 | 1,120 | 1,120 | 496.358 |
| lseu | mixed-propagation | optimal | 19,747 | 47,935 | 1,120 | 1,120 | 597.943 |
| lseu | combined | optimal | 14,063 | 36,178 | 1,120 | 1,120 | 422.004 |
| p0548 | baseline-023 | node_limit | 100,000 | 602,083 | — | 4,033 | 18584.463 |
| p0548 | legacy-policy | node_limit | 100,000 | 602,083 | — | 4,033 | 18636.758 |
| p0548 | certification | node_limit | 100,000 | 602,083 | — | 4,033 | 18788.900 |
| p0548 | reliability | node_limit | 100,000 | 540,211 | — | 4,487 | 16746.173 |
| p0548 | jump | node_limit | 100,000 | 602,083 | — | 4,033 | 18895.452 |
| p0548 | directed-covers | node_limit | 100,000 | 621,955 | — | 5,110 | 20421.483 |
| p0548 | lifted-covers | node_limit | 100,000 | 560,000 | — | 5,785 | 19184.525 |
| p0548 | mixed-propagation | node_limit | 100,000 | 602,083 | — | 4,033 | 19206.783 |
| p0548 | combined | node_limit | 100,000 | 515,657 | 20,592 | 6,946 | 18886.192 |
| rgn | baseline-023 | node_limit | 100,000 | 279,893 | 82.2 | 82.2 | 5107.892 |
| rgn | legacy-policy | node_limit | 100,000 | 279,893 | 82.2 | 82.2 | 4744.732 |
| rgn | certification | optimal | 7,249 | 33,502 | 82.2 | 82.2 | 344.372 |
| rgn | reliability | optimal | 6,503 | 28,031 | 82.2 | 82.2 | 310.594 |
| rgn | jump | optimal | 7,249 | 33,502 | 82.2 | 82.2 | 348.067 |
| rgn | directed-covers | optimal | 7,250 | 33,554 | 82.2 | 82.2 | 340.311 |
| rgn | lifted-covers | optimal | 7,250 | 33,554 | 82.2 | 82.2 | 341.891 |
| rgn | mixed-propagation | optimal | 7,871 | 38,276 | 82.2 | 82.2 | 1663.654 |
| rgn | combined | optimal | 6,606 | 28,341 | 82.2 | 82.2 | 1492.539 |
| p01 | baseline-023 | optimal | 1 | 25 | 263 | 263 | 0.146 |
| p01 | legacy-policy | optimal | 1 | 25 | 263 | 263 | 0.151 |
| p01 | certification | optimal | 1 | 25 | 263 | 263 | 0.145 |
| p01 | reliability | optimal | 1 | 25 | 263 | 263 | 0.143 |
| p01 | jump | optimal | 1 | 25 | 263 | 263 | 0.165 |
| p01 | directed-covers | optimal | 2 | 50 | 263 | 263 | 0.254 |
| p01 | lifted-covers | optimal | 2 | 50 | 263 | 263 | 0.347 |
| p01 | mixed-propagation | optimal | 1 | 25 | 263 | 263 | 0.151 |
| p01 | combined | optimal | 2 | 50 | 263 | 263 | 0.367 |

## Selected three-round repeats

Fresh process for each case/policy/round, one warmup and two measured solves (six measured samples across three rounds). Policy order rotates. Reported times are measured-solve medians and ranges; shared-host variability remains. Nodes/pivots include any charged lookahead or root prepasses.

| Case | Policy | Statuses | Median ms | Range ms | Nodes | Pivots |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| egout | baseline-023 | optimal | 3022.403 | 2844.168–3060.253 | 66553 | 55398 |
| egout | certification | optimal | 517.492 | 510.470–554.139 | 12602 | 14718 |
| egout | reliability | optimal | 489.681 | 474.931–524.343 | 13353 | 15658 |
| egout | combined | optimal | 926.952 | 882.688–1007.194 | 2980 | 5349 |
| gt2 | baseline-023 | optimal | 2736.265 | 2631.789–2745.144 | 72182 | 147116 |
| gt2 | certification | optimal | 2762.806 | 2630.015–2805.384 | 72182 | 147116 |
| gt2 | reliability | optimal | 668.330 | 629.131–705.347 | 16056 | 49131 |
| gt2 | combined | node_limit | 5348.332 | 5208.428–5551.303 | 100000 | 174354 |
| lseu | baseline-023 | optimal | 575.125 | 532.970–603.768 | 19747 | 47935 |
| lseu | certification | optimal | 562.836 | 529.515–634.736 | 19747 | 47935 |
| lseu | reliability | optimal | 398.640 | 378.774–502.801 | 13898 | 33674 |
| lseu | combined | optimal | 438.926 | 430.760–468.173 | 14063 | 36178 |
| rgn | baseline-023 | node_limit | 4979.608 | 4861.519–5234.087 | 100000 | 279893 |
| rgn | certification | optimal | 356.234 | 343.508–389.235 | 7249 | 33502 |
| rgn | reliability | optimal | 338.955 | 306.016–350.270 | 6503 | 28031 |
| rgn | combined | optimal | 1429.916 | 1398.242–1475.245 | 6606 | 28341 |

## Mechanism observations

| Case | Policy | Rational successes | Probes | FJ proposals | Directed / lifted cuts | Mixed rows / changes | First certified node |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| bell5 | certification | 3 | 0 | 0 | 0 / 0 | 0 / 0 | 6 |
| bell5 | reliability | 5 | 64 | 0 | 0 / 0 | 0 / 0 | 118 |
| bell5 | jump | 3 | 0 | 0 | 0 / 0 | 0 / 0 | 6 |
| bell5 | lifted-covers | 3 | 0 | 0 | 0 / 0 | 0 / 0 | 6 |
| bell5 | mixed-propagation | 8 | 0 | 0 | 0 / 0 | 60 / 4860932 | 6 |
| bell5 | combined | 8 | 64 | 0 | 0 / 0 | 60 / 2963789 | 54 |
| egout | certification | 4 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| egout | reliability | 2 | 64 | 0 | 0 / 0 | 0 / 0 | 1 |
| egout | jump | 4 | 0 | 1 | 0 / 0 | 0 / 0 | 1 |
| egout | lifted-covers | 4 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| egout | mixed-propagation | 12 | 0 | 0 | 0 / 0 | 98 / 134345 | 1 |
| egout | combined | 13 | 64 | 1 | 0 / 0 | 98 / 123665 | 1 |
| flugpl | certification | 0 | 0 | 0 | 0 / 0 | 0 / 0 | — |
| flugpl | reliability | 0 | 58 | 0 | 0 / 0 | 0 / 0 | — |
| flugpl | jump | 0 | 0 | 0 | 0 / 0 | 0 / 0 | — |
| flugpl | lifted-covers | 0 | 0 | 0 | 0 / 0 | 0 / 0 | — |
| flugpl | mixed-propagation | 0 | 0 | 0 | 0 / 0 | 17 / 21630 | — |
| flugpl | combined | 0 | 64 | 0 | 0 / 0 | 17 / 12919 | — |
| gt2 | certification | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 35 |
| gt2 | reliability | 0 | 64 | 0 | 0 / 0 | 0 / 0 | 80 |
| gt2 | jump | 0 | 0 | 1 | 0 / 0 | 0 / 0 | 1 |
| gt2 | lifted-covers | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 35 |
| gt2 | mixed-propagation | 0 | 0 | 0 | 0 / 0 | 4 / 1477 | 32 |
| gt2 | combined | 0 | 64 | 1 | 0 / 0 | 4 / 1655 | 1 |
| lseu | certification | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 122 |
| lseu | reliability | 0 | 64 | 0 | 0 / 0 | 0 / 0 | 207 |
| lseu | jump | 0 | 0 | 1 | 0 / 0 | 0 / 0 | 1 |
| lseu | lifted-covers | 0 | 0 | 0 | 2 / 1 | 0 / 0 | 75 |
| lseu | mixed-propagation | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 122 |
| lseu | combined | 0 | 64 | 1 | 2 / 1 | 0 / 0 | 1 |
| p0548 | certification | 0 | 0 | 0 | 0 / 0 | 0 / 0 | — |
| p0548 | reliability | 0 | 64 | 0 | 0 / 0 | 0 / 0 | — |
| p0548 | jump | 0 | 0 | 0 | 0 / 0 | 0 / 0 | — |
| p0548 | lifted-covers | 0 | 0 | 0 | 2 / 2 | 0 / 0 | — |
| p0548 | mixed-propagation | 0 | 0 | 0 | 0 / 0 | 0 / 0 | — |
| p0548 | combined | 0 | 64 | 0 | 2 / 2 | 0 / 0 | 93634 |
| rgn | certification | 1 | 0 | 0 | 0 / 0 | 0 / 0 | 45 |
| rgn | reliability | 1 | 64 | 0 | 0 / 0 | 0 / 0 | 91 |
| rgn | jump | 2 | 0 | 1 | 0 / 0 | 0 / 0 | 1 |
| rgn | lifted-covers | 1 | 0 | 0 | 0 / 0 | 0 / 0 | 45 |
| rgn | mixed-propagation | 1 | 0 | 0 | 0 / 0 | 20 / 26456 | 45 |
| rgn | combined | 2 | 64 | 1 | 0 / 0 | 20 / 23548 | 1 |
| p01 | certification | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| p01 | reliability | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| p01 | jump | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| p01 | lifted-covers | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| p01 | mixed-propagation | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |
| p01 | combined | 0 | 0 | 0 | 0 / 0 | 0 / 0 | 1 |

Root separation metadata reports initial LP objectives/bounds, cuts and prepass work. The bounded internal search trace begins after preprocessing; its node/pivot coordinates exclude prepasses. Total SolveResult counts include them. Compare search trace terminal work plus root_separation_nodes/pivots to total result work.

## Correctness evidence

The input and adjudication SHA-256 values remain unchanged. Every full 6330-case policy file retains the same 19 raw disputed-reference flags; those flags are resolved by the existing adjudications. No new adjudication or tolerance change was introduced.

| Policy | Cases | Raw flags | Failures after existing adjudication |
| --- | ---: | ---: | ---: |
| default | 6,330 | 19 | 0 |
| ft | 6,330 | 19 | 0 |
| no-fix | 6,330 | 19 | 0 |
| devex | 6,330 | 19 | 0 |
| root-cuts | 6,330 | 19 | 0 |
| factor-replay | 6,330 | 19 | 0 |
| row-pricing | 6,330 | 19 | 0 |
| reliability | 6,330 | 19 | 0 |
| jump | 6,330 | 19 | 0 |
| directed | 6,330 | 19 | 0 |
| lifted | 6,330 | 19 | 0 |
| mixed | 6,330 | 19 | 0 |
| combined | 6,330 | 19 | 0 |
| refactor1 | 1,200 | 0 | 0 |
| workers4 | 180 | 0 | 0 |

New targeted checks include exact signed arithmetic and extreme values, independent machine-integer division/product oracles, nonrepresentable rational equalities, incompatible numerical equalities, bounded lookahead/parent restoration, sparse infeasible-seed repair, cut-profit enumeration and full binary cut validity, mixed feasible-grid preservation and sibling restoration, combined-policy small-MIP enumeration, and root-prepass budget accounting.

See PLAN.md for the initial design and execution log, INITIAL-PLAN.md for the original plan, REPRODUCE.md for commands, CHANGESET.patch for review, and data/ for raw observations, hashes and audit outputs.

## Findings and practical limits

Default certification reduces egout from 66553 to 12602 nodes and turns rgn from a 100000-node cap into an optimal numerical solve at 7249 nodes. Reliability branching reduces gt2 to 16056 nodes from 72182. Mixed propagation reduces flugpl to 425 nodes and solves bell5 below its earlier node cap, at substantially higher per-node cost.

On gt2, FJ and mixed propagation can worsen search; the combined policy reaches the node cap. The FJ prototype alone finds no p0548 incumbent under its default work cap. The combined policy does find a feasible p0548 incumbent at objective 20592 through tree search, but remains capped. Optional mechanisms therefore remain independently selectable, rather than being enabled as a universal preset.

## LP performance controls

Five measured fresh processes per binary/case after one discarded round; each process contains one warmup and one measured solve. All LP controls pass. Medians are close, with a retained 159.599 ms native network outlier; no uniform LP speedup is claimed.

| Fixture | Binary | Median ms | Range ms |
| --- | --- | ---: | ---: |
| packing 4096 | baseline | 229.485 | 218.114–253.015 |
| packing 4096 | native | 224.249 | 214.924–235.049 |
| network 2048 | baseline | 74.237 | 72.176–87.781 |
| network 2048 | native | 75.167 | 72.962–159.599 |
| aspect-wide 512 | baseline | 10.214 | 9.738–10.395 |
| aspect-wide 512 | native | 9.708 | 9.469–11.710 |
