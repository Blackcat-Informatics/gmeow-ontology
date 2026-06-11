"""Schema compilation gates (issue #57).

Validates the OWL → LinkML pipeline and the downstream generator fan-out.
These tests are pure-Python (no Docker) and exercise the full compile path.

Marked ``ci_only``: the LinkML + JSON-Schema/Pydantic/TS/GraphQL/OpenAPI
generation is a heavy *secondary external-export* transformation (~45 s), so it
runs in CI and ``make test`` but is excluded from the fast ``make check`` gate.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
import yaml

from gmeow_tools.config import SCHEMAS_DIR
from gmeow_tools.generator import run
from gmeow_tools.gts_views import load_fold
from gmeow_tools.schema_compile import (
    _LINKML_FILE,
    SchemaGenerator,
    emit_linkml,
    gen_graphql,
    gen_json_schema,
    gen_openapi,
    gen_pydantic,
    gen_typescript,
)

pytestmark = pytest.mark.ci_only


def test_emit_linkml_produces_expected_structure() -> None:
    """The LinkML schema dict must contain classes, slots, and enums."""
    schema, warnings = emit_linkml(load_fold())

    assert schema["name"] == "gmeow"
    assert "classes" in schema
    assert "slots" in schema
    assert "enums" in schema
    assert len(schema["classes"]) > 0
    assert len(schema["slots"]) > 0

    # Spot-check a well-known class
    assert "Person" in schema["classes"] or "Entity" in schema["classes"]

    # Warnings are non-fatal; just ensure the list is a list
    assert isinstance(warnings, list)


def test_generators_run_without_error(tmp_path: Path) -> None:
    """Each LinkML generator must serialize without raising."""
    schema_dict, _warnings = emit_linkml(load_fold())
    linkml_path = tmp_path / _LINKML_FILE
    linkml_path.write_text(
        yaml.safe_dump(schema_dict, sort_keys=False), encoding="utf-8"
    )

    json_schema = gen_json_schema(linkml_path)
    assert (
        "$schema" in json_schema
        or "$defs" in json_schema
        or "properties" in json_schema
    )

    pydantic = gen_pydantic(linkml_path)
    assert "class " in pydantic or "pydantic" in pydantic.lower()

    typescript = gen_typescript(linkml_path)
    assert "interface" in typescript or "type" in typescript

    graphql = gen_graphql(linkml_path)
    assert "type" in graphql


def test_openapi_derives_valid_json(tmp_path: Path) -> None:
    """OpenAPI derivation must produce valid JSON with a components/schemas block."""
    schema_dict, _warnings = emit_linkml(load_fold())
    linkml_path = tmp_path / _LINKML_FILE
    linkml_path.write_text(
        yaml.safe_dump(schema_dict, sort_keys=False), encoding="utf-8"
    )

    json_schema_text = gen_json_schema(linkml_path)
    openapi_text = gen_openapi(json_schema_text)

    openapi = json.loads(openapi_text)
    assert openapi["openapi"] == "3.1.0"
    assert "components" in openapi
    assert "schemas" in openapi["components"]
    assert "paths" in openapi


def test_schema_compile_check_no_drift_after_write(tmp_path: Path) -> None:
    """After compiling in-place, a subsequent --check must report no drift."""
    # Use a temporary schemas directory to avoid interfering with dist/
    original_schemas_dir = SCHEMAS_DIR
    try:
        # Monkey-patch SCHEMAS_DIR for the duration of the test
        import gmeow_tools.config as _cfg
        import gmeow_tools.schema_compile as sc

        _cfg.SCHEMAS_DIR = tmp_path
        sc.SCHEMAS_DIR = tmp_path  # type: ignore[attr-defined]

        # First compile
        report1 = run("schemas", check=False)
        assert len(report1.written) == 6  # linkml + 4 generators + openapi
        assert report1.drifted == []

        # Then check
        report2 = run("schemas", check=True)
        assert report2.drifted == [], (
            f"schema artifacts drifted after in-place compile: {report2.drifted}"
        )
        assert report2.orphans == [], (
            "schema artifacts contain orphans after in-place compile:\n  "
            + "\n  ".join(report2.orphans)
        )
    finally:
        _cfg.SCHEMAS_DIR = original_schemas_dir
        sc.SCHEMAS_DIR = original_schemas_dir  # type: ignore[attr-defined]


def test_schema_generator_renders_all_artifacts(tmp_path: Path) -> None:
    """SchemaGenerator produces all six expected artifacts."""
    import gmeow_tools.config as _cfg
    import gmeow_tools.schema_compile as sc

    original = _cfg.SCHEMAS_DIR
    try:
        _cfg.SCHEMAS_DIR = tmp_path
        sc.SCHEMAS_DIR = tmp_path  # type: ignore[attr-defined]
        gen = SchemaGenerator
        gen.render(tmp_path)  # type: ignore
        expected = {
            _LINKML_FILE,
            "gmeow.schema.json",
            "gmeow.py",
            "gmeow.ts",
            "gmeow.graphql",
            "gmeow.openapi.json",
        }
        found = {p.name for p in tmp_path.rglob("*") if p.is_file()}
        assert expected <= found, f"missing artifacts: {expected - found}"
    finally:
        _cfg.SCHEMAS_DIR = original
        sc.SCHEMAS_DIR = original  # type: ignore[attr-defined]


def test_bounded_xsd_integers_map_to_numeric_types() -> None:
    """#345: the bounded XSD integer family must never fall back to string.

    Asserts type STRINGS for one property per target — never totals (the
    count-pin lesson)."""
    import json

    from gmeow_tools.config import PROJECT_ROOT

    schemas = PROJECT_ROOT / "generated" / "schemas"

    import yaml

    linkml = yaml.safe_load((schemas / "gmeow.linkml.yaml").read_text(encoding="utf-8"))
    slot = linkml["slots"]["pixelWidth"]
    assert slot["range"] == "integer"
    assert slot["minimum_value"] == 0

    ts = (schemas / "gmeow.ts").read_text(encoding="utf-8")
    assert "pixelWidth?: number," in ts

    json_schema = json.loads(
        (schemas / "gmeow.schema.json").read_text(encoding="utf-8")
    )
    prop = json_schema["$defs"]["MediaObject"]["properties"]["pixelWidth"]
    assert prop["minimum"] == 0
    assert "integer" in prop["type"]
