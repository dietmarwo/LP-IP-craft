# lp-ip-craft-native

The native Rust LP/MIP engine for the
[`lp-ip-craft`](https://crates.io/crates/lp-ip-craft) contracts: a
bounded-variable dual revised simplex on a sparse LU basis factorization,
inside best-bound branch and bound with diving and pseudocost branching, and
independently checked results. Pure Rust (Rayon for bounded parallel work),
no HiGHS, BLAS or CMake.

```rust,ignore
use lp_ip_craft::{CancellationToken, LpIpSolver, Options};
use lp_ip_craft_native::{NativeOptions, NativeSolver};
let mut solver = NativeSolver::new(NativeOptions::default());
let result = solver.solve(&model, &Options::default(), None,
                          &CancellationToken::default())?;
```

Part of [LP-IP-craft](https://github.com/dietmarwo/LP-IP-craft), which also holds the
Python package and the job worker. To build from source:

```sh
git clone https://github.com/dietmarwo/LP-IP-craft
cd LP-IP-craft
cargo test -p lp-ip-craft-native --locked
```

MIT license.
