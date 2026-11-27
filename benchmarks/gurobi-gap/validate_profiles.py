"""Exercise both opt-in mechanisms with the repository's existing oracles."""
from dataclasses import replace
from pathlib import Path
import runpy
import sys

import lp_ip_craft as lp

ROOT = Path(__file__).resolve().parents[2]
original_solve = lp.solve


def solve_with_policy(model, **args):
    args["backend_options"] = replace(args["backend_options"], adaptive_root=True,
                                      integer_equality_presolve=True)
    return original_solve(model, **args)


if __name__ == "__main__":
    choice = {"exact": "validate_native.py", "scipy": "validate_random.py"}
    path = ROOT / "examples" / choice[sys.argv.pop(1)]
    lp.solve = solve_with_policy
    runpy.run_path(str(path), run_name="__main__")
