# Reproduce 0.24

Use Rust 1.85+ and Python 3.10+ with maturin, pytest, NumPy and SciPy in an isolated environment. No third-party arithmetic or solver dependency was added to the native/core crates. The benchmark workspace alone uses serde_json. The runtime used Rust 1.99.1 and Python 3.12; exact source/binary hashes and runtime details are recorded in `data/build-manifest.json`.

From the source root:

```sh
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
maturin develop --release
python -m pytest -q
python benchmarks/validate_mip_search_024.py
```

The validation script runs all nine previous policy configurations plus six new policy configurations against the same immutable 6330-case input collection (83670 solves across the policy matrix). It then performs the existing 400-model conservative-bound audit. The adjudication file and tolerances are unchanged. Raw flags remain visible; the final audit applies the established adjudications.

Build the uninstrumented benchmark executable:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml
```

Use a separately extracted accepted 0.23 delivery for the baseline. Compile its benchmark workspace and copy that executable before rebuilding the candidate. Do not substitute the current public GitHub branch for the delivered 0.23 source. The baseline ZIP SHA-256 is `52803235788233b037b3ef220af1a2d659f21f8ab297db1ff5ebf2fed02783c0`.

After correctness checks and builds finish, run timing processes sequentially:

```sh
python benchmarks/run_mip_search_024.py \
  --baseline /absolute/path/native023 --native /absolute/path/native024 \
  --output docs/mip-search-024/data/public-policies.json
python benchmarks/run_mip_search_024.py \
  --baseline /absolute/path/native023 --native /absolute/path/native024 \
  --output docs/mip-search-024/data/public-repeat.json --rounds 3 \
  --cases egout gt2 lseu rgn \
  --policies baseline-023 certification reliability combined
python benchmarks/audit_mip_search_024.py
```

Each case/policy launches a fresh process, performs one warmup solve and two measured solves, with one kernel thread, decomposition off, 30 seconds, 100000 nodes and 1000000 pivots. Repeat policy order rotates by round. All records include hashes, primal checks, bounds, nodes, pivots and bounded traces. A root separation prepass is charged to total result nodes/pivots and reported separately from the internal post-preprocessing trace.

`legacy-policy` disables rational certification and all new optional search policies on the candidate, allowing a same-harness control for unintended search changes. The other individual controls add one policy to default certification; `combined` enables all mechanisms.

The independent `benchmarks/inspect_mip_witness.py` uses Python Fraction on final egout/flugpl candidates. It is a diagnostic/reference tool, never a production dependency. It demonstrates the exact equality conflict in flugpl and independently reconstructs/checks the egout boundary witness.

The source package includes `CHANGESET.patch` against the reconstructed local baseline of the accepted 0.23 source. The old worktree git metadata referred to an expired workspace, so the local baseline has a different commit identity from the earlier delivery. Content provenance and the accepted 0.23 ZIP hash are retained.


The LP controls use the same uninstrumented binaries, after all other work finishes:

```sh
python benchmarks/compare_packing_mip.py \
  --binary baseline=/absolute/path/native023 \
  --binary native=/absolute/path/native024 \
  --case packing:4096 --case network:2048 --case aspect-wide:512 \
  --rounds 5 --output docs/mip-search-024/data/lp-controls.json
python benchmarks/audit_mip_search_024.py
python benchmarks/report_mip_search_024.py
```

The raw original public-controller protocol string predates the harness-count audit. Its actual process records contain one warmup and two measured solves; the final report and this document describe that actual protocol. The earlier paused controller log and resumed log are both retained. No numerical data were altered by the reporting fixes.
