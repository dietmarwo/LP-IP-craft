# Native 0.26 implementation and API

`crates/native/src/packing.rs` owns all five changes. NativeOptions validation/metadata and the Python dataclass/parser expose identical settings. The core and native crates still have no third-party numerical dependencies. The two benchmark input tables retain their existing ESA/GPL attribution in `benchmarks/spoc4-025/DATA-SOURCES.md`.

## Controls

The existing packing driver is enabled by `packing_root`, a positive `packing_neighborhoods`, or `packing_cuts`. The new controls configure that driver; they do not independently activate it for unsupported models. Existing unit-packing recognition and general-model fallback remain unchanged.

| Control | Default | Contract |
|---|---|---|
| `packing_combine_repairs` | `False` | Merge improving atomic deltas from a batch snapshot; retain the best standalone fallback. |
| `packing_rounds` | `1` | 1–32 rounds; neighborhoods × rounds must be ≤4096. |
| `packing_neighborhood_growth` | `0` | Additional column capacity per round, 0–1024; effective capacity saturates at 1024. |
| `packing_seed_strategy` | `"incumbent"` | `incumbent`, `rejected`, `mixed`. Mixed alternates incumbent and rejected seeds. |
| `packing_residual_cliques` | `False` | Insert bounded local clique cuts before each residual solve. |
| `packing_clique_work` | `50000` | Per-task adjacency/probe/verification operation cap, 0–1000000. |
| `packing_price_diagnostics` | `False` | Expose finite best-bound resource prices as an opt-in comma-separated diagnostic vector in input model row order. |
| `packing_repair_seconds` | `None` | Finite nonnegative wall-time cap for repair phase; zero skips repairs. |
| `packing_proof_seconds` | `None` | Finite nonnegative cap for subsequent exact root/tree search; zero skips proof work. |

Both time caps are cooperative and additionally constrained by the whole-call deadline. They do not reserve the requested duration: a phase may finish earlier. Root work/iterations retain separate existing caps. Neither local clique work nor first-order iterations count as simplex pivots. Numerical statuses retain the existing solver contract.

## Repair merging

Every batch has up to sixteen tasks and an immutable snapshot, independent of worker count. Results are ordered by task index. Each reconstructed candidate passes the original-model check and conservative incumbent admission. A delta includes every changed column. Overlapping changed columns are rejected; distinct deltas are also rejected when newly selected transfers collide with retained resource owners after that delta's removals. Updates are atomic and never change the input snapshot.

The final merged vector receives another original-model check. Best standalone candidates are retained throughout, so combination cannot worsen the best result from that batch. This is a conservative greedy combination, not an optimal independent-set selection among all possible repair deltas. Whole-repair overlap rejection may miss useful combinations.

## Rounds and seed selection

All rounds use one cumulative node/pivot budget. Shares are based on the number of remaining planned tasks, and each private task additionally retains the existing per-task limits. Batch snapshots refresh after ordered merge. Round boundaries refresh rejected-seed rankings and optionally increase neighborhood capacity. A stalled round does not suppress planned later attempts.

A rejected seed's distinct incumbent blockers are released first. Fixed-one blockers make it ineligible. The seed and all released incumbent columns survive truncation. Other candidates enter only when their resources are unoccupied outside the released set. This guarantees a feasible residual warm start and correct local-to-original reconstruction.

Rejected seeds are ranked by weight minus the total weight of their distinct incumbent blockers, then weight minus incident root prices, fractional hint and original index. Improving seeds persist to the next round if still eligible. Rankings are heuristic; price residuals never justify pruning. Rankings are refreshed per round, so a seed can become stale between batches; a now-selected seed safely produces no residual. There is no live shared mutable search state.

## Local cliques

Only residuals of at most 1024 columns build a dense local Boolean conflict graph. Edge discovery is bounded, and missing edges from an exhausted cap only lose cuts. Deterministic greedy extensions produce up to 64 cliques of up to 64 columns. Each retained clique is rechecked pairwise against the original residual resource incidence. Existing resource-row cliques and duplicates are omitted.

A retained row is `sum x_j <= 1`, for binary pairwise-conflicting columns. Fixed bounds and residual column mappings remain unchanged. The separator does not require an approximate fractional hint. This is a bounded greedy separator, not complete maximal-clique enumeration. Cut building may use its budget before finding a useful clique. Local bounds never become global bounds.

## Proof work and diagnostics

After repairs and optional existing global triangle/odd-cycle setup, exact search receives the minimum of its phase cap and remaining whole-call time. Residual nodes/pivots are subtracted from global limits. Private children disable packing recursion and nested pools. A zero proof cap returns a checked incumbent and the previously checked bound with a `time_limit` termination reason and zero proof nodes/pivots; it does not imply the whole-call wall budget was exhausted.

`diagnostics.packing_*` adds rounds, maximum neighborhood capacity, merge accepted/rejected counts, combined batches, incumbent/rejected/retained seeds, local clique count/work and proof nodes/pivots/status. `packing_round_trajectory` contains semicolon-separated records:

`round,objective,cumulative_tasks,cumulative_nodes,cumulative_pivots,column_cap,repair_elapsed_seconds`

Round indices start at zero; objectives use model units (integer thousandths in the benchmarks). The trajectory records the best incumbent at round completion. Total `nodes`/`iterations` include every residual and proof solve. `packing_clique_seconds` is the sum of task cut times and may exceed parallel wall time; `packing_neighborhood_seconds` is repair-phase wall time. Existing root/greedy/global-cut timings remain separate; `packing_proof_seconds` measures the exact-search phase including its setup. The root-price diagnostic corresponds to the best independently checked price bound, which may have been lattice-rounded.

## Known boundaries

Fixed-work runs have deterministic ordered results across one/four/sixteen workers; wall-time truncation can change which tasks finish. Tasks still copy compact residuals and reconstruct full candidates; no general dynamic shared tree or live incumbent broadcasting is added. Stronger cuts and larger neighborhoods can cost more without improving incumbents. Global exact roots remain the existing simplex implementation. No integer optimality proof on either full warmup is established by a repair result or approximate root point.
