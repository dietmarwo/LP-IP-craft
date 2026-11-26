"""Replay the fixing and interval-propagation part of FPR on frozen model JSON."""
from fractions import Fraction
import json
import math
from pathlib import Path
import sys

class Exhausted(Exception):
    pass


def audit(path):
    model = json.loads(Path(path).read_text())
    model = model.get("model", model)
    def exact(value):
        return None if value is None else Fraction(value)

    bounds = [[exact(l), exact(u)] for l, u in zip(model["lower"], model["upper"])]
    integer = [kind != "continuous" for kind in model["kinds"]]
    initial = [b.copy() for b in bounds]
    for j, (lo, hi) in enumerate(bounds):
        if integer[j]:
            bounds[j] = [None if lo is None else math.ceil(lo),
                         None if hi is None else math.floor(hi)]
    terms = [[(model["indices"][k], Fraction(model["values"][k]))
              for k in range(s, t) if model["values"][k]]
             for s, t in zip(model["indptr"], model["indptr"][1:])]
    row_bounds = [(exact(l), exact(u)) for l, u in zip(model["row_lower"], model["row_upper"])]
    incidence = [0] * len(bounds)
    for row in terms:
        for j, _ in row:
            incidence[j] += 1
    limit = 64 * len(model["values"])
    stats = {"terms": 0, "cap": limit, "branches": 0, "contradictions": 0,
             "tightenings": 0, "max_fixed": 0, "branch_trace": []}

    if any(lo is not None and hi is not None and lo > hi for lo, hi in bounds):
        stats.update(stop="empty_integer_domain", contradictions=1)
        return stats

    def charge(n):
        if stats["terms"] + n > limit:
            raise Exhausted
        stats["terms"] += n

    def propagate(box):
        while True:
            changed = False
            for i, (row, (lhs, rhs)) in enumerate(zip(terms, row_bounds)):
                charge(3 * len(row))
                def activity(j, a, side):
                    value = box[j][side]
                    return None if value is None else a * value

                min_terms = [activity(j, a, int(a < 0)) for j, a in row]
                max_terms = [activity(j, a, int(a > 0)) for j, a in row]
                lo_inf, hi_inf = min_terms.count(None), max_terms.count(None)
                lo_sum = sum(v for v in min_terms if v is not None)
                hi_sum = sum(v for v in max_terms if v is not None)
                if ((rhs is not None and lo_inf == 0 and lo_sum > rhs)
                        or (lhs is not None and hi_inf == 0 and hi_sum < lhs)):
                    return i
                for (j, a), mn, mx in zip(row, min_terms, max_terms):
                    lo, hi = box[j]

                    def tighten_lower(value):
                        new = math.ceil(value) if integer[j] else value
                        return new if lo is None else max(lo, new)

                    def tighten_upper(value):
                        new = math.floor(value) if integer[j] else value
                        return new if hi is None else min(hi, new)

                    if rhs is not None and lo_inf == (mn is None):
                        q = (rhs - (lo_sum - (mn or 0))) / a
                        if a > 0:
                            hi = tighten_upper(q)
                        else:
                            lo = tighten_lower(q)
                    if lhs is not None and hi_inf == (mx is None):
                        q = (lhs - (hi_sum - (mx or 0))) / a
                        if a < 0:
                            hi = tighten_upper(q)
                        else:
                            lo = tighten_lower(q)
                    if lo is not None and hi is not None and lo > hi:
                        return i
                    if box[j] != [lo, hi]:
                        box[j] = [lo, hi]
                        stats["tightenings"] += 1
                        changed = True
            stats["max_fixed"] = max(stats["max_fixed"], sum(lo is not None and lo == hi for lo, hi in box))
            if not changed:
                return None

    def dive(box, depth):
        row = propagate(box)
        if row is not None:
            stats["contradictions"] += 1
            if len(stats["branch_trace"]) < 12:
                stats["branch_trace"].append({"depth": depth, "contradictory_row": row})
            return None
        free = [(hi - lo, -incidence[j], j) for j, (lo, hi) in enumerate(box)
                if integer[j] and lo is not None and hi is not None and hi > lo]
        if not free:
            if any(lo is None or hi is None or lo != hi for lo, hi in box):
                return None
            return [lo for lo, hi in box]
        if depth == 64:
            return None
        _, _, j = min(free)
        lo, hi = box[j]
        for new_box in ([lo, lo], [lo + 1, hi]):
            stats["branches"] += 1
            if len(stats["branch_trace"]) < 12:
                stats["branch_trace"].append({"depth": depth, "column": j, "domain": list(map(float, new_box))})
            child = box.copy()
            child[j] = new_box
            x = dive(child, depth + 1)
            if x is not None:
                return x
        return None

    try:
        x = dive(bounds, 0)
        stats["stop"] = "point" if x is not None else "depth_or_tree"
        if x is not None:
            for j, (value, (lo, hi)) in enumerate(zip(x, initial, strict=True)):
                if ((lo is not None and value < lo) or (hi is not None and value > hi)
                        or (integer[j] and value != int(value))):
                    raise ValueError("returned point violates an original column domain")
            for row, (lhs, rhs) in zip(terms, row_bounds):
                value = sum(a * x[j] for j, a in row)
                if (lhs is not None and value < lhs) or (rhs is not None and value > rhs):
                    raise ValueError("returned point violates an original row")
            objective = Fraction(model.get("objective_offset", 0)) + sum(
                Fraction(c) * v for c, v in zip(model["objective"], x, strict=True))
            stats["checked_objective_exact"] = str(objective)
            stats["checked_point_exact"] = list(map(str, x))
    except Exhausted:
        stats["stop"] = "effort"
    return stats


if __name__ == "__main__":
    print(json.dumps({Path(p).stem: audit(p) for p in sys.argv[1:]}, indent=2))
