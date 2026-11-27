"""Freeze the case roster and input hashes before screening policies."""
import argparse
import gzip
import hashlib
import json
import math
from pathlib import Path
import random
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "benchmarks"))
from prepare_public import parse
sys.path.insert(0, str(ROOT / "benchmarks/rust-solvers"))
from synthetic import CLASSES


PUBLIC_SHA = "36c786b3274f75d9a0062471589813b92d3da91b"
# The catalog identifies the optimum. Related configurations are kept in one
# group for sensitivity analysis, and none of these models is used for tuning.
PUBLIC = {
    "enigma": ("binary feasibility", 0),
    "fixnet6": ("fixed-charge network", 3983),
    "mod008": ("machine loading", 307),
    "noswot": ("mixed general integer", -41),
    "pp08a": ("mixed fixed charge", 7350),
    "set1ch": ("capacitated lot sizing", 54537.75),
    "stein27": ("Stein set covering", 18),
    "stein45": ("Stein set covering", 30),
    "vpm1": ("VPM models", 20),
    "vpm2": ("VPM models", 13.75),
    "misc03": ("mixed binary", 3360),
}
DEVELOPMENT = "bell5 egout flugpl gt2 lseu p0548 p01 rgn".split()
LPS = "afiro adlittle israel e226 etamacro 25fv47".split()


def read_json(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, data):
    path.write_text(json.dumps(data, indent=2, allow_nan=False) + "\n")


def clean_bounds(model):
    for key in ("lower", "upper", "row_lower", "row_upper"):
        model[key] = [x if math.isfinite(x) else None for x in model[key]]
    return model


def freeze(output, public):
    output.mkdir(parents=True)
    models = output / "models"
    models.mkdir()
    entries = []
    unresolved = []
    def add(group, name, family, payload, origin):
        path = models / (group + "-" + name + ".json")
        write(path, payload)
        m = payload["model"]
        rows = sum(1 + (l is not None and u is not None and l != u)
                   for l, u in zip(m["row_lower"], m["row_upper"]) if l is not None or u is not None)
        entries.append(dict(group=group, name=name, family=family, file=str(path.relative_to(output)),
                            sha256=digest(path), variables=len(m["objective"]), rows=len(m["row_lower"]),
                            gurobi_size_eligible=rows <= 2000 and len(m["objective"]) <= 2000,
                            expected=payload.get("expected"), expected_status=payload.get("expected_status"),
                            origin=origin))
    for name in DEVELOPMENT + LPS + ["avgas"]:
        path = next(ROOT.glob("docs/*/corpus/**/" + name + ".json"))
        payload = read_json(path)
        payload["expected_status"] = "optimal"
        add("development" if name in DEVELOPMENT else "lp", name, "existing", payload, str(path.relative_to(ROOT)))
        if name in DEVELOPMENT:
            payload["model"]["kinds"] = ["continuous"] * len(payload["model"]["objective"])
            payload["expected"] = None
            add("relax", name, "existing", payload, str(path.relative_to(ROOT)))
    for path in sorted((ROOT / "benchmarks/rust-solvers/heldout").glob("*.json")):
        if path.name != "manifest.json":
            payload = read_json(path)
            add("regression", path.stem, "existing", payload, str(path.relative_to(ROOT)))
    generator = ROOT / "benchmarks/rust-solvers/synthetic.py"
    for family, build in sorted(CLASSES.items()):
        for seed in (229, 233, 239, 241):
            add("blind", f"{family}-{seed}", family,
                dict(model=build(random.Random(f"{family}-{seed}")), expected_status="optimal"),
                f"{generator.relative_to(ROOT)} seed {seed}")
    sha = subprocess.check_output(["git", "-C", public, "rev-parse", "HEAD"], text=True).strip()
    if sha != PUBLIC_SHA:
        raise ValueError(f"public checkout must be {PUBLIC_SHA}, found {sha}")
    already = {p.stem for p in ROOT.glob("docs/*/corpus/**/*.json")}
    local_pilot = (ROOT / "docs/local-mip-pilot/input-manifest.json").read_text()
    for name, (family, expected) in sorted(PUBLIC.items()):
        origin = public / (name + ".gz")
        if name in already or re.search(rf'"{re.escape(name)}(?:\.mps)?"', local_pilot):
            unresolved.append([name, "used in an earlier pilot"])
            continue
        try:
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / (name + ".mps")
                path.write_bytes(gzip.decompress(origin.read_bytes()))
                m = clean_bounds(parse(path))
        except (ValueError, AssertionError, KeyError) as exc:
            unresolved.append([name, f"conversion rejected: {exc.args!r}"])
            continue
        add("blind", name, family, dict(model=m, expected=expected, expected_status="optimal"),
            f"Data-Miplib3@{sha}/{origin.name} sha256={digest(origin)}")
    write(output / "manifest.json", dict(public_revision=sha, source_base=subprocess.check_output(
        ["git", "-C", ROOT, "rev-parse", "HEAD"], text=True).strip(), roster=entries, excluded=unresolved))
    print(f"Frozen {len(entries)} models, {sum(e['group']=='blind' for e in entries)} blind")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--public", required=True, type=Path)
    a = p.parse_args()
    freeze(a.output.resolve(), a.public.resolve())
