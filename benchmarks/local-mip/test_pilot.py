"""Regression checks using the release driver built by the reproduction guide."""
import json
import os
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

from replay_fixing import audit
from run import REPO, observation

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

    def test_scoring_the_same_records_gives_the_same_bytes(self):
        limits, records = (30, 5000, 250000), {}
        for split, case in (("public", "p"), ("heldout", "h"), ("development", "d")):
            entry = {"case": case, "split": split, "reference": 4.0, "sense": "minimize", "model_sha256": case}
            for arm, first in (("A", 8.0), ("B", 6.0)):
                name, expected = observation(entry, arm, 11, "trace", limits, "binary", 0, 0)
                work = {"phase": "root", "nodes": 1, "pivots": 1}
                events = [{"kind": "root_relaxation", "source": "search_root_lp", "seconds": 0.5,
                           "objective": 3.0, "bound": 3.0, **work},
                          {"kind": "checked_incumbent", "source": "rounding", "seconds": 1.0,
                           "objective": first, "bound": None, **work},
                          {"kind": "root_exit", "source": "disabled", "seconds": 2.0,
                           "objective": first, "bound": None, **work}]
                result = {"status": "node_limit", "objective": first, "bound": 3.0, "nodes": 1,
                          "pivots": 1, "seconds": 2.0, "rss_kib": 1, "metadata": []}
                records[name] = {**expected, "failure": None, "issues": [], "events": events, "result": result}
        with TemporaryDirectory() as tmp:
            path = Path(tmp)
            schedule = {name: {k: v for k, v in row.items() if k not in ("failure", "issues", "events", "result")}
                        for name, row in records.items()}
            (path / "run-config.txt").write_text(json.dumps({"observations": schedule}))
            for name, row in records.items():
                (path / name).write_text(json.dumps(row))
            outputs = [subprocess.run([sys.executable, HERE / "analyze.py", path], capture_output=True, text=True,
                                      check=True, env={**os.environ, "PYTHONHASHSEED": seed}).stdout
                       for seed in ("1", "2", "3", "4")]
        self.assertEqual(len(set(outputs)), 1)
        scored = json.loads(outputs[0])
        self.assertEqual(list(scored["comparisons"]), ["development", "heldout", "public", "external"])
        # 1 s without a point, then a six-decimal gap of (6 - 4) / 6 against (8 - 4) / 8.
        expected = ((1 * 2 + 29 * 0.333333) / 30 + 0.001) / ((1 * 2 + 29 * 0.5) / 30 + 0.001)
        self.assertAlmostEqual(scored["comparisons"]["heldout"]["B/A"]["complete"]["ratio"], expected, places=12)
        self.assertEqual(scored["pre_hook_pairs_verified"], 3)


if __name__ == "__main__":
    unittest.main()
