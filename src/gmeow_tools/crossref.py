"""Generate a CrossRef deposit document for the GMEOW DOI(s).

Blackcat Informatics® mints GMEOW's DOI as a CrossRef member (its own prefix),
not via Zenodo. CrossRef registration is by depositing XML conforming to the
CrossRef deposit schema (5.4.0); an ontology is deposited as a ``<database>`` /
``<dataset>`` record whose DOI resolves to a landing page.

GMEOW follows a **single-anchor** DOI strategy (see ``docs/dois.md``):

* a **concept DOI** denotes the Work (always-latest citation anchor); it
  resolves to the concept IRI (:data:`ONTOLOGY_IRI`);
* an **optional version DOI** denotes the immutable release (the Manifestation);
  it resolves to the release ``owl:versionIRI``. Concept-only is a valid state.

The deposit uses the *whole* schema, not just ``<dataset type="record">``:
contributors (organization + ORCID-identified persons), description, publication
and update dates, publisher and institution identifiers, ``publisher_item``
identifiers, ``version_info``, ``format``, the **AccessIndicators** license
program (``ai:program`` — the CC license made machine-readable for version-of-
record and TDM uses), text-mining full-text URLs, references, and a rich
relations program (``rel:program``):

* ``hasFormat`` intra-work relations to every published serialization (the
  Crossref-native analog of the FAIR Signposting ``item`` links);
* ``isSupplementedBy`` → the source repository;
* the concept↔version edge → ``isVersionOf`` / ``hasVersion`` (read by FRBR role);
* the curated alignment targets (:data:`ALIGNMENT_TARGETS`) → ``isDerivedFrom``
  (upper ontologies) / ``references`` (peer schemas) so the deposit is a
  first-class PID-graph node;
* a documented (unpopulated) ``<component_list>`` seam where future per-profile
  sub-DOIs would attach with ``parent_relation="isPartOf"``.

Granularity (modules, mapping-sets, profiles) and provenance are NOT minted as
DOIs — they ride the content-addressed identifier triangle (``owl:versionIRI`` ↔
SWHID / ``gmeow:gtsHeadId`` / ``gmeow:contentDigest``).

The output is well-formed and follows the schema's element model; validate it
against the official XSD (deposit 5.4.0 + ``AccessIndicators.xsd`` +
``relations.xsd``) and test on the CrossRef *test* system before a production
deposit.
"""

from __future__ import annotations

import datetime
import json
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from xml.etree import ElementTree as ET

from gmeow_validate import build_deposit_xml_native, lint_deposit_native

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    CROSSMARK_ENABLED,
    CROSSMARK_POLICY_DOI,
    DATASET_SLUG,
    DEPOSIT_FORMAT,
    DIST_DIR,
    ONTOLOGY_FILE,
    ONTOLOGY_IRI,
    PROJECT_ROOT,
    REGISTRANT_ACRONYM,
    REGISTRANT_PLACE,
)
from gmeow_tools.self_desc import Contributor, SelfDescription, load_self_description

#: CrossRef deposit schema namespace (version 5.4.0).
CR_NS = "http://www.crossref.org/schema/5.4.0"
#: CrossRef access-indicators (license) program namespace.
AI_NS = "http://www.crossref.org/AccessIndicators.xsd"
#: CrossRef relationships program namespace (relations.xsd).
REL_NS = "http://www.crossref.org/relations.xsd"
_XSI_NS = "http://www.w3.org/2001/XMLSchema-instance"
_SCHEMA_LOCATION = f"{CR_NS} https://www.crossref.org/schemas/crossref5.4.0.xsd"

#: The placeholder prefix that must never reach a real deposit (doi-lint).
PLACEHOLDER_DOI_PREFIX = "10.XXXXX"

#: CITATION.cff — checked for DOI consistency by doi-lint.
_CITATION_CFF = PROJECT_ROOT / "CITATION.cff"

#: Published serializations at ``{ONTOLOGY_IRI}.{ext}``; each becomes a
#: ``hasFormat`` relation (the deposit's machine-readable representations).
_SERIALIZATIONS: tuple[tuple[str, str, str], ...] = (
    ("ttl", "Turtle", "text/turtle"),
    ("rdf", "RDF/XML", "application/rdf+xml"),
    ("nt", "N-Triples", "application/n-triples"),
    ("jsonld", "JSON-LD", "application/ld+json"),
    # Crossref's 5.4.0 mediatype enum does not include the vendor-specific GTS
    # media type. GTS is a CBOR Sequence, so use the schema-valid parent type here.
    ("gts", "GTS content-addressed package", "application/cbor-seq"),
)


@dataclass(frozen=True, slots=True)
class _Relation:
    """One CrossRef relations-program entry."""

    kind: str  # "intra_work_relation" | "inter_work_relation"
    type: str  # relationship-type, e.g. "hasFormat"
    identifier_type: str  # "uri" | "doi" | ...
    target: str
    description: str


@dataclass(frozen=True, slots=True)
class _TdmResource:
    """One Crossref text-mining full-text URL."""

    url: str
    mime_type: str
    content_version: str = "vor"


@dataclass(frozen=True, slots=True)
class _Citation:
    """One Crossref citation-list entry.

    Crossref accepts two deposit-time citation shapes: a fully-structured
    journal-article reference, or a free-text ``unstructured_citation`` (used when
    a reference cannot be parsed into components). We deposit alignment targets in
    the latter shape: they are external ontologies, not journal articles. Emitting
    *partial* structured metadata (``journal_title``/``article_title``) without an
    ``author`` or ``first_page`` makes Crossref treat the entry as a malformed
    journal-article citation and reject it ("Either first page or author must be
    supplied."), so we carry only ``unstructured`` text plus an optional ``doi``.
    """

    key: str
    type: str
    unstructured: str
    doi: str | None = None


def _child(
    parent: ET.Element,
    tag: str,
    text: str | None = None,
    attrs: dict[str, str] | None = None,
    ns: str = CR_NS,
) -> ET.Element:
    """Append a namespaced child element with optional text and attributes."""
    element = ET.SubElement(parent, f"{{{ns}}}{tag}", attrs or {})
    if text is not None:
        element.text = text
    return element


def _live_stamp(description: SelfDescription) -> tuple[str, str]:
    """Return a (timestamp, batch_id) pair for a fresh submission.

    The deposit is a transient submission document, not a committed artifact, so
    the timestamp is the current UTC time: CrossRef uses it to order (re)submissions
    of the same DOI, and a correction must out-rank the previous deposit. The batch
    id embeds it so each generated deposit is uniquely identifiable.
    """
    timestamp = datetime.datetime.now(datetime.UTC).strftime("%Y%m%d%H%M%S")
    batch_id = f"gmeow-{description.version}-{timestamp}"
    return timestamp, batch_id


def _doi_suffix(doi: str) -> str:
    """The suffix of a DOI (the part after ``10.<prefix>/``)."""
    return doi.split("/", 1)[1] if "/" in doi else doi


def _crossref_pid_uri(identifier: str) -> str:
    """Normalize internal authority IRIs to Crossref PID URI constraints."""
    return identifier.replace(
        "http://www.wikidata.org/entity/", "https://www.wikidata.org/entity/", 1
    )


# --------------------------------------------------------------------------- #
# Element builders (one per schema construct)
# --------------------------------------------------------------------------- #


def _add_contributors(parent: ET.Element, contributors: Sequence[Contributor]) -> None:
    """Add the ``<contributors>`` block (organizations + ORCID persons)."""
    node = _child(parent, "contributors")
    for contributor in contributors:
        attrs = {"sequence": contributor.sequence, "contributor_role": contributor.role}
        if contributor.kind == "organization":
            _child(node, "organization", contributor.name, attrs=attrs)
            continue
        person = _child(node, "person_name", attrs=attrs)
        if contributor.given_name:
            _child(person, "given_name", contributor.given_name)
        _child(person, "surname", contributor.surname)
        if contributor.orcid:
            _child(person, "ORCID", contributor.orcid)


def _add_date(parent: ET.Element, date_name: str, iso_date: str) -> None:
    """Add a ``<database_date>`` container with the given inner date element."""
    year, month, day = iso_date.split("-")
    date = _child(
        _child(parent, "database_date"), date_name, attrs={"media_type": "online"}
    )
    _child(date, "month", month)
    _child(date, "day", day)
    _child(date, "year", year)


def _add_publisher(parent: ET.Element, name: str, place: str) -> None:
    """Add the ``<publisher>`` block."""
    node = _child(parent, "publisher")
    _child(node, "publisher_name", name)
    if place:
        _child(node, "publisher_place", place)


def _add_institution(
    parent: ET.Element,
    name: str,
    acronym: str,
    place: str,
    identifiers: Sequence[tuple[str, str]] = (),
) -> None:
    """Add the ``<institution>`` block."""
    node = _child(parent, "institution")
    _child(node, "institution_name", name)
    for id_type, value in identifiers:
        _child(
            node, "institution_id", _crossref_pid_uri(value), attrs={"type": id_type}
        )
    if acronym:
        _child(node, "institution_acronym", acronym)
    if place:
        _child(node, "institution_place", place)


def _add_publisher_item(
    parent: ET.Element,
    item_numbers: Sequence[tuple[str, str]],
    identifiers: Sequence[tuple[str, str]],
) -> None:
    """Add ``<publisher_item>`` (local item numbers + alternative identifiers)."""
    if not item_numbers and not identifiers:
        return
    node = _child(parent, "publisher_item")
    for number_type, value in item_numbers:
        _child(node, "item_number", value, attrs={"item_number_type": number_type})
    for id_type, value in identifiers:
        _child(node, "identifier", value, attrs={"id_type": id_type})


def _add_version_info(parent: ET.Element, version: str, description: str) -> None:
    """Add the ``<version_info>`` block."""
    node = _child(parent, "version_info")
    _child(node, "version", version)
    if description:
        _child(node, "description", description)


def _add_access(parent: ET.Element, license_url: str, start_date: str) -> None:
    """Add the AccessIndicators ``<ai:program>`` (free-to-read + CC license)."""
    if not license_url:
        return
    node = _child(parent, "program", attrs={"name": "AccessIndicators"}, ns=AI_NS)
    _child(node, "free_to_read", attrs={"start_date": start_date}, ns=AI_NS)
    for applies_to in ("vor", "tdm"):
        _child(
            node,
            "license_ref",
            license_url,
            attrs={"start_date": start_date, "applies_to": applies_to},
            ns=AI_NS,
        )


def _add_crossmark(
    parent: ET.Element, policy_doi: str, license_url: str, start_date: str
) -> None:
    """Add a Crossmark block with AccessIndicators nested in custom_metadata."""
    crossmark = _child(parent, "crossmark")
    _child(crossmark, "crossmark_version", "1")
    _child(crossmark, "crossmark_policy", policy_doi)
    if license_url:
        custom_metadata = _child(crossmark, "custom_metadata")
        _add_access(custom_metadata, license_url, start_date)


def _add_relations(parent: ET.Element, relations: Sequence[_Relation]) -> None:
    """Add the ``<rel:program>`` block with one related_item per relation."""
    if not relations:
        return
    program = _child(parent, "program", attrs={"name": "relations"}, ns=REL_NS)
    for relation in relations:
        item = _child(program, "related_item", ns=REL_NS)
        if relation.description:
            _child(item, "description", relation.description, ns=REL_NS)
        _child(
            item,
            relation.kind,
            relation.target,
            attrs={
                "relationship-type": relation.type,
                "identifier-type": relation.identifier_type,
            },
            ns=REL_NS,
        )


def _add_tdm_resources(doi_data: ET.Element, resources: Sequence[_TdmResource]) -> None:
    """Add Crossref full-text URLs for text and data mining."""
    if not resources:
        return
    collection = _child(doi_data, "collection", attrs={"property": "text-mining"})
    for resource in resources:
        item = _child(collection, "item")
        _child(
            item,
            "resource",
            resource.url,
            attrs={
                "mime_type": resource.mime_type,
                "content_version": resource.content_version,
            },
        )


def _add_citation_list(parent: ET.Element, citations: Sequence[_Citation]) -> None:
    """Add Crossref references to the dataset record."""
    if not citations:
        return
    citation_list = _child(parent, "citation_list")
    for citation in citations:
        node = _child(
            citation_list,
            "citation",
            attrs={"key": citation.key, "type": citation.type},
        )
        # A DOI is the strongest cross-link when an alignment target has one. The
        # free-text unstructured_citation is the schema-valid, business-rule-safe
        # carrier for everything else; partial structured metadata (journal_title /
        # article_title) without author/first_page is rejected at deposit time.
        if citation.doi:
            _child(node, "doi", citation.doi)
        _child(node, "unstructured_citation", citation.unstructured)


# --------------------------------------------------------------------------- #
# Relation projection
# --------------------------------------------------------------------------- #


def _alignment_relations() -> list[_Relation]:
    """Project the curated alignment targets into inter-work relations.

    Upper ontologies are foundations GMEOW *derives from*; peer schemas / value
    vocabularies are *referenced*. Identified by DOI when known, else namespace URI
    (CrossRef accepts ``identifier-type="uri"`` and auto-creates the reverse link).
    """
    relations: list[_Relation] = []
    for key in sorted(ALIGNMENT_TARGETS):
        target = ALIGNMENT_TARGETS[key]
        relations.append(
            _Relation(
                kind="inter_work_relation",
                type="isDerivedFrom" if target.kind == "upper" else "references",
                identifier_type="doi" if target.doi else "uri",
                target=target.related_identifier,
                description=f"GMEOW aligns to {target.name} by reference.",
            )
        )
    return relations


def _alignment_citations() -> list[_Citation]:
    """Project alignment targets into Crossref citation-list references."""
    citations: list[_Citation] = []
    for key in sorted(ALIGNMENT_TARGETS):
        target = ALIGNMENT_TARGETS[key]
        identifier = target.related_identifier
        # Deposited as a free-text reference: alignment targets are external
        # ontologies, not journal articles. The name and resolving identifier ride
        # the unstructured_citation; a DOI is added structurally when one exists.
        citations.append(
            _Citation(
                key=f"ref-{key}",
                type="web_resource",
                unstructured=f"{target.name}. {identifier}.",
                doi=target.doi,
            )
        )
    return citations


def _format_relations(base_iri: str = ONTOLOGY_IRI) -> list[_Relation]:
    """``hasFormat`` relations to every published serialization of *base_iri*.

    The concept record points at the always-latest Work serializations
    (``{ONTOLOGY_IRI}.{ext}``); the version record points at its own immutable
    release snapshot (``{version_iri}.{ext}``), so the version DOI never links to
    mutable targets.
    """
    return [
        _Relation(
            kind="intra_work_relation",
            type="hasFormat",
            identifier_type="uri",
            target=f"{base_iri}.{ext}",
            description=f"{label} serialization of the ontology.",
        )
        for ext, label, _mime_type in _SERIALIZATIONS
    ]


def _tdm_resources(base_iri: str = ONTOLOGY_IRI) -> list[_TdmResource]:
    """Text-mining URLs to every public machine-readable serialization."""
    return [
        _TdmResource(f"{base_iri}.{ext}", mime_type)
        for ext, _label, mime_type in _SERIALIZATIONS
    ]


# --------------------------------------------------------------------------- #
# Dataset records
# --------------------------------------------------------------------------- #


def _add_dataset(
    database: ET.Element,
    *,
    description: SelfDescription,
    doi: str,
    resource: str,
    title: str,
    dataset_description: str,
    relations: Sequence[_Relation],
    tdm_resources: Sequence[_TdmResource],
    citations: Sequence[_Citation],
    component_seam: bool,
    crossmark_policy: str | None = None,
) -> None:
    """Append one fully-populated ``<dataset>`` record."""
    dataset = _child(database, "dataset", attrs={"dataset_type": "record"})
    _add_contributors(dataset, description.contributors)
    _child(_child(dataset, "titles"), "title", title)
    _add_date(dataset, "publication_date", description.release_date)
    _add_date(dataset, "update_date", description.release_date)
    _add_publisher_item(
        dataset,
        item_numbers=[("doi-suffix", _doi_suffix(doi)), ("site", DATASET_SLUG)],
        identifiers=[("other", resource)],
    )
    _child(dataset, "description", dataset_description)
    _child(dataset, "format", DEPOSIT_FORMAT)
    if crossmark_policy:
        _add_crossmark(
            dataset, crossmark_policy, description.license_uri, description.release_date
        )
    else:
        _add_access(dataset, description.license_uri, description.release_date)
    _add_relations(dataset, relations)
    _add_version_info(
        dataset,
        description.version,
        f"Release {description.version} of the GMEOW ontology.",
    )
    if component_seam:
        # Profile sub-DOI seam (deferred). When per-profile DOIs are minted, add a
        # <component_list> here with one <component parent_relation="isPartOf"> per
        # profile, keyed off the profile's content-addressed identity (GTS head id).
        # An *empty* <component_list> is XSD-invalid, so the seam is a comment.
        dataset.append(
            ET.Comment(
                " profile sub-DOI seam: future <component_list> with "
                '<component parent_relation="isPartOf"> per profile '
            )
        )
    doi_data = _child(dataset, "doi_data")
    _child(doi_data, "doi", doi)
    _child(doi_data, "resource", resource)
    _add_tdm_resources(doi_data, tdm_resources)
    _add_citation_list(dataset, citations)


def _build_deposit_xml_et(
    description: SelfDescription,
    timestamp: str,
    batch_id: str,
) -> str:
    """Build the deposit XML using Python's ElementTree (for lint's round-trip check).

    This path uses the Python ``_alignment_citations`` / ``_alignment_relations``
    functions directly, which means Python-level monkeypatching (e.g. in tests)
    is respected. ``lint_deposit`` uses this instead of
    :func:`build_deposit_xml` so that test fixtures that inject duplicate
    citation keys via ``monkeypatch.setattr(crossref_mod, "_alignment_citations", …)``
    are visible to the duplicate-key lint check.
    """
    has_version = description.version_doi is not None
    if CROSSMARK_ENABLED:
        if not CROSSMARK_POLICY_DOI.strip():
            raise ValueError(
                "CROSSMARK_POLICY_DOI must be non-empty when CROSSMARK_ENABLED is True."
            )
        crossmark_policy = CROSSMARK_POLICY_DOI
    else:
        crossmark_policy = None

    ET.register_namespace("", CR_NS)
    ET.register_namespace("ai", AI_NS)
    ET.register_namespace("rel", REL_NS)
    ET.register_namespace("xsi", _XSI_NS)
    root = ET.Element(
        f"{{{CR_NS}}}doi_batch",
        {
            f"{{{_XSI_NS}}}schemaLocation": _SCHEMA_LOCATION,
            "version": "5.4.0",
        },
    )

    head = _child(root, "head")
    _child(head, "doi_batch_id", batch_id)
    _child(head, "timestamp", timestamp)
    depositor = _child(head, "depositor")
    _child(depositor, "depositor_name", description.depositor_name)
    _child(depositor, "email_address", description.depositor_email)
    _child(head, "registrant", description.registrant)

    body = _child(root, "body")
    database = _child(body, "database")

    db_metadata = _child(database, "database_metadata", attrs={"language": "en"})
    _add_contributors(db_metadata, description.contributors)
    _child(_child(db_metadata, "titles"), "title", description.title)
    if description.description:
        _child(db_metadata, "description", description.description)
    _add_date(db_metadata, "publication_date", description.release_date)
    _add_date(db_metadata, "update_date", description.release_date)
    _add_publisher(db_metadata, description.registrant, REGISTRANT_PLACE)
    _add_institution(
        db_metadata,
        description.registrant,
        REGISTRANT_ACRONYM,
        REGISTRANT_PLACE,
        identifiers=(
            (("wikidata", description.registrant_wikidata),)
            if description.registrant_wikidata
            else ()
        ),
    )
    _add_publisher_item(
        db_metadata, item_numbers=[("site", DATASET_SLUG)], identifiers=[]
    )
    _add_version_info(
        db_metadata,
        description.version,
        f"Release {description.version} of the GMEOW ontology.",
    )

    concept_relations = [*_format_relations()]
    if description.repo_url:
        concept_relations.append(
            _Relation(
                "inter_work_relation",
                "isSupplementedBy",
                "uri",
                description.repo_url,
                "Source repository for the GMEOW ontology.",
            )
        )
    if has_version:
        assert description.version_doi is not None
        concept_relations.append(
            _Relation(
                "intra_work_relation",
                "hasVersion",
                "doi",
                description.version_doi,
                f"Immutable version DOI for release {description.version}.",
            )
        )
    concept_relations.extend(_alignment_relations())
    _add_dataset(
        database,
        description=description,
        doi=description.concept_doi,
        resource=ONTOLOGY_IRI,
        title=f"{description.title} (concept)",
        dataset_description=description.description,
        relations=concept_relations,
        tdm_resources=_tdm_resources(),
        citations=_alignment_citations(),
        component_seam=False,
        crossmark_policy=crossmark_policy,
    )

    if has_version:
        assert description.version_doi is not None
        version_relations = [
            *_format_relations(description.version_iri),
            _Relation(
                "intra_work_relation",
                "isVersionOf",
                "doi",
                description.concept_doi,
                "Concept DOI for the always-latest GMEOW ontology.",
            ),
        ]
        _add_dataset(
            database,
            description=description,
            doi=description.version_doi,
            resource=description.version_iri,
            title=f"{description.title} (version {description.version})",
            dataset_description=description.description,
            relations=version_relations,
            tdm_resources=_tdm_resources(description.version_iri),
            citations=_alignment_citations(),
            component_seam=True,
            crossmark_policy=crossmark_policy,
        )

    ET.indent(root)
    return ET.tostring(root, encoding="unicode", xml_declaration=True)


def _to_deposit_input_json(description: SelfDescription) -> str:
    """Serialise a ``SelfDescription`` and runtime config to JSON for the Rust backend.

    The returned JSON encodes a ``DepositInput`` struct understood by
    ``gmeow_validate.build_deposit_xml_native``.
    """
    contributors = [
        {
            "kind": c.kind,
            "name": c.name,
            "orcid": c.orcid,
            "sequence": c.sequence,
            "role": c.role,
        }
        for c in description.contributors
    ]
    alignment_targets = [
        {
            "key": key,
            "name": target.name,
            "namespace": target.namespace,
            "kind": target.kind,
            "doi": target.doi,
            "related_identifier": target.related_identifier,
        }
        for key, target in sorted(ALIGNMENT_TARGETS.items())
    ]
    payload = {
        "self_description": {
            "title": description.title,
            "version": description.version,
            "release_date": description.release_date,
            "concept_doi": description.concept_doi,
            "version_doi": description.version_doi,
            "version_iri": description.version_iri,
            "depositor_name": description.depositor_name,
            "depositor_email": description.depositor_email,
            "registrant": description.registrant,
            "registrant_wikidata": description.registrant_wikidata,
            "license_uri": description.license_uri,
            "homepage": description.homepage,
            "description": description.description,
            "repo_url": description.repo_url,
            "contributors": contributors,
        },
        "config": {
            "ontology_iri": ONTOLOGY_IRI,
            "dataset_slug": DATASET_SLUG,
            "deposit_format": DEPOSIT_FORMAT,
            "registrant_place": REGISTRANT_PLACE,
            "registrant_acronym": REGISTRANT_ACRONYM,
            "crossmark_enabled": CROSSMARK_ENABLED,
            "crossmark_policy_doi": CROSSMARK_POLICY_DOI,
            "alignment_targets": alignment_targets,
        },
    }
    return json.dumps(payload)


def _to_lint_input_json(
    description: SelfDescription,
    pre_rendered_xml: str | None = None,
) -> str:
    """Serialise the full lint context to JSON for the Rust backend.

    The returned JSON encodes a ``LintInput`` struct understood by
    ``gmeow_validate.lint_deposit_native``.

    Args:
        description: The self-description metadata to lint.
        pre_rendered_xml: The deposit XML already rendered by the Python path
            (via ``build_deposit_xml``). When provided the Rust linter uses it
            for its round-trip and citation-key checks instead of generating
            XML internally, so Python-level monkeypatching of functions like
            ``_alignment_citations`` is respected in the lint output.
    """
    base = json.loads(_to_deposit_input_json(description))
    # Read optional file contents — lint checks their text, never resolves over network.
    citation_cff: str | None = None
    if _CITATION_CFF.exists():
        citation_cff = _CITATION_CFF.read_text(encoding="utf-8")
    ontology_ttl: str | None = None
    if ONTOLOGY_FILE.exists():
        ontology_ttl = ONTOLOGY_FILE.read_text(encoding="utf-8")
    base["citation_cff"] = citation_cff
    base["ontology_ttl"] = ontology_ttl
    base["pre_rendered_xml"] = pre_rendered_xml
    return json.dumps(base)


def build_deposit_xml(
    *,
    meta: SelfDescription | None = None,
    timestamp: str | None = None,
    batch_id: str | None = None,
) -> str:
    """Build the CrossRef deposit XML for GMEOW's DOI(s).

    Emits the concept record always, and the version record only when a version
    DOI has been minted (``meta.version_doi`` is set). When both are present they
    are cross-linked with ``hasVersion`` / ``isVersionOf``.

    Args:
        meta: Preloaded self-description metadata. Defaults to the checkout file.
        timestamp: CrossRef batch timestamp (``YYYYMMDDHHMMSS``); defaults to the
            current UTC time (CrossRef uses it to order competing submissions).
        batch_id: Unique submission id; defaults to ``gmeow-{version}-{timestamp}``.

    Returns:
        The deposit document as an XML string (with declaration).
    """
    description = meta or load_self_description()
    default_ts, default_batch = _live_stamp(description)
    timestamp = timestamp or default_ts
    batch_id = batch_id or default_batch
    return build_deposit_xml_native(
        _to_deposit_input_json(description), timestamp, batch_id
    )


def write_deposit(
    path: Path | None = None,
    *,
    meta: SelfDescription | None = None,
    **kwargs: str,
) -> Path:
    """Write the CrossRef deposit XML to ``dist/`` for manual submission.

    The deposit is a transient submission document — generated on demand, hand-
    verified, and submitted to CrossRef by the registrant — NOT a committed,
    drift-gated artifact. It therefore lives under ``dist/`` (ephemeral build
    output), never under ``generated/``.

    Args:
        path: Output path (defaults to ``dist/crossref-deposit.xml``).
        meta: Preloaded self-description metadata.
        **kwargs: Forwarded to :func:`build_deposit_xml`.

    Returns:
        The path written.
    """
    out = path or (DIST_DIR / "crossref-deposit.xml")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(build_deposit_xml(meta=meta, **kwargs) + "\n", encoding="utf-8")
    return out


# --------------------------------------------------------------------------- #
# doi-lint — consistency invariants, run at generation time (gmeow-dev crossref)
# --------------------------------------------------------------------------- #


def lint_deposit(meta: SelfDescription | None = None) -> list[str]:
    """Return DOI consistency problems, or ``[]`` if the deposit is sound.

    Run by ``gmeow-dev crossref`` before the deposit is written, so an
    inconsistent deposit can never be produced for submission. Format/consistency
    only — never resolves a DOI over the network (our own DOI is undeposited until
    submitted, so it 404s; a network probe would be a false failure). Checks:

    (a) no ``10.XXXXX`` placeholder survives in self-description, CITATION.cff,
        or the rendered deposit;
    (b) the deposit's DOIs equal the self-description's DOIs;
    (c) the concept resource is the concept IRI and the version resource is the
        ``owl:versionIRI`` (and term IRIs carry no version — structurally true,
        since the concept resource must not contain a version segment);
    (d) concept-only (no version DOI) is valid — not flagged;
    (e) the deposit carries a license and at least one contributor (maximal-schema
        invariants that a thin deposit would silently drop).
    """
    description = meta or load_self_description()
    # Generate the deposit XML through the Python ET path (which respects
    # monkeypatching of functions like ``_alignment_citations``) and pass it
    # to the Rust linter for its round-trip and citation-key checks.
    # Using ``_build_deposit_xml_et`` (not ``build_deposit_xml``) here ensures
    # that Python-level test fixtures that inject duplicate citation keys are
    # visible to the duplicate-key lint check.
    default_ts, _ = _live_stamp(description)
    pre_rendered_xml = _build_deposit_xml_et(description, default_ts, "lint-pre-render")
    return lint_deposit_native(_to_lint_input_json(description, pre_rendered_xml))
