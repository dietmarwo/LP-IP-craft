"""Seeded synthetic MIPs of six standard classes, frozen before tuning.

Group `development` (seeds 1-4) is used while iterating; group `test`
(seeds 101-104) is only evaluated at the end. Every model is written in
the common JSON format (null bounds are infinite) with its seed and class.
After generation, `expected` was filled once with HiGHS 1.15.1's proven
optimum (one thread, zero gap, 60 s cap; all 48 proved). Rerunning this
script resets it to null.
"""
import hashlib, json, random
from pathlib import Path

base = Path(__file__).resolve().parent / 'synthetic'
base.mkdir(exist_ok=True)


def dense(rows, n):
    indptr, indices, values = [0], [], []
    for row in rows:
        for j, v in enumerate(row):
            if v:
                indices.append(j)
                values.append(float(v))
        indptr.append(len(indices))
    return indptr, indices, values


def model(c, lo, up, kinds, rows, rl, ru, sense):
    indptr, indices, values = dense(rows, len(c))
    inf = lambda v: None if v in (float('inf'), float('-inf')) else float(v)
    return dict(objective=[float(v) for v in c], objective_offset=0., lower=[inf(v) for v in lo],
                upper=[inf(v) for v in up], kinds=kinds, sense=sense,
                row_lower=[inf(v) for v in rl], row_upper=[inf(v) for v in ru],
                indptr=indptr, indices=indices, values=values)


INF = float('inf')


def mknap(r):
    # Correlated multidimensional knapsack.
    n, m = 35, 5
    a = [[r.randint(10, 100) for _ in range(n)] for _ in range(m)]
    c = [sum(a[i][j] for i in range(m)) // m + r.randint(0, 10) for j in range(n)]
    b = [sum(row) // 2 for row in a]
    return model(c, [0] * n, [1] * n, ['binary'] * n, a, [-INF] * m, b, 'maximize')


def setcover(r):
    n, m = 160, 60
    rows = []
    for _ in range(m):
        row = [1 if r.random() < 0.06 else 0 for _ in range(n)]
        row[r.randrange(n)] = 1
        rows.append(row)
    c = [r.randint(1, 20) for _ in range(n)]
    return model(c, [0] * n, [1] * n, ['binary'] * n, rows, [1] * m, [INF] * m, 'minimize')


def cfl(r):
    # Capacitated facility location: y_i open, x_ij fraction served.
    f, k = 8, 20
    open_cost = [r.randint(300, 700) for _ in range(f)]
    demand = [r.randint(5, 35) for _ in range(k)]
    cap = [r.randint(60, 140) for _ in range(f)]
    ship = [[r.randint(1, 40) * demand[j] // 10 for j in range(k)] for _ in range(f)]
    n = f + f * k
    x = lambda i, j: f + i * k + j
    c = open_cost + [ship[i][j] for i in range(f) for j in range(k)]
    rows, rl, ru = [], [], []
    for j in range(k):
        row = [0] * n
        for i in range(f):
            row[x(i, j)] = 1
        rows.append(row); rl.append(1); ru.append(1)
    for i in range(f):
        for j in range(k):
            row = [0] * n
            row[x(i, j)] = 1; row[i] = -1
            rows.append(row); rl.append(-INF); ru.append(0)
        row = [0] * n
        for j in range(k):
            row[x(i, j)] = demand[j]
        row[i] = -cap[i]
        rows.append(row); rl.append(-INF); ru.append(0)
    kinds = ['binary'] * f + ['continuous'] * (f * k)
    return model(c, [0] * n, [1] * n, kinds, rows, rl, ru, 'minimize')


def gap(r):
    agents, jobs = 5, 22
    cost = [[r.randint(5, 50) for _ in range(jobs)] for _ in range(agents)]
    use = [[r.randint(5, 25) for _ in range(jobs)] for _ in range(agents)]
    n = agents * jobs
    x = lambda i, j: i * jobs + j
    rows, rl, ru = [], [], []
    for j in range(jobs):
        row = [0] * n
        for i in range(agents):
            row[x(i, j)] = 1
        rows.append(row); rl.append(1); ru.append(1)
    for i in range(agents):
        row = [0] * n
        for j in range(jobs):
            row[x(i, j)] = use[i][j]
        rows.append(row); rl.append(-INF); ru.append(int(0.8 * sum(use[i]) / agents))
    c = [cost[i][j] for i in range(agents) for j in range(jobs)]
    return model(c, [0] * n, [1] * n, ['binary'] * n, rows, rl, ru, 'minimize')


def lotsize(r):
    # Capacitated single-item lot sizing: x_t <= C y_t, s_{t-1} + x_t - s_t = d_t.
    t = 24
    d = [r.randint(0, 40) for _ in range(t)]
    cap = r.randint(60, 110)
    setup = [r.randint(80, 200) for _ in range(t)]
    hold = [r.randint(1, 4) for _ in range(t)]
    prod = [r.randint(1, 5) for _ in range(t)]
    # columns: y (t, binary), x (t), s (t)
    n = 3 * t
    rows, rl, ru = [], [], []
    for k in range(t):
        row = [0] * n
        row[t + k] = 1; row[k] = -cap
        rows.append(row); rl.append(-INF); ru.append(0)
        row = [0] * n
        if k:
            row[2 * t + k - 1] = 1
        row[t + k] = 1; row[2 * t + k] = -1
        rows.append(row); rl.append(d[k]); ru.append(d[k])
    c = setup + prod + hold
    up = [1] * t + [cap] * t + [sum(d)] * t
    kinds = ['binary'] * t + ['continuous'] * (2 * t)
    return model(c, [0] * n, up, kinds, rows, rl, ru, 'minimize')


def intcover(r):
    # General-integer covering, gt2-like: min c x, A x >= b, x in {0..8}.
    n, m = 30, 12
    rows = [[r.randint(0, 9) if r.random() < 0.4 else 0 for _ in range(n)] for _ in range(m)]
    b = [r.randint(40, 120) for _ in range(m)]
    c = [r.randint(50, 400) for _ in range(n)]
    return model(c, [0] * n, [8] * n, ['integer'] * n, rows, b, [INF] * m, 'minimize')


CLASSES = dict(mknap=mknap, setcover=setcover, cfl=cfl, gap=gap, lotsize=lotsize, intcover=intcover)
records = []
for group, seeds in [('development', range(1, 5)), ('test', range(101, 105))]:
    for name, build in CLASSES.items():
        for seed in seeds:
            payload = dict(model=build(random.Random(f'{name}-{seed}')), expected=None,
                           provenance=dict(family=name, seed=seed, group=group))
            path = base / f'{group}-{name}-{seed}.json'
            path.write_text(json.dumps(payload, indent=1) + '\n')
            records.append(dict(file=path.name, group=group, sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
(base / 'manifest.json').write_text(json.dumps(records, indent=2) + '\n')
print(len(records), 'models')
