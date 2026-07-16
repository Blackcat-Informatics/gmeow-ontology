# gmeow_models — the GMEOW ontology as a Pydantic v2 package

This package is a GENERATED, deterministic projection of the GMEOW ontology.
Reading these models is reading the ontology; validating data with them is
using it. It is emitted from the SAME SHACL shape compilation as the GMEOW
JSON Schema, so a model's `model_json_schema()` agrees with the packed schema.

## What each model carries

- A docstring = the term's definition, when-to-use / avoid / how-to-use guidance,
  worked examples, and a docs back-link.
- SHACL-derived `Field(...)` constraints (cardinality, min/max, length, pattern).
- `StrEnum` value vocabularies for the ontology's value families.
- `json_schema_extra` with the class `iri`, `curie`, and content-addressed
  `definitionDigest` for traceability back to the canonical term.

## Loss stance (Principle 17)

This is a closed-record VALIDATION projection of an open-world ontology: it
validates instance shape, it does not reason. The per-term projection-fidelity
table in the GMEOW documentation records exactly what this surface preserves
and drops relative to the canonical `logic:` core.

## Versioning

The wheel version (0.1.0) is the ontology's `owl:versionInfo`
(`ontology/gmeow.ttl`), stamped verbatim into `gmeow_models/__about__.py` and
read by `pyproject.toml`'s `[tool.hatch.version]`. To release a new version,
bump `owl:versionInfo` and `make regenerate` — never hand-edit `__about__.py`
or set `version` in `pyproject.toml` directly.

## Usage

```python
from gmeow_models.<slice> import <Class>

obj = <Class>.model_validate(payload)  # closed-world validation
schema = <Class>.model_json_schema()   # agrees with the packed GMEOW JSON Schema
```

The package ships 570 models across 81 modules (one module per slice,
plus the shared `_base`/`_envelope` scaffolding). Do not edit by hand — it is
regenerated from the ontology.
