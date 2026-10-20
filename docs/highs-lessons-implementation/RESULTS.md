# LP-IP-craft 0.22: executed HiGHS-lessons plan

The ten-stage plan has been implemented and tested. Production numerics remain
owned safe Rust with no HiGHS, BLAS, C/C++ or external numerical dependency.
HiGHS 1.15.0 is an isolated comparison backend, never linked into the release.
Baseline numerical source: 0.21.0, commit
`5e5665e2bda9d0e6ca71b68d4dccf059cd925320`.

The strongest gains are on degree-nine packing, singleton-heavy presolve,
25fv47, and independent-job throughput. Improvements are workload dependent;
ordinary packing and several small jobs have retained regressions. Real MIP
search remains substantially weaker than HiGHS.

## Executed work

| Stage | Implementation | Validation / default decision |
|---|---|---|
| 1. Attribute the gap | Matched serial-dual/presolve controls, engine iterations, genuine 4:1 and 1:4 aspect fixtures | Same fingerprints, planted objectives and original-model KKT; do not confuse square `packing-wide` with a wide matrix |
| 2. Carry sparse support | Indexed FTRAN output through triangular solves and eta updates; touched work clearing/scattering | Alternating orientations, dense/sparse RHS, cancellation and updates; dense-factor fallback retains a contiguous scan |
| 3. Broaden FT | Bounded general-L eligibility with observed update-work and fill gates | Both-orientation residuals, dense-tail rejection and auto/eta/forced-FT controls; dense tails stay outside auto FT |
| 4. Pivot replay | Exact assembled-basis signature plus numerical per-pivot validation and normal-search fallback | Identical/changed/singular/capped bases tested; opt-in, not reuse across arbitrary related MIP bases |
| 5. Pricing | Compare row work with column-file work for dense BTRAN support | Row/column alpha equivalence including logical columns; sparse row path retained; cheap eligibility checks precede work estimation |
| 6. Pivot efficiency | Optional approximate Devex weights, bounded resets; phase-one/degeneracy/weight counters; avoid dominant-component decomposition | Exact DSE remains default; Devex can reduce work or increase pivots; 25fv47 benefits mainly from monolithic solving plus root reductions |
| 7. Presolve/postsolve | Exact-quotient singleton reductions with typed bound-owner history and original-row dual reconstruction | Both senses, negative coefficients, competing owners, subnormals, inexact quotients, zero budgets; original-model KKT after postsolve |
| 8. Root bounds | Outward activity propagation for mixed MIPs, explicit infinities and directed division, before finite-box admission | Dyadic feasible-point enumeration, overflow/free domains, both senses; never invent a big-M |
| 9. Root covers | At most eight exact positive-integer binary cover cuts before any basis/cache exists | Independent witness rescan and exhaustive binary feasible-point/optimality checks; cuts are immutable and global; opt-in |
| 10. Job parallelism | Ordered Rust/Python batch pool, 1–16 workers, private solver state, shared cancellation | Empty/invalid/mixed-error/cancelled jobs and sequential equivalence; nested component workers disabled |

The full plan and decisions made before editing each subsystem are in
[PLAN.md](PLAN.md). Code is in `crates/native/src/{lu,lp,root,batch}.rs`, with
Python conversion/bindings and regression tests alongside it.

## Matched single-instance measurements

Milliseconds below are medians of three fresh-process rounds, after one
discarded round. Each process also warms its solve once. Order rotates/reverses;
models/fingerprints, limits and independent checks match. One numerical thread;
standard-suite component-worker controls are separately labelled. This is a
shared host, not a dedicated benchmarking machine. Tiny differences below a
millisecond are especially sensitive to overhead and noise. Table batches are
separate: do not combine their timings as an ablation study.

| Fixture | 0.21 ms | 0.22 ms | HiGHS ms |
|---|---:|---:|---:|
| Degree-nine square packing 256 | 18.202 | 12.810 | 10.611 |
| Degree-nine square packing 1024 | 363.038 | 311.392 | 201.585 |
| Ordinary packing 4096 | 234.764 | 246.464 | 175.052 |
| Network 2048 | 73.129 | 72.506 | 97.240 |
| Irregular 1024 | 128.270 | 133.589 | 188.521 |
| Irregular 4096 | 5,050.577 | 4,911.810 | 9,657.843 |
| True wide: 512 rows, 2048 columns | 11.418 | 10.701 | 9.631 |
| True tall: 2048 rows, 512 columns | 14.057 | 12.878 | 9.441 |
| Singleton LP: 2048 columns, 6144 rows | 173.063 | 0.472 | 4.471 |

The singleton fixture is deliberately reduction-heavy: its independent planted
optimum is x=1. It demonstrates presolve effectiveness, not general LP dominance.
The inferred-domain fixture starts with free integer x and y>=0; three rows
prove a finite box and optimum 5 per block. Original 0.21 reports Unsupported;
0.22 and HiGHS solve it. All generated fixture checks use original units.

## Public-model capability and search strength

All seven public LPs solve and pass the independent checks. Three previously
unsupported public MIPs now pass finite-domain admission after sound inference.
Two solve, while bell5 reaches the unchanged 100,000-node cap.

| Model | 0.21 | 0.22 ms / nodes | HiGHS ms / nodes |
|---|---|---:|---:|
| 25fv47 | 371.095 ms | 171.451 / 0 | 118.847 / 0 |
| e226 | 6.278 ms | 4.503 / 0 | 6.617 / 0 |
| israel | 3.209 ms | 2.084 / 0 | 3.241 / 0 |
| etamacro | 8.821 ms | 9.301 / 0 | 8.392 / 0 |
| egout | Unsupported | 2,786.146 / 66,553 | 14.889 / 1 |
| flugpl | Unsupported | 243.749 / 23,947 | 98.106 / 72 |
| bell5 | Unsupported | 3,324.537 / 100,000 (node cap) | 527.939 / 1,042 |

On 25fv47, compare pivot counts in the raw data: the original decomposed
0.21 solve takes 6662 pivots; final native/HiGHS counts are recorded alongside
all timing samples in `data/final-public.json`. A stage-one monolithic
0.21 control already took 3093 pivots. Therefore most of this iteration reduction
comes from the component/presolve strategy; it is not evidence that new FT or
pricing alone closed the algorithmic gap. HiGHS remains faster on this LP.

Egout/flugpl now reveal the next MIP problem directly: far more search nodes.
LP-kernel speed alone will not close that difference. Bell5 is explicitly a
failed solve under the common cap, not a claimed optimal result.

## Policy tradeoffs

Exact DSE remains the default. The optional weight policy avoids extra exact
edge-weight work, but can pay for it with additional pivots.

| Fixture | DSE ms / pivots | Devex-style ms / pivots |
|---|---:|---:|
| packing-wide 1024 | 330.647 / 1790 | 376.935 / 2426 |
| irregular 1024 | 122.378 / 1025 | 105.380 / 1028 |
| aspect-tall 512 | 11.404 / 512 | 10.508 / 512 |
| 25fv47  | 168.302 / 3100 | 190.219 / 3717 |

Root cover cuts are also opt-in. They help the long-row-112 case, but make
long-row-64 worse and add overhead without reducing nodes on the sampled
multi-knapsacks. This first implementation does not select covers using a solved
LP violation vector, lift them, or provide general MIR/GMI separation.

| Fixture | Cuts off ms / nodes | Cuts on ms / nodes |
|---|---:|---:|
| long-row-mip 64 | 0.341 / 16 | 0.542 / 23 |
| long-row-mip 112 | 1.150 / 50 | 0.616 / 20 |
| multi-knapsack 18 | 0.288 / 22 | 0.389 / 22 |
| multi-knapsack 5018 | 0.283 / 24 | 0.363 / 24 |

Pivot replay remains off by default: the cache is useful only for a repeated
identical assembled basis, with each numerical pivot checked afresh. Related
node bases do not automatically hit it. The implementation always performs
numerical elimination and retains normal Markowitz fallback.

## Independent-job throughput

32 copies of a planted 256-variable degree-nine packing model, one numerical
thread/private state per job, three measured rounds. Model construction and
external validation are excluded; pool creation and result conversion remain
in wall time. These are concurrent independent solves, not a speedup of one LP.

| Workers | Batch seconds | Jobs/second | Speedup vs one worker |
|---:|---:|---:|---:|
| 1 | 0.854 | 37.5 | 1.00× |
| 2 | 0.478 | 66.9 | 1.78× |
| 4 | 0.245 | 130.8 | 3.49× |
| 8 | 0.159 | 201.7 | 5.38× |
| 16 | 0.155 | 206.0 | 5.50× |

Use `solve_batch(models, workers=4, backend_options=NativeOptions(...))` in
Python, or `NativeSolver::solve_batch` in Rust. Results preserve input order;
a failed job produces its own error without discarding successful jobs. Limits
are per job. Shared cancellation affects running and queued jobs. No starts are
supplied by this first batch API.

## Correctness and provenance

- **129 Rust checks** and **175 Python tests** pass; formatting and strict all-target/all-feature Clippy pass.
- **45,690 frozen oracle runs** pass after unchanged adjudications: default/FT/no-fixing, refactor-every-pivot exact cases, component workers, Devex, root cuts, replay and row-only pricing.
- The original 19 disputed oracle expectations remain raw flags in each full pass; no adjudication or tolerance was changed to hide them.
- **400 exact bound-audit models**, zero unsafe finite bounds; existing prepared-bound exact-state tests remain enabled.
- **84 frozen basis replay observations**, worst backward error **2.17e-12**. Factorization and solve timing are separate in the raw data.
- Standard, extended, old holdout, genuinely fresh seeds, true aspect-ratio, large, public and root-capability comparisons retain failures and regressions. Binary/model/corpus fingerprints are recorded.

The raw files it refers to (`data/audit.json` and the rest of `data/`) are not part of this repository.
`summarize_standard.py` deterministically reconstructs standard summaries from
the retained raw records. The early Devex-labelled trial did not set Devex;
that file is explicitly retained as `rejected-harness-*`. Corrected policy runs
assert effective options. Replay was already wired and was rechecked too.

The final density experiments are retained in `density-fix-comparison.json`
and `ft-density-comparison.json`. Neither gave a consistent controlled packing
improvement without regressions, so both were reverted. The accepted production
code is the same numerical implementation used by the final benchmark files.

## Remaining work

1. **General MIP search strength.** LP-directed/lifted cuts, stronger mixed-row
propagation, reliable branching, conflict/clique reasoning and better incumbent
heuristics are needed for public MIPs. This release supplies only restricted
root covers and root inference, not the HiGHS branch-and-cut architecture.
2. **Broad-support forward solves.** Sparse support propagation helps suitable
bases, but ordinary large packing still exposes costly triangular solves and
vector traversal. Dense tails and bounded fallback paths remain; not every
whole-vector loop is removed. Compare profile categories carefully: the new
`edge_weights` timer moves work previously included in `pivot_other`.
3. **General presolve and application coverage.** Singleton postsolve is a
sound first reversible reduction; doubleton/general substitutions, parallel
rows/columns and general dual maps remain future work. Synthetic families and
ten public models do not replace a captured application workload.

MIPs still need finite bounds of magnitude <=2^53-1 **after** justified root
inference. Unresolved infinities remain Unsupported. Parallel simplex, PAMI,
shared-tree MIP, custom work stealing, IPM and PDLP are not implemented here.
The next tuning pass should use a captured application instance when available.
