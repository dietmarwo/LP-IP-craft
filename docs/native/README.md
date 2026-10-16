# Ten initial native iterations

Each iteration has a plan, implementation checkpoint, result report,
release measurements and numerical-oracle output. Earlier history remains
readable; these reports describe versions through 0.14.0.
The 0.15.0 sparse pilot is documented separately in [../sparse](../sparse). See performance-6-10.md for
the latest alternating comparison and validation-6-10.md for acceptance details.

| Iteration | Version | Main implementation |
|---|---|---|
| 1 | 0.5.0 | Own simplex and finite-box branch-and-bound; remove HiGHS |
| 2 | 0.6.0 | Fixed-column/empty-row presolve and memory preflight |
| 3 | 0.7.0 | In-place pivots and pricing |
| 4 | 0.8.0 | Parent dictionaries and child dual simplex |
| 5 | 0.9.0 | Checked incumbents, conservative pruning and lattice bounds |
| 6 | 0.10.0 | Borrowed node bounds and original sparse model |
| 7 | 0.11.0 | Streamed sparse conversion and inline substitutions |
| 8 | 0.12.0 | Conservative redundant-row presolve |
| 9 | 0.13.0 | Independent component execution and checked postsolve |
| 10 | 0.14.0 | Up to 16 component workers and reserved global budgets |

The final archive includes a local Git bundle with all ten checkpoints.
Clone it with `git clone history/native-iterations.bundle replay`, then inspect
`git log` and build the benchmark at each version. Historical HiGHS adapter
records are separate under ../history and are not part of the current build.
