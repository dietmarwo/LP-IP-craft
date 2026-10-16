# Native performance — iterations 6–10

Base is native version 0.9.0 (iteration 5), not HiGHS. Current delivery is
0.14.0. Linux x86_64, Rust 1.99.1, AMD EPYC 9V74 host. No affinity pinning or
dedicated-core guarantee. Seven rotating/reversed rounds, one untimed warmup
and three timed solves per fixture per invocation: 21 observations per cell.
No compilation or other solver test suite ran during measurement. Model
construction is outside timing. All objectives agree across every run.

| Fixture | v5 ms | v6 ms | v7 ms | v8 ms | v9 ms | v10 ms | v5 / v10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| fixed | 0.2244 | 0.1861 | 0.2051 | 0.2059 | 0.2023 | 0.2022 | 1.11x |
| pivot | 1.4974 | 1.4376 | 1.4450 | 1.4899 | 1.4239 | 1.3686 | 1.09x |
| knapsack | 0.0850 | 0.0771 | 0.0663 | 0.0696 | 0.0650 | 0.0684 | 1.24x |
| partition | 0.1894 | 0.1746 | 0.1346 | 0.1391 | 0.1412 | 0.1488 | 1.27x |
| nodecopy | 1.9577 | 1.7479 | 1.6903 | 1.9186 | 1.6893 | 1.6853 | 1.16x |
| redundant | 8.6226 | 7.3520 | 7.9276 | 2.3292 | 2.2315 | 2.1468 | 4.02x |
| blocks | 88.5665 | 85.4067 | 85.5314 | 89.4433 | 3.0614 | 1.3927 | 63.59x |

## Parallelism within the final version

The same v10 block fixture measures 2.921 ms at a worker cap of 1 and
1.347 ms at a cap of 16 (2.17x). The model has eight
components, so the parallel run launches eight workers. A separate regression
exercises all sixteen workers. No general 16-core scaling claim is made.

## What the fixtures establish

- fixed: 220 columns, 35 rows, 180 fixed columns; unchanged target from v5.
- pivot: connected 110-column, 65-row dense LP.
- knapsack: 16 binary columns, one row.
- partition: 12 binary columns, three rows; below default decomposition threshold.
- nodecopy: connected 20-binary-column, eight-row MIP.
- redundant: 120 columns, 300 provably redundant rows.
- blocks: 512 columns, 256 rows, eight independent 64-column LP blocks.

The last three fixtures were introduced before any iteration-6 numerical
change. The v5 extended benchmark uses that fixture source with its original
numerical implementation unchanged. They expose the specific costs being
addressed; they are not a representative application benchmark.

Performance is not monotone for every fixture at every checkpoint. Streaming
reduces allocation and temporary memory but sometimes has neutral/slower wall
time; row redundancy adds scanning cost; component discovery and thread
startup have overhead. The default row-scan threshold was changed after a
measured regression. Intermediate slowdowns remain in this table and raw data.
The largest gains rely on redundancy or separability. Connected large sparse
models still need a sparse revised-simplex kernel.

## Reproduce

Clone history/native-iterations.bundle, then use tags native-iteration-5
through native-iteration-10. Build the release benchmark at each tag and copy
the executable outside Cargo target before changing revision. For iteration 5,
copy only crates/native/examples/benchmark.rs from tag native-iteration-6 to
add the three extended fixtures before building; keep its solver source intact.
The standalone source-checkpoint files in the bundle preserve these inputs.

```sh
python scripts/compare_native_iterations_6_10.py --binaries B5 B6 B7 B8 B9 B10
```

The script also alternates B10 with arguments `3 1` and `3 16` to compare
worker caps. Raw observations, actual component/worker counts in v10 and
SHA-256 hashes of the six executables are in comparison-6-10.json. Historical
per-iteration probes remain available; this final comparison supersedes them.
