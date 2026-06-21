# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Research-object exports (#58): Croissant, RO-Crate, DataCite, Frictionless.

A GMEOW-described dataset becomes discoverable to the ML/research ecosystem:
the **Croissant** JSON-LD that Google Dataset Search / Hugging Face / Kaggle
index, the **RO-Crate** research package WorkflowHub accepts, the **DataCite**
deposit a dataset DOI needs, and the **Frictionless** ``datapackage.json``
data pipelines read. Each is a GENERATED lossy projection of the canonical
GMEOW instance data (P4/P5) — never a hand-curated metadata file — and each
declares its drops in the format's own native slot (Croissant
``rai:dataLimitation``, Frictionless ``notes``, DataCite TechnicalInfo
description, the RO-Crate descriptor's description).

These four are Python builders, not mapping-DSL profiles: their document
shapes (Croissant's layered JSON-LD, RO-Crate's flat ``@graph``, plain-JSON
Frictionless, DataCite XML) need framing that a CONSTRUCT query cannot
express, so a DSL layer would be a dead declarative twin — a second source of
truth nothing executes. The RDF-shaped sibling (``dcat``) IS a DSL profile;
see ``dsl/mappings/projections/dcat.ttl``.

This module is an INSTANCE-data tool, like :mod:`gmeow_tools.projections`:
it reads user/example A-Boxes with rdflib, so it is intentionally NOT under
the narrow-waist seal (which governs exporters of the ontology's own data).

The ``research-objects`` generator renders the flagship worked example (the
Lillith GraphRAG benchmark) into ``generated/research-objects/lillith/`` —
the no-drift gate — and ``gmeow export`` runs the same builders over
arbitrary user data.
"""

from __future__ import annotations

import json
import zipfile
from dataclasses import dataclass
from html import escape
from pathlib import Path
from typing import TYPE_CHECKING
from xml.etree import ElementTree as ET

from rdflib import RDF, RDFS, Graph, URIRef

from gmeow_tools.config import (
    DIST_DIR,
    EVALS_DIR,
    GENERATED_DIR,
    NAMESPACE,
    PROJECT_ROOT,
)
from gmeow_tools.generator import Generator, GeneratorError, _rel, register, write_text

if TYPE_CHECKING:
    from collections.abc import Sequence

    from rdflib.term import Node

# --------------------------------------------------------------------------- #
# Vocabulary handles (instance-data properties read by every builder)
# --------------------------------------------------------------------------- #


def _g(local: str) -> URIRef:
    return URIRef(NAMESPACE + local)


_DATASET = _g("Dataset")
_DOCUMENT = _g("Document")
_CHUNK = _g("Chunk")
_MODEL_INVOCATION = _g("ModelInvocation")
_IMPORT_ACTIVITY = _g("ImportActivity")
_BUILD_ACTIVITY = _g("BuildActivity")
_BUILDER = _g("Builder")
_SOFTWARE_AGENT = _g("SoftwareAgent")
_STANDPOINT_CLAIM = _g("StandpointClaim")
_ASSESSMENT = _g("Assessment")

_TITLE = _g("title")
_DESCRIPTION = _g("description")
_HAS_LICENSE = _g("hasLicense")
_SPDX_LICENSE_ID = _g("spdxLicenseId")
_WAS_ATTRIBUTED_TO = _g("wasAttributedTo")
_WAS_GENERATED_BY = _g("wasGeneratedBy")
_WAS_DERIVED_FROM = _g("wasDerivedFrom")
_DATE_PUBLISHED = _g("datePublished")
_SOURCE_LOCATION = _g("sourceLocation")
_CONTENT_DIGEST = _g("contentDigest")
_VERSION = _g("version")
_CITE_AS = _g("citeAs")
_CHUNK_OF = _g("chunkOf")
_SPAN_START = _g("spanStart")
_SPAN_END = _g("spanEnd")
_USED_MODEL = _g("usedModel")
_INGESTED_AT = _g("ingestedAt")
_BUILD_SOURCE = _g("buildSource")
_BUILD_OUTPUT = _g("buildOutput")
_BUILD_CONFIG_URI = _g("buildConfigUri")
_HAS_PARTICIPANT = _g("hasParticipant")
_EVENT_TIME = _g("eventTime")
_DESCRIBES_MODEL = _g("describesModel")
_MODEL_VERSION_TAG = _g("modelVersionTag")
_MODEL_PROVIDER = _g("modelProvider")
_VANTAGE = _g("vantage")
_CLAIM_MODALITY = _g("claimModality")
_GROUNDED_IN = _g("groundedIn")
_ASSESSMENT_TARGET = _g("assessmentTarget")
_ASSESSMENT_CRITERION = _g("assessmentCriterion")
_ASSESSMENT_SCORE = _g("assessmentScoreValue")

#: Pinned Croissant 1.0 conformance IRI + the published context (constant so
#: the artifact is deterministic and the gate can pin it).
CROISSANT_CONFORMS_TO = "http://mlcommons.org/croissant/1.0"
CROISSANT_CONTEXT: dict[str, object] = {
    "@language": "en",
    "@vocab": "https://schema.org/",
    "sc": "https://schema.org/",
    "cr": "http://mlcommons.org/croissant/",
    "rai": "http://mlcommons.org/croissant/RAI/",
    "dct": "http://purl.org/dc/terms/",
    "citeAs": "cr:citeAs",
    "conformsTo": "dct:conformsTo",
    "data": {"@id": "cr:data", "@type": "@json"},
    "dataType": {"@id": "cr:dataType", "@type": "@vocab"},
    "field": "cr:field",
    "fileObject": "cr:fileObject",
    "recordSet": "cr:recordSet",
    "sha256": "cr:sha256",
    "md5": "cr:md5",
}

RO_CRATE_CONTEXT = "https://w3id.org/ro/crate/1.1/context"
RO_CRATE_SPEC = "https://w3id.org/ro/crate/1.1"
#: Tiered Workflow-Run-RO-Crate conformance, honestly earned (P1): every
#: crate is at least a Process Run Crate (ModelInvocation/ImportActivity →
#: CreateAction); when the A-Box carries a #47 workflow run — a
#: gmeow:BuildActivity whose gmeow:buildConfigUri names the workflow
#: definition — the crate upgrades to Workflow Run Crate (the definition
#: becomes the ComputationalWorkflow mainEntity, the run's CreateAction
#: takes it as instrument).
PROCESS_RUN_PROFILE = "https://w3id.org/ro/wfrun/process/0.5"
WORKFLOW_RUN_PROFILE = "https://w3id.org/ro/wfrun/workflow/0.5"

#: DataCite Metadata Schema kernel-4 namespace; validate against the official
#: kernel-4.5 XSD (https://schema.datacite.org/meta/kernel-4.5/metadata.xsd)
#: before any production deposit — same reference-only stance as crossref.py.
DATACITE_NS = "http://datacite.org/schema/kernel-4"
_XSI_NS = "http://www.w3.org/2001/XMLSchema-instance"
_DATACITE_SCHEMA_LOCATION = (
    f"{DATACITE_NS} https://schema.datacite.org/meta/kernel-4.5/metadata.xsd"
)
#: 10.5072 is DataCite's reserved TEST prefix — the honest placeholder until
#: the #44 publish act mints a real dataset DOI.
PLACEHOLDER_DOI_PREFIX = "10.5072"

#: The P5 declared losses every research-object export shares.
DECLARED_DROPS: tuple[str, ...] = (
    "reified relators (Copyright, roles, memberships) flatten or vanish",
    "RDF 1.2 statement annotations (confidence, accordingTo, the four clocks)"
    " are dropped",
    "standpoint indexing is dropped — contested claims appear without their vantage",
    "blake3 remains the internal canonical content digest; sha256/md5 are"
    " projected where supplied and the format allows",
)

_FRICTIONLESS_SCHEMA_FILE = (
    PROJECT_ROOT / "imports" / "targets" / "data-package.schema.json"
)

# ET.register_namespace mutates ElementTree's global prefix registry —
# registered once at import, never per call (thread-safety + no re-mutation).
ET.register_namespace("", DATACITE_NS)
ET.register_namespace("xsi", _XSI_NS)


# --------------------------------------------------------------------------- #
# Instance-graph access
# --------------------------------------------------------------------------- #


def load_instance_graph(paths: Sequence[Path]) -> Graph:
    """Parse instance Turtle files into one graph (no ontology merge)."""
    if not paths:
        msg = "no instance files given"
        raise ValueError(msg)
    g = Graph()
    for path in paths:
        g.parse(path, format="turtle")
    return g


def _text(g: Graph, s: Node, p: URIRef) -> str:
    value = g.value(s, p)
    return str(value) if value is not None else ""


def _label(g: Graph, s: Node) -> str:
    return _text(g, s, RDFS.label) or _text(g, s, _TITLE) or str(s)


def _slug(iri: str) -> str:
    tail = iri.rstrip("/").rsplit("/", 1)[-1].rsplit("#", 1)[-1]
    return tail or "resource"


def _as_list(value: object) -> list[object]:
    """Narrow an untyped JSON value to a list (empty when it isn't one)."""
    return value if isinstance(value, list) else []


@dataclass(frozen=True, slots=True)
class DatasetMeta:
    """Catalog metadata read from the gmeow:Dataset node — never hard-coded."""

    iri: str
    title: str
    description: str
    license_id: str
    license_url: str
    creator: str
    date_published: str
    landing_page: str
    version: str | None = None
    cite_as: str | None = None

    @property
    def publication_year(self) -> str:
        """The year of ``date_published`` (DataCite's publicationYear)."""
        return self.date_published[:4]


def dataset_meta(g: Graph) -> DatasetMeta:
    """Extract the dataset descriptor (the licensed gmeow:Dataset node).

    Raises:
        ValueError: If no licensed gmeow:Dataset node is present — every
            research object needs a license and a title to be publishable.
    """
    candidates = sorted(
        (
            s
            for s in g.subjects(RDF.type, _DATASET)
            if isinstance(s, URIRef) and g.value(s, _HAS_LICENSE) is not None
        ),
        key=str,
    )
    if not candidates:
        msg = (
            "no licensed gmeow:Dataset node found — research-object exports "
            "need a dataset descriptor (gmeow:Dataset + gmeow:hasLicense + "
            "gmeow:title); see slices/extensions/graphrag/examples/"
            "lillith-dataset.ttl"
        )
        raise ValueError(msg)
    ds = candidates[0]
    license_node = g.value(ds, _HAS_LICENSE)
    license_id = _text(g, license_node, _SPDX_LICENSE_ID) if license_node else ""
    if not license_id:
        msg = (
            f"dataset descriptor {ds} has a gmeow:License without a "
            "gmeow:spdxLicenseId — every research object needs a resolvable "
            "license identifier"
        )
        raise ValueError(msg)
    date_published = _text(g, ds, _DATE_PUBLISHED)
    if len(date_published) < 4 or not date_published[:4].isdigit():
        msg = (
            f"dataset descriptor {ds} needs a valid gmeow:datePublished "
            "(an ISO date — DataCite's publicationYear comes from it)"
        )
        raise ValueError(msg)
    creator_node = g.value(ds, _WAS_ATTRIBUTED_TO)
    version = _text(g, ds, _VERSION) or None
    cite_as = _text(g, ds, _CITE_AS) or None
    return DatasetMeta(
        iri=str(ds),
        title=_text(g, ds, _TITLE) or _label(g, ds),
        description=_text(g, ds, _DESCRIPTION),
        license_id=license_id,
        license_url=(f"https://spdx.org/licenses/{license_id}" if license_id else ""),
        creator=_label(g, creator_node) if creator_node else "",
        date_published=date_published,
        landing_page=_text(g, ds, _SOURCE_LOCATION) or str(ds),
        version=version,
        cite_as=cite_as,
    )


def _digest_map(g: Graph, doc: Node) -> dict[str, str]:
    """Collect all ``gmeow:contentDigest`` values for ``doc``.

    Each value is expected to be ``algorithm:hex``. Unprefixed values are kept
    under the key ``"digest"`` for backward compatibility. When several values
    share the same algorithm, a conflicting value raises ``ValueError``;
    identical duplicates are ignored.
    """
    if doc is None:
        raise ValueError("_digest_map: doc cannot be None")
    digests: dict[str, str] = {}
    for value in g.objects(doc, _CONTENT_DIGEST):
        raw = str(value)
        if ":" in raw:
            algorithm, _, hex_value = raw.partition(":")
            key = algorithm
        else:
            key = "digest"
            hex_value = raw
        if key not in digests:
            digests[key] = hex_value
        elif digests[key] != hex_value:
            raise ValueError(f"_digest_map: conflicting {key} digests for {doc}")
    return digests


def _primary_digest(digests: dict[str, str]) -> str:
    """Best-effort single digest for formats that only accept one value.

    Prefers blake3, then sha256, then md5, then any available value.
    The returned string preserves the ``algorithm:hex`` form when an algorithm
    prefix is known, matching the historical verbatim ``gmeow:contentDigest``
    value used by RO-Crate and Frictionless.
    """
    for algo in ("blake3", "sha256", "md5"):
        if algo in digests:
            return f"{algo}:{digests[algo]}"
    for algo, value in digests.items():
        return value if algo == "digest" else f"{algo}:{value}"
    return ""


def _documents(g: Graph) -> list[dict[str, object]]:
    """Every gmeow:Document with identity columns, sorted by IRI."""
    out: list[dict[str, object]] = []
    for doc in sorted(set(g.subjects(RDF.type, _DOCUMENT)), key=str):
        out.append(
            {
                "iri": str(doc),
                "name": _label(g, doc),
                "contentUrl": _text(g, doc, _SOURCE_LOCATION),
                "digests": _digest_map(g, doc),
            }
        )
    return out


def _agents(g: Graph) -> list[dict[str, str]]:
    """Every SoftwareAgent, with model-card detail where described."""
    out: list[dict[str, str]] = []
    nodes = set(g.subjects(RDF.type, _SOFTWARE_AGENT)) | set(
        g.subjects(RDF.type, _BUILDER)
    )
    for agent in sorted(nodes, key=str):
        card = next(iter(sorted(g.subjects(_DESCRIBES_MODEL, agent), key=str)), None)
        out.append(
            {
                "iri": str(agent),
                "name": _label(g, agent),
                "version": _text(g, card, _MODEL_VERSION_TAG) if card else "",
                "provider": _text(g, card, _MODEL_PROVIDER) if card else "",
            }
        )
    return out


@dataclass(frozen=True, slots=True)
class _Action:
    """A flattened provenance action.

    ModelInvocation / ImportActivity / the #47 BuildActivity workflow run.
    """

    iri: str
    name: str
    instrument: str
    objects: tuple[str, ...]
    results: tuple[str, ...]
    end_time: str
    workflow: str = ""  # buildConfigUri — the workflow definition (Run Crate)
    agent: str = ""  # the performing Builder/participant


def _activities(g: Graph) -> list[_Action]:
    """ModelInvocations + ImportActivities + BuildActivities, sorted."""
    actions: list[_Action] = []
    nodes = (
        set(g.subjects(RDF.type, _MODEL_INVOCATION))
        | set(g.subjects(RDF.type, _IMPORT_ACTIVITY))
        | set(g.subjects(RDF.type, _BUILD_ACTIVITY))
    )
    for act in sorted(nodes, key=str):
        results = tuple(
            sorted(
                {str(s) for s in g.subjects(_WAS_GENERATED_BY, act)}
                | {str(o) for o in g.objects(act, _BUILD_OUTPUT)}
            )
        )
        objects = tuple(
            sorted(
                {
                    str(src)
                    for result in g.subjects(_WAS_GENERATED_BY, act)
                    for src in g.objects(result, _WAS_DERIVED_FROM)
                }
                | {str(o) for o in g.objects(act, _BUILD_SOURCE)}
            )
        )
        instrument = g.value(act, _USED_MODEL)
        participant = g.value(act, _HAS_PARTICIPANT)
        actions.append(
            _Action(
                iri=str(act),
                name=_label(g, act),
                instrument=str(instrument) if instrument else "",
                objects=objects,
                results=results,
                end_time=_text(g, act, _INGESTED_AT) or _text(g, act, _EVENT_TIME),
                workflow=_text(g, act, _BUILD_CONFIG_URI),
                agent=str(participant) if participant else "",
            )
        )
    return actions


# --------------------------------------------------------------------------- #
# Croissant
# --------------------------------------------------------------------------- #


def _croissant_record_sets(g: Graph) -> list[dict[str, object]]:
    """RecordSets with inline ``data`` rows (chunks, claims, eval scores)."""

    def field(rs: str, name: str, data_type: str) -> dict[str, object]:
        return {
            "@type": "cr:Field",
            "@id": f"{rs}/{name}",
            "name": name,
            "dataType": data_type,
        }

    record_sets: list[dict[str, object]] = []

    chunk_rows = [
        {
            "chunks/id": str(chunk),
            "chunks/source": _text(g, chunk, _CHUNK_OF),
            "chunks/spanStart": int(_text(g, chunk, _SPAN_START) or 0),
            "chunks/spanEnd": int(_text(g, chunk, _SPAN_END) or 0),
            "chunks/digest": _text(g, chunk, _CONTENT_DIGEST),
        }
        for chunk in sorted(set(g.subjects(RDF.type, _CHUNK)), key=str)
    ]
    if chunk_rows:
        record_sets.append(
            {
                "@type": "cr:RecordSet",
                "@id": "chunks",
                "name": "chunks",
                "description": "Content-addressed retrieval segments with"
                " typed offsets into their source documents.",
                "field": [
                    field("chunks", "id", "sc:Text"),
                    field("chunks", "source", "sc:Text"),
                    field("chunks", "spanStart", "sc:Integer"),
                    field("chunks", "spanEnd", "sc:Integer"),
                    field("chunks", "digest", "sc:Text"),
                ],
                "data": chunk_rows,
            }
        )

    claim_rows = [
        {
            "claims/id": str(claim),
            "claims/vantage": _text(g, claim, _VANTAGE),
            "claims/modality": _slug(_text(g, claim, _CLAIM_MODALITY)),
            "claims/grounded": g.value(claim, _GROUNDED_IN) is not None,
        }
        for claim in sorted(set(g.subjects(RDF.type, _STANDPOINT_CLAIM)), key=str)
    ]
    if claim_rows:
        record_sets.append(
            {
                "@type": "cr:RecordSet",
                "@id": "claims",
                "name": "claims",
                "description": "Model-extracted claims: vantage-attributed,"
                " modality-tagged, grounded flag from evidence spans."
                " (Standpoint nuance beyond the flag is a declared drop.)",
                "field": [
                    field("claims", "id", "sc:Text"),
                    field("claims", "vantage", "sc:Text"),
                    field("claims", "modality", "sc:Text"),
                    field("claims", "grounded", "sc:Boolean"),
                ],
                "data": claim_rows,
            }
        )

    score_rows = [
        {
            "evalScores/model": _text(g, a, _ASSESSMENT_TARGET),
            "evalScores/criterion": _slug(_text(g, a, _ASSESSMENT_CRITERION)),
            "evalScores/score": float(_text(g, a, _ASSESSMENT_SCORE) or 0.0),
        }
        for a in sorted(set(g.subjects(RDF.type, _ASSESSMENT)), key=str)
        if _text(g, a, _ASSESSMENT_SCORE)
    ]
    if score_rows:
        record_sets.append(
            {
                "@type": "cr:RecordSet",
                "@id": "evalScores",
                "name": "evalScores",
                "description": "Vantage-indexed rubric assessments from the"
                " gmeow-evals harness (#298).",
                "field": [
                    field("evalScores", "model", "sc:Text"),
                    field("evalScores", "criterion", "sc:Text"),
                    field("evalScores", "score", "sc:Float"),
                ],
                "data": score_rows,
            }
        )
    return record_sets


def build_croissant(g: Graph, ds: DatasetMeta) -> dict[str, object]:
    """Build the Croissant 1.0 JSON-LD document for a GMEOW dataset."""
    distributions: list[dict[str, object]] = []
    for doc in _documents(g):
        content_url = doc["contentUrl"]
        if not content_url:
            raise ValueError(f"build_croissant: missing contentUrl for {doc['iri']}")
        file_object: dict[str, object] = {
            "@type": "cr:FileObject",
            "@id": str(doc["iri"]),
            "name": str(doc["name"]),
            "encodingFormat": "text/plain",
            "contentUrl": str(content_url),
        }
        # `_documents` uses `dict[str, object]`; "digests" is really `dict[str, str]`.
        digests: dict[str, str] = doc["digests"]  # type: ignore[assignment]
        if "sha256" in digests:
            file_object["sha256"] = digests["sha256"]
        if "md5" in digests:
            file_object["md5"] = digests["md5"]
        extra = [
            value if algo == "digest" else f"{algo}:{value}"
            for algo, value in digests.items()
            if algo not in {"sha256", "md5"}
        ]
        if extra:
            # blake3 cannot fill cr:sha256 — keep it findable, declared below.
            file_object["description"] = "content digest: " + ", ".join(extra)
        distributions.append(file_object)

    tools = [
        f"{a['name']}"
        + (f" ({a['provider']} {a['version']})".rstrip() if a["version"] else "")
        for a in _agents(g)
    ]
    limitations = list(DECLARED_DROPS)

    result: dict[str, object] = {
        "@context": CROISSANT_CONTEXT,
        "@type": "sc:Dataset",
        "@id": ds.iri,
        "name": ds.title,
        "description": ds.description,
        "conformsTo": CROISSANT_CONFORMS_TO,
        "license": ds.license_url,
        "creator": {"@type": "sc:Organization", "name": ds.creator},
        "datePublished": ds.date_published,
        "url": ds.landing_page,
        "distribution": distributions,
        "recordSet": _croissant_record_sets(g),
        "rai:dataCollection": "Sources are content-addressed (blake3) and"
        " ingested through attributed gmeow:ImportActivity records; every"
        " derived artifact carries wasGeneratedBy/wasDerivedFrom lineage.",
        "rai:machineAnnotationTools": tools,
        "rai:dataLimitation": limitations,
    }
    if ds.version:
        result["version"] = ds.version
    if ds.cite_as:
        result["citeAs"] = ds.cite_as
    return result


def validate_croissant(doc: dict[str, object]) -> list[str]:
    """Structural Croissant diagnostics (P7). Empty list == clean.

    External full validation: ``mlcroissant validate --jsonld <file>``
    (documented in docs/research-objects.md; optional test runs it when the
    package is importable).
    """
    problems: list[str] = []
    # recordSet may legitimately be empty for a plain document dataset;
    # everything else (incl. at least one distribution) is required.
    for key in (
        "@context",
        "@type",
        "name",
        "description",
        "license",
        "conformsTo",
        "distribution",
    ):
        if not doc.get(key):
            problems.append(f"croissant: missing/empty {key}")
    if doc.get("@type") != "sc:Dataset":
        problems.append("croissant: @type must be sc:Dataset")
    if doc.get("conformsTo") != CROISSANT_CONFORMS_TO:
        problems.append("croissant: conformsTo must pin Croissant 1.0")
    file_ids: set[str] = set()
    for dist in _as_list(doc.get("distribution")):
        if not isinstance(dist, dict) or not dist.get("@id"):
            problems.append("croissant: distribution entry without @id")
            continue
        dist_id = str(dist["@id"])
        file_ids.add(dist_id)
        if dist.get("@type") != "cr:FileObject":
            problems.append(f"croissant: {dist_id} is not a cr:FileObject")
        if not dist.get("contentUrl"):
            problems.append(f"croissant: {dist_id} missing contentUrl")
        sha = dist.get("sha256")
        md5 = dist.get("md5")
        if sha is None and md5 is None:
            problems.append(f"croissant: {dist_id} needs sha256 or md5")
        if sha is not None and (
            not isinstance(sha, str)
            or len(sha) != 64
            or any(c not in "0123456789abcdef" for c in sha)
        ):
            problems.append(f"croissant: {dist_id} sha256 is not 64-hex")
        if md5 is not None and (
            not isinstance(md5, str)
            or len(md5) != 32
            or any(c not in "0123456789abcdef" for c in md5)
        ):
            problems.append(f"croissant: {dist_id} md5 is not 32-hex")
    for rs in _as_list(doc.get("recordSet")):
        if not isinstance(rs, dict):
            continue
        rs_id = str(rs.get("@id", "?"))
        fields = _as_list(rs.get("field"))
        if not fields:
            problems.append(f"croissant: recordSet {rs_id} has no fields")
        declared = {str(f.get("@id")) for f in fields if isinstance(f, dict)}
        for row in _as_list(rs.get("data")):
            if isinstance(row, dict) and set(row) - declared:
                problems.append(f"croissant: recordSet {rs_id} row keys exceed fields")
                break
    return problems


# --------------------------------------------------------------------------- #
# RO-Crate (Process Run Crate)
# --------------------------------------------------------------------------- #


def _ref(iri: str) -> dict[str, str]:
    return {"@id": iri}


def build_ro_crate_metadata(
    g: Graph, ds: DatasetMeta, *, payload: Sequence[str] = ()
) -> dict[str, object]:
    """Build ``ro-crate-metadata.json`` (flat ``@graph``, Process Run Crate).

    Args:
        g: The instance graph.
        ds: The dataset descriptor.
        payload: Crate-relative file names packaged alongside the metadata.
    """
    actions = _activities(g)
    workflows = sorted({a.workflow for a in actions if a.workflow})
    conforms = [_ref(RO_CRATE_SPEC), _ref(PROCESS_RUN_PROFILE)]
    if workflows:
        # A #47 workflow run is present — the crate honestly earns the
        # Workflow Run Crate tier (the definition is the mainEntity).
        conforms.append(_ref(WORKFLOW_RUN_PROFILE))
    root: dict[str, object] = {
        "@id": "./",
        "@type": "Dataset",
        "name": ds.title,
        "description": ds.description,
        "datePublished": ds.date_published,
        "license": _ref(ds.license_url),
        "publisher": _ref(NAMESPACE + "ro-crate/publisher"),
        "hasPart": [_ref(name) for name in payload],
    }
    if workflows:
        root["mainEntity"] = _ref(workflows[0])
    entities: list[dict[str, object]] = [
        {
            "@id": "ro-crate-metadata.json",
            "@type": "CreativeWork",
            "conformsTo": conforms,
            "about": _ref("./"),
            "description": "Generated from canonical GMEOW instance data;"
            " declared drops: " + "; ".join(DECLARED_DROPS) + ".",
        },
        root,
        {
            "@id": ds.license_url,
            "@type": "CreativeWork",
            "name": ds.license_id,
        },
        {
            "@id": NAMESPACE + "ro-crate/publisher",
            "@type": "Organization",
            "name": ds.creator,
        },
    ]
    for name in payload:
        entities.append(
            {
                "@id": name,
                "@type": "File",
                "name": name,
                "encodingFormat": "text/turtle"
                if name.endswith(".ttl")
                else "application/ld+json",
            }
        )
    for doc in _documents(g):
        entity: dict[str, object] = {
            "@id": str(doc["iri"]),
            "@type": "File",
            "name": str(doc["name"]),
        }
        # `_documents` uses `dict[str, object]`; "digests" is really `dict[str, str]`.
        digests: dict[str, str] = doc["digests"]  # type: ignore[assignment]
        primary = _primary_digest(digests)
        if primary:
            entity["identifier"] = primary  # best-effort single digest
        if doc["contentUrl"]:
            entity["contentUrl"] = str(doc["contentUrl"])
        entities.append(entity)
    for agent in _agents(g):
        app: dict[str, object] = {
            "@id": agent["iri"],
            "@type": "SoftwareApplication",
            "name": agent["name"],
        }
        if agent["version"]:
            app["softwareVersion"] = agent["version"]
        entities.append(app)
    for workflow in workflows:
        entities.append(
            {
                "@id": workflow,
                "@type": ["File", "SoftwareSourceCode", "ComputationalWorkflow"],
                "name": workflow.rsplit("/", 1)[-1],
            }
        )
    for act in actions:
        action: dict[str, object] = {
            "@id": act.iri,
            "@type": "CreateAction",
            "name": act.name,
        }
        instrument = act.workflow or act.instrument
        if instrument:
            action["instrument"] = _ref(instrument)
        if act.agent:
            action["agent"] = _ref(act.agent)
        if act.objects:
            action["object"] = [_ref(o) for o in act.objects]
        if act.results:
            action["result"] = [_ref(r) for r in act.results]
        if act.end_time:
            action["endTime"] = act.end_time
        entities.append(action)

    # Backfill: every action object/result resolves to an entity (build
    # sources/outputs are not Documents, so the loops above missed them).
    present = {str(e["@id"]) for e in entities}
    referenced = sorted(
        {iri for act in actions for iri in (*act.objects, *act.results)} - present
    )
    for iri in referenced:
        node = URIRef(iri)
        entity = {"@id": iri, "@type": "Thing", "name": _label(g, node)}
        digest = _text(g, node, _CONTENT_DIGEST)
        if digest:
            entity["identifier"] = digest
        entities.append(entity)

    return {"@context": RO_CRATE_CONTEXT, "@graph": entities}


def build_ro_crate_preview(metadata: dict[str, object]) -> str:
    """A minimal deterministic preview page (crate viewers expect one)."""
    graph = _as_list(metadata.get("@graph"))
    root: dict[str, object] = next(
        (e for e in graph if isinstance(e, dict) and e.get("@id") == "./"), {}
    )

    def esc(entity: dict[str, object], key: str, default: str = "") -> str:
        return escape(str(entity.get(key, default)))

    rows = "\n".join(
        f"<tr><td>{esc(e, '@id')}</td><td>{esc(e, '@type')}</td>"
        f"<td>{esc(e, 'name')}</td></tr>"
        for e in graph
        if isinstance(e, dict)
    )
    return (
        '<!DOCTYPE html>\n<html lang="en">\n<head><meta charset="utf-8">'
        f"<title>{esc(root, 'name', 'RO-Crate')}</title></head>\n<body>\n"
        f"<h1>{esc(root, 'name')}</h1>\n<p>{esc(root, 'description')}</p>\n"
        f"<p>Published: {esc(root, 'datePublished')}</p>\n"
        '<table border="1">\n<tr><th>@id</th><th>@type</th><th>name</th></tr>\n'
        f"{rows}\n</table>\n</body>\n</html>\n"
    )


def package_ro_crate(crate_dir: Path, out_zip: Path) -> Path:
    """Zip a crate directory deterministically (stored, fixed timestamps)."""
    out_zip.parent.mkdir(parents=True, exist_ok=True)
    files = sorted(p for p in crate_dir.rglob("*") if p.is_file())
    if not files:
        msg = f"no files to package in crate directory: {crate_dir}"
        raise ValueError(msg)
    with zipfile.ZipFile(out_zip, "w", compression=zipfile.ZIP_STORED) as zf:
        for f in files:
            info = zipfile.ZipInfo(
                str(f.relative_to(crate_dir)), date_time=(1980, 1, 1, 0, 0, 0)
            )
            zf.writestr(info, f.read_bytes())
    return out_zip


def validate_ro_crate(crate_dir: Path) -> list[str]:
    """Structural RO-Crate diagnostics: descriptor, root, flat graph, parts."""
    problems: list[str] = []
    meta_path = crate_dir / "ro-crate-metadata.json"
    if not meta_path.is_file():
        return ["ro-crate: ro-crate-metadata.json missing"]
    doc = json.loads(meta_path.read_text(encoding="utf-8"))
    graph = doc.get("@graph")
    if doc.get("@context") != RO_CRATE_CONTEXT or not isinstance(graph, list):
        problems.append("ro-crate: bad @context or missing @graph")
        return problems
    by_id = {e.get("@id"): e for e in graph if isinstance(e, dict)}
    descriptor = by_id.get("ro-crate-metadata.json")
    root = by_id.get("./")
    if descriptor is None or root is None:
        problems.append("ro-crate: descriptor or root entity missing")
        return problems
    if descriptor.get("about") != {"@id": "./"}:
        problems.append("ro-crate: descriptor.about must be ./")
    conforms = descriptor.get("conformsTo", [])
    if _ref(RO_CRATE_SPEC) not in conforms:
        problems.append("ro-crate: descriptor must conform to RO-Crate 1.1")
    for key in ("name", "description", "datePublished", "license"):
        if not root.get(key):
            problems.append(f"ro-crate: root missing {key}")
    if _ref(WORKFLOW_RUN_PROFILE) in conforms:
        main = root.get("mainEntity")
        main_id = main.get("@id") if isinstance(main, dict) else None
        main_entity = by_id.get(main_id)
        if main_entity is None:
            problems.append("ro-crate: workflow tier needs a resolving mainEntity")
        elif "ComputationalWorkflow" not in main_entity.get("@type", []):
            problems.append("ro-crate: mainEntity is not a ComputationalWorkflow")
    for part in _as_list(root.get("hasPart")):
        part_id = str(part.get("@id", "")) if isinstance(part, dict) else ""
        if not part_id or part_id not in by_id:
            problems.append(f"ro-crate: hasPart {part_id!r} has no entity")
        elif (
            not part_id.startswith(("http://", "https://"))
            and not (crate_dir / part_id).is_file()
        ):
            problems.append(f"ro-crate: hasPart file {part_id!r} not packaged")
    return problems


# --------------------------------------------------------------------------- #
# Frictionless
# --------------------------------------------------------------------------- #


def build_frictionless(g: Graph, ds: DatasetMeta) -> dict[str, object]:
    """Build the Frictionless ``datapackage.json`` document."""
    resources: list[dict[str, object]] = []
    for doc in _documents(g):
        resource: dict[str, object] = {
            "name": _slug(str(doc["iri"])).lower().replace("_", "-"),
            "path": str(doc["contentUrl"] or doc["iri"]),
            "title": str(doc["name"]),
        }
        # `_documents` uses `dict[str, object]`; "digests" is really `dict[str, str]`.
        digests: dict[str, str] = doc["digests"]  # type: ignore[assignment]
        primary = _primary_digest(digests)
        if primary:
            resource["hash"] = primary  # best-effort single digest
        resources.append(resource)
    package: dict[str, object] = {
        "name": _slug(ds.iri).lower().replace("_", "-"),
        "title": ds.title,
        "description": ds.description,
        "homepage": ds.landing_page,
        "created": ds.date_published,
        "licenses": [
            {"name": ds.license_id, "path": ds.license_url, "title": ds.license_id}
        ],
        "contributors": [{"title": ds.creator}],
        "resources": resources,
        "notes": "Generated lossy projection of canonical GMEOW data; drops: "
        + "; ".join(DECLARED_DROPS)
        + ".",
    }
    if ds.version:
        package["version"] = ds.version
    return package


def validate_frictionless(doc: dict[str, object]) -> list[str]:
    """Validate against the vendored official Data Package JSON Schema."""
    import jsonschema

    schema = json.loads(_FRICTIONLESS_SCHEMA_FILE.read_text(encoding="utf-8"))
    validator = jsonschema.Draft4Validator(schema)
    return [
        f"frictionless: {err.json_path}: {err.message}"
        for err in sorted(validator.iter_errors(doc), key=str)
    ]


# --------------------------------------------------------------------------- #
# DataCite
# --------------------------------------------------------------------------- #


def build_datacite_xml(g: Graph, ds: DatasetMeta, *, doi: str | None = None) -> str:
    """Build the DataCite kernel-4 resource XML for a GMEOW dataset.

    The DOI defaults to a deterministic placeholder under DataCite's reserved
    TEST prefix (10.5072) — minting the real DOI is the #44 publish act.
    """
    doi = doi or f"{PLACEHOLDER_DOI_PREFIX}/gmeow-{_slug(ds.iri).lower()}"

    root = ET.Element(
        f"{{{DATACITE_NS}}}resource",
        {f"{{{_XSI_NS}}}schemaLocation": _DATACITE_SCHEMA_LOCATION},
    )

    def child(
        parent: ET.Element, tag: str, text: str | None = None, **attrs: str
    ) -> ET.Element:
        el = ET.SubElement(parent, f"{{{DATACITE_NS}}}{tag}", attrs)
        if text is not None:
            el.text = text
        return el

    child(root, "identifier", doi, identifierType="DOI")
    creators = child(root, "creators")
    creator = child(creators, "creator")
    child(creator, "creatorName", ds.creator, nameType="Organizational")
    child(child(root, "titles"), "title", ds.title)
    child(root, "publisher", ds.creator)
    child(root, "publicationYear", ds.publication_year)
    child(
        root,
        "resourceType",
        "Research-object benchmark dataset",
        resourceTypeGeneral="Dataset",
    )
    dates = child(root, "dates")
    child(dates, "date", ds.date_published, dateType="Issued")
    rights_list = child(root, "rightsList")
    child(
        rights_list,
        "rights",
        ds.license_id,
        rightsURI=ds.license_url,
        rightsIdentifier=ds.license_id,
        rightsIdentifierScheme="SPDX",
    )
    descriptions = child(root, "descriptions")
    child(descriptions, "description", ds.description, descriptionType="Abstract")
    child(
        descriptions,
        "description",
        "Generated lossy projection of canonical GMEOW instance data. Drops: "
        + "; ".join(DECLARED_DROPS)
        + ".",
        descriptionType="TechnicalInfo",
    )
    related = child(root, "relatedIdentifiers")
    child(
        related,
        "relatedIdentifier",
        ds.landing_page,
        relatedIdentifierType="URL",
        relationType="IsDescribedBy",
    )

    ET.indent(root)
    return ET.tostring(root, encoding="unicode", xml_declaration=True)


# --------------------------------------------------------------------------- #
# The export pipeline + registered generator
# --------------------------------------------------------------------------- #

#: The flagship worked example's canonical inputs.
EXAMPLE_INPUTS: tuple[Path, ...] = (
    PROJECT_ROOT / "slices/extensions/graphrag/examples/lillith-dataset.ttl",
    PROJECT_ROOT / "slices/extensions/graphrag/examples/lillith-pipeline.ttl",
    PROJECT_ROOT / "slices/core/ai/examples/grounded-claim.ttl",
    EVALS_DIR / "corpus.ttl",
    EVALS_DIR / "rubric.ttl",
    GENERATED_DIR / "evals" / "scores.ttl",
)

RESEARCH_OBJECTS_DIR = GENERATED_DIR / "research-objects" / "lillith"
CRATE_ZIP = DIST_DIR / "research-objects" / "lillith.crate.zip"


def _write_json(path: Path, doc: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


def export_research_objects(
    inputs: Sequence[Path],
    out_dir: Path,
    *,
    profiles: Sequence[str] = (
        "croissant",
        "ro-crate",
        "dcat",
        "datacite",
        "frictionless",
    ),
    stem: str = "dataset",
    banner: tuple[str, str] | None = None,
) -> list[Path]:
    """Run the research-object builders over instance data.

    Args:
        inputs: Instance Turtle files (the A-Box; the dataset descriptor must
            be among them).
        out_dir: Output directory.
        profiles: Which exports to produce.
        stem: Output filename stem.
        banner: Optional ``(generator name, source hash)`` for the Turtle
            generated-file banner. The source hash is accepted for generator
            API compatibility but is not embedded (JSON/XML/HTML never get
            banners — losses ride their native fields instead).

    Returns:
        Written paths. Raises GeneratorError on validator diagnostics (P7).
    """
    supported = {"croissant", "ro-crate", "dcat", "datacite", "frictionless"}
    unknown = sorted(set(profiles) - supported)
    if unknown:
        msg = f"unknown research-object profile(s): {', '.join(unknown)}"
        raise ValueError(msg)
    g = load_instance_graph(inputs)
    ds = dataset_meta(g)
    written: list[Path] = []
    problems: list[str] = []

    if "croissant" in profiles:
        doc = build_croissant(g, ds)
        problems += validate_croissant(doc)
        path = out_dir / f"{stem}.croissant.jsonld"
        _write_json(path, doc)
        written.append(path)

    if "ro-crate" in profiles:
        from gmeow_tools.language_tags import retag_graph

        crate_dir = out_dir / "ro-crate"
        crate_dir.mkdir(parents=True, exist_ok=True)
        payload: list[str] = []
        for src in inputs:
            if src.suffix == ".ttl":
                # The crate is a PUBLICATION: internal x-gmeow-* language
                # tags retag to public BCP-47 at this boundary (#287) — the
                # canonical source files keep theirs.
                source_graph = Graph()
                source_graph.parse(src, format="turtle")
                turtle = retag_graph(source_graph).serialize(format="turtle")
                (crate_dir / src.name).write_text(
                    turtle.rstrip("\n") + "\n", encoding="utf-8"
                )
                payload.append(src.name)
        croissant_name = f"{stem}.croissant.jsonld"
        if (out_dir / croissant_name).is_file():
            (crate_dir / croissant_name).write_bytes(
                (out_dir / croissant_name).read_bytes()
            )
            payload.append(croissant_name)
        metadata = build_ro_crate_metadata(g, ds, payload=sorted(payload))
        _write_json(crate_dir / "ro-crate-metadata.json", metadata)
        (crate_dir / "ro-crate-preview.html").write_text(
            build_ro_crate_preview(metadata), encoding="utf-8"
        )
        problems += validate_ro_crate(crate_dir)
        written += sorted(p for p in crate_dir.rglob("*") if p.is_file())

    if "dcat" in profiles:
        from gmeow_tools import sparql
        from gmeow_tools.graph import bind_prefixes
        from gmeow_tools.projections import project_graph

        store = sparql.store_with(include_imports=False, extra_triples=g)
        projected = project_graph("dcat", store)
        bind_prefixes(projected)
        path = out_dir / f"{stem}.dcat.ttl"
        turtle = projected.serialize(format="turtle").rstrip("\n") + "\n"
        if banner is not None:
            write_text(path, turtle, name=banner[0], source_hash=banner[1])
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(turtle, encoding="utf-8")
        written.append(path)

    if "datacite" in profiles:
        path = out_dir / f"{stem}.datacite.xml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(build_datacite_xml(g, ds) + "\n", encoding="utf-8")
        written.append(path)

    if "frictionless" in profiles:
        doc = build_frictionless(g, ds)
        problems += validate_frictionless(doc)
        path = out_dir / "datapackage.json"
        _write_json(path, doc)
        written.append(path)

    if problems:
        raise GeneratorError(
            "research-object validation failed: " + "; ".join(problems)
        )
    return written


@register
class ResearchObjectsGenerator(Generator):
    """Generate the flagship research-object exports (the #58 drift gate)."""

    name: str = "research-objects"

    @property
    def inputs(self) -> Sequence[Path]:
        """The worked example's instance data + the compiled dcat query."""
        return [
            *EXAMPLE_INPUTS,
            GENERATED_DIR / "queries" / "dcat.rq",
        ]

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed artifacts under generated/, plus the git-ignored zip."""
        payload = sorted(p.name for p in EXAMPLE_INPUTS if p.suffix == ".ttl")
        return [
            RESEARCH_OBJECTS_DIR / "lillith.croissant.jsonld",
            RESEARCH_OBJECTS_DIR / "ro-crate" / "ro-crate-metadata.json",
            RESEARCH_OBJECTS_DIR / "ro-crate" / "ro-crate-preview.html",
            *(
                RESEARCH_OBJECTS_DIR / "ro-crate" / name
                for name in (*payload, "lillith.croissant.jsonld")
            ),
            RESEARCH_OBJECTS_DIR / "lillith.dcat.ttl",
            RESEARCH_OBJECTS_DIR / "lillith.datacite.xml",
            RESEARCH_OBJECTS_DIR / "datapackage.json",
            CRATE_ZIP,
        ]

    def render(self, staging: Path) -> None:
        """Render and validate the worked example's research objects."""
        out_dir = staging / RESEARCH_OBJECTS_DIR.relative_to(PROJECT_ROOT)
        source_hash = getattr(self, "_source_hash", "")
        export_research_objects(
            EXAMPLE_INPUTS,
            out_dir,
            stem="lillith",
            banner=(self.name, source_hash) if source_hash else None,
        )
        zip_path = staging / CRATE_ZIP.relative_to(PROJECT_ROOT)
        package_ro_crate(out_dir / "ro-crate", zip_path)

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Default byte drift, but git-ignored outputs (the zip) may be absent."""
        if not committed.exists():
            return []
        if not fresh.exists():
            return [f"{_rel(committed)} (not produced in staging)"]
        if fresh.read_bytes() != committed.read_bytes():
            return [f"{_rel(committed)}"]
        return []
