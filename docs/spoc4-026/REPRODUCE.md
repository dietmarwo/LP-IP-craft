# Reproduce native 0.26

Use the source ZIP or apply its patch to `c2eb282`. Rust 1.85+ and Python 3.10+ are required. On the validation host, the existing local Rust toolchain and Python environment are reused; no external numerical library is linked into the Rust solver.

```bash
python -m venv .venv
. .venv/bin/activate
python -m pip install maturin pytest numpy scipy
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test -p lp-ip-craft -p lp-ip-craft-native --all-targets --all-features --locked
maturin develop --release --locked
python -m pytest -q
python benchmarks/spoc4_026.py --suite --seconds 15
python benchmarks/audit_spoc4_026.py
```

The numerical all-targets command avoids the optional Python cdylib's standalone test-link requirement. Compile and run correctness checks before measurements. The suite launches isolated fresh processes serially; no benchmark or compilation runs alongside another timed solve.

All suite policies use the same explicit deterministic weight-greedy start, with zero-weight columns excluded to match the native greedy seed. The original independent adapter also selects some zero-weight columns (two/six exist in I/II); retaining them can change a later heuristic trajectory despite identical starting objective. The exploratory results with that start remain under `results-adapter`, with their own recorded hashes. Use `--include-zero-start` to reproduce those. `--cold` omits the explicit start; `--seed-file FILE.npy` uses an existing original-order vector. Each record identifies its start kind and hash.

Examples:

```bash
python benchmarks/spoc4_026.py --instance i --policy rejected --output one-run.json
python benchmarks/spoc4_026.py --instance ii --policy rejected --workers 1 --output serial.json
python benchmarks/spoc4_026.py --instance ii --policy rejected --rounds 8 --tasks 64 --output longer.json
python benchmarks/spoc4_026.py --instance ii --policy mixed --root-work 1000000000 --root-iterations 2000 --output root-work.json
python benchmarks/spoc4_026.py --instance ii --policy proof --proof-seconds 5 --output proof.json
```

Whole-call time, root work/iterations, repair task count/rounds/size, worker count and proof time are explicit. With proof disabled, all fixed repair work can finish before the whole-call cap; actual elapsed time is reported. All full-table comparisons are heuristic solution-quality measurements, with separately reported conservative bounds and termination reasons.

Optional independent plain-LP reference (requires `pip install highspy`):

```bash
python benchmarks/spoc4_matching.py --input benchmarks/spoc4-025/data/matching-ii.txt --backend highs --mode lp --lp-algorithm ipm --lp-negate --seconds 120 --threads 1 --output benchmarks/spoc4-026/reference/ii-highs-ipm-120s.json
python benchmarks/audit_spoc4_026.py
```

IPM runs in minimization form with negated costs, without crossover. The optional reference is separate from the owned solver. A time-limited reference is not called an optimum. The audit recomputes any saved dyadic price bound using integer arithmetic and original model incidence. Original-resource candidate audit checks complete original-order solution vectors, integer scores and all three resource namespaces, rather than trusting solver metadata.

The 60-second failed reference is also retained. The completed 120-second-cap run takes about 83 seconds in the recorded environment. To reproduce the strongest tested repair scores, use `--policy rejected --rounds 8`; this performs 512 planned repairs and no exact proof work. Use `packing_combine_repairs=True, packing_rounds=8, packing_neighborhoods=64, packing_seed_strategy="rejected", packing_workers=4, packing_proof_seconds=0` in the Python API, with `packing_root=True` and the recorded 200-million-unit root cap. Local cliques remain off in that recipe.
