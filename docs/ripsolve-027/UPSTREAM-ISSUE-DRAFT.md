# Draft: restart_share=0 can return Optimal with an open tree and a suboptimal incumbent

Not submitted. Reference: published ripsolve 0.4.0 source distribution, SHA256 `a46e01c8e528e24adf9207f7b17a7ca7442ec03b9fe01e04ab33ddf975641871`. Rust 1.99.1, release build, `threads=1`, zero gap tolerance, 20-second time limit. Other search options retain defaults except `restart_share=0`. The documented contract says “Zero never restarts.”

The common-array harness returns the following original-model-feasible points:

| Input | Status | Objective | Reported bound | Independently known optimum |
|---|---|---:|---:|---:|
| lseu | Optimal | 1165 | 1017 | 1120 |
| bell5 | Optimal | 8992602.16152 | 8943662.65351 | 8966406.49152 |
| rgn | Optimal | 82.19999924 | 54.77825956 | 82.19999924 |

The first two points are suboptimal. The third happens to be optimal, but its open reported bound gap still contradicts the termination status. Candidate feasibility and objective were rechecked against the unchanged original inputs; HiGHS and the native solver independently recover the listed optima. These runs are excluded from speed comparisons.

Reproduce from the accompanying source package:

```bash
cargo build --release --manifest-path benchmarks/rust-solvers/Cargo.toml --locked
python benchmarks/rust-solvers/ablate.py --binary target/release/solvercmp --bug-only --seconds 20 --output /tmp/ripsolve-status.json
```

The driver calls `search::solve` on the same CSR coefficients, row/column bounds, kinds and objective, without adding finite surrogate bounds. The reference manifest adaptation and optional `RS_OFF` family exits are included as an exact diff; `RS_OFF` is unset for this reproduction and those exits do not execute. Raw outputs, full candidates and input SHA256 values are retained in `data/ripsolve-status-reproduction.json`.

Source inspection suggests a missing zero guard in `Worker::wants_restart`: its percentage comparison becomes true with a zero right-hand side. The main loop then retains the open node and breaks with restart requested. If `narrow_for_restart` cannot actually narrow the model, the attempt can return the still-default Optimal status with an open frontier. This is a source-based explanation, not a verified upstream fix. A fix should both honor zero as disabled and prevent any unconsumed restart request from converting an open tree to Optimal. Add regression tests for these three inputs and other cases with no additional narrowing.

The default `restart_share=25` runs did not exhibit this wrong-optimality behavior on the measured development set. This report makes no claim about later ripsolve releases or untested settings.
