# gmeow-ontology

The [GMEOW](https://blackcatinformatics.ca/gmeow/) ontology, distributed as an
importable, fully-documented **Pydantic v2** model package (`gmeow_models`).

Reading these models *is* reading the ontology; validating data with them *is*
using it. The package is a GENERATED, deterministic projection emitted from the
same SHACL shape compilation as the GMEOW JSON Schema, so a model's
`model_json_schema()` agrees with the packed schema.

## Install

```console
pip install gmeow-ontology
```

## Use

```python
from gmeow_models.<slice> import <Class>

obj = <Class>.model_validate(payload)   # closed-world validation
schema = <Class>.model_json_schema()    # agrees with the packed GMEOW JSON Schema
```

Every model carries its definition, usage guidance (when to use / avoid / how to
use), and worked examples in its docstring, plus SHACL-derived `Field(...)`
constraints, `StrEnum` value vocabularies, and a content-addressed
`iri`/`curie`/`definitionDigest` in `json_schema_extra` for traceability back to
the canonical term.

## Loss stance (Principle 17)

This is a closed-record VALIDATION projection of an open-world ontology: it
validates instance shape, it does not reason. The per-term projection-fidelity
table in the GMEOW documentation records exactly what this surface preserves and
drops relative to the canonical `logic:` core.

## Versioning

The wheel version is the ontology's `owl:versionInfo` (`ontology/gmeow.ttl`),
stamped verbatim into the generated `gmeow_models/__about__.py` and read by
`pyproject.toml`'s `[tool.hatch.version]`. To release a new version, bump
`owl:versionInfo` and run `make regenerate` — never hand-edit `__about__.py` or
set `version` in `pyproject.toml` directly.

## Do not edit by hand

`gmeow_models/` is regenerated from the ontology by the GMEOW pipeline. Changes
belong in the ontology sources, not here.

## License

AGPL-3.0-only. © Blackcat Informatics® Inc.
