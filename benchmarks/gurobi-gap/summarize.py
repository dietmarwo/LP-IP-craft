"""Score paired, frozen model cohorts and retain all failure classes."""
import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import random
from statistics import geometric_mean, median

SHIFT = .01


def arm(record):
    return record["request"]["solver"] + ":" + record["request"]["profile"]


def solved(record):
    return (record["result"]["status"] == "optimal"
            and not record.get("issues") and (record.get("checked") or {}).get("feasible", False)
            and record["seconds"] <= record["request"]["settings"]["seconds"])


def penalized(record):
    return record["seconds"] if solved(record) else 2 * record["request"]["settings"]["seconds"]


def interval(values):
    rng = random.Random(91271)
    boot = sorted(sum(rng.choices(values, k=len(values)))/len(values) for _ in range(4000))
    return [math.exp(boot[100]), math.exp(boot[3899])]


def assess(records, baseline, candidate, static, reference):
    matrix = defaultdict(lambda: defaultdict(list))
    for rec in records:
        matrix[rec["request"]["case"]][arm(rec)].append(rec)
    picked = [name for name in (baseline, candidate, static, reference) if name is not None]
    if len(set(picked)) != len(picked):
        raise ValueError("each comparison arm must be distinct")
    reps = {len(r) for row in matrix.values() for r in row.values()}
    if len(reps) != 1 or any(set(row) != set(picked) for row in matrix.values()):
        raise ValueError("incomplete pair matrix")
    for case, row in matrix.items():
        observations = [r for obs in row.values() for r in obs]
        signatures = {
            (req["input_sha256"], req.get("start_sha256"), *(req["settings"].get(k)
             for k in ("seconds", "nodes", "pivots", "observe")))
            for req in (r["request"] for r in observations)
        }
        if len(signatures) != 1:
            raise ValueError(f"unmatched input, start or run controls on {case}")
        ref_reps = sorted(r["request"]["repetition"] for r in row[baseline])
        if len(set(ref_reps)) != len(ref_reps) or any(
                sorted(r["request"]["repetition"] for r in obs) != ref_reps for obs in row.values()):
            raise ValueError(f"unpaired repetitions on {case}")
    result = dict(cases=len(matrix), repeats=reps.pop(), arms={}, model_results={}, regressions=[])
    for key in picked:
        runs = [r for row in matrix.values() for r in row[key]]
        result["arms"][key] = dict(
            solved_models=sum(sum(map(solved, row[key])) > len(row[key])/2 for row in matrix.values()),
            solved_runs=sum(map(solved, runs)),
            statuses=dict(Counter(r["result"]["status"] for r in runs)),
            checks_failed=sum(bool(r.get("issues")) for r in runs),
            shifted_par2_geomean=geometric_mean(median(map(penalized, row[key]))+SHIFT for row in matrix.values()),
        )
    base_g, new_g = [], []
    paired = []
    families = defaultdict(list)
    for case, row in matrix.items():
        summaries = {}
        for key, obs in row.items():
            values = [r["seconds"] for r in obs if r["seconds"] is not None]
            status = dict(Counter(r["result"]["status"] for r in obs))
            summaries[key] = dict(time=median(values) if values else None, range=[min(values), max(values)] if values else None,
                                  par2=median(map(penalized, obs)), statuses=status,
                                  nodes=median([r["result"]["nodes"] for r in obs if r["result"].get("nodes") is not None]
                                               or [0]))
        result["model_results"][case] = summaries
        a, b, g = (summaries[key]["par2"] for key in (baseline, candidate, reference))
        d = math.log((b+SHIFT)/(a+SHIFT))
        paired.append(d)
        families[row[baseline][0]["request"]["family"]].append(d)
        if b > 1.25*a and b-a > .02:
            result["regressions"].append(dict(case=case, baseline=a, candidate=b))
        bad = {"license_limit", "unavailable", "worker_error"}
        if not set(summaries[reference]["statuses"]).intersection(bad):
            base_g.append((a+SHIFT)/(g+SHIFT))
            new_g.append((b+SHIFT)/(g+SHIFT))
    result["paired_native_ratio"] = math.exp(sum(paired)/len(paired))
    result["paired_native_ci95"] = interval(paired)
    if len(families) > 1:
        result["family_mean_ci95"] = interval([sum(v)/len(v) for v in families.values()])
    result["gurobi_coverage"] = len(base_g)
    if base_g:
        r0, r1 = geometric_mean(base_g), geometric_mean(new_g)
        result.update(gurobi_ratio_stock=r0, gurobi_ratio_candidate=r1)
        if r0 > 1:
            result["gurobi_log_gap_closure"] = 1-math.log(r1)/math.log(r0)
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("journal", type=Path)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--group", default="blind")
    p.add_argument("--candidate", default="candidate:adaptive+lattice")
    p.add_argument("--baseline", default="stock:default")
    p.add_argument("--no-static", action="store_true", help="compare two native builds with Gurobi")
    a = p.parse_args()
    data = [json.loads(line) for line in a.journal.read_text().splitlines()]
    selected = [rec for rec in data if rec["request"]["group"] == a.group]
    if not selected:
        p.error("selected group has no results")
    summary = assess(selected, a.baseline, a.candidate, None if a.no_static else "stock:gmi+mir", "gurobi:default")
    a.output.write_text(json.dumps(summary, indent=2, allow_nan=False) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k != "model_results"}, indent=2))


if __name__ == "__main__":
    main()
