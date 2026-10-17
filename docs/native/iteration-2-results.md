# Native iteration 2 results

Fixed-column elimination, empty-row removal and dense-memory preflight implemented. All workspace Rust tests pass, including the new presolve on/off differential regression. Python and 400-case external oracle results are included in the final validation record.

| Fixture | Median ms | Predecessor / current |
|---|---:|---:|
| fixed | 0.341 | 63.49x |
| pivot | 61.403 | 1.02x |
| knapsack | 0.458 | 1.54x |
| partition | 0.256 | 1.84x |
