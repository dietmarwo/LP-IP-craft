"""Rerun the unchanged frozen numerical policy matrix into new 0.23 evidence."""
import argparse
import subprocess
import sys
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("--output", type=Path)
a = p.parse_args()
root = Path(__file__).resolve().parent.parent
out = a.output or root / "docs/packing-ftran-mip-diagnostics/data"
out.mkdir(parents=True, exist_ok=True)
inputs = root / "docs/performance-ten/data/oracle-inputs.json"
policies = [("default", []), ("ft", ["--variant", "ft"]), ("no-fix", ["--no-fixing"]),
            ("refactor1", ["--variant", "refactor1", "--suite", "exact"]),
            ("workers4", ["--variant", "workers4", "--suite", "components"]),
            ("devex", ["--variant", "devex"]), ("root-cuts", ["--variant", "root-cuts"]),
            ("factor-replay", ["--variant", "factor-replay"]), ("row-pricing", ["--variant", "row-pricing"])]
for name, args in policies:
    print("START", name, flush=True)
    with (out / f"oracle-{name}.log").open("w") as log:
        subprocess.run([sys.executable, str(root / "benchmarks/oracle_worker.py"), str(inputs),
                        "native", str(out / f"oracle-{name}.json"), *args], check=True, stdout=log)
    print("DONE", name, flush=True)
with (out / "bounds.log").open("w") as log:
    subprocess.run([sys.executable, str(root / "benchmarks/check_bounds.py"), str(root),
                    "native", str(out / "bounds.json")], check=True, stdout=log)
