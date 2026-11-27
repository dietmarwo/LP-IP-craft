"""Recheck raw incumbent vectors and cross-check objectives against Gurobi."""
import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path

from run import CORRECTIONS, corrected_reference, digest, read, validate


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("manifest", type=Path)
    p.add_argument("journal", type=Path)
    p.add_argument("--reference-corrections", type=Path, default=CORRECTIONS)
    p.add_argument("--verified-journal", type=Path)
    a = p.parse_args()
    manifest = {(e["group"], e["name"]): e for e in read(a.manifest)["roster"]}
    records = defaultdict(list)
    runs = [json.loads(line) for line in a.journal.read_text().splitlines()]
    for r in runs:
        req = r["request"]
        records[(req["group"], req["case"])].append(r)
    corrections = read(a.reference_corrections) if a.reference_corrections else {}
    failures, unreferenced, applied = [], [], []
    checked_count = 0
    for key, observations in records.items():
        case = manifest[key]
        path = a.manifest.parent / case["file"]
        expected = case.get("expected")
        status = case.get("expected_status")
        expected = corrected_reference(case["name"], case["sha256"], expected, corrections)
        if case["sha256"] in corrections:
            applied.append(corrections[case["sha256"]])
        if digest(path) != case["sha256"]:
            raise ValueError(f"model file hash mismatch for {key}")
        spec = read(path)["model"]
        # Apply the adjudicated reference before selecting reference solves.
        # Historical journals may have flagged a correct point under an old target.
        reference = []
        for r in observations:
            if r["request"]["solver"] == "gurobi":
                point, issues = validate(spec, r["result"], expected, status)
                if not issues:
                    reference.append((r["result"]["status"], point))
        objectives = [point["objective"] for outcome, point in reference if outcome == "optimal"]
        statuses = {outcome for outcome, _ in reference} & {"optimal", "infeasible", "unbounded"}
        if len(statuses) > 1:
            raise ValueError(f"independent reference statuses disagree for {key}")
        if objectives:
            if expected is None:
                expected = objectives[0]
            if not all(math.isclose(expected, n, abs_tol=1e-6, rel_tol=1e-7) for n in objectives):
                raise ValueError(f"independent reference objectives disagree for {key}")
            status = "optimal"
        if expected is None and not statuses:
            unreferenced.append(key)
        for r in observations:
            if r["request"]["input_sha256"] != case["sha256"]:
                raise ValueError(f"input manifest hash mismatch for {key}")
            act = r["result"]["status"]
            outcome, issues = validate(spec, r["result"], expected, status)
            checked_count += outcome is not None
            if statuses == {"infeasible"} and outcome is not None:
                issues.append("candidate contradicts Gurobi infeasibility")
            if statuses in ({"infeasible"}, {"unbounded"}) and (
                    act in {"optimal", "infeasible", "unbounded"} and act not in statuses):
                issues.append("definitive status contradicts Gurobi")
            r.update(issues=issues, checked=outcome)
            if issues:
                failures.append(dict(group=key[0], case=key[1], solver=r["request"]["solver"],
                                     profile=r["request"]["profile"], status=act, issues=issues))
    print(json.dumps(dict(records=len(runs), input_cases=len(records), checked_candidates=checked_count,
                          failures=failures, unreferenced=unreferenced, reference_corrections=applied,
                          statuses=dict(Counter(r["result"]["status"] for r in runs))), indent=2))
    if failures:
        raise SystemExit(1)
    if a.verified_journal:
        with a.verified_journal.open("x") as out:
            for r in runs:
                out.write(json.dumps(r, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
