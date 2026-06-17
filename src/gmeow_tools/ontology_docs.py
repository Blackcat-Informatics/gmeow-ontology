# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Native, RDF 1.2-aware ontology documentation generator (#440).

This generator deliberately avoids WIDOCO, WebVOWL, pyLODE, MkDocs, Docker,
network calls, and client-side JavaScript. It reads the committed GTS fold
(``generated/dist/gmeow.gts``) plus canonical slice guides, then emits a stable
Markdown tree and a static HTML site under ``dist/ontology-docs/``.

The same :func:`build_ontology_docs` function is used by the ``gts``
generator, so the official web site and the offline bundled docs are generated
from the same data path.
"""

from __future__ import annotations

import functools
import hashlib
import html
import json
import posixpath
import re
import shutil
import tempfile
import time
from collections import defaultdict
from contextlib import suppress
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

from markdown import markdown as markdown_to_html

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    FULL_PROFILE_IRI,
    GTS_GRAPH_ALIGNMENTS,
    GTS_SNAPSHOT_FILE,
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    PREFIXES,
    PROJECT_ROOT,
    REFERENCES_MD_FILE,
    SLICES_DIR,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.export import Term, collect_terms, curie, fold_meta
from gmeow_tools.gts_views import FoldView, load_fold
from gmeow_tools.mapping_dsl import (
    Atom,
    OptionalGroup,
    ProfileBinding,
    ProjectionCell,
    load_dsl,
)
from gmeow_tools.slices import Slice, discover_slices, iter_slice_mapping_files

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

_RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
_RDFS = "http://www.w3.org/2000/01/rdf-schema#"
_OWL = "http://www.w3.org/2002/07/owl#"
_SKOS = "http://www.w3.org/2004/02/skos/core#"
_DCTERMS = "http://purl.org/dc/terms/"

_RDF_TYPE = _RDF + "type"
_RDFS_LABEL = _RDFS + "label"
_RDFS_COMMENT = _RDFS + "comment"
_RDFS_IS_DEFINED_BY = _RDFS + "isDefinedBy"
_RDFS_DATATYPE = _RDFS + "Datatype"
_SKOS_DEFINITION = _SKOS + "definition"
_SKOS_SCOPE_NOTE = _SKOS + "scopeNote"
_SKOS_EXAMPLE = _SKOS + "example"
_DCT_DESCRIPTION = _DCTERMS + "description"
_DOCS_CONCERN = NAMESPACE + "docsConcern"
_DOCS_CONCERN_CLASS = NAMESPACE + "DocumentationConcern"
_GRAPH_BOX_ROLE = NAMESPACE + "graphBoxRole"
_USE_WHEN = NAMESPACE + "useWhen"
_AVOID_WHEN = NAMESPACE + "avoidWhen"
_HOW_TO_USE = NAMESPACE + "howToUse"
_USE_FOR_CONSUMER = NAMESPACE + "useForConsumer"
_AVOID_FOR_CONSUMER = NAMESPACE + "avoidForConsumer"
ONTOLOGY_DOCS_GRAPH_INPUT = GTS_SNAPSHOT_FILE
_INTERNAL_TAG_TEXT_RE = re.compile(r"x-gmeow-(?:[A-Za-z0-9-]+|\*)", re.IGNORECASE)
_GMEOW_CURIE_RE = re.compile(r"\bgmeow:[A-Za-z][A-Za-z0-9_-]*")
_EXTERNAL_PREFIX_RE = "|".join(
    re.escape(prefix)
    for prefix in sorted(
        set(PREFIXES) - {"gmeow", "wd", "wdt"},
        key=len,
        reverse=True,
    )
)
_LINKABLE_IDENTIFIER_RE = re.compile(
    r"(?<![\w:/])(?:gmeow:slices/[A-Za-z0-9_-]+|"
    r"gmeow:[A-Za-z][A-Za-z0-9_-]*|wd:Q[1-9]\d*|wd:P[1-9]\d*|"
    r"wdt:P[1-9]\d*|(?:" + _EXTERNAL_PREFIX_RE + r"):[A-Za-z_][A-Za-z0-9_.-]*|"
    r"Q[1-9]\d+|Principle\s+[1-9]\d*)(?![\w/-])"
)
_BARE_IDENTIFIER_RE = re.compile(r"(?<![\w:/`])([A-Za-z][A-Za-z0-9_-]*)(?![\w-])")
_REPO_MARKDOWN_PATH_RE = re.compile(
    r"(?<![\w:/])((?:docs|slices|dsl|queries|shapes)/[A-Za-z0-9_./-]+\.md)(?![\w/-])"
)
_CONSTITUTION_URL = (
    "https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/CONSTITUTION.md"
)
_REPO_BLOB_URL = "https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/"
_BARE_LOWERCASE_TERMS = {"motivates"}
_GUFO_TERM_HELP: dict[str, tuple[str, str]] = {
    "AbstractIndividualType": (
        "abstract-individual value type",
        "A type whose instances are abstract values rather than concrete objects, "
        "events, or relators. In GMEOW docs this usually means an open value "
        "vocabulary such as statuses, kinds, scales, or controlled values.",
    ),
    "Category": (
        "cross-kind classification",
        "A broad classifier that can apply across multiple identity-bearing kinds.",
    ),
    "Disposition": (
        "capacity or tendency",
        "An intrinsic feature that may manifest under conditions, such as a hazard "
        "as a bearer-specific disposition toward harm.",
    ),
    "Event": ("event occurrence", "A concrete happening in time."),
    "EventType": (
        "event type",
        "A classifier for happenings, processes, executions, observations, or "
        "other temporal occurrences.",
    ),
    "FunctionalComplex": (
        "object-like whole",
        "A concrete object or system treated as a functional whole, such as a "
        "person, organization, device, or software system.",
    ),
    "IntrinsicAspect": (
        "dependent feature",
        "A feature that inheres in a bearer and cannot exist independently from "
        "that bearer.",
    ),
    "IntrinsicMode": (
        "intrinsic dependent feature",
        "A bearer-specific internal feature that is not primarily a measured "
        "quality value. In GMEOW this explains things like emotions, desires, "
        "and intentions as features of an agent rather than standalone objects.",
    ),
    "Kind": (
        "identity-providing type",
        "A main sort of thing whose instances share an identity principle, such "
        "as Person, Document, Event, or Procedure.",
    ),
    "Mixin": (
        "shared classifier",
        "A reusable classifier that can cut across multiple kinds without "
        "providing identity by itself.",
    ),
    "Object": ("concrete object", "A concrete endurant object."),
    "Phase": (
        "temporary state-like specialization",
        "A phase an instance can enter or leave while keeping the same identity.",
    ),
    "PhaseMixin": (
        "temporary cross-kind classifier",
        "A reusable phase-style classifier that can apply across multiple kinds.",
    ),
    "Quality": (
        "measurable intrinsic quality",
        "A measurable intrinsic aspect whose value lies in a value space.",
    ),
    "QualityValue": (
        "quality value",
        "A value in a quality or classification space. In GMEOW this often backs "
        "open enumerations without freezing them as closed OWL enums.",
    ),
    "Relator": (
        "relationship object",
        "A reified relationship that connects participants and can carry its own "
        "metadata, roles, provenance, and temporal scope.",
    ),
    "Role": (
        "context-dependent specialization",
        "A role an instance plays in some context without changing its underlying "
        "identity.",
    ),
    "RoleMixin": (
        "context-dependent cross-kind role",
        "A reusable role classifier that can apply to instances of multiple kinds.",
    ),
    "SituationType": (
        "situation type",
        "A classifier for states of affairs rather than concrete objects or events.",
    ),
    "SubKind": (
        "identity-preserving subtype",
        "A narrower type under a Kind that inherits identity from that Kind.",
    ),
}
_EXTERNAL_TARGET_DESCRIPTIONS: dict[str, str] = {
    "afo": (
        "Audio Feature Ontology for music information retrieval — signal "
        "features, their extraction, and feature provenance."
    ),
    "afv": (
        "Audio Feature Vocabulary enumerating named MIR audio features "
        "(timbre, rhythm, and harmony descriptors)."
    ),
    "bbc": "News-domain vocabulary for articles, stories, topics, and metadata.",
    "bfo": (
        "Basic Formal Ontology 2020, the OBO upper-ontology spine used by "
        "GMEOW for by-reference bridges from gUFO categories into the BFO world."
    ),
    "bibframe": (
        "Library of Congress bibliographic model for works, instances, items, "
        "agents, identifiers, and publication description."
    ),
    "bibo": "Bibliographic ontology for citations, documents, and identifiers.",
    "bio": "Biographical events vocabulary for births, deaths, and relationships.",
    "bot": "Building topology vocabulary for sites, buildings, spaces, and zones.",
    "brick": "Built-environment schema for equipment, points, systems, and sensors.",
    "cc": "Creative Commons rights vocabulary for licenses and reuse conditions.",
    "chord": "OMRAS2 chord ontology for harmonic structures and chord symbols.",
    "codemeta": "Software metadata vocabulary for packages and repositories.",
    "crmarc": "CIDOC CRM archaeological extension for excavation and stratigraphy.",
    "crmdig": "CIDOC CRM digital extension for digitization and digital provenance.",
    "crmsci": "CIDOC CRM scientific extension for observations, samples, and measures.",
    "dcterms": "Dublin Core terms for title, creator, date, rights, and license.",
    "doap": "Description of a Project vocabulary for software project metadata.",
    "gufo": (
        "Foundational UFO-derived upper ontology used for identity, relators, "
        "events, qualities, modes, and type taxonomy."
    ),
    "umbel": "Upper mapping vocabulary for broad entity categories and web hubs.",
    "dolce": "DOLCE+DnS Ultralite upper ontology for objects, events, and qualities.",
    "dqv": "W3C Data Quality Vocabulary for metrics and dataset quality measures.",
    "exif": "W3C EXIF vocabulary for camera, orientation, and image metadata.",
    "fabio": "FRBR-aligned ontology for scholarly works and publication types.",
    "faldo": "Ontology for biological sequence locations, regions, and strands.",
    "fhir": "HL7 healthcare model for clinical and administrative interoperability.",
    "fibo-fbc-fi-fi": "FIBO model for financial instruments and securities.",
    "fibo-fbc-pas-fpas": "FIBO model for financial products, services, and offerings.",
    "fibo-fnd-acc-ae": "FIBO accounting and equity foundations for ownership concepts.",
    "fibo-fnd-acc-cur": "FIBO model for monetary amounts, currencies, and quantities.",
    "fibo-fnd-pas-ps": (
        "FIBO foundations module for products and services; used in "
        "teleology/procedure-adjacent alignment notes."
    ),
    "fibo-iso4217": "FIBO vocabulary for ISO 4217 currency codes.",
    "foaf": "Friend-of-a-Friend vocabulary for people, agents, and social links.",
    "forgefed": "Federated forge vocabulary for repos, issues, and merge requests.",
    "frbr": "Bibliographic model for work, expression, manifestation, and item.",
    "gedcom": "Legacy W3C RDF vocabulary for genealogical records and families.",
    "gedcomx": "GEDCOM X model for genealogical persons, facts, sources, and evidence.",
    "geo": "OGC GeoSPARQL vocabulary for features, geometries, and spatial relations.",
    "geonames": "Gazetteer vocabulary for place identifiers and feature codes.",
    "glottolog": "Language-family and languoid catalog for language identifiers.",
    "gsso": "Ontology for gender, sex, and sexual-orientation concepts.",
    "gtfs": "Transit feed vocabulary for routes, stops, trips, and schedules.",
    "gvp": "Getty Vocabulary Program ontology for authority-vocabulary structure.",
    "homosaurus": "Controlled vocabulary for LGBTQ+ concepts and community terms.",
    "ifc": "ifcOWL representation of Industry Foundation Classes building models.",
    "iiif": "IIIF vocabulary for canvases, manifests, and presentation metadata.",
    "iptc": "IPTC NewsML-G2 vocabulary for news, subjects, rights, and metadata.",
    "ivoa": "International Virtual Observatory vocabulary for astronomy concepts.",
    "jams": (
        "JAMS annotation vocabulary for music analysis observations and segment labels."
    ),
    "lexvo": "Language identifier vocabulary used for language and script references.",
    "lime": "Linguistic Metadata vocabulary for lexicons and lexical datasets.",
    "loinc": "Clinical laboratory and observation code system for measurements.",
    "lrmoo": "Object-oriented library model for bibliography and cultural heritage.",
    "lvont": "Lexvo ontology for languages, scripts, codes, and identifiers.",
    "ma": "W3C Media Ontology for media resources, fragments, and metadata.",
    "mf": "OGC Moving Features vocabulary for trajectories and moving objects.",
    "mo": "Music Ontology for artists, recordings, performances, releases, and tracks.",
    "mbz": (
        "MusicBrainz open music encyclopedia; the genre namespace is bridged "
        "by reference for music-genre alignment."
    ),
    "discogs": (
        "Discogs music database; the style namespace is bridged by reference "
        "for music-genre alignment."
    ),
    "moat": "Meaning Of A Tag vocabulary for tag semantics and tag-to-concept links.",
    "nmo": "Nepomuk Message Ontology for email messages, folders, and recipients.",
    "obscore": "IVOA Observation Core model for astronomical observation datasets.",
    "odrl": "W3C Open Digital Rights Language for policies, parties, and duties.",
    "ontolex": "OntoLex-Lemon model for lexical entries, senses, forms, and lexicons.",
    "org": "W3C Organization ontology for memberships, posts, sites, and orgs.",
    "pon": "Polifonia Ontology Network for music heritage, works, and annotations.",
    "premis": "Preservation metadata for digital objects, events, rights, and fixity.",
    "prov": (
        "W3C provenance ontology for activities, entities, agents, derivation, "
        "attribution, and plans."
    ),
    "qb": "W3C RDF Data Cube vocabulary for statistical observations and datasets.",
    "qudt": "Quantities, Units, Dimensions and Types vocabulary for measurement.",
    "rel": "Relationship vocabulary for interpersonal and kinship predicates.",
    "rstmt": "RightsStatements.org statements for rights and reuse status.",
    "schema": "Schema.org web schema for broadly consumed structured data projections.",
    "sioc": "Vocabulary for posts, forums, containers, accounts, and communities.",
    "skos": "W3C vocabulary for concept schemes, labels, and mapping relations.",
    "snomed": "Clinical terminology for health, anatomy, procedures, and findings.",
    "so": "Sequence Ontology vocabulary for genomic features and annotations.",
    "spdx": "SPDX RDF vocabulary for package, file, copyright, and license data.",
    "spdxlic": "SPDX License List identifiers for licenses and exceptions.",
    "tags": "Tag ontology for tagging acts, tagged resources, tags, and tag metadata.",
    "tgn": "Getty place authority vocabulary for historical and named places.",
    "time": "W3C OWL-Time ontology for instants, intervals, and durations.",
    "vcard": "W3C vCard RDF vocabulary for contacts, addresses, names, and phones.",
    "wgs84": "W3C WGS84 vocabulary for latitude, longitude, and altitude.",
    "wikidata": "Wikidata entity and property space used as a concept-link hub.",
    "wot": "Web of Trust vocabulary for keys, signatures, and certificates.",
    "pplan": (
        "Plan pattern vocabulary for plans, steps, variables, inputs, outputs, "
        "and step precedence."
    ),
    "iao": (
        "OBO Information Artifact Ontology; used as a BFO-world counterpart "
        "for objectives, specifications, and information artifacts."
    ),
    "sumo": (
        "Suggested Upper Merged Ontology; used as a broad external target for "
        "goals, desires, intentions, and normative concepts."
    ),
    "cco": (
        "Common Core Ontologies; used for objective/specification-style "
        "alignment targets."
    ),
    "crm": (
        "CIDOC CRM cultural-heritage ontology; used for events, observation, "
        "attribution, inscription, and relationship alignments."
    ),
    "crminf": (
        "CIDOC CRM extension for argumentation, inference, belief, and "
        "standpoint-like claims."
    ),
    "conceptnet": (
        "Commonsense knowledge graph used as a linkage target for lexical and "
        "conceptual opposition."
    ),
    "atomic": (
        "Commonsense if-then event and social-reasoning knowledge graph often "
        "paired with ConceptNet."
    ),
}
_EXTERNAL_TARGET_EXTRAS: dict[str, tuple[str, str, str, str]] = {
    "pplan": ("P-Plan", PREFIXES["pplan"], "Unknown", "schema"),
    "iao": ("IAO", PREFIXES["iao"], "CC-BY-4.0", "schema"),
    "sumo": ("SUMO", "https://www.ontologyportal.org/SUMO.owl", "GPL", "upper"),
    "cco": (
        "Common Core Ontologies",
        "https://www.commoncoreontologies.org/",
        "BSD-3-Clause",
        "upper",
    ),
    "crm": ("CIDOC CRM", PREFIXES["crm"], "CC-BY-4.0", "schema"),
    "crminf": ("CRMinf", PREFIXES["crminf"], "CC-BY-4.0", "schema"),
    "conceptnet": (
        "ConceptNet",
        "https://conceptnet.io/",
        "CC-BY-SA-4.0",
        "concept_scheme",
    ),
    "atomic": (
        "ATOMIC",
        "https://allenai.org/data/atomic",
        "CC-BY-4.0",
        "concept_scheme",
    ),
}
_EXTERNAL_TARGET_ALIASES: dict[str, str] = {
    "gUFO": "gufo",
    "PROV": "prov",
    "PROV-O": "prov",
    "P-Plan": "pplan",
    "FIBO": "fibo-fnd-pas-ps",
    "FIBO FND-GAO": "fibo-fnd-pas-ps",
    "IAO": "iao",
    "SUMO": "sumo",
    "CCO": "cco",
    "CRM": "crm",
    "CIDOC CRM": "crm",
    "CIDOC-CRM": "crm",
    "CRMinf": "crminf",
    "ConceptNet": "conceptnet",
    "ATOMIC": "atomic",
    "ConceptNet/ATOMIC": "conceptnet",
}
_EXTERNAL_TARGET_RE = re.compile(
    r"(?<![\w:/])("
    + "|".join(
        re.escape(alias)
        for alias in sorted(_EXTERNAL_TARGET_ALIASES, key=len, reverse=True)
    )
    + r")(?![\w/-])"
)
_TICKET_LEADIN_RE = re.compile(
    r"\b(?:Added|Issue|Issues|PR|Pull request)\s+"
    r"\(?#\d+(?:\s*/\s*#\d+)*"
    r"(?:\s*(?:phase|follow-through|backlog|comment)\s*[A-Za-z0-9.-]*)?"
    r"\)?:\s*",
    re.IGNORECASE,
)
_TICKET_PAREN_RE = re.compile(
    r"\(\s*(?:(?:issue|issues|pr|pull request)\s*)?"
    r"#\d+(?:\s*/\s*#\d+)*"
    r"(?:\s*(?:phase|follow-through|backlog|comment)\s*[A-Za-z0-9.-]*)?"
    r"\s*\)",
    re.IGNORECASE,
)
_TICKET_REF_RE = re.compile(
    r"\b(?:issue|issues|pr|pull request)\s+#\d+(?:\s*/\s*#\d+)*"
    r"|#\d+(?:\s*/\s*#\d+)*"
    r"(?:\s*(?:phase|follow-through|backlog|comment)\s*[A-Za-z0-9.-]*)?",
    re.IGNORECASE,
)
_MAX_TERM_LINK_ROWS = 80
_MAX_SLICE_LINK_ROWS = 24

_CATEGORY_DIRS = {
    "class": "classes",
    "property": "properties",
    "individual": "individuals",
    "datatype": "datatypes",
}

_CATEGORY_LABELS = {
    "class": "Classes",
    "property": "Properties",
    "individual": "Individuals",
    "datatype": "Datatypes",
}

_DEFAULT_CONCERNS = {
    NAMESPACE + "concernStatementMetadata": (
        "Statement Metadata",
        "Native RDF 1.2 reifiers, confidence, provenance, and temporal scope.",
    ),
    NAMESPACE + "concernStandpoints": (
        "Standpoints",
        "Perspective-indexed claims without primary, preferred, or winner slots.",
    ),
    NAMESPACE + "concernDisclosure": (
        "Disclosure And Suppression",
        "Projection-time withholding, coarsening, sensitivity, and display control.",
    ),
    NAMESPACE + "concernFrames": (
        "Frames And Units",
        "Reference frames, units, determinacy, and frame-relative values.",
    ),
    NAMESPACE + "concernProvenanceEvidence": (
        "Provenance And Evidence",
        "Attribution, derivation, confidence, evidence, and source lineage.",
    ),
    NAMESPACE + "concernIdentifiersCoreference": (
        "Identifiers And Coreference",
        "External authority links, identifier records, and reference-only bridging.",
    ),
    NAMESPACE + "concernGTSPackaging": (
        "GTS Packaging",
        "The single-file Graph Transport Substrate and bundled docs distribution.",
    ),
}

_BOX_ROLE_CURIES = [
    "gmeow:boxABox",
    "gmeow:boxTBox",
    "gmeow:boxRBox",
    "gmeow:boxCBox",
]

_FOUR_BOXES_SOURCE = PROJECT_ROOT / "docs" / "four-boxes.md"
_FOUR_BOXES_TITLE = "ABox, TBox, RBox, CBox in GMEOW"


@dataclass(slots=True)
class DocTerm:
    """A documented GMEOW term with display metadata."""

    category: str
    iri: str
    curie: str
    label: str
    definition: str
    owner: str = ""
    filename: str = ""
    parents: list[str] = field(default_factory=list)
    prop_kind: str = ""
    domain: str = ""
    range: str = ""
    functional: bool = False
    sub_property_of: list[str] = field(default_factory=list)
    types: list[str] = field(default_factory=list)
    alignments: list[str] = field(default_factory=list)
    comment: str = ""
    scope_notes: list[str] = field(default_factory=list)
    examples: list[str] = field(default_factory=list)
    use_when: list[str] = field(default_factory=list)
    avoid_when: list[str] = field(default_factory=list)
    how_to_use: list[str] = field(default_factory=list)
    use_for_consumer: list[str] = field(default_factory=list)
    avoid_for_consumer: list[str] = field(default_factory=list)
    concerns: list[str] = field(default_factory=list)
    box_roles: list[str] = field(default_factory=list)
    linkages: list[DocLinkage] = field(default_factory=list)


@dataclass(slots=True)
class DocLinkage:
    """A canonical mapping DSL linkage rendered into the documentation."""

    kind: str
    source: str
    predicate: str
    target: str
    cell: str
    role: str = ""
    profile: str = ""
    relation: str = ""
    confidence: str = ""
    mapping_set: str = ""
    comment: str = ""
    lossy_drops: list[str] = field(default_factory=list)
    transform: str = ""
    emits_sssom: bool = False


@dataclass(slots=True)
class DocMappingSet:
    """Summary of one SSSOM mapping set declared in the mapping DSL."""

    file: str
    set_id: str
    license: str
    comment: str
    equivalence_count: int


@dataclass(slots=True)
class DocConcern:
    """A cross-cutting documentation concern declared in the ontology."""

    iri: str
    curie: str
    label: str
    definition: str
    filename: str
    terms: list[DocTerm] = field(default_factory=list)
    slices: list[Slice] = field(default_factory=list)


@dataclass(slots=True)
class DocExample:
    """A slice-local Turtle example discovered from canonical sources."""

    path: Path
    title: str
    slice_name: str
    terms: list[str] = field(default_factory=list)
    external_prefixes: list[str] = field(default_factory=list)
    text: str = ""


@dataclass(slots=True)
class DocDesignDoc:
    """A slice-local design note discovered from ``design/*.md``."""

    path: Path
    title: str
    slice_name: str
    rel: Path
    text: str


@dataclass(slots=True)
class DocRecipe:
    """A task-oriented adoption page backed by one or more examples."""

    slug: str
    title: str
    goal: str
    example_paths: list[Path]
    term_curies: list[str]
    follow_pages: list[Path] = field(default_factory=list)


@dataclass(slots=True)
class DocLearningPath:
    """A curated adoption journey across recipes, examples, and terms."""

    slug: str
    title: str
    audience: str
    goal: str
    recipe_slugs: list[str]
    example_paths: list[Path]
    term_curies: list[str]
    adoption_targets: list[str] = field(default_factory=list)


@dataclass(slots=True)
class Page:
    """One generated Markdown page and its HTML title."""

    rel: Path
    title: str
    markdown: str


@dataclass(slots=True)
class DocsModel:
    """The folded data used to render documentation."""

    view: FoldView
    title: str
    version: str
    description: str
    terms: list[DocTerm]
    terms_by_curie: dict[str, DocTerm]
    linkages: list[DocLinkage]
    mapping_sets: list[DocMappingSet]
    concerns: list[DocConcern]
    slices: dict[str, Slice]
    examples: list[DocExample]
    examples_by_slice: dict[str, list[DocExample]]
    design_docs_by_slice: dict[str, list[DocDesignDoc]]
    recipes: list[DocRecipe]
    learning_paths: list[DocLearningPath]


def _safe_filename(value: str) -> str:
    """Return a stable filename stem for a CURIE or profile token."""
    return (
        value.replace(":", "-")
        .replace("/", "-")
        .replace("#", "-")
        .replace(" ", "-")
        .replace("_", "-")
    )


def _local_name(iri: str) -> str:
    """Return the local name of an IRI."""
    if iri.startswith(NAMESPACE):
        return iri[len(NAMESPACE) :]
    return iri.rsplit("/", 1)[-1].rsplit("#", 1)[-1]


def _term_alias_path(term: DocTerm, exact_aliases: set[str]) -> Path:
    """Return a case-safe HTML alias path for a term."""
    local = _local_name(term.iri)
    if term.curie in exact_aliases:
        slug = local
    else:
        digest = hashlib.sha1(term.iri.encode("utf-8")).hexdigest()[:8]
        slug = f"{_CATEGORY_DIRS[term.category]}-{local.casefold()}-{digest}"
    return Path("terms") / slug / "index.html"


def _exact_term_aliases(terms: Iterable[DocTerm]) -> set[str]:
    """Return terms that may keep exact local-name aliases without case clashes."""
    by_local_casefold: dict[str, list[DocTerm]] = defaultdict(list)
    for term in terms:
        by_local_casefold[_local_name(term.iri).casefold()].append(term)

    exact: set[str] = set()
    for group in by_local_casefold.values():
        exact.add(sorted(group, key=lambda t: t.curie)[0].curie)
    return exact


def _term_md_rel(term: DocTerm) -> Path:
    """Return the Markdown path for a term page."""
    return Path("reference") / _CATEGORY_DIRS[term.category] / term.filename


def _box_role_slug(role_curie: str) -> str:
    """Return a URL slug for a GMEOW graph-box role."""
    return {
        "gmeow:boxABox": "abox",
        "gmeow:boxTBox": "tbox",
        "gmeow:boxRBox": "rbox",
        "gmeow:boxCBox": "cbox",
    }.get(role_curie, role_curie.split(":", 1)[-1].lower())


def _box_role_link(role_curie: str, model: DocsModel, from_rel: Path) -> str:
    """Return a Markdown link to a box-role landing page."""
    term = model.terms_by_curie.get(role_curie)
    label = term.label if term is not None else role_curie
    slug = _box_role_slug(role_curie)
    target = Path("reference") / "boxes" / f"{slug}.md"
    rel = posixpath.relpath(target.as_posix(), start=from_rel.parent.as_posix() or ".")
    return f"[{label}]({rel})"


def _site_path_for_md(site: Path, rel: Path) -> Path:
    """Return the target HTML path for a Markdown page."""
    if rel.name == "index.md":
        return site / rel.parent / "index.html"
    return site / rel.with_suffix("") / "index.html"


def _site_prefix(site: Path, target: Path) -> str:
    """Return a relative prefix from a site page to the site root."""
    rel_parent = target.parent.relative_to(site)
    depth = 0 if rel_parent == Path() else len(rel_parent.parts)
    return "../" * depth


def _markdown_link(label: str, rel: Path) -> str:
    """Return a Markdown link to a generated page."""
    return f"[{label}]({rel.as_posix()})"


def _site_rel_for_md(rel: Path) -> Path:
    """Return a site-relative HTML path for a Markdown source path."""
    if rel.name == "index.md":
        return rel.parent / "index.html"
    return rel.with_suffix("") / "index.html"


def _split_url_suffix(value: str) -> tuple[str, str]:
    """Split a relative URL into path and query/fragment suffix."""
    marks = [idx for idx in (value.find("?"), value.find("#")) if idx >= 0]
    if not marks:
        return value, ""
    first = min(marks)
    return value[:first], value[first:]


def _is_rewriteable_url(value: str) -> bool:
    """Return whether an HTML href/src should be made site-relative."""
    return not (
        value.startswith(("#", "/", "//"))
        or re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", value) is not None
    )


def _clean_directory_index(value: str) -> str:
    """Turn a relative index.html URL into its directory form."""
    if value == "index.html":
        return "./"
    if value.endswith("/index.html"):
        return value[: -len("index.html")]
    return value


def _rewrite_site_paths(body: str, page_rel: Path, target: Path, site: Path) -> str:
    """Rewrite Markdown-relative href/src values for directory-index HTML."""
    source_dir = page_rel.parent.as_posix()
    current_dir = target.parent.relative_to(site).as_posix()
    if current_dir == ".":
        current_dir = "."

    def repl(match: re.Match[str]) -> str:
        attr, raw = match.groups()
        if not _is_rewriteable_url(raw):
            return match.group(0)
        path_part, suffix = _split_url_suffix(raw)
        if not path_part:
            return match.group(0)

        source_path = Path(posixpath.normpath(f"{source_dir}/{path_part}"))
        if source_path.name.endswith(".md"):
            site_path = _site_rel_for_md(source_path)
            rel = posixpath.relpath(site_path.as_posix(), start=current_dir)
            rel = _clean_directory_index(rel)
        else:
            rel = posixpath.relpath(source_path.as_posix(), start=current_dir)
        if not rel.startswith("."):
            rel = f"./{rel}"
        return f'{attr}="{rel}{suffix}"'

    return re.sub(r'\b(href|src)="([^"]+)"', repl, body)


def _escape_md_cell(value: str) -> str:
    """Escape a value for a Markdown table cell."""
    return value.replace("|", "\\|").replace("\n", "<br>")


def _short_text(value: str, *, limit: int = 220) -> str:
    """Return a single-line summary suitable for dense tables."""
    text = " ".join(value.split())
    if len(text) <= limit:
        return text
    return text[: limit - 1].rstrip() + "..."


def _public_markdown_text(
    text: str, tag_map: dict[str, str], *, model: DocsModel, from_rel: Path
) -> str:
    """Replace internal language tags before writing public docs artifacts."""

    def repl(match: re.Match[str]) -> str:
        if match.group(0).endswith("*"):
            return "private-use-language-tag"
        return tag_map.get(match.group(0).lower(), "und")

    cleaned = _strip_ticket_references(_INTERNAL_TAG_TEXT_RE.sub(repl, text))
    return _link_public_identifiers(cleaned, model=model, from_rel=from_rel)


def _public_identifier_link(
    value: str, *, model: DocsModel, from_rel: Path, code_label: bool = False
) -> str | None:
    """Return a Markdown link for a public CURIE or Wikidata identifier."""
    if value.startswith("gmeow:slices/"):
        slice_name = value.rsplit("/", 1)[-1]
        slice_entry = next(
            (s for s in model.slices.values() if s.name == slice_name),
            None,
        )
        if slice_entry is None:
            return None
        label = f"`{value}`" if code_label else value
        rel = posixpath.relpath(
            (Path("slices") / f"{slice_entry.name}.md").as_posix(),
            start=from_rel.parent.as_posix() or ".",
        )
        return f"[{label}]({rel})"
    if value.startswith("gmeow:"):
        term = model.terms_by_curie.get(value)
        if term is None:
            return None
        return _curie_link_from(value, model, from_rel)
    if value.startswith("gufo:"):
        local = value.split(":", 1)[1]
        label = f"`{value}`" if code_label else value
        if local in _GUFO_TERM_HELP:
            rel = _external_terms_rel(from_rel, f"gufo-{local.lower()}")
            return f"[{label}]({rel})"
        return f"[{label}]({PREFIXES['gufo']}{local})"
    principle_match = re.fullmatch(r"Principle\s+([1-9]\d*)", value)
    if principle_match is not None:
        number = principle_match.group(1)
        label = f"`{value}`" if code_label else value
        return f"[{label}]({_CONSTITUTION_URL}#principle-{number})"
    if value.startswith("wd:Q"):
        qid = value.split(":", 1)[1]
        label = f"`{value}`" if code_label else value
        return f"[{label}](https://www.wikidata.org/wiki/{qid})"
    if value.startswith("wd:P"):
        pid = value.split(":", 1)[1]
        label = f"`{value}`" if code_label else value
        return f"[{label}](https://www.wikidata.org/wiki/Property:{pid})"
    if value.startswith("wdt:P"):
        pid = value.split(":", 1)[1]
        label = f"`{value}`" if code_label else value
        return f"[{label}](https://www.wikidata.org/wiki/Property:{pid})"
    if ":" in value:
        prefix, local = value.split(":", 1)
        namespace = PREFIXES.get(prefix)
        if namespace is not None:
            label = f"`{value}`" if code_label else value
            return f"[{label}]({namespace}{local})"
    if re.fullmatch(r"Q[1-9]\d+", value):
        label = f"`{value}`" if code_label else value
        return f"[{label}](https://www.wikidata.org/wiki/{value})"
    return None


def _external_terms_rel(from_rel: Path, anchor: str) -> str:
    """Return a relative link to the generated external terms page."""
    rel = posixpath.relpath(
        (Path("external") / "terms.md").as_posix(),
        start=from_rel.parent.as_posix() or ".",
    )
    return f"{rel}#{anchor}"


def _external_ontologies_rel(from_rel: Path, anchor: str) -> str:
    """Return a relative link to the generated external ontology catalog."""
    rel = posixpath.relpath(
        (Path("external") / "ontologies.md").as_posix(),
        start=from_rel.parent.as_posix() or ".",
    )
    return f"{rel}#{anchor}"


def _bare_gmeow_link(value: str, *, model: DocsModel, from_rel: Path) -> str | None:
    """Link a bare local GMEOW name when it is identifier-shaped."""
    if value not in _BARE_LOWERCASE_TERMS and not any(ch.isupper() for ch in value):
        return None
    matches = [term for term in model.terms if term.curie.split(":", 1)[-1] == value]
    if len(matches) != 1:
        return None
    term = matches[0]
    rel = posixpath.relpath(
        _term_md_rel(term).as_posix(),
        start=from_rel.parent.as_posix() or ".",
    )
    return f"[`{value}`]({rel})"


def _external_target_link(value: str, *, from_rel: Path) -> str | None:
    """Link a prose external ontology name to the generated catalog."""
    if from_rel == Path("external") / "ontologies.md":
        return None
    key = _EXTERNAL_TARGET_ALIASES.get(value)
    if key is None:
        return None
    return (
        f"[{value}]({_external_ontologies_rel(from_rel, _external_target_anchor(key))})"
    )


def _external_target_anchor(key: str) -> str:
    """Return the external ontology catalog anchor for a target key."""
    return f"target-{_safe_filename(key).lower()}"


def _link_public_identifiers(text: str, *, model: DocsModel, from_rel: Path) -> str:
    """Link public ontology and Wikidata identifiers in prose and tables."""
    lines: list[str] = []
    in_fence = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            lines.append(line)
            continue
        if in_fence:
            lines.append(line)
            continue
        lines.append(
            _link_public_identifiers_line(line, model=model, from_rel=from_rel)
        )
    return "\n".join(lines)


def _link_public_identifiers_line(
    line: str, *, model: DocsModel, from_rel: Path
) -> str:
    """Link identifiers in one Markdown line without nesting existing links."""
    stripped = line.lstrip()
    if stripped.startswith("<"):
        return line
    allow_bare = not stripped.startswith(("#", "|"))
    out: list[str] = []
    pos = 0
    while pos < len(line):
        if line[pos] == "[":
            close_label = line.find("]", pos + 1)
            if (
                close_label != -1
                and close_label + 1 < len(line)
                and line[close_label + 1] == "("
            ):
                close_dest = line.find(")", close_label + 2)
                if close_dest != -1:
                    out.append(line[pos : close_dest + 1])
                    pos = close_dest + 1
                    continue
            out.append(line[pos])
            pos += 1
            continue
        if line[pos] == "`":
            close_code = line.find("`", pos + 1)
            if close_code != -1:
                code = line[pos + 1 : close_code]
                linked = _public_identifier_link(
                    code, model=model, from_rel=from_rel, code_label=True
                )
                if linked is not None:
                    out.append(linked)
                else:
                    processed = _link_public_segment(
                        code,
                        model=model,
                        from_rel=from_rel,
                        code_label=True,
                        allow_bare=True,
                    )
                    out.append(processed if processed != code else f"`{code}`")
                pos = close_code + 1
                continue
            out.append(line[pos])
            pos += 1
            continue

        next_specials = [
            idx for idx in (line.find("[", pos), line.find("`", pos)) if idx != -1
        ]
        end = min(next_specials) if next_specials else len(line)
        segment = line[pos:end]

        out.append(
            _link_public_segment(
                segment,
                model=model,
                from_rel=from_rel,
                allow_bare=allow_bare,
            )
        )
        pos = end
    return "".join(out)


def _link_public_segment(
    segment: str,
    *,
    model: DocsModel,
    from_rel: Path,
    code_label: bool = False,
    allow_bare: bool = True,
) -> str:
    """Link identifiers and external target names in a plain Markdown segment."""

    def repo_path_repl(match: re.Match[str]) -> str:
        path = match.group(1)
        return f"[`{path}`]({_REPO_BLOB_URL}{path})"

    def external_repl(match: re.Match[str]) -> str:
        return _external_target_link(match.group(1), from_rel=from_rel) or match.group(
            1
        )

    def bare_repl(match: re.Match[str]) -> str:
        value = match.group(1)
        bare = _bare_gmeow_link(value, model=model, from_rel=from_rel)
        return bare if bare is not None else value

    def identifier_repl(match: re.Match[str]) -> str:
        linked = _public_identifier_link(
            match.group(0), model=model, from_rel=from_rel, code_label=code_label
        )
        return linked if linked is not None else match.group(0)

    linked = segment
    if allow_bare:
        linked = _BARE_IDENTIFIER_RE.sub(bare_repl, linked)
    linked = _EXTERNAL_TARGET_RE.sub(external_repl, linked)
    linked = _REPO_MARKDOWN_PATH_RE.sub(repo_path_repl, linked)
    return _LINKABLE_IDENTIFIER_RE.sub(identifier_repl, linked)


def _strip_ticket_references(text: str) -> str:
    """Remove internal tracker references from public documentation prose."""
    text = _TICKET_LEADIN_RE.sub("", text)
    text = _TICKET_PAREN_RE.sub("", text)
    text = _TICKET_REF_RE.sub("", text)
    lines: list[str] = []
    in_fence = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            lines.append(line.rstrip())
            continue
        if in_fence:
            lines.append(line.rstrip())
            continue

        trailing_break = "  " if line.endswith("  ") else ""
        body = line[:-2] if trailing_break else line
        body = re.sub(r"[ \t]+([,.;:])", r"\1", body)
        body = re.sub(r",\s*\)", ")", body)
        body = re.sub(r"\(\s*,\s*", "(", body)
        body = re.sub(r"\(\s*\)", "", body)
        body = re.sub(r"\s+([)\]])", r"\1", body)
        body = re.sub(r"([(])\s+", r"\1", body)
        body = re.sub(r"\s+—", " —", body)
        body = re.sub(r"—\s*([,.;:])", r"\1", body)
        body = re.sub(r"[ \t]{2,}", " ", body)
        lines.append(body.rstrip() + trailing_break)
    return "\n".join(lines)


def _public_values(view: FoldView, s_tid: int, p_iri: str) -> list[str]:
    """Return public text values for a predicate on a subject."""
    values: list[str] = []
    for obj in view.objects(s_tid, p_iri):
        if view.is_literal(obj):
            values.append(view.lex(obj))
        elif view.is_iri(obj):
            values.append(curie(view.lex(obj)))
        else:
            values.append(view.nq_token(obj))
    return sorted(set(values))


def _public_curies(view: FoldView, s_tid: int, p_iri: str) -> list[str]:
    """Return public CURIE values for an IRI-valued predicate on a subject."""
    values = [
        curie(view.lex(obj)) for obj in view.objects(s_tid, p_iri) if view.is_iri(obj)
    ]
    return sorted(set(values))


def _owner(view: FoldView, tid: int) -> str:
    """Return the CURIE of the term owner, when present."""
    owner_tid = view.value(tid, _RDFS_IS_DEFINED_BY)
    if owner_tid is None or not view.is_iri(owner_tid):
        return ""
    return curie(view.lex(owner_tid))


def _alignments(view: FoldView, tid: int) -> list[str]:
    """Return compact alignment strings for a term."""
    out: list[str] = []
    for p_tid, o_tid in view.predicate_objects(tid, scope=GTS_GRAPH_ALIGNMENTS):
        pred = curie(view.lex(p_tid))
        obj = curie(view.lex(o_tid))
        out.append(f"{pred}={obj}")
    return sorted(out)


def _gmeow_curie(value: object | None) -> str | None:
    """Return a GMEOW CURIE for an IRI-like value, else ``None``."""
    if value is None:
        return None
    text = str(value)
    if not text.startswith(NAMESPACE):
        return None
    return curie(text)


def _confidence(value: float | None) -> str:
    """Render a confidence value compactly."""
    if value is None:
        return ""
    return f"{value:.3f}".rstrip("0").rstrip(".")


def _walk_atoms(items: Iterable[Atom | OptionalGroup]) -> Iterable[Atom]:
    """Yield atoms from a possibly nested mapping pattern."""
    for item in items:
        if isinstance(item, OptionalGroup):
            yield from _walk_atoms(item.items)
        else:
            yield item


def _source_terms_from_atom(atom: Atom) -> set[str]:
    """Return GMEOW CURIEs mentioned in a pattern atom."""
    terms: set[str] = set()
    for value in (atom.predicate, atom.object_value, *atom.path_alts):
        term = _gmeow_curie(value)
        if term is not None:
            terms.add(term)
    if atom.path:
        terms.update(_GMEOW_CURIE_RE.findall(atom.path))
    return terms


def _projection_source_terms(
    projection: ProjectionCell, binding: ProfileBinding
) -> set[str]:
    """Return GMEOW CURIEs used as the source side of a projection mapping."""
    pattern = projection.pattern
    terms: set[str] = set()
    edoal_source = _gmeow_curie(pattern.edoal_source)
    if edoal_source is not None:
        terms.add(edoal_source)
    for atom in _walk_atoms(pattern.atoms):
        terms.update(_source_terms_from_atom(atom))
    for atom in (*pattern.suppress_when, *pattern.project_when, *pattern.exclude_when):
        terms.update(_source_terms_from_atom(atom))
    for entry in binding.value_class_map:
        source_value = _gmeow_curie(entry.when_value)
        if source_value is not None:
            terms.add(source_value)
    return terms


def _binding_targets(binding: ProfileBinding) -> list[str]:
    """Return compact target terms for a projection binding."""
    targets: list[str] = []
    for value in (binding.to_predicate, binding.to_class, binding.edoal_target):
        if value is not None:
            targets.append(curie(str(value)))
    for entry in binding.value_class_map:
        targets.append(curie(str(entry.to_class)))
    for atom in binding.template_atoms:
        for value in (atom.predicate, atom.object_value):
            if value is not None:
                targets.append(curie(str(value)))
    return sorted(dict.fromkeys(targets)) or ["structural output"]


def _target_prefixes(targets: Iterable[str]) -> list[str]:
    """Return external CURIE prefixes from target strings."""
    prefixes: set[str] = set()
    for target in targets:
        for part in target.split(", "):
            if ":" not in part:
                continue
            prefix = part.split(":", 1)[0]
            if prefix != "gmeow":
                prefixes.add(prefix)
    return sorted(prefixes)


_CURIE_TOKEN_RE = re.compile(r"\b[A-Za-z][A-Za-z0-9_-]*:[A-Za-z][A-Za-z0-9_.-]*")


def _title_from_stem(stem: str) -> str:
    """Return a readable title from a file stem."""
    return " ".join(part.capitalize() for part in stem.replace("_", "-").split("-"))


def _collect_examples(
    slices: dict[str, Slice], terms_by_curie: dict[str, DocTerm]
) -> list[DocExample]:
    """Discover slice-local Turtle examples and the terms they demonstrate."""
    examples: list[DocExample] = []
    for slice_entry in sorted(slices.values(), key=lambda s: s.name):
        examples_dir = slice_entry.path / "examples"
        if not examples_dir.exists():
            continue
        for path in sorted(examples_dir.glob("*.ttl")):
            text = path.read_text(encoding="utf-8")
            curies = sorted(set(_CURIE_TOKEN_RE.findall(text)))
            examples.append(
                DocExample(
                    path=path.relative_to(PROJECT_ROOT),
                    title=_title_from_stem(path.stem),
                    slice_name=slice_entry.name,
                    terms=[value for value in curies if value in terms_by_curie],
                    external_prefixes=sorted(
                        {
                            value.split(":", 1)[0]
                            for value in curies
                            if value.split(":", 1)[0] not in {"gmeow", "rdf", "rdfs"}
                            and value.split(":", 1)[0] in PREFIXES
                        }
                    ),
                    text=text.rstrip(),
                )
            )
    return examples


def _collect_design_docs(slices: dict[str, Slice]) -> dict[str, list[DocDesignDoc]]:
    """Discover slice-local Markdown design notes."""
    grouped: dict[str, list[DocDesignDoc]] = defaultdict(list)
    for slice_entry in sorted(slices.values(), key=lambda s: s.name):
        design_dir = slice_entry.path / "design"
        if not design_dir.exists():
            continue
        for path in sorted(design_dir.glob("*.md")):
            rel_path = path.relative_to(PROJECT_ROOT)
            out_rel = Path("slices") / slice_entry.name / "design" / path.name
            grouped[slice_entry.name].append(
                DocDesignDoc(
                    path=rel_path,
                    title=_title_from_stem(path.stem),
                    slice_name=slice_entry.name,
                    rel=out_rel,
                    text=path.read_text(encoding="utf-8").rstrip(),
                )
            )
    return dict(sorted(grouped.items()))


def _default_recipes() -> list[DocRecipe]:
    """Return stable task-oriented recipes backed by canonical examples."""
    return [
        DocRecipe(
            slug="person-names-and-display",
            title="Model Person Names Without a Preferred-Name Slot",
            goal=(
                "Represent coexisting names, pronouns, aliases, usage contexts, "
                "and display suppression without asserting one global winner."
            ),
            example_paths=[Path("slices/core/names/examples/person-names.ttl")],
            term_curies=[
                "gmeow:Person",
                "gmeow:PersonName",
                "gmeow:NameUsage",
                "gmeow:displayable",
            ],
            follow_pages=[Path("slices/names.md"), Path("slices/standpoint.md")],
        ),
        DocRecipe(
            slug="contested-or-attributed-facts",
            title="Model Contested or Attributed Facts",
            goal=(
                "Keep incompatible claims side by side by recording standpoint, "
                "vantage, modality, and evidence on the claim rather than on a "
                "single global fact slot."
            ),
            example_paths=[
                Path("slices/core/standpoint/examples/contested-authorship.ttl")
            ],
            term_curies=[
                "gmeow:StandpointClaim",
                "gmeow:vantage",
                "gmeow:claimModality",
                "gmeow:accordingTo",
            ],
            follow_pages=[Path("slices/standpoint.md"), Path("statements/index.md")],
        ),
        DocRecipe(
            slug="events-and-participants",
            title="Model Events and Participants",
            goal=(
                "Describe an event, participant roles, time, place, and source "
                "evidence without collapsing participation into a flat string."
            ),
            example_paths=[Path("slices/core/events/examples/wedding.ttl")],
            term_curies=[
                "gmeow:Event",
                "gmeow:Participation",
                "gmeow:TimeInterval",
                "gmeow:Place",
            ],
            follow_pages=[Path("slices/events.md"), Path("slices/temporal.md")],
        ),
        DocRecipe(
            slug="documents-and-schema-org",
            title="Publish Documents for Schema.org Consumers",
            goal=(
                "Start with native document and web-presence facts, then inspect "
                "which facts project to schema.org and where projection is lossy."
            ),
            example_paths=[Path("slices/core/documents/examples/web-presence.ttl")],
            term_curies=["gmeow:Document", "gmeow:webUrl", "gmeow:Entity"],
            follow_pages=[Path("slices/documents.md"), Path("linkages/index.md")],
        ),
        DocRecipe(
            slug="offline-gts-distribution",
            title="Describe Offline GTS Distribution",
            goal=(
                "Treat a GTS file as a first-class graph object with segments, "
                "profiles, chain heads, opaque frames, codecs, and compaction "
                "lineage."
            ),
            example_paths=[Path("slices/core/gts/examples/dist-package.ttl")],
            term_curies=[
                "gmeow:GTSDocument",
                "gmeow:GTSProfile",
                "gmeow:GTSSegment",
            ],
            follow_pages=[Path("slices/gts.md")],
        ),
        DocRecipe(
            slug="graph-rag-dataset-lineage",
            title="Model Graph-RAG Dataset Lineage",
            goal=(
                "Connect sources, chunks, extracted entities, evidence spans, "
                "and pipeline provenance so retrieval artifacts remain auditable."
            ),
            example_paths=[
                Path("slices/extensions/graphrag/examples/lillith-dataset.ttl"),
                Path("slices/extensions/graphrag/examples/lillith-pipeline.ttl"),
            ],
            term_curies=[
                "gmeow:Dataset",
                "gmeow:Chunk",
                "gmeow:ExtractedEntity",
                "gmeow:EvidenceSpan",
            ],
            follow_pages=[Path("slices/graphrag.md"), Path("slices/provenance.md")],
        ),
    ]


def _default_learning_paths() -> list[DocLearningPath]:
    """Return curated paths that sequence recipes and examples for adoption."""
    return [
        DocLearningPath(
            slug="model-a-person",
            title="Model a Person Without Flattening Identity",
            audience="Developers importing contact, profile, or biography data.",
            goal=(
                "Start with a person, add names and contact points, then keep "
                "authority links and display choices separate from identity."
            ),
            recipe_slugs=["person-names-and-display"],
            example_paths=[
                Path("slices/core/entities/examples/agent-sortals.ttl"),
                Path("slices/core/names/examples/person-names.ttl"),
                Path("slices/core/contacts/examples/contact-points.ttl"),
                Path("slices/core/coreference/examples/authority-links.ttl"),
            ],
            term_curies=[
                "gmeow:Person",
                "gmeow:PersonName",
                "gmeow:NameUsage",
                "gmeow:ContactPoint",
                "gmeow:displayable",
            ],
            adoption_targets=["schema", "foaf", "vcard", "wikidata"],
        ),
        DocLearningPath(
            slug="model-a-contested-claim",
            title="Model a Contested or Attributed Claim",
            audience="Developers handling evidence, provenance, or disagreement.",
            goal=(
                "Represent claims by vantage and evidence rather than overwriting "
                "facts into a single global truth slot."
            ),
            recipe_slugs=["contested-or-attributed-facts"],
            example_paths=[
                Path("slices/core/standpoint/examples/contested-authorship.ttl"),
                Path("slices/core/evidence/examples/notability-assessment.ttl"),
                Path("slices/core/provenance/examples/import-lineage.ttl"),
                Path("slices/core/attestation/examples/software-release.ttl"),
            ],
            term_curies=[
                "gmeow:StandpointClaim",
                "gmeow:Evidence",
                "gmeow:Attestation",
                "gmeow:accordingTo",
                "gmeow:confidence",
            ],
            adoption_targets=["prov", "crminf", "wikidata"],
        ),
        DocLearningPath(
            slug="publish-web-structured-data",
            title="Publish Web Structured Data",
            audience="Developers projecting native GMEOW into web-facing JSON-LD.",
            goal=(
                "Model documents, events, people, and organizations natively, "
                "then inspect which fields project cleanly to broad consumers."
            ),
            recipe_slugs=["documents-and-schema-org", "events-and-participants"],
            example_paths=[
                Path("slices/core/documents/examples/web-presence.ttl"),
                Path("slices/core/events/examples/wedding.ttl"),
                Path("slices/core/organization/examples/post-and-membership.ttl"),
                Path("slices/core/places/examples/located-place.ttl"),
            ],
            term_curies=[
                "gmeow:Document",
                "gmeow:Event",
                "gmeow:Participation",
                "gmeow:Organization",
                "gmeow:Place",
            ],
            adoption_targets=["schema", "prov", "geo", "org"],
        ),
        DocLearningPath(
            slug="ship-offline-gts-docs",
            title="Ship Offline GTS Documentation",
            audience="Developers distributing GMEOW snapshots or local docs.",
            goal=(
                "Treat the distribution file, embedded docs, profiles, segments, "
                "codecs, and lineage as first-class graph facts."
            ),
            recipe_slugs=["offline-gts-distribution"],
            example_paths=[
                Path("slices/core/gts/examples/dist-package.ttl"),
                Path("slices/core/provenance/examples/import-lineage.ttl"),
                Path("slices/core/rights/examples/licensed-dataset.ttl"),
            ],
            term_curies=[
                "gmeow:GTSDocument",
                "gmeow:GTSProfile",
                "gmeow:GTSSegment",
                "gmeow:usesTransformCodec",
            ],
            adoption_targets=["dcat", "void", "spdx"],
        ),
        DocLearningPath(
            slug="audit-ai-or-graph-rag",
            title="Audit AI and Graph-RAG Pipelines",
            audience="Developers recording extracted facts, chunks, and tools.",
            goal=(
                "Connect generated claims to source chunks, evidence spans, "
                "tools, model context, and provenance before consumers see them."
            ),
            recipe_slugs=["graph-rag-dataset-lineage"],
            example_paths=[
                Path("slices/core/ai/examples/grounded-claim.ttl"),
                Path("slices/extensions/graphrag/examples/lillith-dataset.ttl"),
                Path("slices/extensions/graphrag/examples/lillith-pipeline.ttl"),
                Path("slices/extensions/agentic/examples/agent-trajectory.ttl"),
            ],
            term_curies=[
                "gmeow:Dataset",
                "gmeow:Chunk",
                "gmeow:ExtractedEntity",
                "gmeow:EvidenceSpan",
                "gmeow:usedModel",
            ],
            adoption_targets=["prov", "dcat", "schema"],
        ),
    ]


def _link_sort_key(link: DocLinkage) -> tuple[str, str, str, str, str]:
    """Stable sort key for linkage rows."""
    return (link.kind, link.source, link.profile, link.target, link.cell)


def _collect_linkages() -> tuple[list[DocLinkage], list[DocMappingSet]]:
    """Collect adopter-facing linkage rows from the canonical mapping DSL."""
    dsl = load_dsl()
    linkages: list[DocLinkage] = []
    equivalence_counts: dict[str, int] = defaultdict(int)

    for eq in dsl.equivalences:
        equivalence_counts[eq.sssom_file] += 1
        subject = curie(str(eq.subject))
        target = curie(str(eq.obj))
        predicate = curie(str(eq.predicate))
        rows: list[tuple[str, str, str]] = []
        if str(eq.subject).startswith(NAMESPACE):
            rows.append((subject, target, "subject"))
        if str(eq.obj).startswith(NAMESPACE):
            rows.append((target, subject, "object"))
        for source, obj, role in rows:
            linkages.append(
                DocLinkage(
                    kind="equivalence",
                    source=source,
                    predicate=predicate,
                    target=obj,
                    cell=curie(str(eq.iri)),
                    role=role,
                    confidence=_confidence(eq.confidence),
                    mapping_set=eq.sssom_file,
                    comment=eq.comment,
                )
            )

    for projection in dsl.projections:
        for binding in projection.bindings:
            targets = ", ".join(_binding_targets(binding))
            source_terms = _projection_source_terms(projection, binding)
            for source in sorted(source_terms):
                linkages.append(
                    DocLinkage(
                        kind="projection",
                        source=source,
                        predicate="projects to",
                        target=targets,
                        cell=curie(str(projection.iri)),
                        profile=binding.profile,
                        relation=binding.relation,
                        confidence=_confidence(binding.confidence),
                        mapping_set=binding.sssom_file or "",
                        lossy_drops=sorted(binding.lossy_drops),
                        transform=curie(str(binding.transform))
                        if binding.transform is not None
                        else "",
                        emits_sssom=binding.emit_sssom,
                    )
                )

    mapping_sets: list[DocMappingSet] = []
    for file in sorted(set(dsl.mapping_sets) | set(equivalence_counts)):
        meta = dsl.mapping_sets.get(file)
        mapping_sets.append(
            DocMappingSet(
                file=file,
                set_id=meta.set_id if meta is not None else "",
                license=meta.license if meta is not None else "",
                comment=meta.comment if meta is not None else "",
                equivalence_count=equivalence_counts[file],
            )
        )
    return sorted(linkages, key=_link_sort_key), mapping_sets


def _datatype_terms(view: FoldView) -> list[Term]:
    """Collect GMEOW datatypes not covered by the flat export term model."""
    out: list[Term] = []
    for tid in view.subjects_by_type(_RDFS_DATATYPE):
        if not view.is_iri(tid) or not view.lex(tid).startswith(NAMESPACE):
            continue
        iri = view.lex(tid)
        out.append(
            Term(
                category="datatype",
                iri=iri,
                curie=curie(iri),
                label=view.public_text(tid, _RDFS_LABEL),
                definition=view.public_text(tid, _SKOS_DEFINITION),
                alignments=_alignments(view, tid),
            )
        )
    return sorted(out, key=lambda t: t.curie)


def _doc_term(view: FoldView, term: Term) -> DocTerm:
    """Convert an exporter term into a documentation term."""
    tid = view.tid_of_iri(term.iri)
    comment = ""
    scope_notes: list[str] = []
    examples: list[str] = []
    use_when: list[str] = []
    avoid_when: list[str] = []
    how_to_use: list[str] = []
    use_for_consumer: list[str] = []
    avoid_for_consumer: list[str] = []
    concerns: list[str] = []
    owner = ""
    if tid is not None:
        comment = view.public_text(tid, _RDFS_COMMENT)
        scope_notes = _public_values(view, tid, _SKOS_SCOPE_NOTE)
        examples = _public_values(view, tid, _SKOS_EXAMPLE)
        use_when = _public_values(view, tid, _USE_WHEN)
        avoid_when = _public_values(view, tid, _AVOID_WHEN)
        how_to_use = _public_values(view, tid, _HOW_TO_USE)
        use_for_consumer = _public_curies(view, tid, _USE_FOR_CONSUMER)
        avoid_for_consumer = _public_curies(view, tid, _AVOID_FOR_CONSUMER)
        concerns = [
            view.lex(obj)
            for obj in view.objects(tid, _DOCS_CONCERN)
            if view.is_iri(obj)
        ]
        box_roles = sorted(set(_public_curies(view, tid, _GRAPH_BOX_ROLE)))
        owner = _owner(view, tid)
    else:
        box_roles = []

    return DocTerm(
        category=term.category,
        iri=term.iri,
        curie=term.curie,
        label=term.label or term.curie,
        definition=term.definition,
        owner=owner,
        parents=list(term.parents),
        prop_kind=term.prop_kind,
        domain=term.domain,
        range=term.range,
        functional=term.functional,
        sub_property_of=list(term.sub_property_of),
        types=list(term.types),
        alignments=list(term.alignments),
        comment=comment,
        scope_notes=scope_notes,
        examples=examples,
        use_when=use_when,
        avoid_when=avoid_when,
        how_to_use=how_to_use,
        use_for_consumer=use_for_consumer,
        avoid_for_consumer=avoid_for_consumer,
        concerns=sorted(set(concerns)),
        box_roles=box_roles,
    )


def _resolve_filenames(terms: list[DocTerm]) -> None:
    """Assign stable, case-insensitive-safe filenames to term pages."""
    groups: dict[tuple[str, str], list[DocTerm]] = defaultdict(list)
    for term in terms:
        base = _safe_filename(term.curie)
        groups[(term.category, base.lower())].append(term)
    for members in groups.values():
        if len(members) == 1:
            members[0].filename = f"{_safe_filename(members[0].curie)}.md"
            continue
        for term in sorted(members, key=lambda t: t.curie):
            local = term.curie.split(":", 1)[-1]
            capitals = "-".join(str(i) for i, ch in enumerate(local) if ch.isupper())
            suffix = capitals or "lower"
            term.filename = f"{_safe_filename(term.curie)}-{suffix}.md"


def _concern_label(view: FoldView, iri: str) -> tuple[str, str]:
    """Return label and definition for a concern IRI."""
    tid = view.tid_of_iri(iri)
    default = _DEFAULT_CONCERNS.get(iri, (curie(iri), ""))
    if tid is None:
        return default
    label = view.public_text(tid, _RDFS_LABEL) or default[0]
    definition = view.public_text(tid, _SKOS_DEFINITION) or default[1]
    return label, definition


def _collect_concerns(
    view: FoldView, terms: list[DocTerm], slices: dict[str, Slice]
) -> list[DocConcern]:
    """Collect cross-cutting concerns from ontology metadata."""
    concern_iris = {
        view.lex(tid)
        for tid in view.subjects_by_type(_DOCS_CONCERN_CLASS)
        if view.is_iri(tid)
    }
    concern_iris.update(c for term in terms for c in term.concerns)
    concern_iris.update(_DEFAULT_CONCERNS)

    concerns: list[DocConcern] = []
    for iri in sorted(concern_iris):
        label, definition = _concern_label(view, iri)
        concern = DocConcern(
            iri=iri,
            curie=curie(iri),
            label=label,
            definition=definition,
            filename=f"{_safe_filename(curie(iri))}.md",
            terms=[term for term in terms if iri in term.concerns],
        )
        owner_iris = {
            term.owner.replace("gmeow:slices/", ONTOLOGY_IRI + "/slices/")
            for term in concern.terms
            if term.owner.startswith("gmeow:slices/")
        }
        concern.slices = sorted(
            [s for s in slices.values() if s.iri in owner_iris],
            key=lambda s: s.name,
        )
        concerns.append(concern)
    return concerns


def _load_model(gts_path: Path | None = None) -> DocsModel:
    """Load the folded docs model."""
    view = load_fold(gts_path or GTS_SNAPSHOT_FILE)
    title, version = fold_meta(view)
    onto_tid = view.tid_of_iri(ONTOLOGY_IRI)
    description = (
        view.public_text(onto_tid, _DCT_DESCRIPTION) if onto_tid is not None else ""
    )
    terms = [_doc_term(view, t) for t in [*collect_terms(view), *_datatype_terms(view)]]
    _resolve_filenames(terms)
    terms.sort(key=lambda t: (t.category, t.curie))
    terms_by_curie = {term.curie: term for term in terms}
    linkages, mapping_sets = _collect_linkages()
    for link in linkages:
        term = terms_by_curie.get(link.source)
        if term is not None:
            term.linkages.append(link)
    for term in terms:
        term.linkages.sort(key=_link_sort_key)
    slices = discover_slices()
    examples = _collect_examples(slices, terms_by_curie)
    design_docs_by_slice = _collect_design_docs(slices)
    examples_by_slice: dict[str, list[DocExample]] = defaultdict(list)
    for example in examples:
        examples_by_slice[example.slice_name].append(example)
    return DocsModel(
        view=view,
        title=title,
        version=version,
        description=description,
        terms=terms,
        terms_by_curie=terms_by_curie,
        linkages=linkages,
        mapping_sets=mapping_sets,
        concerns=_collect_concerns(view, terms, slices),
        slices=slices,
        examples=examples,
        examples_by_slice=dict(examples_by_slice),
        design_docs_by_slice=design_docs_by_slice,
        recipes=_default_recipes(),
        learning_paths=_default_learning_paths(),
    )


class _Writer:
    """Writes generated Markdown, HTML, CSS, and SVG files."""

    def __init__(self, outdir: Path, *, source_hash: str = "") -> None:
        self.root = outdir
        self.markdown = outdir / "markdown"
        self.site = outdir / "site"
        self.source_hash = source_hash
        if outdir.exists():
            shutil.rmtree(outdir)
        self.markdown.mkdir(parents=True)
        self.site.mkdir(parents=True)

    def banner(self, style: str = "html") -> str:
        """Return the generated-file banner for a comment style."""
        # Do not include the generator source hash in every docs file. The
        # generator registry still uses hashes for drift checks, but per-file
        # hash banners caused whole-site churn for one-line docs improvements.
        msg = "GENERATED by gmeow ontology-docs. DO NOT EDIT."
        if style == "hash":
            return f"# {msg}\n\n"
        return f"<!-- {msg} -->\n\n"

    def write_markdown(self, page: Page) -> None:
        """Write a Markdown page."""
        path = self.markdown / page.rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(self.banner() + page.markdown.rstrip() + "\n", encoding="utf-8")

    def write_site_page(self, page: Page) -> None:
        """Render and write a static HTML page from Markdown."""
        body = markdown_to_html(
            page.markdown,
            extensions=["tables", "fenced_code", "sane_lists"],
            output_format="html5",
        )
        target = _site_path_for_md(self.site, page.rel)
        body = _rewrite_site_paths(body, page.rel, target, self.site)
        prefix = _site_prefix(self.site, target)
        html_text = _html_shell(page.title, body, prefix)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(self.banner() + html_text, encoding="utf-8")

    def write_css(self) -> None:
        """Write the GMEOW theme override CSS asset."""
        css = """\
:root {
  color-scheme: only light;
  --bg: #ffffff;
  --accent-bg: #f5f8f7;
  --text: #1b1f23;
  --text-light: #58636f;
  --border: #c9d3d0;
  --accent: #0f766e;
  --accent-hover: #0b5f59;
  --accent-text: #ffffff;
  --code: #9a3412;
  --preformatted: #2f3a40;
  --marked: #fff4b8;
  --disabled: #eceff1;
  --standard-border-radius: 5px;
}

@media (prefers-color-scheme: dark) {
  :root {
    color-scheme: only light;
    --bg: #ffffff;
    --accent-bg: #f5f8f7;
    --text: #1b1f23;
    --text-light: #58636f;
    --border: #c9d3d0;
    --accent: #0f766e;
    --accent-hover: #0b5f59;
    --accent-text: #ffffff;
    --code: #9a3412;
    --preformatted: #2f3a40;
    --disabled: #eceff1;
  }
}

body {
  grid-template-columns: 1fr min(70rem, 92%) 1fr;
  font-size: 1.03rem;
}

header {
  padding-top: 1rem;
  padding-bottom: 1rem;
}

header h1 {
  font-size: 1.55rem;
}

nav a {
  font-weight: 650;
}

main {
  padding-top: 1.35rem;
}

h1, h2, h3 {
  letter-spacing: 0;
}

h1 {
  font-size: 2rem;
}

table {
  font-size: 0.94rem;
}

th, td {
  vertical-align: top;
}

code {
  white-space: normal;
}

pre code {
  white-space: pre;
}

summary {
  color: #9a3412;
}

.skip { position: absolute; left: -999px; }
.skip:focus { left: 16px; top: 16px; background: var(--accent-bg); padding: 8px; }
img, svg { max-width: 100%; height: auto; }
"""
        path = self.site / "assets" / "gmeow.css"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(self.banner("hash") + css, encoding="utf-8")

    def write_simple_css(self) -> None:
        """Write the vendored Simple.css asset."""
        source = Path(__file__).with_name("assets") / "simple.css"
        css = source.read_text(encoding="utf-8")
        path = self.site / "assets" / "simple.css"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(self.banner("hash") + css, encoding="utf-8")

    def write_svg(self, rel: Path, svg: str) -> None:
        """Write the same SVG asset to Markdown and site trees."""
        for root in (self.markdown, self.site):
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(self.banner() + svg, encoding="utf-8")

    def write_text_asset(self, rel: Path, text: str, *, site_only: bool = True) -> None:
        """Write a deterministic text asset."""
        roots = [self.site] if site_only else [self.markdown, self.site]
        for root in roots:
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                self.banner("hash") + text.rstrip() + "\n", encoding="utf-8"
            )

    def write_json_asset(
        self, rel: Path, data: object, *, site_only: bool = True
    ) -> None:
        """Write a deterministic valid JSON asset."""
        text = json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True)
        roots = [self.site] if site_only else [self.markdown, self.site]
        for root in roots:
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text + "\n", encoding="utf-8")

    def write_favicon(self) -> None:
        """Write a small SVG favicon."""
        body = "\n".join(
            [
                '<circle cx="32" cy="32" r="24" fill="#0f766e"/>',
                '<path d="M20 23h24v7H29v5h12v7H29v9h-9z" fill="#ffffff"/>',
            ]
        )
        (self.site / "favicon.svg").write_text(
            self.banner() + _svg_shell(64, 64, body),
            encoding="utf-8",
        )


@functools.lru_cache(maxsize=1)
def _citation_doi() -> str:
    """The concept DOI for the docs footer (always-latest citation anchor)."""
    try:
        from gmeow_tools.self_desc import load_self_description

        return load_self_description().concept_doi
    except (FileNotFoundError, ValueError):
        return ""


def _html_shell(title: str, body: str, prefix: str) -> str:
    """Return a full static HTML document."""
    escaped_title = html.escape(title)
    home = prefix or "./"
    doi = _citation_doi()
    # Only render the citation line when a DOI is actually present, so a missing
    # self-description falls back cleanly instead of emitting "https://doi.org/".
    citation = (
        f'<br>\n    Cite as <a href="https://doi.org/{html.escape(doi, quote=True)}">'
        f"doi:{html.escape(doi)}</a> ·"
        if doi
        else ""
    )
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{escaped_title} - GMEOW</title>
  <link rel="icon" href="{prefix}favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="{prefix}assets/simple.css">
  <link rel="stylesheet" href="{prefix}assets/gmeow.css">
</head>
<body>
  <a class="skip" href="#content">Skip to content</a>
  <header>
    <nav aria-label="Primary">
      <a href="{home}">Home</a>
      <a href="{prefix}getting-started/">Getting Started</a>
      <a href="{prefix}learning-paths/">Learning Paths</a>
      <a href="{prefix}recipes/">Recipes</a>
      <a href="{prefix}examples/">Examples</a>
      <a href="{prefix}concerns/">Concerns</a>
      <a href="{prefix}four-boxes/">Four Boxes</a>
      <a href="{prefix}slices/">Slices</a>
      <a href="{prefix}adoption/">Adoption</a>
      <a href="{prefix}linkages/">Linkages</a>
      <a href="{prefix}references/">Bibliography</a>
      <a href="{prefix}reference/">Reference</a>
      <a href="{prefix}external/ontologies/">External</a>
      <a href="{prefix}statements/">RDF 1.2</a>
    </nav>
  </header>
  <main id="content">
{body}
  </main>
  <footer>
    Generated from the GMEOW ontology. Canonical source is RDF/OWL; this
    site is a deterministic projection.{citation}
    © 2026 Blackcat Informatics® Inc. · Ontology licensed CC BY 4.0.
  </footer>
</body>
</html>
"""


def _term_counts(terms: Iterable[DocTerm]) -> dict[str, int]:
    """Count terms by category."""
    counts: dict[str, int] = dict.fromkeys(_CATEGORY_DIRS, 0)
    for term in terms:
        counts[term.category] += 1
    return counts


def _curie_link_from(curie_value: str, model: DocsModel, from_rel: Path) -> str:
    """Return a Markdown link from one generated page to a term, when known."""
    term = model.terms_by_curie.get(curie_value)
    if term is None:
        return f"`{curie_value}`"
    rel = posixpath.relpath(
        _term_md_rel(term).as_posix(),
        start=from_rel.parent.as_posix() or ".",
    )
    return f"[`{curie_value}`]({rel})"


def _linkage_table(
    links: list[DocLinkage],
    *,
    from_rel: Path,
    model: DocsModel,
    limit: int | None = None,
) -> list[str]:
    """Render a deterministic linkage table."""
    visible = links[:limit] if limit is not None else links
    lines = [
        "| Source | Kind | Profile | Predicate/Relation | Target | Evidence |",
        "|---|---|---|---|---|---|",
    ]
    for link in visible:
        source = _curie_link_from(link.source, model, from_rel)
        profile = link.profile or "-"
        relation_bits = [link.predicate]
        if link.relation:
            relation_bits.append(link.relation)
        if link.emits_sssom:
            relation_bits.append("emits SSSOM")
        relation = " / ".join(relation_bits)
        evidence_bits = []
        if link.mapping_set:
            evidence_bits.append(f"`{link.mapping_set}`")
        evidence_bits.append(f"`{link.cell}`")
        if link.confidence:
            evidence_bits.append(f"confidence {link.confidence}")
        if link.lossy_drops:
            evidence_bits.append("lossy: " + "; ".join(link.lossy_drops[:3]))
        if link.transform:
            evidence_bits.append(f"transform `{link.transform}`")
        lines.append(
            f"| {source} | {link.kind} | `{profile}` | "
            f"{_escape_md_cell(relation)} | {_escape_md_cell(link.target)} | "
            f"{_escape_md_cell('; '.join(evidence_bits))} |"
        )
    if limit is not None and len(links) > limit:
        lines.append(
            f"| ... | ... | ... | ... | ... | {len(links) - limit} more rows |"
        )
    return lines


def _alignment_table(
    alignments: Sequence[str], *, model: DocsModel, from_rel: Path
) -> list[str]:
    """Render compact alignment triples as a readable table."""
    lines = ["| Relation | Target |", "|---|---|"]
    for alignment in alignments:
        if "=" in alignment:
            relation, target = alignment.split("=", 1)
            rendered_target = _link_public_segment(
                target,
                model=model,
                from_rel=from_rel,
                code_label=True,
                allow_bare=False,
            )
            lines.append(f"| `{relation}` | {rendered_target} |")
        else:
            rendered = _link_public_segment(
                alignment,
                model=model,
                from_rel=from_rel,
                code_label=True,
                allow_bare=False,
            )
            lines.append(f"| - | {rendered} |")
    return lines


def _html_identifier_link(value: str, *, model: DocsModel, from_rel: Path) -> str:
    """Return an HTML link for a CURIE-like identifier when possible."""
    label = f"<code>{html.escape(value)}</code>"
    if value.startswith("gmeow:"):
        term = model.terms_by_curie.get(value)
        if term is not None:
            rel = posixpath.relpath(
                _term_md_rel(term).as_posix(),
                start=from_rel.parent.as_posix() or ".",
            )
            return f'<a href="{html.escape(rel)}">{label}</a>'
    if value.startswith("gufo:"):
        local = value.split(":", 1)[1]
        if local in _GUFO_TERM_HELP:
            rel = _external_terms_rel(from_rel, f"gufo-{local.lower()}")
            return f'<a href="{html.escape(rel)}">{label}</a>'
    if value.startswith("wd:Q"):
        qid = value.split(":", 1)[1]
        return f'<a href="https://www.wikidata.org/wiki/{qid}">{label}</a>'
    if value.startswith(("wd:P", "wdt:P")):
        pid = value.split(":", 1)[1]
        return f'<a href="https://www.wikidata.org/wiki/Property:{pid}">{label}</a>'
    if ":" in value:
        prefix, local = value.split(":", 1)
        namespace = PREFIXES.get(prefix)
        if namespace is not None:
            href = namespace + local
            return f'<a href="{html.escape(href)}">{label}</a>'
    return label


def _alignment_html_table(
    alignments: Sequence[str], *, model: DocsModel, from_rel: Path
) -> list[str]:
    """Render alignment pairs as HTML safe for use inside a details block."""
    lines = [
        "<table>",
        "<thead><tr><th>Relation</th><th>Target</th></tr></thead>",
        "<tbody>",
    ]
    for alignment in alignments:
        if "=" in alignment:
            relation, target = alignment.split("=", 1)
            relation_html = _html_identifier_link(
                relation, model=model, from_rel=from_rel
            )
            target_html = _html_identifier_link(target, model=model, from_rel=from_rel)
        else:
            relation_html = "-"
            target_html = _html_identifier_link(
                alignment, model=model, from_rel=from_rel
            )
        lines.append(f"<tr><td>{relation_html}</td><td>{target_html}</td></tr>")
    lines.extend(["</tbody>", "</table>"])
    return lines


def _unique_projection_bindings(links: Iterable[DocLinkage]) -> list[DocLinkage]:
    """Dedupe projection rows that mention several source terms."""
    seen: set[tuple[str, str, str, str]] = set()
    out: list[DocLinkage] = []
    for link in sorted(links, key=_link_sort_key):
        if link.kind != "projection":
            continue
        key = (link.cell, link.profile, link.target, link.relation)
        if key in seen:
            continue
        seen.add(key)
        out.append(link)
    return out


def _example_by_path(model: DocsModel, path: Path) -> DocExample | None:
    """Return an example by repository-relative path."""
    path_text = path.as_posix()
    return next(
        (example for example in model.examples if example.path.as_posix() == path_text),
        None,
    )


def _recipe_by_slug(model: DocsModel, slug: str) -> DocRecipe | None:
    """Return a recipe by stable slug."""
    return next((recipe for recipe in model.recipes if recipe.slug == slug), None)


def _repo_source_link(path: Path) -> str:
    """Return a GitHub source link for a repository-relative path."""
    return f"[`{path.as_posix()}`]({_REPO_BLOB_URL}{path.as_posix()})"


def _render_example(
    example: DocExample, *, model: DocsModel, from_rel: Path, max_terms: int = 10
) -> list[str]:
    """Render a source example as linked metadata plus Turtle."""
    terms = [
        _curie_link_from(term, model, from_rel)
        for term in example.terms[:max_terms]
        if term in model.terms_by_curie
    ]
    lines = [
        f"### {example.title}",
        "",
        f"- **Source:** {_repo_source_link(example.path)}",
    ]
    if terms:
        lines.append(f"- **GMEOW terms:** {', '.join(terms)}")
    if example.external_prefixes:
        lines.append(
            "- **External prefixes:** "
            + ", ".join(f"`{prefix}`" for prefix in example.external_prefixes)
        )
    lines.extend(["", "```turtle", example.text, "```", ""])
    return lines


def _snippet_for_term(example: DocExample, term: DocTerm) -> str | None:
    """Return a compact Turtle snippet showing the term in one example."""
    if term.curie not in example.text:
        return None
    prefix_lines: list[str] = []
    body_blocks: list[str] = []
    for block in re.split(r"\n\s*\n", example.text):
        block_text = block.strip()
        if not block_text:
            continue
        if block_text.startswith(("@prefix ", "PREFIX ", "@base ", "BASE ")):
            prefix_lines.append(block_text)
        elif term.curie in block_text:
            body_blocks.append(block_text)
    if not body_blocks:
        return None
    lines = [*prefix_lines[:12], "", *body_blocks[:2]]
    snippet_lines = "\n\n".join(lines).strip().splitlines()
    if len(snippet_lines) > 80:
        snippet_lines = [*snippet_lines[:80], "# ..."]
    return "\n".join(snippet_lines)


def _term_example_snippets(term: DocTerm, model: DocsModel) -> list[str]:
    """Render copyable snippets from examples that mention a term."""
    snippets: list[tuple[DocExample, str]] = []
    owner_slice = term.owner.replace("gmeow:slices/", "") if term.owner else ""
    examples = sorted(
        model.examples,
        key=lambda example: (
            example.slice_name != owner_slice,
            example.slice_name,
            example.path.as_posix(),
        ),
    )
    for example in examples:
        snippet = _snippet_for_term(example, term)
        if snippet is not None:
            snippets.append((example, snippet))
        if len(snippets) >= 2:
            break
    if not snippets:
        return []
    rel = _term_md_rel(term)
    lines = [
        "## Example Snippets",
        "",
        "These snippets are generated from canonical slice examples and trimmed "
        "to the Turtle blocks where this term appears.",
        "",
    ]
    for example, snippet in snippets:
        catalog_rel = posixpath.relpath(
            (Path("examples") / "index.md").as_posix(),
            start=rel.parent.as_posix() or ".",
        )
        catalog_href = f"[open in catalog]({catalog_rel}#{_example_anchor(example)})"
        lines.extend(
            [
                f"### {example.title}",
                "",
                f"- **Source:** {_repo_source_link(example.path)}",
                f"- **Examples catalog:** {catalog_href}#{_example_anchor(example)}",
                "",
                "```turtle",
                snippet,
                "```",
                "",
            ]
        )
    return lines


def _recipe_link(recipe: DocRecipe) -> str:
    """Return a link to a recipe page from the recipe index."""
    return f"[{recipe.title}]({recipe.slug}.md)"


def _relative_page_link(label: str, target: Path, from_rel: Path) -> str:
    """Return a Markdown link from one generated page to another."""
    rel = posixpath.relpath(target.as_posix(), start=from_rel.parent.as_posix() or ".")
    return f"[{label}]({rel})"


def _common_companion_terms(term: DocTerm, model: DocsModel) -> list[str]:
    """Return nearby GMEOW terms that help explain a term in use."""
    candidates: list[str] = []
    candidates.extend(term.parents)
    candidates.extend(term.sub_property_of)
    candidates.extend(term.types)
    for value in (term.domain, term.range):
        if value:
            candidates.append(value)
    for link in term.linkages:
        if link.target.startswith("gmeow:"):
            candidates.append(link.target)
    for concern_iri in term.concerns[:2]:
        concern = next((c for c in model.concerns if c.iri == concern_iri), None)
        if concern is not None:
            candidates.extend(t.curie for t in concern.terms[:8])
    unique: list[str] = []
    for value in candidates:
        if value == term.curie or value not in model.terms_by_curie or value in unique:
            continue
        unique.append(value)
    return unique[:10]


def _term_practical_pattern(term: DocTerm) -> str:
    """Return a concise practical pattern for a term."""
    if term.category == "property" and (term.domain or term.range):
        return (
            f"Use `{term.curie}` from `{term.domain or '?'}` to "
            f"`{term.range or '?'}` when the relationship itself belongs in "
            "the native GMEOW graph."
        )
    if term.category == "class" and term.parents:
        return (
            f"Use `{term.curie}` as a specialized kind of "
            + ", ".join(f"`{parent}`" for parent in term.parents[:3])
            + ". Add statement metadata or a standpoint when the assertion "
            "needs provenance, confidence, or vantage."
        )
    if term.category == "individual" and term.types:
        return (
            f"Use `{term.curie}` as a controlled value typed as "
            + ", ".join(f"`{kind}`" for kind in term.types[:3])
            + "."
        )
    return (
        f"Use `{term.curie}` when the definition matches the source fact. "
        "Prefer a narrower GMEOW term when one exists, and keep projection "
        "concerns in the mapping layer."
    )


def _landing(model: DocsModel) -> Page:
    """Render the home page."""
    counts = _term_counts(model.terms)
    core = sum(1 for s in model.slices.values() if s.is_core)
    extensions = len(model.slices) - core
    lines = [
        f"# {model.title}",
        "",
        model.description
        or (
            "GMEOW is a reasoning-centric OWL 2 DL super-vocabulary with "
            "native RDF 1.2 statement metadata."
        ),
        "",
        f"- **Version:** `{model.version}`",
        f"- **Namespace:** <{NAMESPACE}>",
        f"- **Terms:** {counts['class']} classes, {counts['property']} properties, "
        f"{counts['individual']} individuals, {counts['datatype']} datatypes",
        f"- **Slices:** {core} core, {extensions} extension",
        "",
        "## Start Here",
        "",
        "- [Getting Started](getting-started.md) for the five-minute adoption path.",
        "- [Learning Paths](learning-paths/index.md) for curated adoption journeys.",
        "- [Recipes](recipes/index.md) for task-first modelling walkthroughs.",
        "- [Examples](examples/index.md) for canonical slice-local Turtle examples.",
        "- [Adoption Targets](adoption/index.md) for schema.org, PROV-O, "
        "Wikidata, and other projection surfaces.",
        "- [Bibliography](references/index.md) for the generated bibliography "
        "from the canonical citation ledger.",
        "- [Cross-cutting concerns](concerns/index.md) for reusable ideas.",
        "- [Slices](slices/index.md) for modular guide pages and dependency maps.",
        "- [Linkages](linkages/index.md) for SSSOM alignments and projection coverage.",
        "- [Reference](reference/index.md) for classes, properties, "
        "individuals, and datatypes.",
        "- [RDF 1.2 statement layer](statements/index.md) for reifiers and "
        "statement metadata.",
        "",
        "## Profiles",
        "",
        f"- **Core profile:** <{ONTOLOGY_IRI}>",
        f"- **Full profile:** <{FULL_PROFILE_IRI}>",
        "",
        "## Slices",
        "",
        f"GMEOW is organized as {len(model.slices)} documented ontology slices. "
        "Start with [the slice index](slices/index.md) when you need module "
        "boundaries, dependency order, or guide prose.",
        "",
        "## Reference",
        "",
        "The generated reference is grouped by [classes](reference/classes/index.md), "
        "[properties](reference/properties/index.md), "
        "[individuals](reference/individuals/index.md), and "
        "[datatypes](reference/datatypes/index.md).",
        "",
        "## Distribution",
        "",
        "The same documentation is bundled inside `gmeow.gts` and can be "
        "exported offline with `gmeow export-docs --directory docs-out`.",
        "",
        "## Static Indexes",
        "",
        "- [`search-index.json`](search-index.json) provides a deterministic "
        "machine-readable index for local tooling.",
        "- [`llms-docs.txt`](llms-docs.txt) provides a compact plain-text "
        "overview for search, agents, and offline review.",
        "",
    ]
    return Page(Path("index.md"), model.title, "\n".join(lines))


def _getting_started() -> Page:
    """Render the getting-started page."""
    examples = [
        (
            "people, names, pronouns, aliases, and display suppression",
            "slices/core/names/examples/person-names.ttl",
            "`gmeow:PersonName`, `gmeow:NameUsage`, `gmeow:displayable`",
        ),
        (
            "contested or attributed facts",
            "slices/core/standpoint/examples/contested-authorship.ttl",
            "`gmeow:StandpointClaim`, `gmeow:vantage`, `gmeow:claimModality`",
        ),
        (
            "documents and public web presence",
            "slices/core/documents/examples/web-presence.ttl",
            "`gmeow:Document`, `gmeow:webUrl`, schema.org linkages",
        ),
        (
            "events and participants",
            "slices/core/events/examples/wedding.ttl",
            "`gmeow:Event`, `gmeow:Participation`, temporal frames",
        ),
        (
            "time intervals and calendars",
            "slices/core/temporal/examples/intervals-and-frames.ttl",
            "`gmeow:TimeInterval`, `gmeow:TemporalFrame`",
        ),
        (
            "creative works",
            "slices/core/creative-works/examples/wemi-novel.ttl",
            "WEMI, titles, releases, and external work alignments",
        ),
        (
            "offline distribution and transport",
            "slices/core/gts/examples/dist-package.ttl",
            "`docs/GTS-SPEC.md`, `gmeow:GTSProfile`, `gmeow:GTSSegment`",
        ),
        (
            "agent memory or tool trajectories",
            "slices/extensions/agentic/examples/agent-trajectory.ttl",
            "agentic and provenance terms",
        ),
        (
            "graph-RAG datasets and pipelines",
            "slices/extensions/graphrag/examples/lillith-dataset.ttl",
            "dataset, source, chunk, and extraction terms",
        ),
    ]
    lines = [
        "# Getting Started",
        "",
        "GMEOW is a large ontology, but adoption does not start with the whole "
        "graph. Start with one slice, one example, and one projection target.",
        "You do not need a reasoner, Docker, Java, or an RDF editor to inspect "
        "the vocabulary.",
        "",
        "## Install",
        "",
        "```bash",
        "pip install gmeow",
        "```",
        "",
        "## Export the bundled docs",
        "",
        "```bash",
        "gmeow export-docs --directory gmeow-docs",
        "```",
        "",
        "The exported directory is the same static documentation bundle as this "
        "site: Markdown, HTML, SVG diagrams, slice pages, term pages, linkage "
        "tables, and RDF 1.2 statement documentation.",
        "",
        "## Pick a first path",
        "",
        "For fuller walkthroughs, start with "
        "[Learning Paths](learning-paths/index.md) or [Recipes](recipes/index.md). "
        "Each recipe points to canonical Turtle examples and the term pages "
        "that explain the pattern. The generated [Examples](examples/index.md) "
        "catalog lists every slice-local Turtle example when you want broader "
        "coverage.",
        "",
        "| If you are modelling... | Start with | Then inspect |",
        "|---|---|---|",
        *[f"| {topic} | `{path}` | {inspect} |" for topic, path, inspect in examples],
        "",
        "## Inspect terms while reading examples",
        "",
        "```bash",
        "gmeow describe gmeow:Person",
        "gmeow describe gmeow:NameUsage",
        "gmeow describe gmeow:StandpointClaim",
        "```",
        "",
        "Every generated term page answers four questions: what the term means, "
        "which slice owns it, how it links to other GMEOW terms, and how it "
        "projects to external vocabularies.",
        "",
        "## Read slices as doctrine, not just reference",
        "",
        "A slice page explains why a modelling pattern exists. The term pages then "
        "give exact class/property details. Useful first slices:",
        "",
        "- [names](slices/names.md): co-equal names, pronouns, usage contexts, "
        "and display suppression.",
        "- [standpoint](slices/standpoint.md): claims that coexist by vantage "
        "instead of collapsing to one truth slot.",
        "- [temporal](slices/temporal.md): frame-relative time values and "
        "solver boundaries.",
        "- [teleology](slices/teleology.md): goals, desires, intentions, "
        "commitments, and why `gufo:IntrinsicMode` appears.",
        "- [gts](slices/gts.md): offline transport, profiles, segments, "
        "codecs, and compaction lineage.",
        "",
        "## Follow external links deliberately",
        "",
        "When the docs mention PROV-O, P-Plan, gUFO, FIBO, CIDOC CRM, Wikidata, "
        "or ConceptNet/ATOMIC, those links point to the generated "
        "[External Ontologies](external/ontologies.md) catalog first. That page "
        "lists the target, license, description, and upstream website/namespace. "
        "Formal external terms such as `gufo:IntrinsicMode` point to "
        "[External Terms](external/terms.md), where the docs translate the term "
        "before sending you to the upstream ontology.",
        "",
        "## Understand statement metadata",
        "",
        "GMEOW uses RDF 1.2 / RDF-star style statement metadata for provenance, "
        "confidence, temporal scope, and standpoint. If a fact needs an "
        "`accordingTo`, confidence, validity interval, or attribution, read the "
        "[RDF 1.2 statement layer](statements/index.md) before flattening it.",
        "",
        "## Use linkages as adoption maps",
        "",
        "The [Linkages](linkages/index.md) page is generated from the mapping DSL. "
        "It shows SSSOM alignments, projection profiles, lossy-drop notes, and "
        "external vocabulary coverage. Treat it as the map from native GMEOW to "
        "consumer formats such as schema.org, PROV-O, vCard, FOAF, GeoSPARQL, "
        "and Wikidata.",
        "",
        "## Use the web docs and content negotiation",
        "",
        "Human requests for GMEOW IRIs resolve into this static site. RDF clients "
        "continue to use HTTP content negotiation for Turtle, RDF/XML, N-Triples, "
        "and JSON-LD serializations.",
        "",
    ]
    return Page(Path("getting-started.md"), "Getting Started", "\n".join(lines))


def _recipes_index(model: DocsModel) -> Page:
    """Render the task-oriented recipe index."""
    lines = [
        "# Recipes",
        "",
        "Recipes are small adoption paths generated around canonical slice "
        "examples. They are intentionally task-first: start with the modelling "
        "problem, inspect the Turtle, then jump into the exact terms and slices.",
        "",
        "| Recipe | Goal | Examples |",
        "|---|---|---|",
    ]
    for recipe in model.recipes:
        examples = ", ".join(f"`{path.as_posix()}`" for path in recipe.example_paths)
        lines.append(
            f"| {_recipe_link(recipe)} | {_escape_md_cell(recipe.goal)} | "
            f"{_escape_md_cell(examples)} |"
        )
    lines.append("")
    return Page(Path("recipes") / "index.md", "Recipes", "\n".join(lines))


def _learning_paths_index(model: DocsModel) -> Page:
    """Render curated adoption paths across recipes, examples, and terms."""
    rel = Path("learning-paths") / "index.md"
    lines = [
        "# Learning Paths",
        "",
        "Learning paths sequence recipes, canonical examples, term pages, and "
        "external adoption targets for common implementation goals.",
        "",
        "| Path | Audience | Goal |",
        "|---|---|---|",
    ]
    for path in model.learning_paths:
        lines.append(
            f"| [{path.title}](#{path.slug}) | {_escape_md_cell(path.audience)} | "
            f"{_escape_md_cell(path.goal)} |"
        )
    lines.append("")
    for path in model.learning_paths:
        recipe_links = []
        for slug in path.recipe_slugs:
            recipe = _recipe_by_slug(model, slug)
            if recipe is not None:
                recipe_links.append(
                    _relative_page_link(
                        recipe.title, Path("recipes") / f"{recipe.slug}.md", rel
                    )
                )
        example_links = []
        for example_path in path.example_paths:
            example = _example_by_path(model, example_path)
            if example is None:
                example_links.append(_repo_source_link(example_path))
            else:
                examples_rel = posixpath.relpath(
                    (Path("examples") / "index.md").as_posix(),
                    start=rel.parent.as_posix() or ".",
                )
                example_links.append(
                    f"[{example.title}]({examples_rel}#{_example_anchor(example)})"
                )
        term_links = [
            _curie_link_from(term, model, rel)
            for term in path.term_curies
            if term in model.terms_by_curie
        ]
        target_links = [
            (
                f"[`{target}`]"
                f"({_external_ontologies_rel(rel, _external_target_anchor(target))})"
            )
            for target in path.adoption_targets
        ]
        lines.extend(
            [
                f"## {path.title}",
                f'<a id="{path.slug}"></a>',
                "",
                f"**Audience:** {path.audience}",
                "",
                path.goal,
                "",
                "### Steps",
                "",
            ]
        )
        if recipe_links:
            lines.append("1. Read " + ", ".join(recipe_links) + ".")
        if example_links:
            lines.append("2. Copy and adapt " + ", ".join(example_links) + ".")
        if term_links:
            lines.append("3. Inspect " + ", ".join(term_links) + ".")
        if target_links:
            lines.append("4. Check adoption targets " + ", ".join(target_links) + ".")
        lines.append("")
    return Page(rel, "Learning Paths", "\n".join(lines))


def _example_anchor(example: DocExample) -> str:
    """Return the stable Markdown/HTML anchor for an example path."""
    stem = _safe_filename(example.path.with_suffix("").as_posix()).lower()
    return f"example-{stem}"


def _examples_index(model: DocsModel) -> Page:
    """Render the canonical examples catalog."""
    rel = Path("examples") / "index.md"
    lines = [
        "# Examples",
        "",
        "These examples are discovered from `slices/**/examples/*.ttl` and "
        "rendered from canonical Turtle sources. Use them as copyable starting "
        "points, then follow the linked terms and slice pages for the doctrine "
        "behind each pattern.",
        "",
        "| Slice | Example | Terms | External Prefixes | Source |",
        "|---|---|---|---|---|",
    ]
    for example in sorted(model.examples, key=lambda ex: (ex.slice_name, ex.path)):
        slice_link = _relative_page_link(
            example.slice_name,
            Path("slices") / f"{example.slice_name}.md",
            rel,
        )
        title = f'<a id="{_example_anchor(example)}"></a>{example.title}'
        terms = ", ".join(
            _curie_link_from(term, model, rel)
            for term in example.terms[:8]
            if term in model.terms_by_curie
        )
        if len(example.terms) > 8:
            terms = f"{terms}, ..." if terms else "..."
        prefixes = ", ".join(f"`{prefix}`" for prefix in example.external_prefixes)
        lines.append(
            f"| {slice_link} | {title} | {terms or '-'} | {prefixes or '-'} | "
            f"{_repo_source_link(example.path)} |"
        )
    lines.append("")
    return Page(rel, "Examples", "\n".join(lines))


def _recipe_page(recipe: DocRecipe, model: DocsModel) -> Page:
    """Render one task-oriented recipe."""
    rel = Path("recipes") / f"{recipe.slug}.md"
    term_links = [
        _curie_link_from(curie_value, model, rel)
        for curie_value in recipe.term_curies
        if curie_value in model.terms_by_curie
    ]
    lines = [
        f"# {recipe.title}",
        "",
        recipe.goal,
        "",
    ]
    if term_links:
        lines.extend(["## Core Terms", "", ", ".join(term_links), ""])
    if recipe.follow_pages:
        lines.extend(
            [
                "## Read Next",
                "",
                *[
                    "- "
                    + _relative_page_link(
                        page.stem.replace("-", " ").title(), page, rel
                    )
                    for page in recipe.follow_pages
                ],
                "",
            ]
        )
    lines.extend(["## Examples", ""])
    for path in recipe.example_paths:
        example = _example_by_path(model, path)
        if example is None:
            lines.extend(
                [
                    f"### {_title_from_stem(path.stem)}",
                    "",
                    f"Example source not found: {_repo_source_link(path)}",
                    "",
                ]
            )
            continue
        lines.extend(_render_example(example, model=model, from_rel=rel))
    return Page(rel, recipe.title, "\n".join(lines))


def _about_page(model: DocsModel) -> Page:
    """Render a small generated about page for legacy links."""
    lines = [
        "# About GMEOW",
        "",
        model.description or "GMEOW is a reasoning-centric OWL 2 DL super-vocabulary.",
        "",
        f"- **Version:** `{model.version}`",
        f"- **Namespace:** <{NAMESPACE}>",
        "",
        "## Publication",
        "",
        "This documentation is generated directly from the GMEOW ontology fold, "
        "slice manifests, and slice guide Markdown.",
        "",
    ]
    return Page(Path("about.md"), "About GMEOW", "\n".join(lines))


def _changelog_page(model: DocsModel) -> Page:
    """Render a compatibility changelog page without external generators."""
    lines = [
        "# Changelog",
        "",
        f"The generated documentation currently describes GMEOW `{model.version}`.",
        "",
        "For source-level change history, use the repository history for the "
        "ontology modules and generator code. This page is retained so links "
        "from the previous documentation attempt do not resolve to a 404.",
        "",
    ]
    return Page(Path("changelog.md"), "Changelog", "\n".join(lines))


def _visualization_page() -> Page:
    """Render the static visualization index for legacy links."""
    lines = [
        "# Visualizations",
        "",
        "GMEOW publishes static SVG diagrams instead of a client-side graph UI.",
        "",
        "- [Slice dependency map](../diagrams/slices.svg)",
        "- [Cross-cutting concern map](../diagrams/concerns.svg)",
        "",
    ]
    return Page(Path("visualization") / "index.md", "Visualizations", "\n".join(lines))


def _quality_page() -> Page:
    """Render a local quality-gates page for legacy OOPS report links."""
    lines = [
        "# Quality Gates",
        "",
        "The web documentation is generated without external pitfall scanners. "
        "Local quality gates live in the repository toolchain:",
        "",
        "- `make validate` for syntax, annotation completeness, SHACL, and examples.",
        "- `make check-generated` for deterministic generated artifacts.",
        "- `make lint` for Python, shell, Markdown, YAML, and static checks.",
        "",
    ]
    return Page(Path("quality") / "oops-report.md", "Quality Gates", "\n".join(lines))


def _references_page() -> Page:
    """Render the generated citation ledger bibliography."""
    body = REFERENCES_MD_FILE.read_text(encoding="utf-8")
    content = "\n".join(
        line
        for line in body.splitlines()
        if not line.startswith("<!-- GENERATED")
        and not line.startswith("<!-- Source hash:")
        and line != "This bibliography is generated from `metadata/references.ttl`."
        and "github.com/Blackcat-Informatics/gmeow-ontology" not in line
    ).strip()
    if content.startswith("# GMEOW Citation Ledger"):
        content = content.removeprefix("# GMEOW Citation Ledger").lstrip()
    lines = [
        "# References",
        "",
        "This bibliography is generated from the canonical citation ledger at "
        "`metadata/references.ttl`. The ontology-docs generator consumes the "
        "`generated/references/references.md` projection rather than maintaining "
        "a second bibliography.",
        "",
        "## Exports",
        "",
        "- [`generated/references/references.md`](https://github.com/"
        "Blackcat-Informatics/gmeow-ontology/blob/main/generated/references/"
        "references.md)",
        "- [`generated/references/references.csl.json`](https://github.com/"
        "Blackcat-Informatics/gmeow-ontology/blob/main/generated/references/"
        "references.csl.json)",
        "- [`generated/references/references.bib`](https://github.com/"
        "Blackcat-Informatics/gmeow-ontology/blob/main/generated/references/"
        "references.bib)",
        "",
        content,
        "",
    ]
    return Page(Path("references") / "index.md", "References", "\n".join(lines))


def _reference_index(model: DocsModel) -> Page:
    """Render the reference index."""
    counts = _term_counts(model.terms)
    lines = [
        "# Reference",
        "",
        "Browse GMEOW terms by category. Cross-ontology links and projection "
        "coverage are summarized in [Linkages](../linkages/index.md).",
        "",
    ]
    for category, label in _CATEGORY_LABELS.items():
        rel = Path("reference") / _CATEGORY_DIRS[category] / "index.md"
        lines.append(f"- {_markdown_link(label, rel)}: {counts[category]} terms")
    lines.append("")
    return Page(Path("reference") / "index.md", "Reference", "\n".join(lines))


def _category_index(category: str, terms: list[DocTerm]) -> Page:
    """Render one category index."""
    label = _CATEGORY_LABELS[category]
    lines = [
        f"# {label}",
        "",
        f"{len(terms)} documented {label.lower()}.",
        "",
        "| Term | Label | Defined By | Box Roles | Linkages |",
        "|---|---|---|---|---|",
    ]
    for term in terms:
        link = _markdown_link(f"`{term.curie}`", Path(term.filename))
        roles = ", ".join(f"`{role}`" for role in term.box_roles) or "-"
        lines.append(
            f"| {link} | {_escape_md_cell(term.label)} | `{term.owner or '-'}` | "
            f"{roles} | {len(term.linkages)} |"
        )
    lines.append("")
    return Page(
        Path("reference") / _CATEGORY_DIRS[category] / "index.md",
        label,
        "\n".join(lines),
    )


def _term_page(term: DocTerm, model: DocsModel) -> Page:
    """Render one term reference page."""
    lines = [
        f"# {term.label}",
        "",
        f"- **CURIE:** `{term.curie}`",
        f"- **IRI:** <{term.iri}>",
        f"- **Category:** {term.category}",
    ]
    if term.owner:
        lines.append(f"- **Defined by:** `{term.owner}`")
    if term.box_roles:
        lines.append(
            "- **Box roles:** " + ", ".join(f"`{role}`" for role in term.box_roles)
        )
    lines.append("")
    if term.definition:
        lines.extend([term.definition, ""])
    if term.comment and term.comment != term.definition:
        lines.extend([term.comment, ""])

    facts: list[str] = []
    if term.category == "class" and term.parents:
        facts.append("**Subclass of:** " + ", ".join(f"`{p}`" for p in term.parents))
    if term.category == "property":
        meta = [f"{term.prop_kind} property" if term.prop_kind else "property"]
        if term.domain or term.range:
            meta.append(f"`{term.domain or '?'}` -> `{term.range or '?'}`")
        if term.functional:
            meta.append("functional")
        facts.append("**Property shape:** " + "; ".join(meta))
        if term.sub_property_of:
            facts.append(
                "**Sub-property of:** "
                + ", ".join(f"`{p}`" for p in term.sub_property_of)
            )
    if term.category == "individual" and term.types:
        facts.append("**Types:** " + ", ".join(f"`{t}`" for t in term.types))
    if facts:
        lines.extend(["## Structure", "", *facts, ""])

    lines.extend(["## Practical Pattern", "", _term_practical_pattern(term), ""])
    lines.extend(_term_example_snippets(term, model))
    companions = _common_companion_terms(term, model)
    if companions:
        lines.extend(
            [
                "## Common Companion Terms",
                "",
                ", ".join(
                    _curie_link_from(value, model, _term_md_rel(term))
                    for value in companions
                ),
                "",
            ]
        )

    if term.concerns:
        concern_links = []
        for concern in model.concerns:
            if concern.iri in term.concerns:
                concern_links.append(
                    _markdown_link(
                        concern.label,
                        Path("../../concerns") / concern.filename,
                    )
                )
        if concern_links:
            lines.extend(
                ["## Cross-Cutting Concerns", "", ", ".join(concern_links), ""]
            )

    if term.linkages:
        equivalences = [link for link in term.linkages if link.kind == "equivalence"]
        projections = [link for link in term.linkages if link.kind == "projection"]
        if projections:
            profile_summary: dict[str, set[str]] = defaultdict(set)
            for link in projections:
                for prefix in _target_prefixes([link.target]):
                    profile_summary[link.profile or "-"].add(prefix)
            lines.extend(
                [
                    "## Projects To",
                    "",
                    "| Profile | External Targets |",
                    "|---|---|",
                    *[
                        "| `{profile}` | {targets} |".format(
                            profile=profile,
                            targets=", ".join(
                                f"`{prefix}`" for prefix in sorted(prefixes)
                            )
                            or "-",
                        )
                        for profile, prefixes in sorted(profile_summary.items())
                    ],
                    "",
                ]
            )
        if equivalences:
            targets = _target_prefixes(link.target for link in equivalences)
            lines.extend(
                [
                    "## External Equivalences",
                    "",
                    "Equivalent or closely aligned targets: "
                    + (", ".join(f"`{prefix}`" for prefix in targets) or "-"),
                    "",
                ]
            )
        lines.extend(
            [
                "## Linkages",
                "",
                "Generated from the canonical mapping DSL. SSSOM files are "
                "the generated public interchange form for term equivalences.",
                "",
            ]
        )
        if equivalences:
            lines.extend(["### Term Equivalences", ""])
            lines.extend(
                _linkage_table(
                    equivalences,
                    from_rel=_term_md_rel(term),
                    model=model,
                    limit=_MAX_TERM_LINK_ROWS,
                )
            )
            lines.append("")
        if projections:
            lines.extend(["### Projection Coverage", ""])
            lines.extend(
                _linkage_table(
                    projections,
                    from_rel=_term_md_rel(term),
                    model=model,
                    limit=_MAX_TERM_LINK_ROWS,
                )
            )
            lines.append("")

    advice_fields = [
        ("Use when", term.use_when),
        ("Avoid when", term.avoid_when),
        ("How to use", term.how_to_use),
        ("Scope notes", term.scope_notes),
        ("Examples", term.examples),
    ]
    if (
        any(values for _, values in advice_fields)
        or term.use_for_consumer
        or term.avoid_for_consumer
    ):
        lines.extend(["## Usage Advice", ""])
        for label, values in advice_fields:
            if values:
                lines.extend(
                    [f"### {label}", "", *[f"- {value}" for value in values], ""]
                )
        if term.use_for_consumer:
            lines.extend(
                [
                    "### Use For Consumers",
                    "",
                    *[f"- `{consumer}`" for consumer in term.use_for_consumer],
                    "",
                ]
            )
        if term.avoid_for_consumer:
            lines.extend(
                [
                    "### Avoid For Consumers",
                    "",
                    *[f"- `{consumer}`" for consumer in term.avoid_for_consumer],
                    "",
                ]
            )

    if term.alignments:
        lines.extend(
            [
                "<details>",
                "<summary>Published Alignment Graph</summary>",
                "",
            ]
        )
        lines.extend(
            [
                "<h3>Alignments</h3>",
                *_alignment_html_table(
                    term.alignments, model=model, from_rel=_term_md_rel(term)
                ),
                "",
            ]
        )
        lines.extend(["</details>", ""])

    return Page(_term_md_rel(term), term.label, "\n".join(lines))


def _slice_index(model: DocsModel) -> Page:
    """Render the slice index."""
    lines = [
        "# Slices",
        "",
        "Each slice is a self-contained ontology unit. The manifest is the sole "
        "source of identity, tier, dependencies, profiles, and consumers.",
        "",
        "![Slice dependency map](../diagrams/slices.svg)",
        "",
        "| Slice | Tier | Profiles | Dependencies | Consumers |",
        "|---|---|---|---|---|",
    ]
    for s in sorted(model.slices.values(), key=lambda x: x.name):
        profiles = ", ".join(sorted(s.profiles)) or "-"
        deps = ", ".join(curie(d).replace("gmeow:slices/", "") for d in s.depends_on)
        consumers = "; ".join(s.consumers) or "-"
        lines.append(
            f"| [{s.name}]({s.name}.md) | {s.tier} | {profiles} | "
            f"{deps or '-'} | {_escape_md_cell(consumers)} |"
        )
    lines.append("")
    return Page(Path("slices") / "index.md", "Slices", "\n".join(lines))


def _slice_page(slice_entry: Slice, model: DocsModel) -> Page:
    """Render a slice guide page."""
    guide = slice_entry.path / "docs.md"
    body = guide.read_text(encoding="utf-8") if guide.exists() else ""
    owner = curie(slice_entry.iri)
    local_terms = [t for t in model.terms if t.owner == owner]
    local_linkages = sorted(
        [link for term in local_terms for link in term.linkages],
        key=_link_sort_key,
    )
    page_rel = Path("slices") / f"{slice_entry.name}.md"
    lines = [
        f"# {slice_entry.title or slice_entry.name}",
        "",
        f"- **IRI:** <{slice_entry.iri}>",
        f"- **Tier:** {slice_entry.tier}",
        f"- **Group:** {slice_entry.group}",
        "",
        "## What This Slice Covers",
        "",
        f"This slice owns {len(local_terms)} terms and contributes "
        f"{len(local_linkages)} mapping or projection rows. Use it when its "
        "terms match the native fact you want to preserve; use the linkage "
        "tables to see how those facts leave GMEOW for consumer vocabularies.",
        "",
    ]
    if slice_entry.depends_on:
        dep_links: list[str] = []
        for dep in slice_entry.depends_on:
            dep_name = curie(dep).replace("gmeow:slices/", "")
            if dep_name in model.slices:
                dep_links.append(f"[`gmeow:slices/{dep_name}`]({dep_name}.md)")
            else:
                dep_links.append(f"`{curie(dep)}`")
        lines.extend(
            [
                "## Dependencies",
                "",
                *[f"- {dep}" for dep in dep_links],
                "",
            ]
        )
    if slice_entry.consumers:
        lines.extend(
            [
                "## Consumers",
                "",
                *[f"- {consumer}" for consumer in slice_entry.consumers],
                "",
            ]
        )
    design_docs = model.design_docs_by_slice.get(slice_entry.name, [])
    if design_docs:
        lines.extend(
            [
                "## Design Documents",
                "",
                "This slice includes slice-local design notes. They are authored "
                "beside the slice and rendered into the docs as part of the "
                "same deterministic documentation bundle.",
                "",
                "| Document | Source |",
                "|---|---|",
            ]
        )
        for design_doc in design_docs:
            design_rel = posixpath.relpath(
                design_doc.rel.as_posix(), start=page_rel.parent.as_posix()
            )
            lines.append(
                f"| [{design_doc.title}]({design_rel}) | "
                f"{_repo_source_link(design_doc.path)} |"
            )
        lines.append("")
    lines.extend(
        [
            "## Local Map",
            "",
            f"![{slice_entry.name} map](../diagrams/slices/{slice_entry.name}.svg)",
            "",
        ]
    )
    examples = model.examples_by_slice.get(slice_entry.name, [])
    if examples:
        lines.extend(["## Examples", ""])
        for example in examples:
            lines.extend(_render_example(example, model=model, from_rel=page_rel))
    if local_terms:
        lines.extend(["## Terms", ""])
        grouped: dict[str, list[DocTerm]] = defaultdict(list)
        for term in local_terms:
            grouped[term.category].append(term)
        for category in _CATEGORY_DIRS:
            terms = sorted(grouped.get(category, []), key=lambda t: t.curie)
            if not terms:
                continue
            lines.extend(
                [
                    f"### {_CATEGORY_LABELS[category]}",
                    "",
                    "| Term | Label | Definition |",
                    "|---|---|---|",
                ]
            )
            for term in terms:
                rel = Path("..") / _term_md_rel(term)
                lines.append(
                    f"| {_markdown_link(f'`{term.curie}`', rel)} | "
                    f"{_escape_md_cell(term.label)} | "
                    f"{_escape_md_cell(_short_text(term.definition, limit=160))} |"
                )
            lines.append("")
    if local_linkages:
        profiles = sorted({link.profile for link in local_linkages if link.profile})
        targets = _target_prefixes(link.target for link in local_linkages)
        lines.extend(
            [
                "## Linkages",
                "",
                f"- **Rows:** {len(local_linkages)}",
                "- **Projection profiles:** "
                f"{', '.join(f'`{p}`' for p in profiles) or '-'}",
                "- **External vocabularies:** "
                f"{', '.join(f'`{p}`' for p in targets) or '-'}",
                "",
            ]
        )
        lines.extend(
            _linkage_table(
                local_linkages,
                from_rel=page_rel,
                model=model,
                limit=_MAX_SLICE_LINK_ROWS,
            )
        )
        lines.append("")
    if body:
        lines.extend(["## Guide", "", body])
    return Page(
        page_rel,
        slice_entry.name,
        "\n".join(lines),
    )


def _design_doc_page(design_doc: DocDesignDoc) -> Page:
    """Render a slice-local design note."""
    source = _repo_source_link(design_doc.path)
    slice_rel = posixpath.relpath(
        (Path("slices") / f"{design_doc.slice_name}.md").as_posix(),
        start=design_doc.rel.parent.as_posix(),
    )
    lines = [
        f"# {design_doc.title}",
        "",
        f"- **Slice:** [{design_doc.slice_name}]({slice_rel})",
        f"- **Source:** {source}",
        "",
        design_doc.text,
        "",
    ]
    return Page(design_doc.rel, design_doc.title, "\n".join(lines))


def _design_doc_pages(model: DocsModel) -> list[Page]:
    """Render all discovered slice-local design notes."""
    return [
        _design_doc_page(design_doc)
        for slice_name in sorted(model.design_docs_by_slice)
        for design_doc in model.design_docs_by_slice[slice_name]
    ]


def _profile_pages(model: DocsModel) -> list[Page]:
    """Render profile index and pages."""
    groups: dict[str, list[Slice]] = defaultdict(list)
    for s in model.slices.values():
        for profile in s.profiles:
            groups[profile].append(s)
    groups["core"] = [s for s in model.slices.values() if s.is_core]
    groups["full"] = list(model.slices.values())
    pages = [
        Page(
            Path("profiles") / "index.md",
            "Profiles",
            "\n".join(
                [
                    "# Profiles",
                    "",
                    "Profiles aggregate slices into named consumption surfaces.",
                    "",
                    *[
                        f"- [{profile}]({_safe_filename(profile)}.md): "
                        f"{len(groups[profile])} slices"
                        for profile in sorted(groups)
                    ],
                    "",
                ]
            ),
        )
    ]
    for profile, slices in sorted(groups.items()):
        lines = [f"# {profile}", "", f"{len(slices)} slices.", ""]
        for s in sorted(slices, key=lambda x: x.name):
            lines.append(f"- [{s.name}](../slices/{s.name}.md) - {s.title or s.name}")
        lines.append("")
        profile_links = _unique_projection_bindings(
            link for link in model.linkages if link.profile == profile
        )
        if profile_links:
            targets = _target_prefixes(link.target for link in profile_links)
            lossy = sum(1 for link in profile_links if link.lossy_drops)
            lines.extend(
                [
                    "## Projection Linkages",
                    "",
                    f"- **Projection bindings:** {len(profile_links)}",
                    f"- **Bindings with lossy drops:** {lossy}",
                    "- **External vocabularies:** "
                    f"{', '.join(f'`{p}`' for p in targets) or '-'}",
                    "",
                ]
            )
            lines.extend(
                _linkage_table(
                    profile_links,
                    from_rel=Path("profiles") / f"{_safe_filename(profile)}.md",
                    model=model,
                    limit=_MAX_SLICE_LINK_ROWS,
                )
            )
            lines.append("")
        pages.append(
            Page(
                Path("profiles") / f"{_safe_filename(profile)}.md",
                profile,
                "\n".join(lines),
            )
        )
    return pages


def _linkages_page(model: DocsModel) -> Page:
    """Render the top-level linkage/adoption page."""
    projection_links = _unique_projection_bindings(model.linkages)
    equivalence_links = [link for link in model.linkages if link.kind == "equivalence"]
    profile_counts: dict[str, int] = defaultdict(int)
    profile_lossy: dict[str, int] = defaultdict(int)
    profile_targets: dict[str, set[str]] = defaultdict(set)
    prefix_counts: dict[str, int] = defaultdict(int)

    for link in projection_links:
        profile_counts[link.profile] += 1
        if link.lossy_drops:
            profile_lossy[link.profile] += 1
        for prefix in _target_prefixes([link.target]):
            profile_targets[link.profile].add(prefix)
            prefix_counts[prefix] += 1
    for link in equivalence_links:
        for prefix in _target_prefixes([link.target]):
            prefix_counts[prefix] += 1

    sssom_rows = sum(s.equivalence_count for s in model.mapping_sets)
    lines = [
        "# Linkages",
        "",
        "This page is generated from the canonical mapping DSL under "
        "`dsl/mappings/` plus slice-local `mappings/` directories. Generated "
        "SSSOM, EDOAL, FnO, and SPARQL artifacts are downstream views.",
        "",
        f"- **SSSOM term-equivalence rows:** {sssom_rows}",
        f"- **SSSOM mapping sets:** {len(model.mapping_sets)}",
        f"- **Projection bindings:** {len(projection_links)}",
        f"- **Documented GMEOW-term linkage rows:** {len(model.linkages)}",
        "",
        "## Projection Profiles",
        "",
        "| Profile | Bindings | Lossy Bindings | Round Trip | Target Vocabularies |",
        "|---|---:|---:|---|---|",
    ]
    for profile in sorted(profile_counts):
        vocabularies = ", ".join(f"`{p}`" for p in sorted(profile_targets[profile]))
        round_trip = "lossy" if profile_lossy[profile] else "not guaranteed"
        lines.append(
            f"| `{profile}` | {profile_counts[profile]} | "
            f"{profile_lossy[profile]} | {round_trip} | {vocabularies or '-'} |"
        )

    target_groups: dict[str, list[tuple[str, int]]] = defaultdict(list)
    targets = _external_targets()
    for prefix, count in prefix_counts.items():
        kind = targets.get(prefix, ("", "", "", "other"))[3]
        target_groups[kind].append((prefix, count))
    lines.extend(["", "## Adoption Targets", ""])
    for kind, rows in sorted(target_groups.items()):
        lines.extend([f"### {kind.replace('_', ' ').title()}", ""])
        for prefix, count in sorted(rows, key=lambda item: (-item[1], item[0])):
            if prefix in targets:
                name = targets[prefix][0]
                href = _external_ontologies_rel(
                    Path("linkages") / "index.md", _external_target_anchor(prefix)
                )
                lines.append(f"- [{name}]({href}) (`{prefix}`): {count} linkage rows")
            else:
                lines.append(f"- `{prefix}`: {count} linkage rows")
        lines.append("")

    lines.extend(
        [
            "",
            "## SSSOM Mapping Sets",
            "",
            "| File | Rows | License | Set Id | Comment |",
            "|---|---:|---|---|---|",
        ]
    )
    for mapping_set in model.mapping_sets:
        lines.append(
            f"| `{mapping_set.file}` | {mapping_set.equivalence_count} | "
            f"`{mapping_set.license or '-'}` | "
            f"`{mapping_set.set_id or '-'}` | "
            f"{_escape_md_cell(_short_text(mapping_set.comment) or '-')} |"
        )

    lines.extend(
        [
            "",
            "## External Vocabulary Coverage",
            "",
            "| Prefix | Linkage Rows |",
            "|---|---:|",
        ]
    )
    for prefix, count in sorted(
        prefix_counts.items(), key=lambda item: (-item[1], item[0])
    ):
        lines.append(f"| `{prefix}` | {count} |")

    lines.extend(
        [
            "",
            "## Sample Linkage Rows",
            "",
            "<details>",
            "<summary>Show sample rows</summary>",
            "",
        ]
    )
    lines.extend(
        _linkage_table(
            sorted(model.linkages, key=_link_sort_key),
            from_rel=Path("linkages") / "index.md",
            model=model,
            limit=80,
        )
    )
    lines.extend(["", "</details>", ""])
    return Page(Path("linkages") / "index.md", "Linkages", "\n".join(lines))


def _links_for_target(model: DocsModel, prefix: str) -> list[DocLinkage]:
    """Return linkage rows that mention one external target prefix."""
    return [
        link for link in model.linkages if prefix in _target_prefixes([link.target])
    ]


def _adoption_target_prefixes(model: DocsModel) -> list[str]:
    """Return known external prefixes worth rendering as adoption pages."""
    known = set(PREFIXES) | set(_external_targets())
    return sorted(
        {
            prefix
            for link in model.linkages
            for prefix in _target_prefixes([link.target])
            if prefix in known and prefix not in {"gmeow", "wd", "wdt"}
        }
    )


def _adoption_index(model: DocsModel) -> Page:
    """Render the adoption target index."""
    targets = _external_targets()
    rows: list[tuple[str, str, str, str, str, int, int]] = []
    for prefix in _adoption_target_prefixes(model):
        links = _links_for_target(model, prefix)
        if not links:
            continue
        name, namespace, license_value, kind = targets.get(
            prefix, (prefix, PREFIXES.get(prefix, ""), "Unknown", "other")
        )
        projections = sum(1 for link in links if link.kind == "projection")
        equivalences = sum(1 for link in links if link.kind == "equivalence")
        rows.append(
            (prefix, name, namespace, license_value, kind, projections, equivalences)
        )
    rel = Path("adoption") / "index.md"
    lines = [
        "# Adoption Targets",
        "",
        "These pages explain how GMEOW links to external vocabularies and "
        "projection surfaces. They are generated from the canonical mapping DSL "
        "and the external ontology catalog.",
        "",
        "| Target | Kind | Projection Rows | Equivalence Rows | License | Namespace |",
        "|---|---|---:|---:|---|---|",
    ]
    for (
        prefix,
        name,
        namespace,
        license_value,
        kind,
        projections,
        equivalences,
    ) in sorted(rows, key=lambda row: (-row[5] - row[6], row[0])):
        lines.append(
            f"| [{name}]({_safe_filename(prefix).lower()}.md) (`{prefix}`) | "
            f"{kind} | {projections} | {equivalences} | "
            f"`{license_value or '-'}` | <{namespace}> |"
        )
    lines.append("")
    return Page(rel, "Adoption Targets", "\n".join(lines))


def _adoption_target_page(prefix: str, model: DocsModel) -> Page:
    """Render one adoption target page from linkage rows."""
    links = sorted(_links_for_target(model, prefix), key=_link_sort_key)
    targets = _external_targets()
    name, namespace, license_value, kind = targets.get(
        prefix, (prefix, PREFIXES.get(prefix, ""), "Unknown", "other")
    )
    projections = [link for link in links if link.kind == "projection"]
    equivalences = [link for link in links if link.kind == "equivalence"]
    profiles = sorted({link.profile for link in projections if link.profile})
    lossy = [link for link in projections if link.lossy_drops]
    source_terms = sorted({link.source for link in links})
    slices = sorted(
        {
            model.terms_by_curie[source].owner.replace("gmeow:slices/", "")
            for source in source_terms
            if source in model.terms_by_curie and model.terms_by_curie[source].owner
        }
    )
    rel = Path("adoption") / f"{_safe_filename(prefix).lower()}.md"
    external_href = _external_ontologies_rel(rel, _external_target_anchor(prefix))
    lines = [
        f"# {name}",
        "",
        f"- **Prefix:** `{prefix}`",
        f"- **Kind:** {kind}",
        f"- **License:** `{license_value or '-'}`",
        f"- **Namespace / website:** <{namespace}>",
        f"- **External catalog:** [open catalog entry]({external_href})",
        "",
        "## Coverage",
        "",
        f"- **GMEOW source terms:** {len(source_terms)}",
        f"- **Projection rows:** {len(projections)}",
        f"- **Equivalence rows:** {len(equivalences)}",
        "- **Profiles:** " + (", ".join(f"`{profile}`" for profile in profiles) or "-"),
        "- **Slices:** "
        + (
            ", ".join(
                _relative_page_link(
                    slice_name, Path("slices") / f"{slice_name}.md", rel
                )
                for slice_name in slices
            )
            or "-"
        ),
        "",
        "## How To Use This Target",
        "",
        "Use native GMEOW terms as the authoring surface, then inspect the "
        "projection rows below to see which facts can leave GMEOW for this "
        "consumer vocabulary. Treat lossy notes as adoption warnings, not as "
        "implementation details.",
        "",
    ]
    if lossy:
        lines.extend(
            [
                "## Loss Notes",
                "",
                "| Profile | Source | Dropped Structure |",
                "|---|---|---|",
            ]
        )
        for link in lossy[:40]:
            lines.append(
                f"| `{link.profile or '-'}` | "
                f"{_curie_link_from(link.source, model, rel)} | "
                f"{_escape_md_cell('; '.join(link.lossy_drops))} |"
            )
        if len(lossy) > 40:
            lines.append(f"| ... | ... | {len(lossy) - 40} more lossy rows |")
        lines.append("")
    if source_terms:
        lines.extend(
            ["## Source Terms", "", "| Term | Label | Slice |", "|---|---|---|"]
        )
        for source in source_terms[:80]:
            term = model.terms_by_curie.get(source)
            if term is None:
                continue
            slice_name = term.owner.replace("gmeow:slices/", "")
            lines.append(
                f"| {_curie_link_from(source, model, rel)} | "
                f"{_escape_md_cell(term.label)} | `{slice_name or '-'}` |"
            )
        if len(source_terms) > 80:
            lines.append(f"| ... | ... | {len(source_terms) - 80} more terms |")
        lines.append("")
    lines.extend(["## Mapping Rows", ""])
    lines.extend(_linkage_table(links, from_rel=rel, model=model, limit=120))
    lines.append("")
    return Page(rel, name, "\n".join(lines))


def _adoption_pages(model: DocsModel) -> list[Page]:
    """Render adoption target index and per-target pages."""
    prefixes = _adoption_target_prefixes(model)
    return [
        _adoption_index(model),
        *[_adoption_target_page(prefix, model) for prefix in prefixes],
    ]


def _external_targets() -> dict[str, tuple[str, str, str, str]]:
    """Return all external ontology targets surfaced in documentation."""
    targets = {
        key: (target.name, target.namespace, target.license, target.kind)
        for key, target in ALIGNMENT_TARGETS.items()
    }
    targets.update(_EXTERNAL_TARGET_EXTRAS)
    return targets


def _external_ontologies_page() -> Page:
    """Render the external ontology and vocabulary catalog."""
    lines = [
        "# External Ontologies",
        "",
        "GMEOW links to external ontologies and vocabularies by reference. "
        "This catalog explains what each target is, why it appears in the docs, "
        "its license, and where to inspect the upstream vocabulary.",
        "",
        "| Target | Kind | License | Website / Namespace | Description |",
        "|---|---|---|---|---|",
    ]
    for key, (name, namespace, license_id, kind) in sorted(
        _external_targets().items(), key=lambda item: item[1][0].casefold()
    ):
        description = _EXTERNAL_TARGET_DESCRIPTIONS.get(
            key,
            "External vocabulary or concept scheme used as a linkage or "
            "projection target.",
        )
        anchor = f'<span id="{_external_target_anchor(key)}"></span>'
        lines.append(
            f"| {anchor}{name} (`{key}`) | {kind} | `{license_id}` | "
            f"<{namespace}> | {_escape_md_cell(description)} |"
        )
    lines.append("")
    return Page(
        Path("external") / "ontologies.md",
        "External Ontologies",
        "\n".join(lines),
    )


def _external_terms_page() -> Page:
    """Render plain-language explanations for external ontology terms."""
    lines = [
        "# External Terms",
        "",
        "These short explanations translate formal external ontology terms that "
        "appear in GMEOW documentation. They are adoption aids, not replacements "
        "for the upstream definitions.",
        "",
        "## gUFO",
        "",
        "| Term | Plain-language role | Formal IRI | Notes |",
        "|---|---|---|---|",
    ]
    for local, (label, description) in sorted(_GUFO_TERM_HELP.items()):
        iri = PREFIXES["gufo"] + local
        lines.append(
            f'| <span id="gufo-{local.lower()}"></span>`gufo:{local}` | '
            f"{label} | <{iri}> | {_escape_md_cell(description)} |"
        )
    lines.append("")
    return Page(Path("external") / "terms.md", "External Terms", "\n".join(lines))


def _concern_index(concerns: list[DocConcern]) -> Page:
    """Render the concern index."""
    lines = [
        "# Cross-Cutting Concerns",
        "",
        "These pages are generated from `gmeow:docsConcern` annotations in "
        "the ontology.",
        "",
        "![Concern map](../diagrams/concerns.svg)",
        "",
    ]
    for concern in concerns:
        lines.append(f"- [{concern.label}]({concern.filename}) - {concern.definition}")
    lines.append("")
    return Page(
        Path("concerns") / "index.md",
        "Cross-Cutting Concerns",
        "\n".join(lines),
    )


def _concern_page(concern: DocConcern) -> Page:
    """Render one cross-cutting concern page."""
    lines = [
        f"# {concern.label}",
        "",
        f"- **IRI:** <{concern.iri}>",
        f"- **CURIE:** `{concern.curie}`",
        "",
        concern.definition,
        "",
        f"![{concern.label} map]"
        f"(../diagrams/concerns/{_safe_filename(concern.curie)}.svg)",
        "",
    ]
    if concern.slices:
        lines.extend(["## Participating Slices", ""])
        for s in concern.slices:
            lines.append(f"- [{s.name}](../slices/{s.name}.md) - {s.title or s.name}")
        lines.append("")
    if concern.terms:
        lines.extend(
            ["## Terms", "", "| Term | Category | Definition |", "|---|---|---|"]
        )
        for term in sorted(concern.terms, key=lambda t: t.curie):
            lines.append(
                f"| [`{term.curie}`](../{_term_md_rel(term).as_posix()}) | "
                f"{term.category} | {_escape_md_cell(term.definition[:180])} |"
            )
        lines.append("")
    return Page(Path("concerns") / concern.filename, concern.label, "\n".join(lines))


def _statements_page(model: DocsModel) -> Page:
    """Render the RDF 1.2 statement layer summary."""
    annotations = model.view.annotations()
    reifiers = model.view.reifiers()
    pred_counts: dict[str, int] = {}
    for _, p_tid, _ in annotations:
        pred = curie(model.view.lex(p_tid))
        pred_counts[pred] = pred_counts.get(pred, 0) + 1

    lines = [
        "# RDF 1.2 Statement Layer",
        "",
        "GMEOW authors statement-level metadata as native RDF 1.2/RDF-star and "
        "generates OWL axiom annotations only as a reasoning compatibility form.",
        "",
        f"- **Reified statements:** {len(reifiers)}",
        f"- **Statement annotations:** {len(annotations)}",
        f"- **RDF 1.2 artifact:** `{STATEMENT_RDF12_FILE.relative_to(PROJECT_ROOT)}`",
        "",
        "## Annotation Predicates",
        "",
    ]
    for pred, count in sorted(
        pred_counts.items(), key=lambda item: (-item[1], item[0])
    ):
        lines.append(f"- `{pred}`: {count}")
    lines.extend(
        [
            "",
            "<details>",
            "<summary>How To Read A Reifier</summary>",
            "",
            "A statement reifier names a quoted base triple and carries "
            "metadata such as "
            "`gmeow:confidence`, `gmeow:accordingTo`, `gmeow:validFrom`, and "
            "`gmeow:wasAttributedTo`. The metadata qualifies the statement, not the "
            "subject or object globally.",
            "",
            "</details>",
            "",
        ]
    )
    return Page(
        Path("statements") / "index.md",
        "RDF 1.2 Statement Layer",
        "\n".join(lines),
    )


def _four_boxes_page(model: DocsModel) -> Page:
    """Render the top-level four-box doctrine page."""
    rel = Path("four-boxes") / "index.md"
    source_text = _FOUR_BOXES_SOURCE.read_text(encoding="utf-8").rstrip()
    # Drop the source's leading SPDX/HTML comments and first heading so the
    # generated page keeps a single H1.
    body = re.sub(r"^(<!--.*?-->\s*)+", "", source_text, flags=re.DOTALL)
    body = re.sub(r"^#\s+[^\n]*\n+", "", body)

    nav = [
        "",
        "## Box Role Landing Pages",
        "",
        *[f"- {_box_role_link(role, model, rel)}" for role in _BOX_ROLE_CURIES],
        "",
    ]
    markdown = f"# {_FOUR_BOXES_TITLE}\n\n{body}\n" + "\n".join(nav)
    return Page(rel, "Four Boxes", markdown)


def _box_role_pages(model: DocsModel) -> list[Page]:
    """Render one landing page for each graph-box role."""
    pages: list[Page] = []
    for role_curie in _BOX_ROLE_CURIES:
        term = model.terms_by_curie.get(role_curie)
        slug = _box_role_slug(role_curie)
        rel = Path("reference") / "boxes" / f"{slug}.md"
        title = term.label if term is not None else role_curie
        doctrine_link = _relative_page_link(
            "Four Boxes doctrine", Path("four-boxes") / "index.md", rel
        )
        members = sorted(
            (t for t in model.terms if role_curie in t.box_roles),
            key=lambda t: t.curie,
        )
        lines = [
            f"# {title}",
            "",
            f"- **CURIE:** `{role_curie}`",
            f"- **Doctrine:** {doctrine_link}",
            "",
        ]
        if term is not None and term.definition:
            lines.extend([term.definition, ""])
        lines.append("## Terms")
        lines.append("")
        if members:
            lines.append("| Term | Category | Label | Definition |")
            lines.append("|---|---|---|---|")
            for member in members:
                term_link = _curie_link_from(member.curie, model, rel)
                lines.append(
                    f"| {term_link} | {member.category} | "
                    f"{_escape_md_cell(member.label)} | "
                    f"{_escape_md_cell(_short_text(member.definition, limit=180))} |"
                )
        else:
            lines.append("No terms are annotated with this role yet.")
        lines.append("")
        pages.append(Page(rel, title, "\n".join(lines)))
    return pages


def _all_pages(model: DocsModel) -> list[Page]:
    """Render every Markdown page."""
    pages = [
        _landing(model),
        _getting_started(),
        _about_page(model),
        _changelog_page(model),
        _visualization_page(),
        _quality_page(),
        _learning_paths_index(model),
        _recipes_index(model),
        _examples_index(model),
        _references_page(),
        _reference_index(model),
        *_adoption_pages(model),
        _linkages_page(model),
        _external_ontologies_page(),
        _external_terms_page(),
    ]
    pages.extend(_recipe_page(recipe, model) for recipe in model.recipes)
    by_category: dict[str, list[DocTerm]] = defaultdict(list)
    for term in model.terms:
        by_category[term.category].append(term)
    for category in _CATEGORY_DIRS:
        pages.append(
            _category_index(
                category,
                sorted(by_category[category], key=lambda t: t.curie),
            )
        )
    pages.extend(_term_page(term, model) for term in model.terms)
    pages.append(_slice_index(model))
    pages.extend(
        _slice_page(s, model)
        for s in sorted(model.slices.values(), key=lambda x: x.name)
    )
    pages.extend(_design_doc_pages(model))
    pages.extend(_profile_pages(model))
    pages.append(_concern_index(model.concerns))
    pages.extend(_concern_page(concern) for concern in model.concerns)
    pages.append(_statements_page(model))
    pages.append(_four_boxes_page(model))
    pages.extend(_box_role_pages(model))
    return pages


def _svg_text(x: int, y: int, text: str, *, size: int = 13) -> str:
    """Return an SVG text element."""
    return (
        f'<text x="{x}" y="{y}" font-size="{size}" font-family="sans-serif" '
        f'fill="#171717">{html.escape(text)}</text>'
    )


def _svg_box(x: int, y: int, w: int, h: int, label: str, subtitle: str = "") -> str:
    """Return a labeled SVG box."""
    parts = [
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" '
        'fill="#ffffff" stroke="#0f766e" stroke-width="1.5"/>',
        _svg_text(x + 10, y + 24, label, size=14),
    ]
    if subtitle:
        parts.append(_svg_text(x + 10, y + 44, subtitle, size=11))
    return "\n".join(parts)


def _svg_shell(width: int, height: int, body: str) -> str:
    """Return a full SVG document."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"
  viewBox="0 0 {width} {height}" role="img">
<rect width="100%" height="100%" fill="#fbfaf7"/>
{body}
</svg>
"""


def _slice_dependency_svg(slices: list[Slice]) -> str:
    """Render a bounded global slice dependency SVG."""
    cols = 4
    box_w, box_h = 220, 64
    gap_x, gap_y = 32, 30
    rows = (len(slices) + cols - 1) // cols
    width = cols * box_w + (cols + 1) * gap_x
    height = rows * (box_h + gap_y) + gap_y
    by_iri = {s.iri: i for i, s in enumerate(slices)}
    coords: dict[str, tuple[int, int]] = {}
    for i, s in enumerate(slices):
        x = gap_x + (i % cols) * (box_w + gap_x)
        y = gap_y + (i // cols) * (box_h + gap_y)
        coords[s.iri] = (x, y)
    lines: list[str] = [
        '<defs><marker id="arrow" markerWidth="10" markerHeight="8" '
        'refX="9" refY="4" orient="auto"><path d="M0,0 L10,4 L0,8 Z" '
        'fill="#9a3412"/></marker></defs>'
    ]
    for s in slices:
        sx, sy = coords[s.iri]
        for dep in s.depends_on:
            if dep not in by_iri:
                continue
            dx, dy = coords[dep]
            lines.append(
                f'<line x1="{sx + box_w / 2:.0f}" y1="{sy}" '
                f'x2="{dx + box_w / 2:.0f}" y2="{dy + box_h}" '
                'stroke="#9a3412" stroke-width="1" marker-end="url(#arrow)" '
                'opacity="0.45"/>'
            )
    for s in slices:
        x, y = coords[s.iri]
        lines.append(_svg_box(x, y, box_w, box_h, s.name, s.tier))
    return _svg_shell(width, height, "\n".join(lines))


def _slice_local_svg(slice_entry: Slice, terms: list[DocTerm]) -> str:
    """Render a local slice SVG."""
    local_terms = terms[:12]
    height = 120 + max(1, len(local_terms)) * 36
    lines = [_svg_box(24, 24, 300, 60, slice_entry.name, slice_entry.tier)]
    y = 112
    for term in local_terms:
        lines.append(_svg_box(64, y, 360, 28, term.curie, term.category))
        lines.append(
            f'<line x1="174" y1="84" x2="244" y2="{y}" '
            'stroke="#0f766e" opacity="0.35"/>'
        )
        y += 36
    return _svg_shell(470, height, "\n".join(lines))


def _concern_svg(concern: DocConcern) -> str:
    """Render a concern-specific SVG."""
    lines = [_svg_box(120, 24, 320, 62, concern.label, concern.curie)]
    y = 128
    for term in sorted(concern.terms, key=lambda t: t.curie)[:14]:
        lines.append(_svg_box(36, y, 390, 30, term.curie, term.category))
        lines.append(
            f'<line x1="230" y1="86" x2="230" y2="{y}" '
            'stroke="#0f766e" opacity="0.35"/>'
        )
        y += 42
    if not concern.terms:
        lines.append(_svg_text(54, 136, "No term annotations are present yet."))
    return _svg_shell(500, max(190, y + 24), "\n".join(lines))


def _concerns_overview_svg(concerns: list[DocConcern]) -> str:
    """Render a compact concern overview SVG."""
    lines: list[str] = []
    for i, concern in enumerate(concerns):
        x = 30 + (i % 2) * 360
        y = 28 + (i // 2) * 82
        lines.append(
            _svg_box(x, y, 320, 56, concern.label, f"{len(concern.terms)} terms")
        )
    rows = (len(concerns) + 1) // 2
    return _svg_shell(740, max(120, rows * 82 + 32), "\n".join(lines))


def _write_diagrams(writer: _Writer, model: DocsModel) -> None:
    """Write all SVG diagrams."""
    slices = sorted(model.slices.values(), key=lambda s: s.name)
    writer.write_svg(Path("diagrams") / "slices.svg", _slice_dependency_svg(slices))
    owner_to_terms: dict[str, list[DocTerm]] = defaultdict(list)
    for term in model.terms:
        owner_to_terms[term.owner].append(term)
    for s in slices:
        owner = curie(s.iri)
        writer.write_svg(
            Path("diagrams") / "slices" / f"{s.name}.svg",
            _slice_local_svg(
                s,
                sorted(owner_to_terms.get(owner, []), key=lambda t: t.curie),
            ),
        )
    writer.write_svg(
        Path("diagrams") / "concerns.svg", _concerns_overview_svg(model.concerns)
    )
    for concern in model.concerns:
        writer.write_svg(
            Path("diagrams") / "concerns" / f"{_safe_filename(concern.curie)}.svg",
            _concern_svg(concern),
        )


def _write_term_aliases(writer: _Writer, model: DocsModel) -> None:
    """Write term aliases for slash-namespace dereferencing."""
    exact_aliases = _exact_term_aliases(model.terms)
    seen_casefolded: dict[str, str] = {}
    for term in model.terms:
        source = writer.site / _term_md_rel(term).with_suffix("") / "index.html"
        target = writer.site / _term_alias_path(term, exact_aliases)
        if source.exists():
            target_key = target.relative_to(writer.site).as_posix().casefold()
            if prior := seen_casefolded.get(target_key):
                msg = f"case-conflicting term aliases: {prior} and {term.curie}"
                raise ValueError(msg)
            seen_casefolded[target_key] = term.curie
            target.parent.mkdir(parents=True, exist_ok=True)
            href = posixpath.relpath(
                source.relative_to(writer.site).as_posix(),
                start=target.parent.relative_to(writer.site).as_posix(),
            )
            href = _clean_directory_index(href)
            target.write_text(
                writer.banner() + _term_alias_html(term, href),
                encoding="utf-8",
            )


def _term_alias_html(term: DocTerm, href: str) -> str:
    """Return a tiny static HTML alias page for slash-namespace term IRIs."""
    title = html.escape(term.curie)
    safe_href = html.escape(href, quote=True)
    label = html.escape(term.label)
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta http-equiv="refresh" content="0; url={safe_href}">
  <link rel="canonical" href="{safe_href}">
  <title>{title} - GMEOW term</title>
</head>
<body>
  <main>
    <h1>{label}</h1>
    <p><a href="{safe_href}">Open the canonical reference page.</a></p>
  </main>
</body>
</html>
"""


def _search_index(model: DocsModel) -> list[dict[str, object]]:
    """Return a deterministic static search index."""
    rows: list[dict[str, object]] = [
        {
            "kind": "references",
            "title": "References",
            "path": _site_rel_for_md(Path("references") / "index.md").as_posix(),
            "summary": "Generated bibliography from metadata/references.ttl.",
            "keywords": [
                "bibliography",
                "citation ledger",
                "metadata/references.ttl",
                "references.bib",
                "references.csl.json",
            ],
        },
        {
            "kind": "doctrine",
            "title": "Four Boxes",
            "path": _site_rel_for_md(Path("four-boxes") / "index.md").as_posix(),
            "summary": ("ABox, TBox, RBox, and CBox doctrine for GMEOW graph layers."),
            "keywords": [
                "four boxes",
                "abox",
                "tbox",
                "rbox",
                "cbox",
                "gmeow:graphBoxRole",
            ],
        },
    ]
    for role_curie in _BOX_ROLE_CURIES:
        term = model.terms_by_curie.get(role_curie)
        slug = _box_role_slug(role_curie)
        rel = Path("reference") / "boxes" / f"{slug}.md"
        rows.append(
            {
                "kind": "box-role",
                "title": term.label if term is not None else role_curie,
                "curie": role_curie,
                "path": _site_rel_for_md(rel).as_posix(),
                "summary": _short_text(
                    term.definition if term is not None else "", limit=320
                ),
                "keywords": sorted(
                    {
                        role_curie,
                        slug,
                        "graph box role",
                        *(term.label.split() if term is not None else []),
                    }
                ),
            }
        )
    for term in model.terms:
        rows.append(
            {
                "kind": f"term:{term.category}",
                "title": term.label,
                "curie": term.curie,
                "path": _site_rel_for_md(_term_md_rel(term)).as_posix(),
                "slice": term.owner.replace("gmeow:slices/", ""),
                "summary": _short_text(term.definition, limit=320),
                "keywords": sorted(
                    {
                        term.curie,
                        term.label,
                        term.owner,
                        *term.parents,
                        *term.types,
                        *term.box_roles,
                        *term.domain.split(),
                        *term.range.split(),
                    }
                ),
            }
        )
    for slice_entry in sorted(model.slices.values(), key=lambda s: s.name):
        example_count = len(model.examples_by_slice.get(slice_entry.name, []))
        rows.append(
            {
                "kind": "slice",
                "title": slice_entry.title or slice_entry.name,
                "path": _site_rel_for_md(
                    Path("slices") / f"{slice_entry.name}.md"
                ).as_posix(),
                "slice": slice_entry.name,
                "summary": (f"{slice_entry.tier} slice with {example_count} examples"),
                "keywords": sorted(
                    {
                        slice_entry.name,
                        slice_entry.tier,
                        *slice_entry.profiles,
                        *[curie(dep) for dep in slice_entry.depends_on],
                    }
                ),
            }
        )
    for recipe in model.recipes:
        rows.append(
            {
                "kind": "recipe",
                "title": recipe.title,
                "path": _site_rel_for_md(
                    Path("recipes") / f"{recipe.slug}.md"
                ).as_posix(),
                "recipe": recipe.slug,
                "summary": recipe.goal,
                "keywords": sorted({recipe.slug, *recipe.term_curies}),
            }
        )
    for path in model.learning_paths:
        rows.append(
            {
                "kind": "learning-path",
                "title": path.title,
                "path": _site_rel_for_md(Path("learning-paths") / "index.md").as_posix()
                + f"#{path.slug}",
                "summary": path.goal,
                "keywords": sorted(
                    {
                        path.slug,
                        path.audience,
                        *path.recipe_slugs,
                        *[p.as_posix() for p in path.example_paths],
                        *path.term_curies,
                        *path.adoption_targets,
                    }
                ),
            }
        )
    for prefix in _adoption_target_prefixes(model):
        links = _links_for_target(model, prefix)
        name, _, _, kind = _external_targets().get(prefix, (prefix, "", "", "other"))
        rows.append(
            {
                "kind": "adoption-target",
                "title": name,
                "path": _site_rel_for_md(
                    Path("adoption") / f"{_safe_filename(prefix).lower()}.md"
                ).as_posix(),
                "summary": f"{len(links)} linkage rows for {prefix}.",
                "keywords": sorted(
                    {prefix, name, kind, *[link.profile for link in links]}
                ),
            }
        )
    for slice_name, design_docs in sorted(model.design_docs_by_slice.items()):
        for design_doc in design_docs:
            rows.append(
                {
                    "kind": "slice-design",
                    "title": design_doc.title,
                    "path": _site_rel_for_md(design_doc.rel).as_posix(),
                    "slice": slice_name,
                    "summary": _short_text(design_doc.text, limit=320),
                    "keywords": sorted({slice_name, design_doc.path.as_posix()}),
                }
            )
    for example in model.examples:
        rows.append(
            {
                "kind": "example",
                "title": example.title,
                "path": (
                    _site_rel_for_md(Path("examples") / "index.md").as_posix()
                    + f"#{_example_anchor(example)}"
                ),
                "slice": example.slice_name,
                "summary": (
                    f"Canonical Turtle example for the {example.slice_name} slice."
                ),
                "keywords": sorted({example.path.as_posix(), *example.terms}),
            }
        )
    return sorted(rows, key=lambda row: (str(row["kind"]), str(row["title"])))


def _llms_docs_text(model: DocsModel) -> str:
    """Return a compact plain-text docs index for offline search and agents."""
    lines = [
        f"# {model.title} documentation index",
        "",
        model.description,
        "",
        "## Recipes",
    ]
    for recipe in model.recipes:
        lines.append(
            f"- {recipe.slug}: {recipe.title}; terms " + ", ".join(recipe.term_curies)
        )
    lines.extend(["", "## Learning Paths"])
    for path in model.learning_paths:
        lines.append(
            f"- {path.slug}: {path.title}; audience {path.audience}; terms "
            f"{', '.join(path.term_curies)}"
        )
    lines.extend(["", "## Adoption Targets"])
    for prefix in _adoption_target_prefixes(model):
        links = _links_for_target(model, prefix)
        name = _external_targets().get(prefix, (prefix, "", "", ""))[0]
        lines.append(f"- {prefix}: {name}; linkage rows {len(links)}")
    lines.extend(["", "## Four Boxes"])
    for role_curie in _BOX_ROLE_CURIES:
        term = model.terms_by_curie.get(role_curie)
        label = term.label if term is not None else role_curie
        count = sum(1 for t in model.terms if role_curie in t.box_roles)
        lines.append(f"- {role_curie}: {label}; {count} annotated terms")
    lines.extend(["", "## References"])
    lines.append(
        "- references: generated bibliography from metadata/references.ttl; "
        "exports generated/references/references.md, references.csl.json, "
        "references.bib"
    )
    lines.extend(["", "## Slices"])
    for slice_entry in sorted(model.slices.values(), key=lambda s: s.name):
        lines.append(
            f"- {slice_entry.name}: {slice_entry.title or slice_entry.name}; "
            f"tier {slice_entry.tier}; profiles "
            f"{', '.join(sorted(slice_entry.profiles)) or '-'}"
        )
    lines.extend(["", "## Slice Design Documents"])
    for slice_name, design_docs in sorted(model.design_docs_by_slice.items()):
        for design_doc in design_docs:
            lines.append(
                f"- {slice_name}: {design_doc.title}; source "
                f"{design_doc.path.as_posix()}"
            )
    lines.extend(["", "## Examples"])
    for example in sorted(model.examples, key=lambda ex: (ex.slice_name, ex.path)):
        lines.append(
            f"- {example.path.as_posix()}: {example.title}; slice "
            f"{example.slice_name}; terms {', '.join(example.terms[:12]) or '-'}"
        )
    lines.extend(["", "## Terms"])
    for term in model.terms:
        lines.append(
            f"- {term.curie} ({term.category}; {term.owner or 'unowned'}): "
            f"{_short_text(term.definition, limit=240)}"
        )
    return "\n".join(lines)


def _write_static_indexes(writer: _Writer, model: DocsModel) -> None:
    """Write static search/index aids."""
    writer.write_json_asset(
        Path("search-index.json"), _search_index(model), site_only=False
    )
    writer.write_text_asset(
        Path("llms-docs.txt"), _llms_docs_text(model), site_only=False
    )


def build_ontology_docs(
    outdir: Path, *, source_hash: str = "", gts_path: Path | None = None
) -> None:
    """Render the complete ontology documentation tree.

    Args:
        outdir: Destination directory, normally ``dist/ontology-docs/``.
        source_hash: Source hash supplied by the generator framework. The docs
            writer intentionally does not embed it in each output file.
        gts_path: Optional GTS snapshot path for tests.
    """
    model = _load_model(gts_path)
    writer = _Writer(outdir, source_hash=source_hash)
    tag_map = {k.lower(): v for k, v in model.view.tag_map().items()}
    pages = [
        Page(
            page.rel,
            page.title,
            _public_markdown_text(
                page.markdown, tag_map, model=model, from_rel=page.rel
            ),
        )
        for page in _all_pages(model)
    ]
    for page in pages:
        writer.write_markdown(page)
        writer.write_site_page(page)
    _write_diagrams(writer, model)
    _write_term_aliases(writer, model)
    _write_static_indexes(writer, model)
    writer.write_simple_css()
    writer.write_css()
    writer.write_favicon()


_ONTOLOGY_DOCS_CACHE_DIR = PROJECT_ROOT / ".cache" / "ontology-docs"
_ONTOLOGY_DOCS_CACHE_WAIT_SECONDS = 900.0
_ONTOLOGY_DOCS_CACHE_POLL_SECONDS = 0.25
_ONTOLOGY_DOCS_CACHE_LOCK_STALE_SECONDS = 900.0
_ONTOLOGY_DOCS_RENDERER_INPUTS = (
    PROJECT_ROOT / "src" / "gmeow_tools" / "config.py",
    PROJECT_ROOT / "src" / "gmeow_tools" / "export.py",
    PROJECT_ROOT / "src" / "gmeow_tools" / "gts_views.py",
    PROJECT_ROOT / "src" / "gmeow_tools" / "mapping_dsl.py",
    PROJECT_ROOT / "src" / "gmeow_tools" / "slices.py",
)


def ontology_docs_cache_inputs() -> Sequence[Path]:
    """Inputs whose changes invalidate the content-addressed docs cache."""
    return [*ontology_docs_inputs(), *_ONTOLOGY_DOCS_RENDERER_INPUTS]


def ontology_docs_cache_key() -> str:
    """Return the content hash for the current ontology-docs inputs."""
    from gmeow_tools.generator import source_hash as compute_source_hash

    return compute_source_hash(ontology_docs_cache_inputs())


def _remove_stale_ontology_docs_lock(lock: Path) -> bool:
    """Remove a dead cache lock directory when its mtime is stale."""
    try:
        lock_mtime = lock.stat().st_mtime
    except FileNotFoundError:
        return True
    except OSError:
        return False
    if time.time() - lock_mtime < _ONTOLOGY_DOCS_CACHE_LOCK_STALE_SECONDS:
        return False
    with suppress(OSError):
        shutil.rmtree(lock)
    return not lock.exists()


def cached_ontology_docs_tree() -> Path:
    """Return a content-addressed cached ontology-docs tree, building if needed."""
    key = ontology_docs_cache_key()
    entry = _ONTOLOGY_DOCS_CACHE_DIR / key
    tree = entry / "tree"
    complete = entry / ".complete"
    if complete.exists() and tree.is_dir():
        return tree
    _ONTOLOGY_DOCS_CACHE_DIR.mkdir(parents=True, exist_ok=True)

    lock = _ONTOLOGY_DOCS_CACHE_DIR / f".lock-{key}"
    deadline = time.monotonic() + _ONTOLOGY_DOCS_CACHE_WAIT_SECONDS
    while True:
        if complete.exists() and tree.is_dir():
            return tree
        try:
            lock.mkdir()
            break
        except FileExistsError:
            if _remove_stale_ontology_docs_lock(lock):
                continue
            if time.monotonic() >= deadline:
                msg = f"timed out waiting for ontology-docs cache key {key}"
                raise RuntimeError(msg) from None
            time.sleep(_ONTOLOGY_DOCS_CACHE_POLL_SECONDS)

    tmp: Path | None = None
    try:
        if complete.exists() and tree.is_dir():
            return tree
        tmp = Path(
            tempfile.mkdtemp(dir=_ONTOLOGY_DOCS_CACHE_DIR, prefix=f".tmp-{key}-")
        )
        tmp_tree = tmp / "tree"
        build_ontology_docs(tmp_tree)
        (tmp / ".complete").write_text(key, encoding="utf-8")
        if entry.exists():
            with suppress(FileNotFoundError):
                shutil.rmtree(entry)
        tmp.replace(entry)
        tmp = None
    finally:
        with suppress(OSError):
            lock.rmdir()
        if tmp is not None:
            with suppress(OSError):
                shutil.rmtree(tmp)

    if complete.exists() and tree.is_dir():
        return tree
    msg = f"ontology-docs cache population failed for key {key}"
    raise RuntimeError(msg)


def build_ontology_docs_cached(outdir: Path) -> None:
    """Copy the content-addressed ontology-docs cache into *outdir*."""
    tree = cached_ontology_docs_tree()
    if outdir.exists():
        shutil.rmtree(outdir)
    shutil.copytree(tree, outdir)


def ontology_docs_inputs() -> Sequence[Path]:
    """Canonical sources that drive the generated docs bundle.

    Must list EVERY file whose content reaches the rendered docs — the docs
    fold into the GTS bundle (#bundle), and the drift gate skips regeneration
    when this hash is unchanged. An omission here is silent staleness: a
    rendered input changes, the hash does not, the committed snapshot is never
    rebuilt. ``examples/*.ttl`` (rendered verbatim by :func:`_collect_examples`)
    and the vendored ``simple.css`` (copied into every page) were such holes.
    """
    return [
        PROJECT_ROOT / "src" / "gmeow_tools" / "ontology_docs.py",
        # The footer cites the concept DOI read from the self-description (via
        # _citation_doi → self_desc); both the data and its loader feed the output.
        Path(__file__).with_name("self_desc.py"),
        PROJECT_ROOT / "metadata" / "gmeow-self.ttl",
        Path(__file__).with_name("assets") / "simple.css",
        PROJECT_ROOT / "docs" / "four-boxes.md",
        ONTOLOGY_DOCS_GRAPH_INPUT,
        REFERENCES_MD_FILE,
        STATEMENT_RDF12_FILE,
        *sorted(MAPPING_DSL_DIR.rglob("*.ttl")),
        *iter_slice_mapping_files(),
        *sorted(SLICES_DIR.glob("*/*/manifest.ttl")),
        *sorted(SLICES_DIR.glob("*/*/docs.md")),
        *sorted(SLICES_DIR.glob("*/*/design/*.md")),
        *sorted(SLICES_DIR.glob("*/*/examples/*.ttl")),
    ]
