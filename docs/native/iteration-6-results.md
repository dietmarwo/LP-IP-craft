# Native iteration 6 results

Version 0.10.0 removes model/matrix cloning from node solves. Core bound-view checks match separately materialized models. All Rust tests, 98 Python tests and 600 independent oracle cases pass.

| Fixture | Median ms | Predecessor / current |
|---|---:|---:|
| fixed | 0.2062 | 1.07x |
| pivot | 1.3422 | 1.12x |
| knapsack | 0.0717 | 1.19x |
| partition | 0.1656 | 1.14x |
| nodecopy | 1.7988 | 1.10x |
| redundant | 7.1830 | 1.16x |
| blocks | 83.8345 | 1.08x |

Initial probes are host-dependent. Final alternating comparisons will supersede these timings; objectives, node and pivot counts are checked separately.
