> Historical record for iterations 1–5. Current delivery: [performance-6-10.md](performance-6-10.md).

# Native solver performance across five iterations

All comparisons use our own native versions. These are not HiGHS speedups.
Release Rust 1.99.1, Linux x86_64, AMD EPYC 9V74 host, one solver thread.
Seven rounds rotate and reverse process order. Each invocation warms each
fixture once and then times three solves: 21 measured runs per fixture and
iteration. Models are built outside solve timing. No compilation ran during
the comparison; no timing threshold is enforced in tests.

| Fixture | Iteration 1 ms | Iteration 2 ms | Iteration 3 ms | Iteration 4 ms | Iteration 5 ms | Initial / final |
|---|---:|---:|---:|---:|---:|---:|
| fixed | 22.4637 | 0.3634 | 0.2392 | 0.2115 | 0.2004 | 112.09x |
| pivot | 64.8040 | 61.1389 | 1.5578 | 1.4266 | 1.4316 | 45.27x |
| knapsack | 0.7302 | 0.4710 | 0.2450 | 0.1873 | 0.0806 | 9.06x |
| partition | 0.4644 | 0.2708 | 0.2094 | 0.2270 | 0.1930 | 2.41x |

The fixed-column case is a 220-variable/35-row LP with 180 fixed columns;
the pivot case is a 110-variable/65-row dense LP; knapsack has 16 binary
variables and one row; partition has 12 binary variables and three rows.
They are controlled development fixtures, not representative GTOC problems.

Improvements are workload dependent. Iteration 4 reduces branch-node pivots
but regresses about 8% on the tiny partition fixture; iteration 5 recovers
that loss. The pivot LP is effectively unchanged between iterations 4 and 5.
The slow, copying/Bland first baseline is a correctness reference, so large
ratios do not imply a competitive solver on general LP/MIP workloads.

| Integer fixture | Nodes iteration 1 | Nodes iteration 5 | Pivots iteration 1 | Pivots iteration 5 |
|---|---:|---:|---:|---:|
| knapsack | 33 | 15 | 851 | 36 |
| partition | 55 | 47 | 531 | 88 |

Objectives agree across every recorded run. Raw observations and binary
hashes are in `comparison.json`; initial per-iteration runs are retained in
`iteration-N-benchmarks.json`. Reproduce by building the benchmark example
at each checkpoint, copying it outside Cargo target, then running
`scripts/compare_native_iterations.py --binaries B1 B2 B3 B4 B5`.
The optional history bundle preserves the five source checkpoints.
