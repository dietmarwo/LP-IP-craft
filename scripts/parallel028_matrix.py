"""Reproduce final scaling, immutable-upstream, held-out and mixed-budget timings."""

import argparse
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--baseline", type=Path, required=True)
    p.add_argument("--baseline-python", type=Path, required=True)
    p.add_argument("--output", type=Path, default=ROOT / "docs/parallel-028/data")
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=True)

    def run(name, cases, baseline=False, repeats=3):
        command = [
            sys.executable,
            str(ROOT / "scripts/parallel028.py"),
            str(a.output / (name + ".json")),
            "final=" + str(a.binary.resolve()),
        ]
        if baseline:
            command += ["baseline=" + str(a.baseline.resolve())]
        command += ["--repeats", str(repeats)]
        for c in cases:
            command += ["--case", ":".join(map(str, c))]
        with (a.output / (name + ".log")).open("w") as log:
            subprocess.run(
                command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True
            )
        print("COMPLETED", name, flush=True)

    primary = []
    for family, sizes in [
        ("packing", [512, 1024]),
        ("irregular", [128, 256]),
        ("selection", [20, 28]),
        ("dense", [192, 384]),
    ]:
        for n in sizes:
            for t in [1, 2, 4, 8]:
                primary.append([family, n, "single", t, 1, 1])
    for mode in ["batch", "skewbatch"]:
        for t in [1, 2, 4, 8]:
            primary.append(["packing", 128, mode, 1, t, 64])
    for mode in ["components", "skewcomponents"]:
        for t in [1, 2, 4, 8]:
            primary.append(["packing", 128, mode, 1, t, 1])
    for mode, family, n in [("tree", "selection", 28), ("residual", "ring", 64)]:
        for t in [1, 2, 4, 8]:
            primary.append([family, n, mode, 1, t, 1])
    run("final-matrix", primary)
    common = [c for c in primary if c[3] == 1 and c[4] in [1, 8] and c[0] != "ring"]
    run("common-upstream", common, True)
    held = []
    for family, sizes in [
        ("packing", [768, 1536]),
        ("irregular", [160, 320]),
        ("selection", [24, 30]),
        ("dense", [256, 512]),
    ]:
        for n in sizes:
            for t in [1, 8]:
                held.append([family, n, "single", t, 1, 1])
    for n in [96, 160]:
        for mode in ["components", "skewcomponents"]:
            for t in [1, 8]:
                held.append(["packing", n, mode, 1, t, 1])
    run("heldout", held)
    run(
        "factor-scaling",
        [
            ["factor", n, "single", t, 1, 3]
            for n in [384, 512, 768, 1024]
            for t in [1, 2, 4, 8]
        ],
        repeats=7,
    )
    run(
        "large-solver-scaling",
        [["dense", n, "single", t, 1, 1] for n in [768, 1024] for t in [1, 2, 4, 8]],
    )
    run(
        "mixed-batches",
        [
            ["dense", 384, "batch", inner, outer, 16]
            for inner, outer in [(1, 1), (1, 2), (1, 4), (1, 8), (2, 4), (4, 2), (8, 1)]
        ],
    )
    run(
        "scheduling-confirmation",
        [
            ["packing", 128, "skewbatch", 1, 8, 36],
            ["packing", 128, "batch", 1, 8, 128],
            ["packing", 128, "skewcomponents", 1, 8, 1],
        ],
        True,
        repeats=7,
    )
    for kind in ["public", "spoc"]:
        command = [
            sys.executable,
            str(ROOT / "scripts/parallel028_applications.py"),
            "--baseline-python",
            str(a.baseline_python.resolve()),
            "--output",
            str(a.output / ("applications-" + kind + ".json")),
            "--repeats",
            "3",
            "--" + kind + "-only",
        ]
        with (a.output / ("applications-" + kind + ".log")).open("w") as log:
            subprocess.run(
                command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True
            )
        print("COMPLETED", "applications-" + kind, flush=True)


if __name__ == "__main__":
    main()
