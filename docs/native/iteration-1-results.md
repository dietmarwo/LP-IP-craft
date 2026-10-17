# Native iteration 1 results

Implemented version 0.5.0. Workspace Rust tests pass (including 10 new native tests and the existing core doctest). The independent SciPy oracle passes 400/400 generated bounded LP/MIP cases. Full Python boundary results are recorded in the final validation report.

Release benchmark medians (7 timed runs, one untimed warmup):

| Fixture | Milliseconds | Nodes | Pivots |
|---|---:|---:|---:|
| fixed | 21.630 | 0 | 220 |
| pivot | 62.601 | 0 | 1708 |
| knapsack | 0.703 | 33 | 851 |
| partition | 0.470 | 55 | 531 |
