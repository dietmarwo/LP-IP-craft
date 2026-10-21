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


def model_from_json(model):
    """An ``lp_ip_craft.Model`` from an ``lp-ip.model/1`` value (null bounds are infinite)."""
    import lp_ip_craft

    inf = float("inf")

    def side(values, infinite):
        return [infinite if v is None else v for v in values]

    return lp_ip_craft.Model(
        objective=model["objective"], objective_offset=model["objective_offset"], sense=model["sense"],
        lower=side(model["variable_lower"], -inf), upper=side(model["variable_upper"], inf),
        kinds=model["kinds"], row_lower=side(model["row_lower"], -inf), row_upper=side(model["row_upper"], inf),
        indptr=model["matrix"]["indptr"], indices=model["matrix"]["indices"], values=model["matrix"]["values"])
