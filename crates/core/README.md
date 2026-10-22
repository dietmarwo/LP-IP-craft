# lp-ip-craft

[Optimization ecosystem](https://dietmarwo.github.io/optimization-jobs/) ·
[Shared solver-selection and agent guide](https://github.com/dietmarwo/optimization-jobs/blob/main/ai-context.md)

Validated LP and MIP models, the solver contracts (`LpIpSolver`, `Options`,
`SolveResult`, `CancellationToken`) and independent numerical checks of
returned points: bounds, row activities, integrality, objective and, for LP
optima, duals and reduced costs. The crate has no dependencies; the native
engine is [`lp-ip-craft-native`](https://crates.io/crates/lp-ip-craft-native).

```sh
cargo add lp-ip-craft lp-ip-craft-native
```

The [`lp-ip-craft-native` README](https://crates.io/crates/lp-ip-craft-native)
has a complete example; the [API documentation](https://docs.rs/lp-ip-craft)
describes the model, the options and the result conventions.

Part of [LP-IP-craft](https://github.com/dietmarwo/LP-IP-craft), which also holds the
Python package and the job worker. To build from source:

```sh
git clone https://github.com/dietmarwo/LP-IP-craft
cd LP-IP-craft
cargo test -p lp-ip-craft --locked
```

MIT license.
