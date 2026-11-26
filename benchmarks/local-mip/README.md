# LocalMIP root pilot

The benchmark uses LP-IP-craft's general serial MIP search, consumes frozen
MPS conversions, and emits checked incumbent trajectories from solve entry.
It uses one process per observation and checks the final point against the
original arrays with an independent Python evaluator. The package's solver
dependencies are unchanged. HiGHS is used as an independent MPS reader in
the benchmark preparation step.

| Arm | Policy at the first fractional search root |
| --- | --- |
| A | Native FJ at its ordinary step limit |
| B | Native FJ followed by one LocalMIP tight-move attempt |
| C | Native FJ with the pilot's work, wall and patience limits |
| D | One LocalMIP tight-move attempt under those limits |

All four use the same root-guided diving, L1 pump, and RENS policy and the
same limits on total wall time, nodes and pivots. The local limit is
`min(2000000, 64 * max(nnz, 1))` coefficient visits and 250 ms. The arms
share the existing FJ setting for RENS children. C and D choose their worker
at the main root hook. Each run reports applied moves, local stop reason, original objective
and safe bound where available. Scope and timestamp for the old serial
`diagnostics.mip_search` fields follow the same contract as before.

The native code uses Lin, Zou and Cai's violated-row tight moves and a
one-attempt adaptation of the reference work/patience accounting. Sources
are the [LocalMIP paper](https://doi.org/10.4230/LIPIcs.CP.2024.19),
[Spoorendonk's HiGHS study](https://arxiv.org/abs/2609.22938) and the
[reference implementation](https://github.com/spoorendonk/mip-heuristics/tree/4e3bf0021c178189b26cce19ca65cbe78470d5e3).

## Reproduction

Run from the repository root on Linux. Supply paths for the two checkouts
and experiment output. The baseline is
`2468f091855a00085ad39e3c6f7922732f22fda4`. A stock-compatible driver can be
built by copying `benchmarks/local-mip/driver.rs` and the benchmark native
manifest into a baseline worktree, replacing the `local-mip-pilot` feature
entry there with `local-mip-pilot=[]`, and copying the benchmark lockfile.
Build that driver without features. The pilot binary uses the feature shown
below. The external checkout is pinned to the reference implementation
revision above.

```sh
cargo build --manifest-path benchmarks/Cargo.toml --release --locked \
  --bin local-mip-pilot --features local-mip-pilot
python -m pip install highspy==1.16.0
python benchmarks/local-mip/prepare.py --upstream ../mip-heuristics \
  --data ../local-mip-data --stock ../lp-stock/benchmarks/target/release/local-mip-pilot
python benchmarks/local-mip/run.py --binary benchmarks/target/release/local-mip-pilot \
  --data ../local-mip-data --out ../local-mip-smoke --phase smoke
python benchmarks/local-mip/run.py --binary benchmarks/target/release/local-mip-pilot \
  --data ../local-mip-data --out ../local-mip-campaign --phase campaign
python benchmarks/local-mip/analyze.py ../local-mip-campaign --output ../local-mip-scored.json
```

For parallel case throughput, pass distinct physical CPUs using `--cores`.
Each pair stays on the same CPU and gets its own process and watchdog. Use
`--phase root --cases CASE ...` for a replay through the first fractional
search root. The trace's root exit gives the checked objective and the
dispatch reason. Run the prepared roster under the stock policy with
`--phase context`. Use `analyze.py` with no arguments to test the integral.
Compare archived file hashes before reusing the same manifest or extending
its campaign.

The Rust and observed solvers both use `NativeOptions::local_mip_seed`.
Plain mode supports A and B. C and D require the benchmark root controls.
When a metered worker produces the first checked point at the main root,
the solver keeps up to eight subsequent fractional-node rounding attempts.
A successful rounding or an accepted integral leaf ends this allowance.
Existing global limits and proof admission still apply to every solve.

For a reproduction using the supplied evidence, pass
`--verify-against /path/to/evidence/input-manifest.json` to `prepare.py`.
Each output directory holds a frozen run schedule. Repeat the same command
to resume missing work. Use a fresh directory when changing the build,
selection, CPU placement, mode, memory limit or runner. The scorer verifies
the file inventory and run identities before comparing arms. Pass
`--legacy` only for a first-edition archive whose schedule was not recorded.

After building the release driver, run the review regression checks with
`python benchmarks/local-mip/test_pilot.py`.

To repeat the fixing and interval-propagation diagnostic, run
`python benchmarks/local-mip/replay_fixing.py MODEL.json [MODEL.json ...]`.
It checks a completed point against the original rows, bounds and integer
domains and reports its exact rational objective. The DFS uses a depth cap of
64 and a coefficient budget of 64 times the fixture's stored nonzeros.
Continuous-variable completion and domain-shift repair remain open FPR work.
