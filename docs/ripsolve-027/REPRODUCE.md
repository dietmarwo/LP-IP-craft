# Reproduction

Rust 1.85+ builds the production crates; the pinned benchmark reference may need a newer compiler. Recorded builds use Rust 1.99.1. Python 3.10+ is supported by the wrapper.

```bash
python -m venv .venv
. .venv/bin/activate
python -m pip install maturin pytest numpy scipy highspy==1.15.1
cargo test --workspace --locked
cargo test -p lp-ip-craft -p lp-ip-craft-native --all-targets --all-features --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
maturin develop --release --locked
python -m pytest -q
cargo build --release --manifest-path benchmarks/rust-solvers/Cargo.toml --locked
```

The complete archive includes the frozen 6,330-case inputs and unchanged adjudications. Their SHA256 values are checked by the audit, not regenerated. Raw gate files are under `docs/ripsolve-027/data`.

```bash
python benchmarks/oracle_worker.py docs/performance-ten/data/oracle-inputs.json native docs/ripsolve-027/data/oracle-default.json
python benchmarks/oracle_worker.py docs/performance-ten/data/oracle-inputs.json native docs/ripsolve-027/data/oracle-combined.json --native-options '{"mip_presolve":true,"coefficient_tightening":true,"dual_presolve":true,"gmi_cuts":true,"mir_cuts":true,"mip_cut_rounds":4,"root_heuristics":true,"heuristic_lp_calls":8,"branching_score":"product","pseudocost_min_step":0.01}'
python benchmarks/rust-solvers/check_gates.py
python benchmarks/rust-solvers/check_gates.py --bounds default
python benchmarks/rust-solvers/check_gates.py --bounds combined
```

Raw oracle-worker flags include pre-existing disagreements with the original SciPy oracle. `check_gates.py` applies the frozen adjudications, and the final accepted failure count must be zero. The 400-model bound code imports the original example unchanged; only explicit backend settings are selected. Frozen oracle MIPs separately check returned global bounds against independent reference optima.

Compile and finish correctness gates before timing. The comparison driver runs fresh processes serially with identical frozen arrays. Native/HiGHS use one solver thread; ripsolve's default `threads=1` can additionally spawn background CDCL, so a separate `serial` profile disables CDCL. It normalizes JSON null bounds to their specified infinities; no arbitrary finite bound substitution is permitted. Candidate checks use the original model and objective offset/sense. A timeout or process error is retained as a result.

```bash
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --seconds 10 --policies presolve gmi mir combined heuristics branch-product branch-clamped --solvers native ripsolve highs --output docs/ripsolve-027/data/public.json
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --heldout --seconds 3 --policies presolve mir combined --solvers native ripsolve highs --output docs/ripsolve-027/data/heldout.json
python benchmarks/rust-solvers/ablate.py --binary target/release/solvercmp --seconds 5 --output docs/ripsolve-027/data/ripsolve-ablations.json
python benchmarks/rust-solvers/ablate.py --binary target/release/solvercmp --bug-only --seconds 20 --output docs/ripsolve-027/data/ripsolve-status-reproduction.json
LPIP_COMPARE_NODES=1000000 LPIP_COMPARE_PIVOTS=10000000 python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --seconds 60 --policies combined heuristics --solvers native ripsolve highs --output docs/ripsolve-027/data/public-60s.json
LPIP_COMPARE_NODES=1000000 python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --seconds 60 --policies serial --solvers ripsolve --output docs/ripsolve-027/data/ripsolve-serial.json
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --families heldout --seconds 3 --policies presolve mir combined --solvers native ripsolve highs --output docs/ripsolve-027/data/families.json
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --heldout --seconds 3 --policies default branch-product branch-clamped --solvers native --output docs/ripsolve-027/data/branch-heldout.json
python benchmarks/rust-solvers/run.py --binary target/release/solvercmp --families heldout --seconds 3 --policies default branch-product branch-clamped --solvers native --output docs/ripsolve-027/data/branch-families.json
python benchmarks/rust-solvers/plain_lp.py --output docs/ripsolve-027/data/plain-lp.json
python benchmarks/rust-solvers/summarize.py
```

For a pinned native 0.26 comparison, build/install that commit in a separate Python directory, preserve its `lp_ip_craft` package including the extension, and pass `--baseline-python /absolute/path` with `--solvers baseline native ripsolve highs`. Do not import a rebuilt extension into an already running Python process. `worker.py` selects only pre-existing options for baseline runs.

`prepare.py --highs /path/to/pinned/HiGHS` records the first twenty eligible unseen sorted MPS inputs under the predeclared dimension/parser rule, and every exclusion. The pinned check set provides eleven eligible MIPs; no claim of twenty independent public holdouts is made. Additional seeded development/held-out families are generated by `families.py` using disjoint seeds, before outcomes are inspected. They are reported separately from public MIPs.

SpOC4 uses the existing `benchmarks/spoc4_026.py` at fixed repair work (rejected seeds, eight rounds, four workers). Run both instances against the saved baseline package and the new package with explicit PYTHONPATH, then compare the original-order `.solution.npy` vectors byte for byte. General root modes stay off in those runs and are disabled in private packing children. The SpOC4 paths do not gain claims of general-MIP optimality.
