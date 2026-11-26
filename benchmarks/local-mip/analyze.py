"""Score checked incumbent streams and compare matched instance/seed pairs."""
import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import random
from statistics import mean, median

from run import read_record

SHIFT = 0.001


def penalty(value, reference):
    if value is None or reference is None:
        return 2.0
    if (value < 0 < reference) or (reference < 0 < value):
        return 1.0
    return max(0., round((value - reference) / max(1., abs(value), abs(reference)), 6))


def integral(points, reference, horizon, coverage=None):
    """Integrate the right-continuous incumbent step function on [0, horizon)."""
    observed = horizon if coverage is None else min(horizon, max(0., coverage))
    current, previous, area = None, 0., 0.
    for seconds, value in points:
        if seconds >= observed:
            break
        area += (seconds - previous) * penalty(current, reference)
        current, previous = value, seconds
    area += (observed - previous) * penalty(current, reference)
    area += 2. * (horizon - observed)
    return area / horizon


def measurements(row):
    horizon = row["settings"]["seconds"]
    sign = 1 if row["sense"] == "minimize" else -1
    events = row["events"]
    result = row["result"] or {}
    checked = [e for e in events if e["kind"] == "checked_incumbent" and e["seconds"] < horizon]
    point = checked[0] if checked else None
    final = checked[-1] if checked else None
    proofs = [e for e in events if e["kind"] == "certified_incumbent" and e["seconds"] < horizon]
    root = next((e for e in events if e["kind"] == "root_exit"), None)
    root_points = [e for e in checked if e["phase"] == "root"]
    local_improvements = [e for e in checked if e["source"] == "local_mip"]
    first_time = point["seconds"] if point else None
    ref = row["reference"]
    alerts = [e for e in checked if ref is not None and sign * (e["objective"] - ref) < -1e-8 * max(1., abs(ref))]
    stages = defaultdict(lambda: [0, 0, 0.])
    for event in events:
        if event["kind"] == "stage":
            counts = stages[event["source"]]
            counts[0] += event["stage_nodes"]
            counts[1] += event["stage_pivots"]
            counts[2] += event["stage_seconds"]
    prefix = "diagnostics.root_local_search."
    local_work = {k[len(prefix):]: v for k, v in result.get("metadata", []) if k.startswith(prefix)}
    points = [(e["seconds"], sign * e["objective"]) for e in checked]
    oriented_ref = None if ref is None else sign * ref
    incomplete = bool(row["failure"] or row["issues"] or root is None)
    coverage = events[-1]["seconds"] if events else 0.
    return {"case": row["case"], "split": row["split"], "arm": row["settings"]["arm"],
            "seed": row["settings"]["seed"], "integral": integral(points, oriented_ref, horizon),
            "conservative_integral": integral(points, oriented_ref, horizon, coverage if incomplete else None),
            "first_seconds": first_time, "capped_first_seconds": first_time if point else horizon,
            "first_point": point, "final_point": final, "first_certified": proofs[0] if proofs else None,
            "checkpoints": {str(t): next((e["objective"] for e in reversed(checked) if e["seconds"] < t), None)
                            for t in (1, 5, horizon)},
            "root_objective": root_points[-1]["objective"] if root_points else None, "root": root,
            "local_improvements": local_improvements, "local_work": local_work, "stages": stages,
            "final_objective": result.get("objective"), "status": result.get("status"), "safe_bound": result.get("bound"),
            "nodes": result.get("nodes"), "pivots": result.get("pivots"), "rss_kib": result.get("rss_kib"),
            "runtime": result.get("seconds"), "incomplete": incomplete, "failure": row["failure"],
            "issues": row["issues"], "reference_alerts": alerts}


def ratio(logs):
    return math.exp(mean(logs)) if logs else None


def compare(rows, base, pilot, conservative=False):
    pairs = defaultdict(dict)
    for r in rows:
        if r["arm"] in (base, pilot):
            key = (r["case"], r["seed"])
            if r["arm"] in pairs[key]:
                raise ValueError(f"duplicate result for {key}/{r['arm']}")
            pairs[key][r["arm"]] = r
    differences, lefts, rights = defaultdict(list), defaultdict(list), defaultdict(list)
    count, included = 0, []
    field = "conservative_integral" if conservative else "integral"
    for (case, seed), pair in pairs.items():
        if not all(arm in pair for arm in (base, pilot)):
            continue
        a, b = pair[base], pair[pilot]
        if not conservative and (a["incomplete"] or b["incomplete"]):
            continue
        count += 1
        left, right = math.log(a[field] + SHIFT), math.log(b[field] + SHIFT)
        differences[case].append(right - left)
        lefts[case].append(left)
        rights[case].append(right)
        included.append((a, b))
    cases = {k: mean(v) for k, v in differences.items()}
    values = list(cases.values())
    rng = random.Random(20261126)
    interval = None
    if len(values) > 1:
        boot = sorted(ratio(rng.choices(values, k=len(values))) for _ in range(4000))
        interval = [boot[100], boot[-101]]
    counts = Counter("better" if v < -1e-12 else "worse" if v > 1e-12 else "tied" for v in values)
    n = len(values)
    return {"arms": [base, pilot], "complete_pairs": count, "scheduled_pair_keys": len(pairs),
            "instances": n, "ratio": ratio(values), "bootstrap_95": interval,
            "instance_ratios": {k: math.exp(v) for k, v in sorted(cases.items())}, "counts": dict(counts),
            "without_one": {k: ratio([v for case, v in cases.items() if case != k]) for k in cases} if n > 1 else {},
            "sgm": [math.exp(mean(mean(x) for x in series.values())) - SHIFT if series else None for series in (lefts, rights)],
            "both_feasible": sum(a["first_point"] is not None and b["first_point"] is not None for a, b in included),
            "coverage_gained": sum(a["first_point"] is None and b["first_point"] is not None for a, b in included),
            "coverage_lost": sum(a["first_point"] is not None and b["first_point"] is None for a, b in included),
            "capped_first_median": [median(r[i]["capped_first_seconds"] for r in included) if included else None for i in range(2)]}


def self_test():
    assert integral([(2., 8.), (6., 4.)], 4., 10.) == 0.6
    assert integral([], 4., 10.) == 2.
    assert integral([(0., -4.), (1., 4.)], 4., 10.) == 0.1
    assert integral([(0., 4.), (10., 0.)], 4., 10.) == 0.
    assert integral([(0., 4.)], 4., 10., 5.) == 1.
    assert integral([(2., 0.)], 0., 10.) == 0.4
    assert penalty(-5., -4.) == 0. and penalty(-3., -4.) == 0.25


def check_root_prefix(rows):
    """Confirm paired arms receive equivalent state at the shared root hook."""
    groups = defaultdict(dict)
    for row in rows:
        prefix = []
        for event in row["events"]:
            prefix.append(tuple(event[k] for k in ("kind", "source", "objective", "bound", "nodes", "pivots")))
            if event["kind"] == "root_relaxation":
                groups[(row["case"], row["settings"]["seed"])][row["settings"]["arm"]] = prefix
                break
    verified = 0
    for key, arms in groups.items():
        for a, b in (("A", "B"), ("C", "D"), ("A", "C")):
            if a in arms and b in arms:
                if arms[a] != arms[b]:
                    raise ValueError(f"different pre-hook trajectory for {key} in {a}/{b}")
                verified += 1
    return verified


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path, nargs="?")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--legacy", action="store_true", help="allow a first-edition campaign without a frozen run list")
    args = parser.parse_args()
    self_test()
    if args.campaign is None:
        return
    config = json.loads((args.campaign / "run-config.txt").read_text())
    roster = config.get("observations")
    files = {p.name: p for p in args.campaign.glob("*.json")}
    if roster is None:
        if not args.legacy:
            parser.error("missing schedule (first-edition archives require --legacy)")
        raw = [json.loads(path.read_text()) for path in sorted(files.values())]
        if any(r["binary_sha256"] != config["binary_sha256"] for r in raw):
            parser.error("mixed solver builds")
    else:
        missing, unexpected = roster.keys() - files.keys(), files.keys() - roster.keys()
        if missing or unexpected:
            parser.error(f"incomplete or mixed campaign: {len(missing)} missing, {len(unexpected)} unexpected")
        raw = [read_record(files[name], expected) for name, expected in roster.items()]
    if not raw or any(row["settings"]["mode"] == "plain" or row["settings"]["seconds"] is None for row in raw):
        parser.error("scoring requires timed observations with incumbent traces")
    prefix_checks = check_root_prefix(raw)
    rows = sorted(map(measurements, raw),
                  key=lambda r: (r["split"], r["case"], r["seed"], r["arm"]))
    # Sorted, so that the same records give the same bytes in every process.
    subsets = {s: [r for r in rows if r["split"] == s] for s in sorted({r["split"] for r in rows})}
    subsets["external"] = subsets.get("heldout", []) + subsets.get("development", [])
    comparisons = {s: {f"{b}/{a}": {"complete": compare(rs, a, b),
                                     "conservative": compare(rs, a, b, conservative=True)}
                       for a, b in [("A", "B"), ("C", "D")] if any(r["arm"] == b for r in rs)}
                   for s, rs in subsets.items()}
    output = json.dumps({"pre_hook_pairs_verified": prefix_checks, "comparisons": comparisons, "rows": rows},
                        allow_nan=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
