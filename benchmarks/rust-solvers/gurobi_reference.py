"""Optional reference, using the same original coefficient arrays as native."""
import math
import time


def finite(value):
    return float(value) if value is not None and math.isfinite(value) else None


def solve(spec, settings, start=None, observed=False, switches=None, presolve=False):
    import gurobipy as gp

    columns = len(spec["objective"])
    variable_types = {"continuous": "C", "integer": "I", "binary": "B"}
    mip = any(kind != "continuous" for kind in spec["kinds"])
    events = []
    with gp.Env(empty=True) as env:
        env.setParam("OutputFlag", 0)
        try:
            env.start()
        except gp.GurobiError as exc:
            if exc.errno != 10009:
                raise
            return dict(seconds=0, result=dict(status="unavailable", errno=exc.errno, error=str(exc)))
        with gp.Model(env=env) as model:
            model.Params.Threads = settings.get("threads", 1)
            model.Params.Seed = settings.get("seed", 0)
            model.Params.TimeLimit = settings["seconds"]
            model.Params.FeasibilityTol = model.Params.IntFeasTol = model.Params.OptimalityTol = 1e-7
            model.Params.MIPGap = model.Params.MIPGapAbs = 0
            for name, value in (switches or {}).items():
                model.setParam(name, value)
            nodes = settings.get("nodes")
            if nodes is not None and mip:
                model.Params.NodeLimit = nodes
            if settings.get("pivots") is not None:
                model.Params.IterationLimit = settings["pivots"]
            x = []
            for j in range(columns):
                lower = spec["lower"][j]
                upper = spec["upper"][j]
                x.append(model.addVar(
                    lb=-gp.GRB.INFINITY if lower is None else lower,
                    ub=gp.GRB.INFINITY if upper is None else upper,
                    obj=spec["objective"][j], vtype=variable_types[spec["kinds"][j]],
                ))
            model.ObjCon = spec.get("objective_offset", 0.0)
            model.ModelSense = gp.GRB.MAXIMIZE if spec["sense"] == "maximize" else gp.GRB.MINIMIZE
            ptr = spec["indptr"]
            for i, (lower, upper) in enumerate(zip(spec["row_lower"], spec["row_upper"])):
                first, last = ptr[i:i + 2]
                row = gp.LinExpr(spec["values"][first:last], [x[j] for j in spec["indices"][first:last]])
                if lower is not None and upper is not None and lower == upper:
                    model.addConstr(row == lower)
                    continue
                if lower is not None:
                    model.addConstr(row >= lower)
                if upper is not None:
                    model.addConstr(row <= upper)
            model.update()
            if start is not None and mip:
                model.setAttr("Start", x, start)
            dimensions = [model.NumConstrs, model.NumVars, model.NumNZs]

            def observe(m, where):
                cb = gp.GRB.Callback
                if where == cb.MIPSOL:
                    events.append(dict(kind="candidate", seconds=m.cbGet(cb.RUNTIME),
                                       objective=m.cbGet(cb.MIPSOL_OBJ),
                                       candidate=m.cbGetSolution(x), bound=callback_bound(m.cbGet(cb.MIPSOL_OBJBND)),
                                       nodes=m.cbGet(cb.MIPSOL_NODCNT)))
                if where == cb.MIP:
                    now = m.cbGet(cb.RUNTIME)
                    bound = callback_bound(m.cbGet(cb.MIP_OBJBND))
                    if not events or now - events[-1]["seconds"] >= .025:
                        events.append(dict(kind="frontier", seconds=now, bound=bound,
                                           nodes=m.cbGet(cb.MIP_NODCNT)))
                if where == cb.MIPNODE and not any(e["kind"] == "root_node" for e in events):
                    if m.cbGet(cb.MIPNODE_NODCNT) == 0:
                        events.append(dict(kind="root_node", seconds=m.cbGet(cb.RUNTIME),
                                           bound=callback_bound(m.cbGet(cb.MIPNODE_OBJBND)), nodes=0))

            def callback_bound(value):
                return finite(value) if abs(value) < gp.GRB.INFINITY else None

            metadata = dict(input_dimensions=dimensions, switches=switches or {})
            try:
                if presolve:
                    with model.presolve() as reduced:
                        metadata["presolved_dimensions"] = [reduced.NumConstrs, reduced.NumVars, reduced.NumNZs]
                began = time.perf_counter()
                model.optimize(observe if observed else None)
                seconds = time.perf_counter() - began
            except gp.GurobiError as exc:
                status = "license_limit" if exc.errno == 10010 else "error"
                return dict(seconds=0, result=dict(status=status, errno=exc.errno, error=str(exc),
                                                   metadata=metadata))
            statuses = {gp.GRB.OPTIMAL: "optimal", gp.GRB.INFEASIBLE: "infeasible",
                        gp.GRB.INF_OR_UNBD: "infeasible_or_unbounded", gp.GRB.UNBOUNDED: "unbounded",
                        gp.GRB.TIME_LIMIT: "time_limit", gp.GRB.NODE_LIMIT: "node_limit",
                        gp.GRB.ITERATION_LIMIT: "iteration_limit", gp.GRB.NUMERIC: "numerical_failure"}
            candidate = model.getAttr("X", x) if model.SolCount else None
            bound = callback_bound(model.ObjBound) if mip else None
            return dict(seconds=seconds, result=dict(
                status=statuses.get(model.Status, f"gurobi_status_{model.Status}"),
                objective=model.ObjVal if candidate is not None else None,
                bound=bound, candidate=candidate,
                nodes=int(model.NodeCount), pivots=int(model.IterCount),
                barrier_iterations=int(model.BarIterCount),
                work=model.Work, version=".".join(map(str, gp.gurobi.version())),
                metadata=metadata, events=events,
            ))
