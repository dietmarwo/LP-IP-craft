# ripsolve

A branch-and-cut solver for mixed-integer programs, in pure Rust.

[![crates.io](https://img.shields.io/crates/v/ripsolve.svg)](https://crates.io/crates/ripsolve)
[![docs.rs](https://docs.rs/ripsolve/badge.svg)](https://docs.rs/ripsolve)

Columns may be binary, general integer, or continuous. Every node of the search solves
a bounded LP relaxation with the simplex method, and the dual bound that comes back,
strengthened by presolve and cutting planes, is what prunes the tree.

No dependency links against another solver.

```toml
[dependencies]
ripsolve = "0.3"
```

```rust
use ripsolve::{Problem, search};
use std::path::Path;

let problem = Problem::from_file(Path::new("model.lp"))?;
problem.validate()?;

let solution = search::solve(&problem, search::Options::default());
println!("{:?} {:?}", solution.status, solution.objective);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Both LP and MPS are read, chosen by extension. See the [API
documentation](https://docs.rs/ripsolve) for assembling a model directly and for the
search options.

## Scope

Built to be fast on small and medium models, up to roughly a thousand rows. In that
range it is quicker than HiGHS, SCIP and CBC on most of its benchmark set, and
competitive with a leading commercial solver. Above that range it is not competitive.

Implemented: LP and MPS readers, presolve, knapsack cover cuts, Gomory mixed-integer
cuts, node-local cut separation, pseudocost branching, primal heuristics, a sparse LU
factorization, best-bound search, and parallel tree search.

Not implemented: quadratic terms, SOS constraints, callbacks, lazy constraints,
multi-objective.

## Related

A command-line application is published as
[`ripsolve-cli`](https://crates.io/crates/ripsolve-cli), and a `gurobipy`-shaped Python
module lives in the [repository](https://github.com/ripsolve/ripsolve).

## Licence

MIT.
