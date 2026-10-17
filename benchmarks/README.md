# Reproduce the performance checks

The production workspace (root Cargo.toml) has no HiGHS dependency. This separate benchmark workspace adds only serde_json. An optional isolated reference workspace can reconstruct the old HiGHS adapter; it is never built by the production workspace.

From the project root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --manifest-path benchmarks/Cargo.toml
benchmarks/target/release/performance-native packing 4096 warm 1 3
```

Arguments are fixture, size (0 for legacy/import fixtures), warm/cold node reoptimization, component worker cap, and measured repetitions. Every process warms up once. Construction, fingerprinting and independent checks are excluded from the solve timer. Results include objective, feasibility/KKT acceptance, nodes, pivots, phase times and effective options. `CARGO_TARGET_DIR`, when set, changes the binary location.

Fixtures: fixed, pivot, knapsack, partition, nodecopy, redundant, blocks; packing, network, irregular, selection; box-lp, sparse-import-lp, transport-lp, partition-mip. The final comparison uses all previously collected sizes, warm/cold selection and separately labelled parallel components. The 4096-row irregular probe is separately labelled because it has only one retained timing.

Optional old references (requires network plus HiGHS's C++ build prerequisites):

```sh
python benchmarks/reference/bootstrap_sources.py
cargo build --release --manifest-path benchmarks/reference/harness/Cargo.toml
python benchmarks/compare_three.py \
  --native benchmarks/target/release/performance-native \
  --baseline benchmarks/reference/harness/target/release/comparison-native \
  --highs benchmarks/reference/harness/target/release/comparison-highs \
  --output comparison.json --rounds 5
```

The bootstrap pins upstream native 0.16 and the pre-native git revision, applies the exact delivered 0.4 adapter patch, and verifies all old backend code hashes. HiGHS version 1.15.0 is pinned by that reference Cargo.lock. Do not compare unpinned system installations.

For Python validation, install a release build using `maturin develop --release` in a virtual environment with pytest and NumPy, then `python -m pytest -q`. The frozen oracle input and adjudications are in `docs/performance-ten/data`. Run `oracle_worker.py DATASET native OUTPUT`; variants are default, dantzig, refactor1, refactor8 and workers4. Original SciPy status-4 cases and one mistaken MIP classification are preserved in raw inputs: apply `oracle-adjudication.json` when interpreting status comparisons. These adjudications are numerical evidence, not exact certificates. Exact rational/enumeration fixtures are labelled separately in the input.

`measure.py` reproduces a development iteration comparison when `binaries/iteration-N` and its preceding binary are present next to the script. Historical changes are supplied as ten ordered patches in the ZIP's `history` folder. Final results are in `docs/performance-ten/RESULTS.md`; all regressions and unsuccessful tuning attempts remain visible in the raw evidence.

Run `python benchmarks/audit.py` to re-audit the bundled raw oracle and ablation records against the preserved adjudications. `analyze_results.py` recomputes the summary tables from that data. This is a read/reanalysis of the saved solver outcomes, not a new solve.

## Kernel profiling and basis-update comparisons (0.18)

The optional sixth/seventh positional arguments select `auto`, `eta`, or `ft`,
and the maximum refactor interval (default 64):

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml
benchmarks/target/release/performance-native packing 4096 warm 1 5 auto 64
cargo build --release --manifest-path benchmarks/Cargo.toml --features kernel-profile
benchmarks/target/release/performance-native packing 4096 warm 1 5 ft 64
```

The feature adds `kernel_profile`: triples of name, exclusive nanoseconds and
call count. Nested timed operations are subtracted from their parent. Profile
on one solving thread; worker-thread timings are not aggregated. Call
`lp_ip_craft_native::profile::reset()` before a solve and `snapshot()` afterwards
when using your own harness. Normal builds compile out all kernel clocks.
Use uninstrumented builds for reported performance. `LPIP_TRACE_FACTORS=1`
with this feature prints basis dimension, L/U nonzeros and previous average
update support to stderr at refactorization.

`oracle_worker.py --variant ft` forces Forrest–Tomlin even for tiny/dense bases;
`--variant eta` exercises the retained product-form implementation. Automatic
selection is deterministic and uses observed update support and factor fill,
not benchmark names or measured wall-clock timings. See `docs/basis-updates`.

## Irregular LU work (0.19)

Capture is opt-in and absent from normal builds:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --features basis-replay
LPIP_CAPTURE_DIR=corpus/irregular-4096 LPIP_CAPTURE_AT=0,1,16,32,48,64,66 \
  benchmarks/target/release/performance-native irregular 4096 warm 1 1
benchmarks/target/release/basis-replay corpus/irregular-4096/basis-0066.bin 3 4 natural
python benchmarks/replay_compare.py --binary new=benchmarks/target/release/basis-replay \
  --corpus docs/irregular-lu/corpus --output replay.json --rounds 3
```

The binary basis files include matrix values and recent pricing/dual vectors,
plus deterministic sparse and dense controls. Capture each fixture in a fresh
directory/process. Replay validates normalized backward errors in both orientations;
loading and independent residual calculations are outside the timers. The retained
corpus is from 0.18, so matrix inputs do not change with new pivot trajectories.

Extra performance-native arguments after the earlier fixture/size/warm/workers/repeats:
`basis_method interval search ordering rebuild_mode` (e.g. `auto 256 4 auto adaptive`).
`basis_method` is auto/eta/ft, `ordering` auto/natural/min_degree, and rebuild_mode
adaptive/fixed. The configured interval is a hard maximum. Defaults use 256 with
work-based dense rebuilding; sparse/small bases rebuild within 64 updates.
Force `adaptive_refactor=False` in Python for interval ablations. All entry caps,
pivot tests and original-model acceptance checks remain enabled.

`factor_ordering="min_degree"` enables a project-owned bounded merged-row
minimum-degree heuristic; it is not the COLAMD reference implementation. Dense
rows and a symbolic-work cap limit ordering overhead. Auto uses it only on large
bases after substantial dense fill was observed. `factor_pivot_search` accepts
1..64; the default remains 4 after the measured search-budget sweep.

## Packing, search and coverage (0.20)

Detailed pre-implementation plan: `docs/packing-mip/PLAN.md`. New fixtures:
packing-wide/permuted, long-row-mip, integer-knapsack, multi-knapsack,
graph-mip and mixed-overload. Binary tags encode seed variants: e.g. 1018 means
18 variables with a separate seed. Reference objectives use exhaustive binary
enumeration, capacity DP or analytic elimination of the continuous overload.

```sh
python benchmarks/build_packing_references.py --baseline-repo /path/to/0.19 \
  --highs-repo benchmarks/reference/highs-0.4.0/LP-IP-craft --output /tmp/lpip-refs
python benchmarks/compare_packing_mip.py \
  --binary baseline=/path/to/packing-baseline --binary native=/path/to/performance-native \
  --binary highs=/path/to/packing-highs --output comparison.json --rounds 5
```

Add `--holdout` for the reserved seeded MIP suite, or
`--public-corpus docs/packing-mip/corpus` for all ten original public inputs.
Each comparison records every failure and unsupported input, including warmups;
one process round is discarded. Do not infer coverage-wide performance from
accepted-only timing averages. The twelfth optional harness argument `strong`
enables root lookahead; default off because the measured overhead did not pay.
All probe relaxations count as nodes, and all their pivots count against the
whole-call limit. `diagnostics.strong_branch_probes/pivots` separate probe work.

The thirteenth optional harness argument `no_fix` disables conservative reduced-
cost fixing. `oracle_worker.py --no-fixing` supplies the same ablation for frozen
oracle cases. After generating the final raw files and copying the test logs,
`python benchmarks/audit_packing_mip.py` checks unchanged adjudications, every
acceptance count, safe bounds, corpus checksums and basis replay. Run
`python benchmarks/report_packing_mip.py` to regenerate the complete tables.
