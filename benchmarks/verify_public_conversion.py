"""Compare prepared CSR to HiGHS's own original MPS reader, without solving."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--reader", type=Path, required=True)
p.add_argument("--corpus", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
rows = []
for path in sorted(a.corpus.glob("*.json")):
    if path.name == "manifest.json":
        continue
    prepared = json.loads(path.read_text())["model"]
    original = path.with_suffix(".mps")
    reference = json.loads(subprocess.check_output([str(a.reader.resolve()), str(original)], text=True))
    differences = [key for key in prepared if prepared[key] != reference[key]]
    rows.append(dict(model=path.stem, original_sha256=hashlib.sha256(original.read_bytes()).hexdigest(),
                     equal_fields=[key for key in prepared if key not in differences], differences=differences))
out = dict(reference="Pinned HiGHS 1.15.0 original MPS reader; no solve/presolve",
           reader_sha256=hashlib.sha256(a.reader.read_bytes()).hexdigest(), records=rows)
a.output.write_text(json.dumps(out, indent=2) + "\n")
assert all(not row["differences"] for row in rows), rows
print("Original MPS conversions verified:", len(rows))
