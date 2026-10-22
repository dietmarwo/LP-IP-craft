<p align="center">
  <img src="https://raw.githubusercontent.com/dietmarwo/LP-IP-craft/main/docs/images/lp-ip-craft.png" alt="LP-IP-craft: linear and mixed-integer optimization in Rust">
</p>

# lp-ip-craft

Linear programming (LP) and mixed-integer linear programming (MILP) in Python,
solved by a native Rust engine: a bounded-variable dual revised simplex on a
sparse LU factorization, presolve, cuts and branch-and-bound. Every returned
point is evaluated independently of the solver (objective, row activities,
bound and integrality violations), and `lp_residuals` checks LP duals.

## Install

```sh
python -m pip install lp-ip-craft
```

Wheels (abi3, CPython 3.10 or newer) cover Linux x86_64, Windows x64 and macOS
x86_64/arm64 and need no compiler. On other platforms pip builds the source
distribution, which needs Rust 1.85 or newer. The package depends on nothing
else at run time.

## Example

```python
from lp_ip_craft import Model, solve

# maximize 5x + 4y  subject to  6x + 4y <= 24,  x + 2y <= 6,  0 <= x, y <= 10, integer
model = Model(
    objective=[5.0, 4.0], sense="maximize",
    lower=[0.0, 0.0], upper=[10.0, 10.0],
    row_lower=[float("-inf"), float("-inf")], row_upper=[24.0, 6.0],
    indptr=[0, 2, 4], indices=[0, 1, 0, 1], values=[6.0, 4.0, 1.0, 2.0],  # A in CSR form
    kinds=["integer", "integer"],
)
result = solve(model, backend="native")
print(result.status, result.candidate, result.evaluation.objective, result.best_bound)
# optimal [4.0, 0.0] 20.0 20.0
```

Pass `backend="native"`: the default `backend="dummy"` only evaluates a start
point. `Options` sets the tolerances and the numerical width (`threads`,
1 to 8); `NativeOptions` holds the engine's controls, including time, node
and iteration limits, and rejects unknown names. `solve_batch` solves
independent models with up to 16 workers. `evaluate` and `lp_residuals`
check points and duals from any solver.

## Limits

- A MIP needs finite bounds on all variables, continuous ones included, as
  given or derived by root inference (which runs with `presolve` on).
- Linear objectives only: no QP or MIQP.
- `optimal` and the gap are numerical findings under the stated tolerances.
  For hard industrial MIPs, compare against a mature solver such as HiGHS.

## More

- [Repository](https://github.com/dietmarwo/LP-IP-craft), with the Rust crates
  `lp-ip-craft` and `lp-ip-craft-native` and the source build
- [AI agent guide](https://github.com/dietmarwo/LP-IP-craft/blob/main/ai-context.md):
  modelling, options, results, MPS import and the job API
- [Changelog](https://github.com/dietmarwo/LP-IP-craft/blob/main/CHANGELOG.md)
- [Shared solver-selection guide](https://github.com/dietmarwo/optimization-jobs/blob/main/ai-context.md)
  and [ecosystem overview](https://dietmarwo.github.io/optimization-jobs/):
  when black-box optimization (fcmaes) or SQP (SQP-craft) fits better, and
  durable shell/MCP jobs through the `lp-ip-job-worker` (built from source)

MIT license.
