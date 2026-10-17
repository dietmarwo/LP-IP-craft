"""Pinned original public MIPs and independent planted packing holdouts.

Requires NumPy/SciPy for untimed public reference solves. No bounds are guessed.
"""
import argparse
import hashlib
import json
import math
import random
import shutil
from pathlib import Path

from prepare_public import parse


def clean(model):
    return {k: [v if not isinstance(v, float) or math.isfinite(v) else None for v in a]
            if isinstance(a, list) else a for k, a in model.items()}


def packing(n, degree, seed):
    rng = random.Random(seed)
    columns, rows = list(range(n)), list(range(n))
    rng.shuffle(columns)
    rng.shuffle(rows)
    objective, values, indices, indptr, upper = [0.0] * n, [], [], [0], []
    for i in rows:
        row = {}
        offsets = [0, 1, 7] if degree == 3 else [k * k + 3 * k for k in range(9)]
        for k, offset in enumerate(offsets):
            j = columns[(i + offset) % n]
            row[j] = row.get(j, 0.0) + (2.0 if degree == 3 and k == 0 else 4.0 if k == 0 else 1.0)
        dual = float(rng.randrange(1, 8))
        for j, value in sorted(row.items()):
            indices.append(j)
            values.append(value)
            objective[j] += dual * value
        upper.append(sum(row.values()))
        indptr.append(len(indices))
    model = dict(sense="maximize", objective=objective, objective_offset=0.0,
                 lower=[0.0] * n, upper=[2.0] * n, row_lower=[None] * n,
                 row_upper=upper, kinds=["continuous"] * n,
                 indptr=indptr, indices=indices, values=values)
    return dict(model=model, expected=sum(objective), provenance=dict(
        family="permuted packing", degree=degree, seed=seed,
        oracle="x=1 and positive planted row duals have identical objectives"))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--names", nargs="+", default=["lseu", "p0548", "rgn", "gt2", "p01", "small_mip"])
    a = p.parse_args()
    import numpy as np
    import scipy
    from scipy.optimize import Bounds, LinearConstraint, milp
    from scipy.sparse import csr_matrix
    public, fresh = a.output / "public", a.output / "fresh-packing"
    public.mkdir(parents=True, exist_ok=True)
    fresh.mkdir(parents=True, exist_ok=True)
    manifest = []
    for name in a.names:
        src = a.source / (name + ".mps")
        m = parse(src)
        A = csr_matrix((m["values"], m["indices"], m["indptr"]), shape=(len(m["row_lower"]), len(m["objective"])))
        r = milp(m["objective"], integrality=np.array([k != "continuous" for k in m["kinds"]], int),
                 bounds=Bounds(m["lower"], m["upper"]),
                 constraints=LinearConstraint(A, m["row_lower"], m["row_upper"]), options={"time_limit": 30.0})
        assert r.success, (name, r.message)
        violation = max(float(np.max(np.r_[np.array(m["lower"]) - r.x, r.x - np.array(m["upper"]),
                                            np.array(m["row_lower"]) - A @ r.x, A @ r.x - np.array(m["row_upper"])])), 0.0)
        assert violation <= 1e-7, (name, violation)
        provenance = dict(name=name, source="HiGHS 1.15.0/check/instances/" + src.name,
                          source_sha256=hashlib.sha256(src.read_bytes()).hexdigest(),
                          transform="identical CSR; null preserves infinity; no finite-bound substitution",
                          oracle="SciPy " + scipy.__version__ + " milp/HiGHS", oracle_status=int(r.status),
                          reference_primal_violation=violation)
        payload = dict(model=clean(m), expected=float(r.fun + m["objective_offset"]), provenance=provenance)
        dest = public / (name + ".json")
        dest.write_text(json.dumps(payload, indent=2) + "\n")
        shutil.copyfile(src, public / src.name)
        manifest.append(dict(file=dest.name, sha256=hashlib.sha256(dest.read_bytes()).hexdigest(),
                             n=len(m["objective"]), m=len(m["row_lower"]), nnz=len(m["values"]),
                             integer_columns=sum(k != "continuous" for k in m["kinds"]), **provenance))
        print(name, payload["expected"], flush=True)
    (public / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    records = []
    for seed in [20261017, 20261123]:
        for n, degree in [(1024, 3), (4096, 3), (1024, 9)]:
            dest = fresh / f"packing-d{degree}-{n}-seed{seed}.json"
            dest.write_text(json.dumps(packing(n, degree, seed), indent=2) + "\n")
            records.append(dict(file=dest.name, sha256=hashlib.sha256(dest.read_bytes()).hexdigest()))
    (fresh / "manifest.json").write_text(json.dumps(records, indent=2) + "\n")


if __name__ == "__main__":
    main()
