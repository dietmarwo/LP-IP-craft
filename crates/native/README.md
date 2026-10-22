# lp-ip-craft-native

[Optimization ecosystem](https://dietmarwo.github.io/optimization-jobs/) ·
[Shared solver-selection and agent guide](https://github.com/dietmarwo/optimization-jobs/blob/main/ai-context.md)

The native Rust LP/MIP engine for the
[`lp-ip-craft`](https://crates.io/crates/lp-ip-craft) contracts: a
bounded-variable dual revised simplex on a sparse LU basis factorization,
inside best-bound branch and bound with diving and pseudocost branching, and
independently checked results. Pure Rust (Rayon for bounded parallel work),
no HiGHS, BLAS or CMake.

```sh
cargo add lp-ip-craft lp-ip-craft-native
```

```rust
use lp_ip_craft::{
    Bounds, CancellationToken, CsrMatrix, LpIpSolver, Model, ModelData, Options, Sense, Status,
    VariableKind,
};
use lp_ip_craft_native::NativeSolver;

fn main() -> lp_ip_craft::Result<()> {
    // maximize 7 x0 + 11 x1 + 5 x2 + 6 x3  subject to  3 x0 + 5 x1 + 2 x2 + 4 x3 <= 8.5,  x binary
    let model = Model::new(ModelData {
        sense: Sense::Maximize,
        objective: vec![7.0, 11.0, 5.0, 6.0],
        objective_offset: 0.0,
        variables: Bounds::new(vec![0.0; 4], vec![1.0; 4]),
        kinds: vec![VariableKind::Binary; 4],
        matrix: CsrMatrix::from_dense(1, 4, &[3.0, 5.0, 2.0, 4.0])?,
        rows: Bounds::new(vec![f64::NEG_INFINITY], vec![8.5]),
    })?;
    let result = NativeSolver::default().solve(
        &model,
        &Options::default(),
        None,
        &CancellationToken::default(),
    )?;
    assert_eq!(result.status, Status::Optimal);
    println!("{:?} {:?}", result.candidate, result.evaluation.map(|e| e.objective));
    // Some([1.0, 1.0, 0.0, 0.0]) Some(18.0)
    Ok(())
}
```

`Options` sets the tolerances and the numerical width (`threads`, 1 to 8);
`NativeOptions` (via `NativeSolver::new`) holds the engine's controls: time,
node and iteration limits, presolve, cuts, branching and decomposition.
Limits: a MIP needs finite bounds on all variables, continuous ones included,
as given or derived by root inference; the objective is linear (no QP or
MIQP); `Optimal` and gaps are numerical findings under the stated
tolerances. See the [API documentation](https://docs.rs/lp-ip-craft-native)
and the [AI agent guide](https://github.com/dietmarwo/LP-IP-craft/blob/main/ai-context.md).

Part of [LP-IP-craft](https://github.com/dietmarwo/LP-IP-craft), which also holds the
Python package and the job worker. To build from source:

```sh
git clone https://github.com/dietmarwo/LP-IP-craft
cd LP-IP-craft
cargo test -p lp-ip-craft-native --locked
```

MIT license.
