"""Schema compilation gates (issue #57).

Validates the OWL → LinkML pipeline and the downstream generator fan-out.
These tests are pure-Python (no Docker) and exercise the full compile path.

Marked ``maintainer``: the LinkML + Pydantic/TS/GraphQL generation is a heavy
*secondary external-export* transformation (~45 s), so it runs in CI and
``make test`` but is excluded from the fast ``make check`` gate. JSON Schema +
OpenAPI moved to the native Rust SHACL emitter (#700).
"""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from gmeow_tools.config import PROJECT_ROOT, SCHEMAS_DIR
from gmeow_tools.genlib import source_hash
from gmeow_tools.gts_views import load_fold
from gmeow_tools.schema_compile import (
    _LINKML_FILE,
    SchemaGenerator,
    emit_linkml,
    gen_graphql,
    gen_pydantic,
    gen_typescript,
)

pytestmark = pytest.mark.maintainer


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

    pydantic = gen_pydantic(linkml_path)
    assert "class " in pydantic or "pydantic" in pydantic.lower()

    typescript = gen_typescript(linkml_path)
    assert "interface" in typescript or "type" in typescript

    graphql = gen_graphql(linkml_path)
    assert "type" in graphql


#: The four committed LinkML schema artifacts the Rust ``schemas.rs`` leaf asserts
#: on. JSON Schema (``gmeow.schema.json``) + OpenAPI (``gmeow.openapi.json``) moved
#: to the native Rust ``stage-export-json-schema`` leaf (#700).
_SCHEMA_ARTIFACTS = (
    "gmeow.linkml.yaml",
    "gmeow.py",
    "gmeow.ts",
    "gmeow.graphql",
)


def _render_schemas(staging: Path) -> SchemaGenerator:
    """Drive the schemas lane directly into ``staging`` (orchestrator-free)."""
    try:
        import linkml  # noqa: F401
    except ImportError:  # pragma: no cover - exercised only without the ext lane
        pytest.skip("linkml toolkit not installed in this environment")
    gen = SchemaGenerator()
    # Stamp the provenance source hash exactly as the Rust leaf would.
    object.__setattr__(gen, "_source_hash", source_hash(gen.inputs))
    gen.render(staging)
    return gen


def test_schema_generator_renders_all_artifacts(tmp_path: Path) -> None:
    """SchemaGenerator produces all four expected LinkML artifacts."""
    _render_schemas(tmp_path)
    out_dir = tmp_path / SCHEMAS_DIR.relative_to(PROJECT_ROOT)
    found = {p.name for p in out_dir.rglob("*") if p.is_file()}
    expected = set(_SCHEMA_ARTIFACTS)
    assert expected <= found, f"missing artifacts: {expected - found}"


def test_schema_compile_no_drift_against_committed(tmp_path: Path) -> None:
    """A fresh render is byte-identical to the committed ``generated/schemas/*``.

    This is exactly what the Rust ``schemas.rs`` leaf asserts: the lane is a
    pure function of its inputs, so a freshly rendered tree must reproduce the
    committed artifact bytes verbatim — no drift.
    """
    for name in _SCHEMA_ARTIFACTS:
        if not (SCHEMAS_DIR / name).exists():
            pytest.skip(f"committed schema artifact {name} not present in checkout")

    _render_schemas(tmp_path)
    out_dir = tmp_path / SCHEMAS_DIR.relative_to(PROJECT_ROOT)

    drifts: list[str] = []
    for name in _SCHEMA_ARTIFACTS:
        fresh = (out_dir / name).read_bytes()
        committed = (SCHEMAS_DIR / name).read_bytes()
        if fresh != committed:
            drifts.append(name)
    assert not drifts, f"schema artifacts drifted from committed bytes: {drifts}"


def test_bounded_xsd_integers_map_to_numeric_types() -> None:
    """#345: the bounded XSD integer family must never fall back to string.

    Asserts type STRINGS for one property per target — never totals (the
    count-pin lesson). The JSON Schema assertions moved to the native Rust
    SHACL emitter (#700); this gate now covers the LinkML + TS lanes."""
    from gmeow_tools.config import PROJECT_ROOT

    schemas = PROJECT_ROOT / "generated" / "schemas"

    import yaml

    linkml = yaml.safe_load((schemas / "gmeow.linkml.yaml").read_text(encoding="utf-8"))
    slot = linkml["slots"]["pixelWidth"]
    assert slot["range"] == "integer"
    assert slot["minimum_value"] == 0

    ts = (schemas / "gmeow.ts").read_text(encoding="utf-8")
    assert "pixelWidth?: number," in ts


def test_range_open_object_properties_are_uriorcurie() -> None:
    """#382: a rangeless ObjectProperty holds IRI references — never strings.

    Rangeless DatatypeProperties and AnnotationProperties keep ``string``;
    explicitly-ranged slots are untouched.
    """
    schema, _warnings = emit_linkml(load_fold())
    slots = schema["slots"]
    # rangeless owl:ObjectProperty → uriorcurie (CURIEs are GMEOW practice)
    assert slots["contradictsClaim"]["range"] == "uriorcurie"
    assert slots["connectsTo"]["range"] == "uriorcurie"
    # rangeless owl:DatatypeProperty / owl:AnnotationProperty → string
    assert slots["anchorMeaning"]["range"] == "string"
    assert slots["accordingTo"]["range"] == "string"
    # explicit ranges are untouched by the default
    assert slots["signingKey"]["range"] == "CryptographicKey"
