"""Generate signed sparse integer equality chains with an independent DP oracle."""
import argparse
from fractions import Fraction
from pathlib import Path
import random

from freeze import digest, write


def instance(seed):
    rng = random.Random(seed)
    periods = 7
    coefficients = [rng.choice((Fraction(3, 4), Fraction(4, 5), Fraction(9, 10)))
                    for _ in range(periods-1)]
    costs = [rng.randint(-12, 12) for _ in range(2*periods-1)]
    ascending = seed % 2 == 0
    best = min if ascending else max
    lower = [rng.randint(-39, -32) for _ in range(periods)] + [-7]*(periods-1)
    upper = [rng.randint(31, 39) for _ in range(periods)] + [8]*(periods-1)
    offset = seed / 8
    table = {x: costs[0]*x for x in range(lower[0], upper[0]+1)}
    rows = []
    for i, a in enumerate(coefficients):
        y = periods+i
        row = [(i, float(a)), (i+1, -1.), (y, 1.)]
        rows.append(row)
        updated = {}
        for x, partial in table.items():
            base = a*x
            if base.denominator != 1:
                continue
            for z in range(lower[y], upper[y]+1):
                nxt = int(base)+z
                if lower[i+1] <= nxt <= upper[i+1]:
                    value = partial + costs[y]*z + costs[i+1]*nxt
                    updated[nxt] = best(value, updated.get(nxt, value))
        table = updated
    expected = best(table.values())+offset
    order = list(range(len(costs)))
    rng.shuffle(order)
    rng.shuffle(rows)
    position = {old: j for j, old in enumerate(order)}
    indices, values, indptr = [], [], [0]
    for row in rows:
        for j, a in sorted((position[j], a) for j, a in row):
            indices.append(j)
            values.append(a)
        indptr.append(len(indices))
    model = dict(
        sense="minimize" if ascending else "maximize", objective_offset=offset,
        objective=[costs[j] for j in order], lower=[lower[j] for j in order],
        upper=[upper[j] for j in order], kinds=["integer"]*len(costs),
        row_lower=[0.]*(periods-1), row_upper=[0.]*(periods-1), indptr=indptr,
        indices=indices, values=values,
    )
    return dict(model=model, expected=expected, expected_status="optimal", seed=seed)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    a = p.parse_args()
    a.output.mkdir(parents=True)
    roster = []
    for seed in (317, 318, 329, 330, 331, 342):
        name = f"chain-{seed}"
        path = a.output / (name+".json")
        payload = instance(seed)
        write(path, payload)
        roster.append(dict(name=name, group="equality", family="sparse equality chains", file=path.name,
                           sha256=digest(path), expected=payload["expected"], expected_status="optimal"))
    write(a.output / "manifest.json", dict(roster=roster))


if __name__ == "__main__":
    main()
