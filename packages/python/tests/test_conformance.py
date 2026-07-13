"""Off-gate cross-surface conformance for the generated ``gmeow_models`` package.

Confirms LIVE (running real Pydantic) what the on-gate Rust gate
(``emitted_models_agree_with_packed_schema_defs``) proves structurally:

1. the package imports and its cross-slice ``model_rebuild()`` sweep resolves
   (no import cycle);
2. every class docstring's ``>>>`` doctest executes;
3. every model's ``model_json_schema()`` renders and carries its
   ``iri``/``curie``/``definitionDigest`` traceability, with its required set a
   subset of its property set;
4. the constraint core of ``model_json_schema()`` agrees with the packed GMEOW
   JSON Schema (``generated/schemas/gmeow.schema.json``). A model that adds no
   ontology parent must agree EXACTLY; a model that flattens a single Python base
   carries its parent's fields too, so it must be a SUPERSET (Pydantic flattens
   inheritance; the packed ``$def`` is per-shape).

Run via ``make maint-pydantic-conformance`` (uv-managed; off the core gate by
design — the on-gate hard-fail is the Rust structural check).
"""

from __future__ import annotations

import doctest
import importlib
import json
from pathlib import Path

import gmeow_models

_GMEOW = "https://blackcatinformatics.ca/gmeow/"
# packages/python/tests/ -> packages/python -> packages -> repo root
_REPO = Path(__file__).resolve().parents[3]
_PACKED = json.loads(
    (_REPO / "generated" / "schemas" / "gmeow.schema.json").read_text()
)["$defs"]


def _def_key_for(iri: str, curie: str) -> str:
    """The packed ``$defs`` key for a class: the primary ``gmeow`` namespace is
    keyed by the bare LOCAL NAME of the IRI (last ``/`` or ``#`` segment), every
    other namespace by the full CURIE — the ``Namespaces::def_key`` rule the
    compiler and the Rust emitter share."""
    if iri.startswith(_GMEOW):
        return iri[len(_GMEOW) :].rsplit("/", 1)[-1].rsplit("#", 1)[-1]
    return curie


def _models():
    for name in gmeow_models.__all__:
        obj = getattr(gmeow_models, name)
        if isinstance(obj, type) and hasattr(obj, "model_json_schema"):
            yield name, obj


def _has_ontology_parent(model) -> bool:
    return model.__bases__[0].__name__ != "ConfiguredBaseModel"


def _root(js: dict) -> dict:
    """Resolve a top-level ``$ref`` — Pydantic hoists a self-referential model's
    real schema into ``$defs`` and leaves a ``$ref`` at the root, so the constraint
    core lives one hop away. Part of the Task-8a normalizer (Pydantic side)."""
    if "$ref" in js:
        name = js["$ref"].split("/")[-1]
        return js.get("$defs", {}).get(name, js)
    return js


def test_package_imports_and_rebuilds():
    assert len(gmeow_models.__all__) > 100
    assert "ConfiguredBaseModel" in gmeow_models.__all__


def test_class_docstring_doctests_run():
    failures = 0
    seen: set[str] = set()
    for _, model in _models():
        if model.__module__ in seen:
            continue
        seen.add(model.__module__)
        results = doctest.testmod(
            importlib.import_module(model.__module__), verbose=False, report=False
        )
        failures += results.failed
    assert failures == 0, f"{failures} docstring doctests failed"


def test_every_model_schema_renders_with_traceability():
    problems: list[str] = []
    for name, model in _models():
        extra = model.model_config.get("json_schema_extra") or {}
        if not extra.get("curie"):
            continue  # ConfiguredBaseModel scaffolding carries no identity
        for key in ("iri", "curie", "definitionDigest", "$id"):
            if key not in extra:
                problems.append(f"{name}: json_schema_extra missing {key}")
        js = _root(model.model_json_schema(by_alias=True))
        req, props = set(js.get("required", [])), set(js.get("properties", {}))
        if not req <= props:
            problems.append(f"{name}: required {sorted(req - props)} not in properties")
    assert not problems, "\n".join(problems)


def test_model_json_schema_agrees_with_packed_defs():
    mismatches: list[str] = []
    checked = 0
    for name, model in _models():
        extra = model.model_config.get("json_schema_extra") or {}
        curie, iri = extra.get("curie"), extra.get("iri", "")
        if not curie:
            continue
        packed = _PACKED.get(_def_key_for(iri, curie))
        if packed is None:
            mismatches.append(f"{name}: no packed $def for {_def_key_for(iri, curie)!r}")
            continue
        js = _root(model.model_json_schema(by_alias=True))
        want_props = set(packed.get("properties", {}))
        got_props = set(js.get("properties", {}))
        want_req = set(packed.get("required", []))
        got_req = set(js.get("required", []))
        if _has_ontology_parent(model):
            # Pydantic flattens the single Python base's fields in; the packed $def
            # is per-shape, so the live schema must be a SUPERSET of it.
            if not want_props <= got_props:
                mismatches.append(f"{name}: props not superset (missing {sorted(want_props - got_props)[:4]})")
            if not want_req <= got_req:
                mismatches.append(f"{name}: required not superset (missing {sorted(want_req - got_req)[:4]})")
        else:
            if want_props != got_props:
                mismatches.append(
                    f"{name}: props differ (missing {sorted(want_props - got_props)[:4]}, extra {sorted(got_props - want_props)[:4]})"
                )
            if want_req != got_req:
                mismatches.append(
                    f"{name}: required differ (missing {sorted(want_req - got_req)[:4]}, extra {sorted(got_req - want_req)[:4]})"
                )
        checked += 1
    assert checked > 100, f"only {checked} models checked — corpus not loaded?"
    assert not mismatches, "model_json_schema disagreements:\n" + "\n".join(mismatches)
