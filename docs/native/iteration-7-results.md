# Native iteration 7 results

Version 0.11.0 uses inline variable substitutions, one dense allocation and reusable sparse-row scratch. Empty rows are compacted without losing objective rows. Rust, Python and all 600 oracle cases pass.

| Fixture | Median ms | Predecessor / current |
|---|---:|---:|
| fixed | 0.1976 | 1.04x |
| pivot | 1.4223 | 0.94x |
| knapsack | 0.0626 | 1.15x |
| partition | 0.1292 | 1.28x |
| nodecopy | 1.7144 | 1.05x |
| redundant | 7.7285 | 0.93x |
| blocks | 89.6464 | 0.94x |
