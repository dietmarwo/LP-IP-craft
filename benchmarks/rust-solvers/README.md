# Common-array solver comparisons

This standalone benchmark crate is outside the production Cargo workspace. It links native and a pinned ripsolve 0.4.0 source tree. Production numerical crates gain no dependencies. See THIRD_PARTY_NOTICES.md and the included licences.

`run.py` reads existing JSON corpus inputs and starts one fresh process per policy. `solvercmp` constructs both solvers from identical arrays. `worker.py` handles a preserved native 0.26 Python package and HiGHS 1.15.1. Every returned candidate is evaluated on the original model, including sense/offset. Null variable/row bounds remain their specified infinities. Original MPS/JSON holdouts and selection exclusions are frozen in `heldout/manifest.json`; seeded big-M/knapsack inputs use disjoint development and holdout seeds in `families/manifest.json`.

The development list has eight MIPs plus avgas, a continuous LP control. Frozen HiGHS check inputs supply eleven eligible public holdouts, including four infeasible models, one unsupported native domain and one unresolved set-partitioning instance. Do not silently exclude those cases from solved counts.

```bash
cargo build --release --manifest-path benchmarks/rust-solvers/Cargo.toml --locked
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --seconds 10 --policies presolve gmi mir combined heuristics branch-product branch-clamped --solvers native ripsolve highs --output docs/ripsolve-027/data/public.json
LPIP_COMPARE_NODES=1000000 LPIP_COMPARE_PIVOTS=10000000 python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --seconds 60 --policies combined heuristics --solvers native ripsolve highs --output docs/ripsolve-027/data/public-60s.json
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --seconds 60 --policies serial --solvers ripsolve --output docs/ripsolve-027/data/ripsolve-serial.json
python benchmarks/rust-solvers/plain_lp.py --output docs/ripsolve-027/data/plain-lp.json
python benchmarks/rust-solvers/summarize.py
```

Add `--baseline-python /absolute/preserved/package/parent` and include `baseline` in `--solvers` to compare the pinned 0.26 extension. `run.py --heldout` and `--families heldout` select frozen inputs; `--cases` filters named cases. Timed processes run serially after all builds/tests. Rust timing covers the solve call, after input parsing/model construction. Python baseline timing includes wrapper work; HiGHS timing includes its import/setup but excludes Python process startup. These are single host measurements, not precise cross-language kernel timing.

Native defaults are node cap 100,000 and pivot cap 1,000,000, with 16 cut rounds for cut profiles. Reference ripsolve retains its default unlimited node budget unless `LPIP_COMPARE_NODES` is set; HiGHS similarly accepts the node override. The final 60-second run sets a common 1,000,000-node cap where supported and a native 10,000,000-pivot cap; no equivalent global pivot cap is exposed by the reference APIs used here. Caps can stop a run before its time limit. Compare termination status as well as wall time.

Native/HiGHS use one solver thread. **ripsolve `threads=1` limits branch-and-bound workers but default CDCL can spawn a background thread.** The default reference rows retain that behavior. The explicit `serial` profile disables CDCL to enforce a single search thread and is reported separately. Thus the initial phrase “one thread” in exploratory protocol strings meant a requested option, not a verified total thread count. Source inspection corrected that assumption; no default reference output was silently replaced.

Profiles `no-presolve`, `no-cuts`, `no-restart` and `bare` are reference options experiments. `no-restart` sets the documented disable value `restart_share=0`, which reproduces wrong Optimal statuses; those outputs are quarantined. `bare` disables the listed options in main.rs, but retains some root heuristics, probing, reduced-cost fixes and restart behavior. It is **not** an exact reconstruction of the original plan's missing historical `bare_bb` binary. `RS_OFF=gomory` or `mir` uses the included optional family exits; unset preserves the published numerical path. Do not infer a single causal branching difference from unlike cross-solver LP solutions.

Reference root fields belong to the returned restart pass. For example, the p0548 MIR-off ablation reports a root field of 8724 and a final optimum/bound of 8691. Keep those fields as observations, not certified original-model bounds. The root comparison table uses independently checked native stages and the original LP reference instead.

`summarize.py` checks hashes, common models, original candidate feasibility, objective/bound consistency and known/HiGHS optima. Unsupported and limit results are listed, not accepted as solved. It writes `comparison-audit.json` and TABLES.md. Shifted geometric means use shift 0.01 seconds, charge every unproved/error case the full cap and cap successful times at that cap. `pilot.json` is exploratory evidence from before final fairness/budget fixes and is not used for headline tables.

The initial HiGHS LP adapter used its unused MIP bound field for avgas. `correct_references.py` retained that original record and replaced it with a bound computed from the LP row duals via the independent bound formula; see `reference-adapter-corrections.json`. The corrected adapter also handles objective offsets exactly once. This is a measurement correction, not a native solver correction.
