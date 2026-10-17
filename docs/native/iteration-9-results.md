# Native iteration 9 results

Version 0.13.0 discovers independent restricted-row components, solves them serially and recomputes original-model feasibility/KKT residuals. Connected discovery exits as soon as all variables are joined. Rust tests, 108 Python tests, 600 base oracle cases and 180 component cases pass.

| Fixture | Median ms | Predecessor / current |
|---|---:|---:|
| fixed | 0.1924 | 1.02x |
| pivot | 1.5179 | 0.95x |
| knapsack | 0.0620 | 0.98x |
| partition | 0.1325 | 0.98x |
| nodecopy | 1.8161 | 1.00x |
| redundant | 2.0956 | 1.03x |
| blocks | 2.9487 | 29.81x |
