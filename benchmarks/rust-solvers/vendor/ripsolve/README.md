# ripsolve for Python

A branch-and-cut solver for mixed-integer programs, written in pure Rust, with a
`gurobipy`-shaped interface. A gurobipy script that stays inside the supported
feature set should run unchanged after swapping the import.

```sh
pip install ripsolve
```

Wheels cover Linux, macOS and Windows for CPython 3.9 and later, and nothing else
needs installing: the solver is compiled into the module rather than linked against
another one.

```python
import ripsolve as gp
from ripsolve import GRB

m = gp.Model("knapsack")
x = m.addVars(n, vtype=GRB.BINARY, name="x")
m.setObjective(gp.quicksum(value[j] * x[j] for j in range(n)), GRB.MAXIMIZE)
m.addConstr(gp.quicksum(weight[j] * x[j] for j in range(n)) <= capacity)
m.optimize()

print(m.ObjVal, [x[j].X for j in range(n)])
```

## Building from source

Where no wheel fits, `pip install ripsolve` builds from the source distribution,
which needs a Rust toolchain ([rustup.rs](https://rustup.rs)). From a clone of the
repository the same build is:

```sh
pip install ./python
```

For development, `build.sh` writes the module beside this file with nothing but
cargo and a Python with development headers, and the tests run against it:

```sh
./build.sh                      # writes ripsolve.so beside this file
python3 -m pytest tests -q
```

## What is supported

| | |
|---|---|
| Model | `Model(name)`, `optimize`, `optimize(callback)`, `getVars`, `write`, `setParam`, `Params` |
| Variables | `addVar`, `addVars`, `.X`, `.VarName`, `.VType`, `.Obj`, `.LB`, `.UB` |
| Expressions | `+ - *`, unary `-`, `<= >= ==`, `quicksum` |
| Constraints | `addConstr`, `addConstrs`, `addLConstr`, `.ConstrName` |
| Objective | `setObjective(expr, GRB.MINIMIZE / GRB.MAXIMIZE)` |
| Attributes | `ObjVal`, `ObjBound`, `Status`, `SolCount`, `NodeCount`, `Runtime`, `MIPGap`, `NumVars`, `NumConstrs`, `ModelName`, `ModelSense` |
| Parameters | `TimeLimit`, `Threads`, `MIPGap`, `OutputFlag`, `LazyConstraints`, `PreCrush` |
| Callbacks | `GRB.Callback.MIPNODE` / `MIPSOL`, `cbGet` (`MIPSOL_OBJ`, `MIPNODE_STATUS`), `cbGetSolution`, `cbGetNodeRel`, `cbLazy`, `cbCut` |
| Files | `read(path)` for LP and MPS |

## What is not

No quadratic terms, no SOS constraints, and no multi-objective support. Callbacks are
called at `MIPNODE`, for the root's and each node's relaxation, and at `MIPSOL`, for
every point before it may become the incumbent; the other `where` codes, `cbSetSolution`
and `terminate` are not there. `MIPNODE` runs at every node, so a costly separator
should return quickly where it finds nothing. One callback runs at a time even with
several threads, as in gurobipy. `addVar` accepts `GRB.BINARY`, `GRB.INTEGER` and
`GRB.CONTINUOUS`, and rejects anything else rather than silently solving a model it
cannot honour.

`vtype` defaults to continuous, as gurobipy's does. Defaulting to binary would suit
this solver's history and is exactly the wrong trade: a ported script calling
`addVar()` would silently get a different model.

Unknown parameter names raise `KeyError` rather than being ignored, so a
misspelling fails loudly. Attribute names, as in gurobipy, may be written in any
case: `model.status` is `model.Status`.

## Threads

`optimize()` releases the GIL, so a solve does not block other Python threads.
`Threads` defaults to the machine's parallelism, as Gurobi's does.

## Links

- Project page: <https://ripsolve.org>
- Source: <https://github.com/ripsolve/ripsolve>
- The Rust library's API: <https://docs.rs/ripsolve>
