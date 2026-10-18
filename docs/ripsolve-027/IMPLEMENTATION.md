# Native 0.27 implementation and controls

Baseline is GitHub main 825ee42679c3892310d8658d2a71678b88cb6dd3. New algorithms are opt-in. Existing packing dispatch, generic defaults and all production numerical dependencies remain unchanged. `certified_cuts.rs`, `mip_presolve.rs`, `rootcuts.rs` and `primal.rs` contain the new numerical mechanisms, and `dyadic.rs` (added in review) their exact arithmetic; `root.rs` integrates them into preprocessing/separation. `lp.rs` exposes advisory BTRAN rows and retained combinatorial basis extension. Rust/Python options and metadata mirror one another.

| Control | Default | Meaning |
|---|---:|---|
| `mip_presolve` | false | Bounded root fixpoint using existing exact integer and outward mixed propagation; box-proven redundant rows are freed without renumbering. |
| `coefficient_tightening` | false | Binary big-M reduction, with an exact i128 fast path and an exact dyadic fallback (Rat in 0.27.0). |
| `dual_presolve` | false | Optimum-preserving empty/singleton objective fixes; never treat this as feasible-set equality. |
| `mip_presolve_rounds` | 8 | 0–32 bounded rounds. |
| `gmi_cuts` | false | Exact reaggregation and GMI from advisory optimal-basis rows. |
| `mir_cuts` | false | Exact MIR, including certified variable-upper-bound substitution. |
| `mip_cut_rounds` | 8 | 0–50 root cut rounds, followed by the final resolved relaxation. |
| `mip_cut_limit` | 16 | 1–64 selected cuts per round. |
| `cut_exact_work` | 4,000,000 | At most 100,000,000 charged limb operations per separation call (each inline 128-bit operation charges one since the review); also caps exact coefficient-tightening work across presolve. A final bounded arithmetic operation can cross the limit before the next checkpoint. |
| `root_heuristics` | false | Checked rounding, guided diving and genuine L1 feasibility-pump projections after the first checked root relaxation. |
| `heuristic_lp_calls` | 16 | 0–128 heuristic LP calls, additionally inside parent limits. |
| `branching_score` | weighted_sum | weighted_sum or product. Product uses a monotone square-root product to avoid overflow. |
| `pseudocost_min_step` | 0 | Finite 0–0.5 normalization floor for tiny branching steps. Zero retains the old calculation. |
| `branch_trace_limit` | 0 | 0–256 observations; each records the selected variable and up to fifteen other fractional columns with directional history/estimates. |

Use these controls independently. A combined policy need not dominate MIR-only or presolve-only; the results distinguish them. Enabling GMI and MIR splits the exact-work allowance so GMI cannot consume the whole allowance before MIR starts. Cut efficacy still decides which valid rows are inserted. Three stagnant rounds (less than 0.1% objective change) stop the root loop. With at most fifty rounds and sixty-four rows per round, the pool is bounded at 3,200 new rows. There is no cut aging. Since the review, cut rows strictly slack at the final accepted root optimum are removed before the search (`diagnostics.root_cuts_pruned`); this leaves the root LP value unchanged.

All root LPs consume global node/pivot/time limits. Heuristic LPs use the same budget; their objectives and infeasibility statuses never prune the original model. The heuristic parent bounds/basis are restored and checked before separation continues. Pump projections contain actual absolute-deviation columns for general integers, rather than labeling simple rounding a pump. Cycle detection and bounded perturbations stop repeated failed targets.

`diagnostics.root_stages` records checked presolved/root-round objective, conservative bound, elapsed LP time, row count and cumulative pivots. Exact separation work is counted separately; separation time is included in whole-call wall time and has no dedicated timer. Presolve keys count rounds, bound changes, redundant-row removals, forcing observations, coefficient-row rewrites, dual fixes and rational limb work. A coefficient-row rewrite may tighten several terms; forcing observations can recur across rounds. GMI/MIR counters, recognized VUBs, rejected proposals, basis reuse and heuristic calls/candidates/cycles/failures are also exposed. `diagnostics.branch_trace` is advisory data and is bounded independently of the existing MIP progress trace.

The root basis extension retains the combinatorial column partition and nonbasic bound states, adds logical basic columns and refactorizes the enlarged numerical system. It is not a Forrest–Tomlin update across changing matrix dimensions. All original row/column mappings and postsolve checks remain in force. Root-stage conservative bounds are retained on limits; a limited child search cannot turn an open tree into Optimal.

The native pseudocost implementation already used directional global means with two-observation smoothing. The new experiments therefore test scoring and the missing minimum-step normalization, rather than claiming to add a global prior that was already present. Cross-solver branching traces can differ because the LP points/bases and presolve differ; the native scoring fixture holds candidate values/history fixed to isolate the policy change.

Reference code is benchmark-only. `benchmarks/rust-solvers/` contains the common-array native/ripsolve executable, separate Python baseline/HiGHS driver, cut-family and options ablation driver, frozen held-out inputs and notices. Its reference tree is pinned to ripsolve 0.4.0; the exact manifest/RS_OFF adaptation diff is included. No reference solver is called from native production code. See VALIDITY.md for the arithmetic arguments and RESULTS.md for performance limitations.
