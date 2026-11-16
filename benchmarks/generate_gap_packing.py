"""Small binary packing cases and independent HiGHS optima for the gap panel."""

from pathlib import Path
import json
import numpy as np
from scipy.optimize import Bounds, LinearConstraint, milp

ROOT = Path(__file__).resolve().parents[1]
folder = ROOT / "docs/benchmarks/gap-limits/corpus"
folder.mkdir(parents=True, exist_ok=True)
for n in [32, 48, 64]:
    rng = np.random.default_rng(20261116 + n)
    m = n // 3
    A = np.zeros((m, n))
    for j in range(n):
        rows = rng.choice(m, 3, replace=False)
        A[rows, j] = rng.integers(1, 10, 3)
    profit = rng.integers(1, 21, n).astype(float)
    rhs = np.floor(0.4 * A.sum(axis=1))
    reference = milp(
        -profit,
        integrality=np.ones(n),
        bounds=Bounds(np.zeros(n), np.ones(n)),
        constraints=LinearConstraint(A, np.full(m, -np.inf), rhs),
        options={"time_limit": 30.0, "mip_rel_gap": 0.0},
    )
    assert reference.success, reference.message
    ptr = [0]
    indices = []
    values = []
    for row in A:
        cols = np.flatnonzero(row)
        indices.extend(map(int, cols))
        values.extend(map(float, row[cols]))
        ptr.append(len(indices))
    model = dict(
        sense="maximize",
        objective=profit.tolist(),
        objective_offset=0.0,
        lower=[0.0] * n,
        upper=[1.0] * n,
        row_lower=[None] * m,
        row_upper=rhs.tolist(),
        kinds=["binary"] * n,
        indptr=ptr,
        indices=indices,
        values=values,
    )
    record = dict(
        model=model,
        expected=float(-reference.fun),
        provenance=dict(
            generator="benchmarks/generate_gap_packing.py",
            seed=20261116 + n,
            oracle="scipy.optimize.milp / HiGHS",
            oracle_status=int(reference.status),
        ),
    )
    (folder / f"binary-packing-n{n}.json").write_text(json.dumps(record, indent=2) + "\n")
    print(n, record["expected"], flush=True)
