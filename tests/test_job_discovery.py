"""The worker's discovery data (`payload_schemas`, `input_schemas` and
`examples` of `describe`): every example and tutorial payload and model
validates, and the schemas reject what the worker rejects.

Runs without the worker binary. With LP_IP_JOB_WORKER set to a built
lp-ip-job-worker (the job API CI does that), it also checks that `describe`
embeds exactly these files.
"""
import copy
import json
import os
import subprocess
from pathlib import Path

import pytest

from discovery_files import EXAMPLES, INPUT_SCHEMAS, PAYLOAD_SCHEMAS, REPO, validator

TUTORIALS = REPO / "tutorials" / "job-api"
MODEL = "lp-ip.model/1"


def errors(schema, value):
    return [e.message for e in validator(schema).iter_errors(value)]


def tutorial_jobs():
    jobs = [(p, json.loads(p.read_text(encoding="utf-8"))) for p in sorted(TUTORIALS.glob("*.json"))]
    return [(p, j) for p, j in jobs if j.get("schema_version") == "optimization.job/0.1"]


def test_schemas_are_draft_2020_12_with_ids():
    for key, schema in [*PAYLOAD_SCHEMAS.items(), *INPUT_SCHEMAS.items()]:
        validator(schema).check_schema(schema)
        assert schema["$schema"] == "https://json-schema.org/draft/2020-12/schema"
        kind = "input" if key in INPUT_SCHEMAS else "payload"
        assert schema["$id"] == f"urn:optimization-jobs:{kind}:{key}"


@pytest.mark.parametrize("example", EXAMPLES, ids=lambda e: e["description"][:30])
def test_example_payloads_and_inputs_validate(example):
    assert example["profile"] in PAYLOAD_SCHEMAS
    assert errors(PAYLOAD_SCHEMAS[example["profile"]], example["payload"]) == []
    assert example["resources"] == {"cpu_slots": 1, "outer_workers": 1, "inner_threads": 1}
    for item in example["inputs"]:
        assert item["format"] in INPUT_SCHEMAS
        assert errors(INPUT_SCHEMAS[item["format"]], item["content"]) == []


def test_every_profile_has_an_example():
    assert {e["profile"] for e in EXAMPLES} == set(PAYLOAD_SCHEMAS)


def test_tutorial_payloads_and_models_validate():
    jobs = tutorial_jobs()
    assert len(jobs) == 4
    models = set()
    for path, job in jobs:
        assert errors(PAYLOAD_SCHEMAS[job["payload_schema"]], job["payload"]) == [], path
        for item in job["inputs"]:
            models.add(item["file"])
            model = json.loads((path.parent / item["file"]).read_text(encoding="utf-8"))
            assert errors(INPUT_SCHEMAS[MODEL], model) == [], item["file"]
    assert len(models) == 3


SOLVE = EXAMPLES[1]["payload"]
EVALUATE = EXAMPLES[2]["payload"]
LP = EXAMPLES[0]["inputs"][0]["content"]


def edited(value, path, new):
    """A copy of value with the entry at path replaced (or removed for ...)."""
    value = copy.deepcopy(value)
    target = value
    for key in path[:-1]:
        target = target[key]
    if new is ...:
        del target[path[-1]]
    else:
        target[path[-1]] = new
    return value


@pytest.mark.parametrize("schema, value, path, new", [
    ("lp-ip.solve/1", SOLVE, ["unknown"], 1),
    ("lp-ip.solve/1", SOLVE, ["presolve"], ...),
    ("lp-ip.solve/1", SOLVE, ["presolve"], "yes"),
    ("lp-ip.solve/1", SOLVE, ["starts"], []),
    ("lp-ip.solve/1", SOLVE, ["starts"], [[]]),
    ("lp-ip.solve/1", SOLVE, ["starts"], [[1.0, "x"]]),
    ("lp-ip.solve/1", SOLVE, ["node_limit"], 0),
    ("lp-ip.solve/1", SOLVE, ["iteration_limit"], -1),
    ("lp-ip.solve/1", SOLVE, ["iteration_limit"], 1.5),
    ("lp-ip.solve/1", SOLVE, ["feasibility_tolerance"], 0.0),
    ("lp-ip.solve/1", SOLVE, ["integrality_tolerance"], 0.5),
    ("lp-ip.solve/1", SOLVE, ["model_input"], ""),
    ("lp-ip.solve/1", SOLVE, ["model_input"], ".."),
    ("lp-ip.solve/1", SOLVE, ["model_input"], "a/b"),
    ("lp-ip.evaluate/1", EVALUATE, ["points"], []),
    ("lp-ip.evaluate/1", EVALUATE, ["points"], [[]]),
    ("lp-ip.evaluate/1", EVALUATE, ["starts"], [None]),
    ("lp-ip.evaluate/1", EVALUATE, ["integrality_tolerance"], ...),
    (MODEL, LP, ["schema_version"], "lp-ip.model/2"),
    (MODEL, LP, ["sense"], "max"),
    (MODEL, LP, ["objective"], []),
    (MODEL, LP, ["objective_offset"], None),
    (MODEL, LP, ["kinds", 0], "bool"),
    (MODEL, LP, ["variable_upper", 0], "inf"),
    (MODEL, LP, ["row_lower"], ...),
    (MODEL, LP, ["comment"], "unknown field"),
    (MODEL, LP, ["matrix", "indptr"], []),
    (MODEL, LP, ["matrix", "indices", 0], -1),
    (MODEL, LP, ["matrix", "format"], "csc"),
])
def test_schemas_reject_what_the_worker_rejects(schema, value, path, new):
    schema = {**PAYLOAD_SCHEMAS, **INPUT_SCHEMAS}[schema]
    assert errors(schema, value) == []
    assert errors(schema, edited(value, path, new)) != []


def test_describe_embeds_these_files():
    worker = os.environ.get("LP_IP_JOB_WORKER")
    if not worker:
        pytest.skip("set LP_IP_JOB_WORKER to a built lp-ip-job-worker")
    d = json.loads(subprocess.run([str(Path(worker)), "describe"], capture_output=True, text=True,
                                  check=True).stdout)
    assert set(d["profiles"]) == set(PAYLOAD_SCHEMAS)
    assert d["payload_schemas"] == PAYLOAD_SCHEMAS
    assert d["input_schemas"] == INPUT_SCHEMAS
    assert d["examples"] == EXAMPLES
    assert len(json.dumps(d)) < 64 * 1024
