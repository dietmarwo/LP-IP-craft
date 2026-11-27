"""Audit the blind public conversions with an independent HiGHS MPS reader."""
import argparse
import gzip
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "benchmarks/local-mip"))
from prepare import equivalent, parse_mps

from freeze import digest, read_json


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("manifest", type=Path)
    p.add_argument("public", type=Path)
    a = p.parse_args()
    records = []
    for entry in read_json(a.manifest)["roster"]:
        if not entry["origin"].startswith("Data-Miplib3@"):
            continue
        path = a.manifest.parent / entry["file"]
        model = read_json(path)["model"]
        source = a.public / (entry["name"] + ".gz")
        if digest(path) != entry["sha256"] or digest(source) != entry["origin"].split("sha256=")[1]:
            raise ValueError("model or source hash changed")
        _, names = parse_mps(gzip.decompress(source.read_bytes()).decode("ascii"))
        version = equivalent(model, names, source)
        records.append(dict(case=entry["name"], model_sha256=entry["sha256"], highs=version))
    print(json.dumps(records, indent=2))


if __name__ == "__main__":
    main()
