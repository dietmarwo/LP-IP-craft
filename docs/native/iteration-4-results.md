# Native iteration 4 results

Parent-state reuse and dual simplex are implemented, with a bounded cold fallback. Original-unit LP residuals also check primal feasibility, so the adapter no longer clones the model solely to clear integrality for a second feasibility evaluation. A successful warm acceptance check is reused.

| Fixture | Median ms | Predecessor / current | Pivots before → after |
|---|---:|---:|---:|
| fixed | 0.228 | 1.11x | 40 → 40 |
| pivot | 1.861 | 0.84x | 68 → 68 |
| knapsack | 0.172 | 1.33x | 300 → 110 |
| partition | 0.202 | 0.98x | 185 → 103 |

The tiny partition fixture can regress despite fewer pivots: dictionary copying and conservative cold retries have overhead. The final alternating comparison reports that result without suppressing it.
