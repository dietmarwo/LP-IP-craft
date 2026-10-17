"""Sequential 0.23 evidence runs; build/check everything before invoking."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

p = argparse.ArgumentParser()
for name in ["baseline", "native", "highs", "baseline-profile", "native-profile", "baseline-replay", "native-replay"]:
    p.add_argument("--" + name, required=True)
p.add_argument("--rounds", type=int, default=3)
p.add_argument("--output", type=Path)
p.add_argument("--only", action="append")
a = p.parse_args()
root = Path(__file__).resolve().parent.parent
out = a.output or root / "docs/packing-ftran-mip-diagnostics/data"
out.mkdir(parents=True, exist_ok=True)
binary = ["--binary", "baseline=" + a.baseline, "--binary", "native=" + a.native, "--binary", "highs=" + a.highs]
common = ["--rounds", str(a.rounds)]
env = os.environ.copy()
env["LPIP_HIGHS_SOLVER"] = "simplex"
groups = [
    ("extended", "compare_packing_mip.py", binary),
    ("standard", "compare_three.py", ["--baseline", a.baseline, "--native", a.native, "--highs", a.highs]),
    ("holdout", "compare_packing_mip.py", binary + ["--holdout"]),
    ("aspect", "compare_packing_mip.py", binary + ["--case", "aspect-wide:128", "--case", "aspect-tall:128", "--case", "aspect-wide:512", "--case", "aspect-tall:512"]),
    ("large", "compare_large.py", ["--baseline", a.baseline, "--native", a.native, "--highs", a.highs]),
    ("public", "compare_packing_mip.py", binary + ["--public-corpus", str(root / "docs/packing-mip/corpus")]),
    ("new-public", "compare_packing_mip.py", binary + ["--public-corpus", str(root / "docs/packing-ftran-mip-diagnostics/corpus/public")]),
    ("fresh-packing", "compare_packing_mip.py", binary + ["--public-corpus", str(root / "docs/packing-ftran-mip-diagnostics/corpus/fresh-packing")]),
    ("fresh-mip", "compare_packing_mip.py", binary + ["--case", "multi-knapsack:12018", "--case", "graph-mip:13018", "--case", "mixed-overload:12016", "--case", "integer-knapsack:48", "--case", "long-row-mip:112"]),
    ("profiles", "profile_kernels.py", ["--before", a.baseline_profile, "--after", a.native_profile]),
    ("replay", "replay_compare.py", ["--binary", "baseline=" + a.baseline_replay, "--binary", "native=" + a.native_replay, "--corpus", str(root / "docs/irregular-lu/corpus")]),
    ("mip-diagnostics", "diagnose_mip.py", ["--binary", a.native, "--corpus", str(root / "docs/packing-mip/corpus"), "--corpus", str(root / "docs/packing-ftran-mip-diagnostics/corpus/public"), "--ablations"]),
]
for name, script, args in groups:
    if a.only and name not in a.only:
        continue
    print("START", name, flush=True)
    filename = name if name == "mip-diagnostics" else "final-" + name
    with (out / (filename + ".log")).open("w") as log:
        subprocess.run([sys.executable, str(root / "benchmarks" / script), *args,
                        *common, "--output", str(out / (filename + ".json"))],
                       check=True, cwd=root, stdout=log, stderr=subprocess.STDOUT, env=env)
    print("DONE", name, flush=True)
