# lp-ip-craft

Validated LP and MIP models, the solver contracts (`LpIpSolver`, `Options`,
`SolveResult`, `CancellationToken`) and independent numerical checks of
returned points: bounds, row activities, integrality, objective and, for LP
optima, duals and reduced costs. The crate has no dependencies; the native
engine is [`lp-ip-craft-native`](https://crates.io/crates/lp-ip-craft-native).

Part of [LP-IP-craft](https://github.com/dietmarwo/LP-IP-craft), which also holds the
Python package and the job worker. To build from source:

```sh
git clone https://github.com/dietmarwo/LP-IP-craft
cd LP-IP-craft
cargo test -p lp-ip-craft --locked
```

MIT license.
