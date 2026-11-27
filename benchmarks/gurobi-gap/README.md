# LP-IP-craft and Gurobi MIP comparison

This driver solves the same original-model coefficient arrays in LP-IP-craft
and Gurobi. Gurobi is optional. The release extension provides headline
timings. The same builds produce additional diagnostics in separate runs.

The recorded campaign, its figures and their review are in
[docs/gurobi-gap](../../docs/gurobi-gap/RESULTS.md); the raw records ship in
the archive named in [docs/EVIDENCE.md](../../docs/EVIDENCE.md).

## Reproduction

The harness compares two builds of the Python package: a baseline, called
`stock`, and the build under test, called `candidate`. The campaign's
baseline is `bcc4af35c66c2afe00066f50d736b2a06da48c72`, the last commit
before `adaptive_root` and `integer_equality_presolve` existed (version
0.30.0); only its default arm can run there. Build both packages into
separate locations and use one Python environment to run the harness. The
example assumes Linux and an available Rust toolchain.

```bash
git worktree add --detach ../lp-gap-stock bcc4af35c66c2afe00066f50d736b2a06da48c72
python -m pip install maturin scipy highspy pytest jsonschema
python -m pip install gurobipy==14.0.0
for tree in . ../lp-gap-stock; do
  (cd "$tree" && PYO3_BUILD_EXTENSION_MODULE=1 cargo build --release --locked -p lp-ip-craft-python)
done
mkdir -p ../gap-builds/{stock,candidate}/lp_ip_craft
cp ../lp-gap-stock/python/lp_ip_craft/* ../gap-builds/stock/lp_ip_craft/
cp python/lp_ip_craft/* ../gap-builds/candidate/lp_ip_craft/
cp ../lp-gap-stock/target/release/lib_native.so ../gap-builds/stock/lp_ip_craft/_native.abi3.so
cp target/release/lib_native.so ../gap-builds/candidate/lp_ip_craft/_native.abi3.so
git clone https://github.com/coin-or-tools/Data-Miplib3.git ../gap-public
git -C ../gap-public checkout --detach 36c786b3274f75d9a0062471589813b92d3da91b
python benchmarks/gurobi-gap/freeze.py --public ../gap-public --output ../gap-data
python benchmarks/gurobi-gap/verify_inputs.py ../gap-data/manifest.json ../gap-public
```

Set `--cpus` to any available CPU for a serial comparison. Omit it to use
the process's existing affinity. Each observation has a fresh model and
process. The reported solve time excludes model construction and imports.
For local timing use a release build with no profiling features. The
fixed GMI/MIR profile uses 16 rounds. The adaptive profile uses the ordinary
round ceiling and the bounded policy.

```bash
python benchmarks/gurobi-gap/run.py --manifest ../gap-data/manifest.json \
  --output ../gap-builds/final.jsonl --group development blind regression lp \
  --stock ../gap-builds/stock --candidate ../gap-builds/candidate \
  --arms stock:default candidate:adaptive+lattice stock:gmi+mir gurobi:default \
  --repeats 5 --seconds 10
python benchmarks/gurobi-gap/audit.py ../gap-data/manifest.json ../gap-builds/final.jsonl
python benchmarks/gurobi-gap/summarize.py ../gap-builds/final.jsonl \
  --output ../gap-builds/blind-summary.json
```

Use `--observe` for native search timing and Gurobi callback events.
`--presolve-report` adds a separate Gurobi presolve call before solve timing.
Native diagnostics may be absent when an instance is decomposed. Add
`+serial` to the native arm to observe the single-tree path. The diagnostics
include distinct root LP objectives and global or conservative bounds. The
clock history is sampled and its reported checkpoint is the last available
observation at or before the requested time. Callback candidates are
checked against the original arrays. The final raw result includes the
original-coordinate vector and both feasibility residuals.

```bash
python benchmarks/gurobi-gap/run.py --manifest ../gap-data/manifest.json \
  --output ../gap-builds/diagnostics.jsonl --group development \
  --stock ../gap-builds/stock --candidate ../gap-builds/candidate \
  --arms candidate:adaptive+lattice+serial stock:gmi+mir+serial gurobi:default \
  --repeats 1 --observe --presolve-report
python benchmarks/gurobi-gap/trace_summary.py ../gap-builds/diagnostics.jsonl
python benchmarks/gurobi-gap/equalities.py ../equality-data
python benchmarks/gurobi-gap/run.py --manifest ../equality-data/manifest.json \
  --output ../gap-builds/equalities.jsonl --group equality \
  --stock ../gap-builds/stock --candidate ../gap-builds/candidate \
  --arms stock:default candidate:lattice gurobi:default --repeats 3
```

Without Gurobi, leave `gurobi:default` out of `--arms` and score with
`--no-reference`: the summary then holds the native arms and their paired
ratio and omits the Gurobi ratios.

The existing comparison runner also accepts `--solvers gurobi`. Its
default reference set does not require Gurobi. The Gurobi adapter reports
license-size refusals directly. Native regression cases with unsupported
bounds stay in the results. The shifted time score applies twice the
time limit to every run without validated optimal termination by the cap.

## Verification

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
PYTHONPATH=../gap-builds/candidate python -m pytest -q tests benchmarks/gurobi-gap/test_reference.py
PYTHONPATH=../gap-builds/candidate python benchmarks/gurobi-gap/validate_profiles.py exact
PYTHONPATH=../gap-builds/candidate python benchmarks/gurobi-gap/validate_profiles.py scipy
```

## Comparing two revisions that both have the options

Set `--stock` to the preceding build and `--candidate` to the revision under
review. Use `--arms stock:adaptive+lattice candidate:adaptive+lattice gurobi:default`
to compare the same options. Score an audited journal with
`summarize.py JOURNAL --group GROUP --baseline stock:adaptive+lattice --no-static --output SCORE.json`.
The `+no_obj_lattice` modifier disables integral-objective bound tightening,
including the returned bound on eligible mixed models. A Gurobi `--pivots` limit is
passed as `IterationLimit`. Solver iteration accounting can differ.

New freezes use the verified `noswot` MIP objective of −41. The runner and
auditor also recognize the preceding frozen payload by SHA-256 and check
its stored −43 target before applying the correction. The original fixture
bytes stay usable. The audit independently checks recorded callback points
and the final candidate.
