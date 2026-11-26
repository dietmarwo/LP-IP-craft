"""Freeze public and mipfeas input bytes, references and eligibility."""
import argparse
import gzip
import hashlib
import json
import math
from collections import Counter
from pathlib import Path
import subprocess
import sys
import tempfile
from urllib.request import urlopen
import csv

import highspy

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from mps_to_model import MpsError, parse_mps, read_text  # noqa: E402

REFERENCE_REV = "4e3bf0021c178189b26cce19ca65cbe78470d5e3"
PLANNED = {
    "development": ("qap10", "gen-ip054", "assign1-5-8", "markshare2"),
    "heldout": ("cod105", "neos-3024952-loue", "enlight_hard", "graphdraw-domain",
                "b1c1s1", "50v-10", "timtab1", "csched008"),
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def shape(model):
    return {"columns": len(model["objective"]), "rows": len(model["row_lower"]),
            "nnz": len(model["values"]), "kinds": dict(Counter(model["kinds"])),
            "sense": model["sense"], "objective_offset": model.get("objective_offset", 0.)}


def equivalent(model, names, path):
    reader = highspy.Highs()
    reader.setOptionValue("output_flag", False)
    reader.setOptionValue("small_matrix_value", 1e-12)
    with tempfile.TemporaryDirectory() as temp:
        mps = Path(temp) / "input.mps"
        mps.write_bytes(gzip.decompress(path.read_bytes()))
        status = reader.readModel(str(mps))
    if status == highspy.HighsStatus.kError:
        raise ValueError("independent MPS reader failed")
    lp = reader.getLp()
    names_cols = list(lp.col_names_)
    names_rows = list(lp.row_names_)
    def permutation(expected, observed):
        lookup = {name: i for i, name in enumerate(observed)}
        if len(lookup) != len(observed) or len(expected) != len(observed) or set(expected) != lookup.keys():
            raise ValueError("independent MPS row/column names differ or repeat")
        return [lookup[n] for n in expected]

    cols = permutation(names["columns"], names_cols)
    rows = permutation(names["rows"], names_rows)

    def equal(f, a):
        return math.isclose(f, a, rel_tol=1e-12, abs_tol=1e-12)

    for field, got, order in [("lower", lp.col_lower_, cols), ("upper", lp.col_upper_, cols),
                              ("row_lower", lp.row_lower_, rows), ("row_upper", lp.row_upper_, rows),
                              ("objective", lp.col_cost_, cols)]:
        sign = -1 if field in {"lower", "row_lower"} else 1
        if not all((sign * got[i] >= 1e30 if f is None else equal(f, got[i]))
                   for f, i in zip(model[field], order, strict=True)):
            raise ValueError(f"independent MPS {field} differs")
    if (model["sense"] == "minimize") != (lp.sense_ == highspy.ObjSense.kMinimize) or not equal(model["objective_offset"], lp.offset_):
        raise ValueError("independent MPS objective sense or offset differs")
    for k, i in zip(model["kinds"], cols, strict=True):
        if (k != "continuous") != (bool(lp.integrality_) and lp.integrality_[i] == highspy.HighsVarType.kInteger):
            raise ValueError("independent MPS variable integrality differs")
    a = lp.a_matrix_
    if a.format_ != highspy.MatrixFormat.kColwise:
        raise ValueError("unexpected matrix layout from independent MPS reader")
    found = {(a.index_[k], j): a.value_[k] for j in range(lp.num_col_)
             for k in range(a.start_[j], a.start_[j + 1])}
    for i, r in enumerate(rows):
        for k in range(model["indptr"][i], model["indptr"][i + 1]):
            j = cols[model["indices"][k]]
            if not equal(model["values"][k], found.pop((r, j), 0.)):
                raise ValueError(f"independent MPS coefficient differs at {names_rows[r]}/{names_cols[j]}")
    if any(abs(x) > 1e-12 for x in found.values()):
        raise ValueError("independent MPS matrix has additional nonzeros")
    return reader.version()


def eligibility(binary, fixture):
    try:
        output = subprocess.check_output([str(binary), str(fixture), "stock", "5", "1000", "50000", "11", "plain"],
                                         text=True, timeout=20)
    except subprocess.TimeoutExpired:
        return {"status": "watchdog", "message": "20-second eligibility watchdog"}
    result = json.loads(output.splitlines()[-1])
    return {"status": result["status"], "message": result["message"]}


def stratum(row):
    binaries, integers, continuous = map(int, row[3:6])
    if continuous:
        return "mixed_continuous"
    if binaries and integers:
        return "mixed_discrete"
    return "binary" if binaries else "general_integer"


def download(url, path):
    if path.exists():
        return
    for attempt in range(3):
        try:
            with urlopen(url, timeout=35) as handle:
                data = handle.read()
            gzip.decompress(data)
            path.write_bytes(data)
            return
        except (OSError, EOFError):
            if attempt == 2:
                raise


def external_record(name, split, reference, data, binary):
    mps = data / "mps" / f"{name}.mps.gz"
    source = f"https://miplib.zib.de/WebData/instances/{mps.name}"
    download(source, mps)
    record = {"case": name, "split": split, "source": source, "mps_sha256": sha(mps),
              "reference": float(reference[10]), "reference_kind": reference[1],
              "stratum": stratum(reference)}
    try:
        model, names = parse_mps(read_text(mps))
        model["lower"] = model.pop("variable_lower")
        model["upper"] = model.pop("variable_upper")
        model.update(model.pop("matrix"))
        record.update(shape(model), warnings=names["warnings"])
        record["mps_audit_highs_version"] = equivalent(model, names, mps)
    except (MpsError, ValueError) as error:
        record["excluded"] = str(error)
        return record
    fixture = data / "models" / f"{name}.json"
    fixture.write_text(json.dumps(model, separators=(",", ":"), allow_nan=False) + "\n")
    record.update(fixture=str(fixture.relative_to(data)), model_sha256=sha(fixture),
                  eligibility=eligibility(binary, fixture))
    outcome = record["eligibility"]
    if outcome["status"] == "unsupported" and "requires finite variable bounds" in outcome["message"]:
        record["excluded"] = "native finite-bound admission"
    return record


def public_records(binary):
    directories = ("docs/packing-mip/corpus", "docs/packing-ftran-mip-diagnostics/corpus/public")
    for directory in directories:
        for fixture in sorted((REPO / directory).glob("*.json")):
            if fixture.stem == "manifest":
                continue
            payload = json.loads(fixture.read_text())
            model = payload["model"]
            if all(k == "continuous" for k in model["kinds"]) and fixture.stem != "avgas":
                continue
            yield {"case": fixture.stem, "split": "public" if fixture.stem != "avgas" else "control",
                   "fixture": str(fixture.relative_to(REPO)), "model_sha256": sha(fixture),
                   "mps_sha256": sha(fixture.with_suffix(".mps")), "reference": payload["expected"],
                   "eligibility": eligibility(binary, fixture), **shape(model)}
    fixture = REPO / "benchmarks/rust-solvers/heldout/infeasible-mip0.json"
    yield {"case": fixture.stem, "split": "control", "fixture": str(fixture.relative_to(REPO)),
           "model_sha256": sha(fixture), "reference": None, "eligibility": eligibility(binary, fixture),
           **shape(json.loads(fixture.read_text())["model"])}


def frozen_inputs(manifest):
    # Eligibility wall times can differ. The accepted roster, conversion and
    # references are the identity used by the experiment.
    fields = ("case", "split", "replaces", "excluded", "fixture", "sense", "objective_offset",
              "reference", "reference_kind", "stratum", "source", "model_sha256", "mps_sha256")
    return ({k: manifest[k] for k in ("repo", "mipfeas_reference_revision", "mipfeas_reference_sha256", "roster_sha256")},
            [{k: r[k] for k in fields if k in r} for r in manifest["models"]])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--stock", type=Path, required=True)
    parser.add_argument("--verify-against", type=Path, help="freeze accepted inputs to the archived manifest")
    args = parser.parse_args()
    frozen = json.loads(args.verify_against.read_text()) if args.verify_against else None
    data = args.data.resolve()
    binary = args.stock.resolve()
    for name in ("mps", "models"):
        (data / name).mkdir(parents=True, exist_ok=True)
    upstream = args.upstream / "bench"
    if subprocess.check_output(["git", "-C", args.upstream, "rev-parse", "HEAD"], text=True).strip() != REFERENCE_REV:
        parser.error("upstream revision differs from the frozen reference")
    roster = set((upstream / "instances_mipfeas.txt").read_text().splitlines())
    reference_file = upstream / "mipfeas_optimal_objective.csv"
    with reference_file.open() as stream:
        refs = {r[0]: r for r in csv.reader(stream) if r[0] in roster}
    planned = {name for names in PLANNED.values() for name in names}
    records = list(public_records(binary))
    used = planned | {r["case"] for r in records}
    alternatives = sorted(n for n in refs.keys() - used
                          if all(int(refs[n][i]) <= cap for i, cap in [(2, 4200), (6, 4500), (7, 60000)]))
    for split, names in PLANNED.items():
        for name in names:
            possible = [name] + [n for n in alternatives if n not in used and stratum(refs[n]) == stratum(refs[name])]
            for candidate in possible:
                used.add(candidate)
                record = external_record(candidate, split, refs[candidate], data, binary)
                if candidate != name:
                    record["replaces"] = name
                records.append(record)
                print(candidate, record.get("excluded", record.get("eligibility")), flush=True)
                if not record.get("excluded"):
                    break
    manifest = {"repo": subprocess.check_output(["git", "-C", REPO, "rev-parse", "HEAD"], text=True).strip(),
                "mipfeas_reference_revision": REFERENCE_REV, "mipfeas_reference_sha256": sha(reference_file),
                "roster_sha256": sha(upstream / "instances_mipfeas.txt"), "models": records}
    if frozen is not None and frozen_inputs(manifest) != frozen_inputs(frozen):
        parser.error("frozen inputs differ in source hashes, conversion, selection or references")
    (data / "manifest.json").write_text(json.dumps(manifest, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
