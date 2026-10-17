# Native iteration 8 results

Version 0.12.0 adds conservative row-activity presolve with an arithmetic gate, opt-out and a measured m/n >= 1 default threshold (configurable). Rust tests, 100 Python tests and all 600 oracle cases pass.

| Fixture | Median ms | Predecessor / current |
|---|---:|---:|
| fixed | 0.1972 | 1.00x |
| pivot | 1.4400 | 0.99x |
| knapsack | 0.0605 | 1.04x |
| partition | 0.1297 | 1.00x |
| nodecopy | 1.8144 | 0.94x |
| redundant | 2.1620 | 3.57x |
| blocks | 87.9107 | 1.02x |

The initial unrestricted scan regressed row-light fixtures; the default threshold was added and all checks/measurements rerun. Final alternating timings supersede these initial probes.
