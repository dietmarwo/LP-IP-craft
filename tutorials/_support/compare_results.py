"""Compare a fresh chapter summary with the recorded one.

Timing fields are ignored. Integers, strings and booleans must match exactly;
floats must agree to a relative tolerance of 1e-9. Usage:

    python tutorials/_support/compare_results.py RECORDED.json NEW.json
"""

from __future__ import annotations

import json
import math
import sys

TIMING = ("seconds", "time", "elapsed", "wall", "duration")


def differences(recorded, new, path="", out=None):
    out = [] if out is None else out
    if isinstance(recorded, dict) and isinstance(new, dict):
        for key in sorted(set(recorded) | set(new)):
            if any(t in key.lower() for t in TIMING):
                continue
            if key not in recorded or key not in new:
                out.append(f"{path}/{key}: present in only one summary")
            else:
                differences(recorded[key], new[key], f"{path}/{key}", out)
    elif isinstance(recorded, list) and isinstance(new, list):
        if len(recorded) != len(new):
            out.append(f"{path}: length {len(recorded)} != {len(new)}")
        else:
            for i, (a, b) in enumerate(zip(recorded, new, strict=True)):
                differences(a, b, f"{path}[{i}]", out)
    elif isinstance(recorded, float) or isinstance(new, float):
        a, b = float(recorded), float(new)
        if not math.isclose(a, b, rel_tol=1e-9, abs_tol=1e-12):
            out.append(f"{path}: {recorded!r} != {new!r}")
    elif recorded != new:
        out.append(f"{path}: {str(recorded)[:80]!r} != {str(new)[:80]!r}")
    return out


def main(argv):
    if len(argv) != 3:
        raise SystemExit(__doc__)
    with open(argv[1]) as f:
        recorded = json.load(f)
    with open(argv[2]) as f:
        new = json.load(f)
    found = differences(recorded, new)
    for line in found[:20]:
        print(line)
    if found:
        raise SystemExit(f"{len(found)} differences from the recorded summary")
    print("matches the recorded summary")


if __name__ == "__main__":
    main(sys.argv)
