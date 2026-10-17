# LP-IP-craft 0.23 — executed packing/FTRAN and MIP diagnosis

Only continuation items 1 and 2 were implemented. Production remains owned safe Rust, with no external numerical engine. The accepted 0.22 source is the baseline. Search algorithms, defaults, tolerances, frozen adjudications and budgets are preserved; the new trace is opt-in.

## Changes

1. For FT forward solves without trailing conventional etas, reuse the captured output index list directly instead of copying it into another list, marking it, then filtering/exporting it again. FT-plus-eta and dense-tail fallbacks retain the old support-union path.
2. Seed captured FT forward reach from the already recorded spike. This removes duplicate dense RHS discovery without changing seed order, traversal thresholds, cooldowns or numerical arithmetic.
3. Separate profile costs for support, symbolic reach, workspace clearing, upper arithmetic and output transfer. Normal builds compile out these kernel clocks/counters.
4. Add bounded search observations in existing backend metadata: raw root LP objective versus conservative bound, original-model candidate acceptance versus incumbent certification, frontier progress, LP work, prune reasons and exact integer-propagation coverage. See [DIAGNOSTICS.md](DIAGNOSTICS.md).
5. Add six original public MIPs and six independently planted fresh packing holdouts, plus fresh generated MIP seeds. Retain original MPS and conversion/reference provenance.

No Feasibility Jump, reliability branching, new cut separator, mixed-node propagation or parallel search was added.

## Measurement protocol

Each ordinary comparison uses one discarded fresh-process round and three measured rounds, rotated/reversed order, one warmup per process, matched input fingerprints and independent original-model checks. One numerical thread; component controls are separately labelled. Public/default timing permits existing decomposition; the trace diagnosis explicitly disables decomposition. HiGHS is the pinned isolated 1.15.0 reference; LP controls request serial dual simplex, with normal presolve. Whole-call budgets are 30 seconds, 100000 MIP relaxations/probes and 1000000 native simplex pivots. Builds and correctness workloads are not concurrent with these measurements. This is a shared host, not exclusive hardware. Different table batches are not interchangeable ablations.

`accepted` in the harness means independently checked optimality against the reference; a limited run may retain a feasible candidate while accepted=false. Unsupported cases are capability failures, not zero-time solutions.

## Matched LP timing

| Batch | Fixture | 0.22 ms | 0.23 ms | Time change | HiGHS ms |
|---|---|---|---|---|---|
| extended | packing 1024 | 24.909 | 23.418 | -6.0% | 21.398 |
| extended | packing 2048 | 59.327 | 56.147 | -5.4% | 62.506 |
| extended | packing 4096 | 233.530 | 214.376 | -8.2% | 166.239 |
| extended | packing-wide 256 | 12.519 | 13.007 | 3.9% | 10.607 |
| extended | packing-wide 1024 | 330.443 | 325.958 | -1.4% | 201.805 |
| extended | packing-permuted 1024 | 14.061 | 13.386 | -4.8% | 13.667 |
| extended | irregular 1024 | 131.122 | 130.933 | -0.1% | 188.280 |
| standard | network 2048 | 73.062 | 82.187 | 12.5% | 99.796 |
| aspect | aspect-wide 512 | 10.083 | 9.435 | -6.4% | 9.534 |
| aspect | aspect-tall 512 | 11.608 | 11.273 | -2.9% | 8.658 |
| large | irregular 4096 | 5,099.513 | 4,959.563 | -2.7% | 9,093.497 |

Negative time change is faster. Inspect retained ranges and all smaller cases in the raw data. Gains are workload-dependent; there are retained regressions.

### Focused network repeat

The standard-suite network result warranted an additional nine-measured-round rotated comparison; the original result above is retained.

| Backend | Fixture | Median ms | Range ms |
|---|---|---|---|
| baseline | network 2048 | 77.218 | 70.894 / 83.914 |
| native | network 2048 | 73.531 | 71.044 / 89.761 |
| highs | network 2048 | 98.298 | 93.874 / 109.440 |
| baseline | packing 4096 | 249.810 | 232.243 / 276.825 |
| native | packing 4096 | 224.401 | 215.923 / 237.952 |
| highs | packing 4096 | 161.575 | 158.923 / 184.500 |
| baseline | aspect-wide 512 | 9.834 | 9.417 / 12.667 |
| native | aspect-wide 512 | 10.122 | 9.534 / 11.509 |
| highs | aspect-wide 512 | 9.957 | 9.174 / 11.246 |

### Fresh packing holdouts

| Fresh fixture | 0.22 ms | 0.23 ms | Time change |
|---|---|---|---|
| packing-d3-1024-seed20261017 | 37.025 | 37.488 | 1.3% |
| packing-d3-1024-seed20261123 | 38.932 | 36.708 | -5.7% |
| packing-d3-4096-seed20261017 | 552.407 | 543.396 | -1.6% |
| packing-d3-4096-seed20261123 | 415.419 | 447.732 | 7.8% |
| packing-d9-1024-seed20261017 | 178.250 | 188.159 | 5.6% |
| packing-d9-1024-seed20261123 | 163.342 | 166.346 | 1.8% |

### Fresh-holdout follow-up under default decomposition

The initial three-round results above are retained. Their regressions prompted this independent five-round rerun with the original default decomposition settings. Do not pool medians from the two batches.

| Fresh fixture | 0.22 repeat ms | 0.23 repeat ms | Time change |
|---|---|---|---|
| packing-d3-1024-seed20261017 | 36.802 | 36.941 | 0.4% |
| packing-d3-1024-seed20261123 | 36.198 | 35.957 | -0.7% |
| packing-d3-4096-seed20261017 | 558.726 | 546.165 | -2.2% |
| packing-d3-4096-seed20261123 | 442.025 | 438.876 | -0.7% |
| packing-d9-1024-seed20261017 | 186.548 | 183.494 | -1.6% |
| packing-d9-1024-seed20261123 | 168.968 | 169.722 | 0.4% |

### Five-way FTRAN ablation and rejected third experiment

Five measured rounds after a discarded round, decomposition explicitly off for every version. `export` and `seeded` are the early 0.22-based experiment binaries; candidate 2 is the accepted 0.23 numerical source. Candidate 3 additionally used a known-seed early rejection, but did not improve whole-solve times consistently and was reverted. The patch is retained in `experiments/experiment3-rejected.patch`. The degree-nine JSON model is two-component under default settings, so these monolithic times must not be compared directly to the earlier default-decomposition table.

| Fixture | Baseline | Export | Seeded | Candidate 2 | Rejected 3 |
|---|---|---|---|---|---|
| packing 4096 | 247.007 | 220.140 | 218.555 | 216.694 | 218.110 |
| network 2048 | 70.931 | 73.219 | 71.978 | 73.391 | 73.433 |
| aspect-wide 512 | 9.652 | 9.404 | 9.572 | 9.545 | 9.536 |
| packing-d3-4096-seed20261123 | 439.565 | 435.187 | 419.875 | 420.257 | 422.949 |
| packing-d3-4096-seed20261017 | 535.915 | 551.593 | 528.419 | 538.127 | 553.501 |
| packing-d9-1024-seed20261017 | 364.595 | 368.318 | 344.783 | 348.034 | 353.623 |

### Forward-solve attribution

Both profile binaries use the detailed exclusive timing categories; these instrumented costs are diagnostic and are not the uninstrumented headline timings.

| Packing4096 exclusive category | 0.22 profile ms | 0.23 profile ms |
|---|---|---|
| ftran_support | 9.690 | 0.263 |
| ftran_support_export | 9.887 | 1.642 |
| reach_ft_ftran | 14.115 | 10.245 |
| spike_capture | 10.413 | 10.857 |
| ftran_ft_upper | 93.737 | 102.027 |
| ftran_output | 5.589 | 5.766 |
| ftran_workspace_clear | 0.493 | 0.569 |

The accepted implementation still has full spike-capture scans, output copies/clears, dense tails and fallback loops. It does not make every forward solve hyper-sparse. Frozen generic-basis replay does not exercise every captured FT update path; dedicated update tests and the full oracle matrix cover those changes.

## Public models and retained failures

| Model | 0.22 ms / nodes / status | 0.23 ms / nodes / status | HiGHS ms / nodes / status |
|---|---|---|---|
| 25fv47 | 176.011 / 0 / optimal | 166.136 / 0 / optimal | 120.221 / 0 / optimal |
| adlittle | 0.547 / 0 / optimal | 0.497 / 0 / optimal | 1.365 / 0 / optimal |
| afiro | 0.069 / 0 / optimal | 0.067 / 0 / optimal | 0.422 / 0 / optimal |
| avgas | 0.039 / 0 / optimal | 0.039 / 0 / optimal | 0.282 / 0 / optimal |
| bell5 | 3,392.952 / 100,000 / node_limit | 3,259.763 / 100,000 / node_limit | 542.117 / 1,042 / optimal |
| e226 | 5.047 / 0 / optimal | 4.594 / 0 / optimal | 6.057 / 0 / optimal |
| egout | 2,738.724 / 66,553 / optimal | 2,827.526 / 66,553 / optimal | 14.243 / 1 / optimal |
| etamacro | 9.297 / 0 / optimal | 8.583 / 0 / optimal | 7.701 / 0 / optimal |
| flugpl | 230.723 / 23,947 / optimal | 237.903 / 23,947 / optimal | 92.926 / 72 / optimal |
| israel | 2.072 / 0 / optimal | 2.138 / 0 / optimal | 3.095 / 0 / optimal |
| gt2 | 2,726.206 / 72,182 / optimal | 2,694.015 / 72,182 / optimal | 37.105 / 1 / optimal |
| lseu | 542.099 / 19,747 / optimal | 550.266 / 19,747 / optimal | 213.712 / 5 / optimal |
| p01 | 0.139 / 1 / optimal | 0.134 / 1 / optimal | 5.088 / 1 / optimal |
| p0548 | 18,165.185 / 100,000 / node_limit | 18,314.703 / 100,000 / node_limit | 74.227 / 0 / optimal |
| rgn | 4,707.987 / 100,000 / node_limit | 4,875.742 / 100,000 / node_limit | 201.749 / 1 / optimal |
| small_mip | 0.040 / 1 / optimal | 0.042 / 1 / optimal | 0.270 / 0 / optimal |

## Search diagnosis

The main finding is that tolerance-feasible candidates do not always become usable pruning certificates. Egout and flugpl obtain no certified incumbent bounds and do no incumbent-bound pruning in the observed runs, despite accepted candidates. This changes the next-work priority for those mixed models: investigate certification before assuming more incumbent heuristics or branching alone will close the gap. See [DIAGNOSIS.md](DIAGNOSIS.md) for evidence and limits.

| Model | Status | Root LP objective | Root conservative bound | Fractional integers | First certificate: nodes | Exact propagation rows |
|---|---|---|---|---|---|---|
| long-row-mip | optimal | 1,754.444444 | 1,754.000000 | 1 | 1 | 1/1 |
| integer-knapsack | optimal | 2,134.566667 | 2,134.000000 | 1 | 1 | 1/1 |
| mixed-overload | optimal | 134.250000 | 134.250000 | 1 | 1 | 0/2 |
| graph-mip | optimal | 140.000000 | 140.000000 | 18 | 1 | 43/43 |
| multi-knapsack | optimal | 138.112167 | 138.000000 | 3 | 1 | 5/5 |
| small_mip | optimal | 3.236842 | 3.236842 | 0 | — | 0/5 |
| rgn | node_limit | 48.799999 | 48.799999 | 16 | — | 4/24 |
| p0548 | node_limit | 429.682902 | 430.000000 | 45 | — | 174/174 |
| p01 | optimal | 263.000000 | 263.000000 | 0 | 1 | 30/30 |
| lseu | optimal | 834.682353 | 835.000000 | 11 | 122 | 28/28 |
| gt2 | optimal | 13,460.233074 | 13,461.000000 | 11 | 35 | 25/29 |
| flugpl | optimal | 1,167,185.725592 | 1,167,185.725592 | 10 | — | 0/17 |
| egout | optimal | 367.084884 | 367.084884 | 13 | — | 0/98 |
| bell5 | node_limit | 8,608,417.946508 | 8,608,417.946508 | 25 | 7008 | 29/89 |

These are serial, decomposition-off observations, not an attribution of HiGHS's entire advantage. Root inference and integer-lattice tightening can strengthen the conservative root bound. Original-model acceptance and certification milestones are separate. Ineligible propagation rows include mixed rows; no extra inference was added. Full event traces, terminal gaps, visit/depth/LP work, truncation and prune statistics remain in `data/mip-diagnostics.json`.

### Trace overhead and work equivalence

| Model | Off ms | On ms | Overhead | Same statuses/nodes/pivots |
|---|---|---|---|---|
| bell5 | 3,288.316 | 3,511.592 | 6.8% | True |
| egout | 2,968.025 | 2,947.485 | -0.7% | True |
| flugpl | 251.118 | 237.376 | -5.5% | True |
| gt2 | 2,707.488 | 2,723.782 | 0.6% | True |
| lseu | 578.892 | 551.956 | -4.7% | True |
| p01 | 0.135 | 0.151 | 12.1% | True |
| p0548 | 18,192.739 | 18,388.326 | 1.1% | True |
| rgn | 4,801.568 | 4,742.035 | -1.2% | True |
| small_mip | 0.041 | 0.051 | 26.7% | True |
| multi-knapsack | 0.391 | 0.433 | 10.8% | True |
| graph-mip | 0.232 | 0.262 | 13.2% | True |
| mixed-overload | 0.080 | 0.105 | 31.3% | True |
| integer-knapsack | 0.178 | 0.213 | 19.2% | True |
| long-row-mip | 1.103 | 1.121 | 1.6% | True |

Overhead is measured with three fresh-process rounds after a discarded round. Tiny jobs have noisy percentages; enabled clocks, event capture and original-candidate rechecks count against wall budgets. Work equality is not expected at a wall-time cutoff.

### Existing-policy probes

| Model | Existing control | Status | Nodes | Pivots | First certified nodes |
|---|---|---|---|---|---|
| bell5 | heuristic-off | node_limit | 100000 | 106717 | 31375 |
| bell5 | presolve-off | error | — | — | — |
| bell5 | strong-branching | node_limit | 100000 | 105524 | 7008 |
| bell5 | root-covers | node_limit | 100000 | 105524 | 7008 |
| egout | heuristic-off | optimal | 66553 | 55400 | — |
| egout | presolve-off | error | — | — | — |
| egout | strong-branching | optimal | 66553 | 55398 | — |
| egout | root-covers | optimal | 66553 | 55398 | — |
| flugpl | heuristic-off | optimal | 23947 | 15622 | — |
| flugpl | presolve-off | error | — | — | — |
| flugpl | strong-branching | optimal | 23947 | 15622 | — |
| flugpl | root-covers | optimal | 23947 | 15622 | — |
| gt2 | heuristic-off | node_limit | 100000 | 190011 | 48 |
| gt2 | presolve-off | optimal | 37408 | 131644 | 191 |
| gt2 | strong-branching | optimal | 49493 | 130198 | 39 |
| gt2 | root-covers | node_limit | 100000 | 180156 | 35 |
| lseu | heuristic-off | optimal | 16348 | 40900 | 136 |
| lseu | presolve-off | optimal | 38696 | 117278 | 886 |
| lseu | strong-branching | optimal | 12951 | 31476 | 37 |
| lseu | root-covers | optimal | 19754 | 48109 | 122 |
| rgn | heuristic-off | node_limit | 100000 | 279893 | — |
| rgn | presolve-off | node_limit | 100000 | 357773 | — |
| rgn | strong-branching | node_limit | 100000 | 279852 | — |
| rgn | root-covers | node_limit | 100000 | 325493 | — |
| multi-knapsack | heuristic-off | optimal | 33 | 82 | 10 |
| multi-knapsack | presolve-off | optimal | 48 | 100 | 1 |
| multi-knapsack | strong-branching | optimal | 15 | 45 | 1 |
| multi-knapsack | root-covers | optimal | 30 | 76 | 1 |

Each probe is one fresh process with warmup and one retained observation, not a repeated timing comparison. `presolve-off` disables several reductions and can lose finite-bound admission, so it does not isolate node propagation. These probes do not justify changing any default by themselves.

## Verification

- 130 Rust checks and 189 Python tests pass; formatting and strict all-target/all-feature Clippy pass.
- 45,690 frozen oracle runs pass under unchanged adjudications. The original 19 raw disputed expectations remain visible in each full policy pass.
- 400-model exact bound audit: 0 unsafe finite bounds.
- 126 frozen basis replay observations; maximum backward error 2.17e-12. These replay generic solves on frozen bases; captured FT support changes also have dedicated update/zero/cancellation/eta-fallback tests.
- All six added public conversions match HiGHS's own original MPS reader, independently of the solver comparison on converted CSR.
- The diagnostics tests cover enabled/disabled work equality, original objective offsets and senses, cancellation, limits, capped traces, uncertified starts, mixed-row coverage, components and root preprocessing.

## Reproduction and provenance

See [REPRODUCE.md](REPRODUCE.md), the pre-implementation [PLAN.md](PLAN.md), the build manifest, unchanged input/adjudication hashes, fixture manifests and raw logs. `audit_packing_diagnostics.py` validates retained evidence; running the audit alone is not a new solve or timing measurement. All historical 0.22 evidence remains unchanged.
