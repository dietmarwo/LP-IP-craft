"""Summarize the original-model incumbent and bound histories."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "local-mip"))
from analyze import integral


def summarize(row, model):
    horizon = row["request"]["settings"]["seconds"]
    sign = 1 if model["model"]["sense"] == "minimize" else -1
    res = row["result"]
    events = sorted(res.get("events", []), key=lambda e: e["seconds"])
    if row.get("checked") and row["checked"]["feasible"]:
        events.append(dict(kind="checked_incumbent", seconds=row["seconds"],
                           objective=row["checked"]["objective"], bound=res.get("bound")))
    best_primal, best_dual, first, first_certified = None, None, None, None
    points, history = [], []
    gap_times = {"1%": None, "0.1%": None}
    for e in events:
        now = e["seconds"]
        if e["kind"] == "candidate":
            obj = e["checked"]["objective"] if e.get("checked", {}).get("feasible") else None
        elif e["kind"] in ("checked_incumbent", "frontier_sample"):
            obj = e.get("objective")
        else:
            obj = None
        if obj is not None and (best_primal is None or sign*obj < sign*best_primal):
            best_primal = obj
            points.append((now, sign*obj))
            if first is None:
                first = now
        if e["kind"] == "certified_incumbent" and first_certified is None:
            first_certified = now
        b = e.get("bound")
        if b is not None and (best_dual is None or sign*b > sign*best_dual):
            best_dual = b
        if best_primal is not None and best_primal != 0 and best_dual is not None:
            absolute = sign*(best_primal-best_dual)
            if absolute >= -1e-7:
                for tag, thresh in (("1%", .01), ("0.1%", .001)):
                    if gap_times[tag] is None and absolute/abs(best_primal) <= thresh:
                        gap_times[tag] = now
        history.append(dict(time=now, checked_incumbent=best_primal, bound=best_dual))
    checkpoints = {str(t): next((x for x in reversed(history) if x["time"] <= t), None)
                   for t in (1, 5, 10) if t <= horizon}
    reference = model.get("expected")
    return dict(case=row["request"]["case"], solver=row["request"]["solver"],
                profile=row["request"]["profile"], first_checked_seconds=first,
                first_certified_seconds=first_certified, time_to_relative_gap=gap_times,
                checkpoints=checkpoints,
                primal_integral=integral(points, sign*reference, horizon) if reference is not None else None)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("journal", type=Path)
    a = p.parse_args()
    rows = [json.loads(line) for line in a.journal.read_text().splitlines()]
    data = [summarize(row, json.loads(Path(row["request"]["case_path"]).read_text())) for row in rows]
    print(json.dumps(data, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
