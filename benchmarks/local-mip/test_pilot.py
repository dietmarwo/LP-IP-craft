"""Regression checks using the release driver built by the reproduction guide."""
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

from replay_fixing import audit
from run import REPO

HERE = Path(__file__).resolve().parent
BINARY = REPO / "benchmarks/target/release/local-mip-pilot"


def solve(case, arm, seed, mode, node_limit=100):
    fixture, = REPO.glob(f"docs/**/corpus/**/{case}.json")
    run = subprocess.run([BINARY, fixture, arm, "none", str(node_limit), "250000", str(seed), mode],
                         check=True, text=True, capture_output=True, timeout=40)
    records = [json.loads(line) for line in run.stdout.splitlines()]
    return records[:-1], records[-1]


class PilotReviewTests(unittest.TestCase):
    def test_first_local_incumbent_preserves_early_tree_rounding(self):
        a, baseline = solve("bell5", "A", 29, "trace")
        b, pilot = solve("bell5", "B", 29, "trace")
        first = next(e for e in b if e["kind"] == "checked_incumbent")
        self.assertEqual((first["phase"], first["source"]), ("root", "local_mip"))
        self.assertTrue(any(e["source"] == "rounding" and e["phase"] == "tree"
                            and e["kind"] == "checked_incumbent" and e["objective"] < first["objective"] for e in b))
        self.assertTrue(any(e["source"] == "rounding" and e["kind"] == "checked_incumbent" for e in a))
        self.assertEqual(pilot["objective"], baseline["objective"])
        self.assertTrue(pilot["checked"])

    def test_native_entry_point_replays_observed_seed(self):
        roots = []
        for seed in (11, 29):
            _, plain = solve("gt2", "B", seed, "plain")
            trace, observed = solve("gt2", "B", seed, "trace")
            for field in ("status", "candidate", "objective", "bound", "nodes", "pivots"):
                self.assertEqual(plain[field], observed[field])
            self.assertEqual(dict(plain["metadata"])["local_mip_seed"], str(seed))
            roots.append(next(e["objective"] for e in trace if e["kind"] == "root_exit"))
        self.assertNotEqual(*roots)

    def test_fixing_replay_respects_fractional_integer_bounds(self):
        model = dict(lower=[0.25], upper=[1.75], kinds=["integer"], values=[1], indices=[0],
                     indptr=[0, 1], row_lower=[None], row_upper=[2], objective=[3], objective_offset=0.5)
        with TemporaryDirectory() as tmp:
            path = Path(tmp) / "model.json"
            path.write_text(json.dumps(model))
            result = json.loads(json.dumps(audit(path)))
            self.assertEqual((result["stop"], result["checked_point_exact"],
                              result["checked_objective_exact"]), ("point", ["1"], "7/2"))
            model["upper"] = [0.75]
            path.write_text(json.dumps(model))
            self.assertEqual(audit(path)["stop"], "empty_integer_domain")

    def test_missing_unexpected_and_mislabeled_observations_fail_scoring(self):
        with TemporaryDirectory() as tmp:
            path = Path(tmp)
            cfg = {"observations": {"run.json": {"case": "intended"}}}
            (path / "run-config.txt").write_text(json.dumps(cfg))
            def reject(reason):
                run = subprocess.run([sys.executable, HERE / "analyze.py", path], capture_output=True, text=True)
                self.assertNotEqual(run.returncode, 0)
                self.assertIn(reason, run.stderr)
            reject("1 missing")
            (path / "run.json").write_text('{"case":"wrong"}')
            reject("run identity differs")
            (path / "extra.json").write_text("{}")
            reject("1 unexpected")


if __name__ == "__main__":
    unittest.main()
