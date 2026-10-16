# Native iteration 3 results

Implemented allocation-free pivot updates and deterministic most-negative pricing with Bland fallback. Both reference switches remain available. Rust differential tests and existing suites pass; full Python/oracle results are retained in the validation record.

| Fixture | Median ms | Predecessor / current | Pivots before → after |
|---|---:|---:|---:|
| fixed | 0.252 | 1.35x | 40 → 40 |
| pivot | 1.558 | 39.40x | 1708 → 68 |
| knapsack | 0.229 | 2.00x | 574 → 300 |
| partition | 0.197 | 1.30x | 291 → 185 |
