"""Compile canonical OWL → LinkML developer schemas + downstream generators.

``gmeow compile-schemas`` renders, from the merged GMEOW ontology graph:

* ``dist/schemas/gmeow.linkml.yaml`` — the LinkML schema (lossy OWL→LinkML);
* ``dist/schemas/gmeow.schema.json`` — JSON Schema (via LinkML JsonSchemaGenerator);
* ``dist/schemas/gmeow.py`` — Pydantic models (via LinkML PydanticGenerator);
* ``dist/schemas/gmeow.ts`` — TypeScript interfaces (via LinkML TypescriptGenerator);
* ``dist/schemas/gmeow.graphql`` — GraphQL type stubs (via LinkML GraphqlGenerator);
* ``dist/schemas/gmeow.openapi.json`` — OpenAPI 3.1 derived from the JSON Schema.

The OWL→LinkML mapping is intentionally lossy:

* OWL restrictions (intersection, cardinality, value constraints) become BNodes
  and are dropped.
* RDF 1.2 reification, standpoint indexing, and the four-clocks temporal model
  are not representable in LinkML and are dropped.
* ``owl:inverseOf`` is dropped (LinkML has no inverse slot construct).
* External-class ranges degrade to ``string`` with a warning.
* Multiple ``rdfs:domain`` / ``rdfs:range`` values: the first named URIRef is kept,
  the rest warned.

Value vocabularies (individuals of GMEOW classes) are emitted as LinkML enums.
"""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any

import yaml

from gmeow_tools.config import (
    GTS_SNAPSHOT_FILE,
    PREFIXES,
    PROJECT_ROOT,
    SCHEMAS_DIR,
)
from gmeow_tools.generator import Generator, register, write_text
from gmeow_tools.gts_views import FoldView, load_fold

_GMEOW = str(PREFIXES["gmeow"])
_XSD = str(PREFIXES["xsd"])

#: Artifacts emitted by the schema compiler.
_LINKML_FILE = "gmeow.linkml.yaml"
_JSON_SCHEMA_FILE = "gmeow.schema.json"
_PYDANTIC_FILE = "gmeow.py"
_TYPESCRIPT_FILE = "gmeow.ts"
_GRAPHQL_FILE = "gmeow.graphql"
_OPENAPI_FILE = "gmeow.openapi.json"

#: OWL XSD → LinkML built-in type name.
_XSD_TO_LINKML: Mapping[str, str] = {
    _XSD + "string": "string",
    _XSD + "boolean": "boolean",
    _XSD + "integer": "integer",
    _XSD + "int": "integer",
    _XSD + "long": "integer",
    _XSD + "short": "integer",
    _XSD + "byte": "integer",
    _XSD + "nonNegativeInteger": "integer",
    _XSD + "positiveInteger": "integer",
    _XSD + "nonPositiveInteger": "integer",
    _XSD + "negativeInteger": "integer",
    _XSD + "unsignedByte": "integer",
    _XSD + "unsignedShort": "integer",
    _XSD + "unsignedInt": "integer",
    _XSD + "unsignedLong": "integer",
    _XSD + "decimal": "decimal",
    _XSD + "float": "float",
    _XSD + "double": "double",
    _XSD + "dateTime": "datetime",
    _XSD + "date": "date",
    _XSD + "time": "time",
    _XSD + "duration": "duration",
    _XSD + "anyURI": "uri",
    "http://www.w3.org/2000/01/rdf-schema#Literal": "string",
}


#: Bounds implied by the bounded XSD integer family, carried into the slot as
#: LinkML minimum_value/maximum_value so the generated targets keep the
#: constraint instead of merely the integer-ness (#345). unsignedLong's upper
#: bound exceeds what JSON consumers represent exactly and is omitted.
_XSD_INTEGER_BOUNDS: Mapping[str, tuple[int | None, int | None]] = {
    _XSD + "nonNegativeInteger": (0, None),
    _XSD + "positiveInteger": (1, None),
    _XSD + "nonPositiveInteger": (None, 0),
    _XSD + "negativeInteger": (None, -1),
    _XSD + "byte": (-128, 127),
    _XSD + "unsignedByte": (0, 255),
    _XSD + "unsignedShort": (0, 65535),
    _XSD + "unsignedInt": (0, 4294967295),
    _XSD + "unsignedLong": (0, None),
}


_RDF_TYPE_IRI = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
_OWL_NS = str(PREFIXES["owl"])
_RDFS_NS = str(PREFIXES["rdfs"])


def _local_name_str(iri: str) -> str:
    """Local name of an IRI string (after last ``#`` or ``/``)."""
    cut = max(iri.rfind("#"), iri.rfind("/"))
    return iri[cut + 1 :] if cut >= 0 else iri


def _iri_to_linkml_range_str(
    iri: str,
    class_names: set[str],
    warnings: list[str],
    prop_local: str,
) -> str:
    """Map an OWL range IRI string to a LinkML range string (fold-side)."""
    if iri in _XSD_TO_LINKML:
        return _XSD_TO_LINKML[iri]
    local = _local_name_str(iri)
    if iri.startswith(_GMEOW) and local in class_names:
        return local
    warnings.append(f"{prop_local}: range {iri} is external — degrading to string")
    return "string"


def emit_linkml(view: FoldView) -> tuple[dict[str, Any], list[str]]:
    """Extract the LinkML schema dict from the GTS snapshot (narrow waist).

    The fold counterpart of the rdflib emitter, with one deliberate
    difference: multi-``rdfs:comment`` descriptions pick the
    lexicographically smallest comment instead of graph order (which is
    process-unstable) — the determinism fix the plan called for.
    """
    warnings: list[str] = []
    schema: dict[str, Any] = {
        "id": "https://blackcatinformatics.ca/gmeow/linkml",
        "name": "gmeow",
        "description": (
            "GMEOW developer schema generated from canonical OWL. "
            "Lossy by design: restrictions, reification, standpoint, "
            "inverseOf, and temporal scope are dropped."
        ),
        "prefixes": {
            "gmeow": PREFIXES["gmeow"],
            "linkml": "https://w3id.org/linkml/",
        },
        "imports": ["linkml:types"],
        "default_range": "string",
        "types": {
            "duration": {
                "uri": _XSD + "duration",
                "typeof": "string",
            }
        },
        "classes": {},
        "slots": {},
        "enums": {},
    }

    gmeow_ns = _GMEOW

    def is_gmeow(tid: int) -> bool:
        return view.is_iri(tid) and view.lex(tid).startswith(gmeow_ns)

    def description(tid: int) -> str | None:
        comments = sorted(
            view.lex(o)
            for o in view.objects(tid, _RDFS_NS + "comment")
            if view.is_literal(o)
        )
        return comments[0] if comments else None

    # Classes ---------------------------------------------------------------
    class_names: set[str] = set()
    class_iris: dict[str, str] = {}
    pending_is_a: dict[str, str] = {}
    classes = sorted(view.subjects_by_type(_OWL_NS + "Class"), key=view.lex)
    for cls in classes:
        if not is_gmeow(cls):
            continue
        iri = view.lex(cls)
        local = _local_name_str(iri)
        if not local:
            continue
        class_names.add(local)
        class_iris[local] = iri

        cls_def: dict[str, Any] = {"class_uri": iri}
        label_text = view.public_text(cls, _RDFS_NS + "label")
        if label_text:
            cls_def["title"] = label_text
        desc = description(cls)
        if desc is not None:
            cls_def["description"] = desc

        supers = sorted(
            (
                view.lex(o)
                for o in view.objects(cls, _RDFS_NS + "subClassOf")
                if view.is_iri(o)
            ),
        )
        if supers:
            chosen = next(
                (
                    s
                    for s in supers
                    if s.startswith(gmeow_ns) and _local_name_str(s) in class_names
                ),
                supers[0],
            )
            super_local = _local_name_str(chosen)
            if super_local == local:
                warnings.append(f"{local}: self-referential superclass — dropping is_a")
            elif chosen.startswith(gmeow_ns) and super_local in class_names:
                cls_def["is_a"] = super_local
            elif chosen.startswith(gmeow_ns):
                pending_is_a[local] = super_local
            if len(supers) > 1:
                warnings.append(
                    f"{local}: multiple superclasses — keeping {super_local}"
                )

        schema["classes"][local] = cls_def

    for local, super_local in pending_is_a.items():
        if super_local in class_names:
            schema["classes"][local]["is_a"] = super_local
        else:
            warnings.append(
                f"{local}: superclass {super_local} not found — dropping is_a"
            )

    # Slots -------------------------------------------------------------------
    functional = set(view.subjects_by_type(_OWL_NS + "FunctionalProperty"))
    object_tid = view.tid_of_iri(_OWL_NS + "ObjectProperty")
    datatype_tid = view.tid_of_iri(_OWL_NS + "DatatypeProperty")
    props = sorted(
        {
            t
            for kind in ("ObjectProperty", "DatatypeProperty", "AnnotationProperty")
            for t in view.subjects_by_type(_OWL_NS + kind)
        },
        key=view.lex,
    )
    for prop in props:
        if not is_gmeow(prop):
            continue
        iri = view.lex(prop)
        local = _local_name_str(iri)
        if not local:
            continue

        slot: dict[str, Any] = {"slot_uri": iri}
        label_text = view.public_text(prop, _RDFS_NS + "label")
        if label_text:
            slot["title"] = label_text
        desc = description(prop)
        if desc is not None:
            slot["description"] = desc

        ranges = sorted(
            view.lex(r)
            for r in view.objects(prop, _RDFS_NS + "range")
            if view.is_iri(r)
        )
        if ranges:
            slot["range"] = _iri_to_linkml_range_str(
                ranges[0], class_names, warnings, local
            )
            bounds = _XSD_INTEGER_BOUNDS.get(ranges[0])
            if bounds is not None:
                minimum, maximum = bounds
                if minimum is not None:
                    slot["minimum_value"] = minimum
                if maximum is not None:
                    slot["maximum_value"] = maximum
            if len(ranges) > 1:
                warnings.append(f"{local}: multiple ranges — keeping {slot['range']}")
        else:
            slot["range"] = "string"

        domains = sorted(
            view.lex(d)
            for d in view.objects(prop, _RDFS_NS + "domain")
            if view.is_iri(d)
        )
        if domains:
            domain_local = _local_name_str(domains[0])
            if domains[0].startswith(gmeow_ns) and domain_local in class_names:
                slot["domain"] = domain_local
            if len(domains) > 1:
                warnings.append(f"{local}: multiple domains — keeping {domain_local}")

        is_functional = prop in functional
        is_object = object_tid is not None and view.has(prop, _RDF_TYPE_IRI, object_tid)
        is_datatype = datatype_tid is not None and view.has(
            prop, _RDF_TYPE_IRI, datatype_tid
        )
        if is_functional:
            slot["multivalued"] = False
        elif is_object or is_datatype:
            slot["multivalued"] = True

        schema["slots"][local] = slot

    # Enums ---------------------------------------------------------------------
    type_tid = view.tid_of_iri(_RDF_TYPE_IRI)
    individuals_by_class: dict[str, list[int]] = {}
    typed_subjects = sorted({s for s, p, o, _ in view.quads() if p == type_tid})
    for ind in typed_subjects:
        if not is_gmeow(ind):
            continue
        for cls in view.objects(ind, _RDF_TYPE_IRI):
            if not is_gmeow(cls):
                continue
            cls_local = _local_name_str(view.lex(cls))
            if cls_local not in class_names:
                continue
            individuals_by_class.setdefault(cls_local, []).append(ind)

    for cls_local, inds in sorted(individuals_by_class.items()):
        if not inds:
            continue
        enum_name = f"{cls_local}Enum"
        enum_def: dict[str, Any] = {
            "enum_uri": class_iris[cls_local],
            "permissible_values": {},
        }
        for ind in sorted(inds, key=view.lex):
            ind_iri = view.lex(ind)
            ind_local = _local_name_str(ind_iri)
            pv: dict[str, Any] = {"meaning": ind_iri}
            label_text = view.public_text(ind, _RDFS_NS + "label")
            if label_text:
                pv["title"] = label_text
            desc = description(ind)
            if desc is not None:
                pv["description"] = desc
            enum_def["permissible_values"][ind_local] = pv
        schema["enums"][enum_name] = enum_def

    # Attach slots to their domain classes ---------------------------------------
    for slot_name, slot_def in schema["slots"].items():
        domain = slot_def.get("domain")
        if domain and domain in schema["classes"]:
            schema["classes"][domain].setdefault("slots", []).append(slot_name)

    return schema, warnings


def _write_yaml(
    data: dict[str, Any],
    path: Path,
    *,
    name: str = "",
    source_hash: str = "",
) -> None:
    """Dump a dict as YAML with an optional generated banner."""
    dumped = yaml.safe_dump(data, sort_keys=False, allow_unicode=True)
    write_text(path, dumped, name=name, source_hash=source_hash)


def _snapshot_version() -> str:
    """The ontology version from the snapshot's header (owl:versionInfo)."""
    from gmeow_tools.config import ONTOLOGY_IRI

    view = load_fold()
    onto = view.tid_of_iri(ONTOLOGY_IRI)
    version = (
        view.value(onto, "http://www.w3.org/2002/07/owl#versionInfo")
        if onto is not None
        else None
    )
    if version is None:
        msg = "snapshot lacks owl:versionInfo on the ontology header"
        raise ValueError(msg)
    return view.lex(version)


def gen_json_schema(linkml_path: Path) -> str:
    """Run the LinkML JSON Schema generator."""
    from linkml.generators.jsonschemagen import JsonSchemaGenerator

    gen = JsonSchemaGenerator(str(linkml_path), mergeimports=True)
    return gen.serialize()  # type: ignore[no-any-return]


def gen_pydantic(linkml_path: Path) -> str:
    """Run the LinkML Pydantic generator."""
    from linkml.generators.pydanticgen import PydanticGenerator

    gen = PydanticGenerator(str(linkml_path), mergeimports=True)
    return gen.serialize()  # type: ignore[no-any-return]


def gen_typescript(linkml_path: Path) -> str:
    """Run the LinkML TypeScript generator."""
    from linkml.generators.typescriptgen import TypescriptGenerator

    gen = TypescriptGenerator(str(linkml_path), mergeimports=True)
    return gen.serialize()  # type: ignore[no-any-return]


def gen_graphql(linkml_path: Path) -> str:
    """Run the LinkML GraphQL generator."""
    from linkml.generators.graphqlgen import GraphqlGenerator

    gen = GraphqlGenerator(str(linkml_path), mergeimports=True)
    return gen.serialize()  # type: ignore[no-any-return]


def gen_openapi(json_schema_text: str) -> str:
    """Derive an OpenAPI 3.1 spec from the JSON Schema text.

    The returned string is a minimal OpenAPI document whose
    ``components/schemas`` block is the parsed JSON Schema.
    A thin path set (``GET /entities/{id}``) is added so the
    spec validates as a complete OpenAPI document.
    """
    schema_obj = json.loads(json_schema_text)
    # Wrap the JSON Schema under a single named component so top-level keys
    # like "$schema" and "title" do not leak as component names.
    component_name = schema_obj.get("title", "GMEOWSchema")
    openapi: dict[str, Any] = {
        "openapi": "3.1.0",
        "info": {
            "title": "GMEOW API",
            "description": (
                "OpenAPI 3.1 derived from the GMEOW LinkML developer schema. "
                "Lossy by design — see gmeow.linkml.yaml for caveats."
            ),
            "version": _snapshot_version(),
        },
        "paths": {
            "/entities/{id}": {
                "get": {
                    "summary": "Retrieve a GMEOW entity",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": True,
                            "schema": {"type": "string"},
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "A GMEOW entity",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": f"#/components/schemas/{component_name}"
                                    }
                                }
                            },
                        }
                    },
                }
            }
        },
        "components": {"schemas": {component_name: schema_obj}},
    }
    return json.dumps(openapi, indent=2) + "\n"


def _normalize_text(text: str) -> str:
    """Strip trailing whitespace per line and ensure exactly one trailing newline."""
    lines = text.split("\n")
    cleaned = "\n".join(line.rstrip() for line in lines)
    return cleaned.rstrip("\n") + "\n"


def _write_artifacts(
    linkml_text: str,
    json_schema_text: str,
    pydantic_text: str,
    typescript_text: str,
    graphql_text: str,
    openapi_text: str,
    out_dir: Path,
) -> None:
    """Write all six schema artifacts to ``out_dir``."""
    out_dir.mkdir(parents=True, exist_ok=True)
    mapping = {
        _LINKML_FILE: linkml_text,
        _JSON_SCHEMA_FILE: json_schema_text,
        _PYDANTIC_FILE: pydantic_text,
        _TYPESCRIPT_FILE: typescript_text,
        _GRAPHQL_FILE: graphql_text,
        _OPENAPI_FILE: openapi_text,
    }
    for name, text in mapping.items():
        path = out_dir / name
        path.write_text(_normalize_text(text), encoding="utf-8")


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #


@register
class SchemaGenerator(Generator):
    """Compile canonical OWL → LinkML + downstream artifacts."""

    name: str = "schemas"

    @property
    def inputs(self) -> Sequence[Path]:
        """Canonical inputs for the schema generator."""
        return [GTS_SNAPSHOT_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed outputs for the schema generator."""
        return [
            SCHEMAS_DIR / _LINKML_FILE,
            SCHEMAS_DIR / _JSON_SCHEMA_FILE,
            SCHEMAS_DIR / _PYDANTIC_FILE,
            SCHEMAS_DIR / _TYPESCRIPT_FILE,
            SCHEMAS_DIR / _GRAPHQL_FILE,
            SCHEMAS_DIR / _OPENAPI_FILE,
        ]

    def render(self, staging: Path) -> None:
        """Render schema artifacts into the staging tree."""
        schema_dict, _warnings = emit_linkml(load_fold())

        if SCHEMAS_DIR.is_relative_to(PROJECT_ROOT):
            out_dir = staging / SCHEMAS_DIR.relative_to(PROJECT_ROOT)
        else:
            out_dir = staging
        linkml_path = out_dir / _LINKML_FILE
        _write_yaml(
            schema_dict,
            linkml_path,
            name=self.name,
            source_hash=getattr(self, "_source_hash", ""),
        )

        json_schema_text = gen_json_schema(linkml_path)
        pydantic_text = gen_pydantic(linkml_path)
        # The Pydantic generator embeds the absolute source path, which breaks
        # determinism because the LinkML file lives in a temp directory.
        # Normalize it to a stable relative string.
        pydantic_text = pydantic_text.replace(str(linkml_path), _LINKML_FILE)
        typescript_text = gen_typescript(linkml_path)
        graphql_text = gen_graphql(linkml_path)
        openapi_text = gen_openapi(json_schema_text)

        _write_artifacts(
            linkml_path.read_text(encoding="utf-8"),
            json_schema_text,
            pydantic_text,
            typescript_text,
            graphql_text,
            openapi_text,
            out_dir,
        )
