"""The discovery data of lp-ip-job-worker for the tests: the JSON Schemas and
examples that the worker's ``describe`` embeds from
``crates/lp-ip-job-worker/discovery``."""
import json
import os
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
DISCOVERY = REPO / "crates" / "lp-ip-job-worker" / "discovery"


def _load(name):
    return json.loads((DISCOVERY / name).read_text(encoding="utf-8"))


PAYLOAD_SCHEMAS = {
    "lp-ip.solve/1": _load("lp-ip.solve-1.schema.json"),
    "lp-ip.evaluate/1": _load("lp-ip.evaluate-1.schema.json"),
}
INPUT_SCHEMAS = {"lp-ip.model/1": _load("lp-ip.model-1.schema.json")}
EXAMPLES = _load("examples.json")


def validator(schema):
    """A draft 2020-12 validator. jsonschema (the test extra) is required in CI
    (JOB_API_REQUIRE_SCHEMA=1); elsewhere its absence skips the test."""
    if os.environ.get("JOB_API_REQUIRE_SCHEMA") == "1":
        import jsonschema
    else:
        jsonschema = pytest.importorskip("jsonschema")
    return jsonschema.Draft202012Validator(schema)

