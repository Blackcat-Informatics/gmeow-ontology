"""Single source of truth for GMEOW IRIs, paths, prefixes, and link policy.

Every other module imports from here so that namespace strings, filesystem
locations, and the license-aware link policy are defined exactly once. All
filesystem locations are :class:`pathlib.Path` objects (never bare strings).
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path

# --------------------------------------------------------------------------- #
# Version
# --------------------------------------------------------------------------- #

#: Semantic version of the ontology release. Drives ``owl:versionIRI`` and the
#: CITATION/DOI metadata. Bump on every release (releases are immutable).
VERSION = "0.1.0"

#: Release date (ISO-8601). Used for the DOI deposit publication date.
RELEASE_DATE = "2026-06-03"

#: Human-readable title (shared by metadata, citation, and DOI deposit).
TITLE = "GMEOW — Global Metadata and Entity Ontology for the Web"

# --------------------------------------------------------------------------- #
# IRIs (slash namespace, schema.org style — see plan)
# --------------------------------------------------------------------------- #

#: Ontology IRI (the document IRI, no trailing slash).
ONTOLOGY_IRI = "https://blackcatinformatics.ca/gmeow"

#: Vocabulary namespace (term IRIs are NAMESPACE + local name).
NAMESPACE = ONTOLOGY_IRI + "/"

#: VoID dataset node IRI (subject of linkset descriptions).
VOID_DATASET_IRI = ONTOLOGY_IRI + "/.well-known/void.ttl#dataset"


def version_iri(version: str = VERSION) -> str:
    """Return the ``owl:versionIRI`` for a given semantic version.

    Args:
        version: Semantic version string (defaults to :data:`VERSION`).

    Returns:
        The immutable, version-specific IRI for the release artifact.
    """
    return f"{NAMESPACE}{version}"


# --------------------------------------------------------------------------- #
# Filesystem layout
# --------------------------------------------------------------------------- #

#: Repository root (this file lives at ``<root>/src/gmeow_tools/config.py``).
PROJECT_ROOT = Path(__file__).resolve().parents[2]

ONTOLOGY_DIR = PROJECT_ROOT / "ontology"
MODULES_DIR = ONTOLOGY_DIR / "modules"
ONTOLOGY_FILE = ONTOLOGY_DIR / "gmeow.ttl"

IMPORTS_DIR = PROJECT_ROOT / "imports"
#: Validation-time target-vocabulary axiom snapshots (domain/range/inverseOf only).
#: A SUBDIR of ``imports/`` so it is NOT picked up by ``iter_import_files()``
#: (which globs ``imports/*.ttl`` non-recursively) and never enters the published
#: CC BY 4.0 artifact. Used solely by the SSSOM alignment-direction linter; only
#: IMPORT_OK targets are vendored here (reference-only ones are fetched live).
TARGET_SNAPSHOT_DIR = IMPORTS_DIR / "targets"
MAPPINGS_DIR = PROJECT_ROOT / "mappings"
SHAPES_DIR = PROJECT_ROOT / "shapes"
SHAPES_FILE = SHAPES_DIR / "gmeow-shapes.ttl"
QUERIES_DIR = PROJECT_ROOT / "queries"
COMPETENCY_DIR = QUERIES_DIR / "competency"
QC_DIR = QUERIES_DIR / "qc"
#: Reasoned-graph negative-test queries (ROBOT ``verify``; any returned row is a
#: violation — the OBO QC pattern). Run over the reasoned merged ontology.
VERIFY_DIR = QUERIES_DIR / "verify"
#: Per-profile projection CONSTRUCT queries (the FnO/EDOAL executors).
PROJECTION_QUERY_DIR = QUERIES_DIR / "projections"
#: FnO function catalog + EDOAL complex-alignment specs (consumable, not reasoned).
PROJECTIONS_DIR = PROJECT_ROOT / "projections"
#: Single-source mapping DSL (the GMEOW-grounded authoring layer). ``gmeow
#: compile-mappings`` renders these cells into the SSSOM / EDOAL / FnO / SPARQL
#: artifacts. Authored, never generated; not in the reasoned import closure.
MAPPING_DSL_DIR = PROJECT_ROOT / "mapping-dsl"
#: Single-source statement DSL (the canonical RDF 1.2 / RDF* statement-metadata
#: layer — provenance, confidence, temporal scope). ``gmeow compile-statements``
#: renders these cells to the RDF 1.2 lead artifact + the OWL axiom-annotation
#: compatibility downcast. Authored, never generated; a spec layer (CONSTITUTION
#: Principles 2-3). The RDF 1.2 form is canonical; the OWL form is the generated,
#: reasoning-lossless downcast the OWL 2 DL reasoners consume.
STATEMENT_DSL_DIR = PROJECT_ROOT / "statement-dsl"
#: Generated statement-metadata artifacts (committed, like mappings/ — so the
#: ``compile-statements --check`` no-drift gate has a committed target).
STATEMENTS_DIR = PROJECT_ROOT / "statements"
#: The RDF 1.2 / RDF* lead serialization (canonical statement-metadata form).
STATEMENT_RDF12_FILE = STATEMENTS_DIR / "gmeow.rdf12.ttl"
#: The OWL 2 axiom-annotation downcast (generated; consumed by the reasoner).
STATEMENT_OWL_FILE = STATEMENTS_DIR / "gmeow-statements.owl.ttl"
#: Vendored coverage fixtures (public site graphs) used by the coverage harness.
FIXTURES_DIR = PROJECT_ROOT / "tests" / "fixtures" / "coverage"
METADATA_DIR = PROJECT_ROOT / "metadata"
VOID_FILE = METADATA_DIR / "void.ttl"
DCAT_FILE = METADATA_DIR / "dcat.ttl"

APACHE_DIR = PROJECT_ROOT / "apache"
APACHE_CONF = APACHE_DIR / "gmeow.conf"

CATALOG_FILE = PROJECT_ROOT / "catalog-v001.xml"

#: Generated outputs (git-ignored, published on release).
DIST_DIR = PROJECT_ROOT / "dist"
DOCS_DIR = PROJECT_ROOT / "docs" / "_generated"

# --------------------------------------------------------------------------- #
# Pinned Docker images (the Java toolchain — see plan)
# --------------------------------------------------------------------------- #

ROBOT_IMAGE = "obolibrary/robot:v1.9.7"
WIDOCO_IMAGE = "ghcr.io/dgarijo/widoco:v1.4.25"
#: Apache Jena CLI (riot + sparql) — the required RDF 1.2 / triple-term engine.
#: No maintained public Jena 5.4 CLI image exists, so this pinned tag is built
#: from ``docker/jena/Dockerfile`` (``make pull-images`` / CI build it). A private
#: mirror under the same tag is pulled if present.
JENA_IMAGE = "stain/jena:5.4.0"

# --------------------------------------------------------------------------- #
# CrossRef DOI deposit
#
# Blackcat Informatics mints GMEOW's DOI as a CrossRef member (its own prefix),
# rather than via Zenodo. The values below are the registrant-specific deposit
# parameters; the prefix is a placeholder until membership is finalized.
# --------------------------------------------------------------------------- #

#: CrossRef DOI prefix assigned to the registrant. PLACEHOLDER — replace with the
#: real prefix (e.g. "10.71234") once CrossRef membership is finalized.
CROSSREF_DOI_PREFIX = "10.XXXXX"
#: DOI suffix for the ontology (DOI = ``{prefix}/{suffix}``).
CROSSREF_DOI_SUFFIX = "gmeow"
#: Depositor / registrant identity for the deposit batch.
CROSSREF_DEPOSITOR_NAME = "Blackcat Informatics Inc."
#: Depositor email registered with CrossRef. PLACEHOLDER — set to the real one.
CROSSREF_DEPOSITOR_EMAIL = "doi@blackcatinformatics.ca"
CROSSREF_REGISTRANT = "Blackcat Informatics Inc."


def full_doi() -> str:
    """Return the full GMEOW DOI (``{prefix}/{suffix}``)."""
    return f"{CROSSREF_DOI_PREFIX}/{CROSSREF_DOI_SUFFIX}"


# --------------------------------------------------------------------------- #
# Namespace prefixes (single registry — drives serialization + JSON-LD context)
# --------------------------------------------------------------------------- #

PREFIXES: dict[str, str] = {
    # GMEOW + RDF core
    "gmeow": NAMESPACE,
    "owl": "http://www.w3.org/2002/07/owl#",
    "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
    "xsd": "http://www.w3.org/2001/XMLSchema#",
    "skos": "http://www.w3.org/2004/02/skos/core#",
    # Metadata / documentation
    "dcterms": "http://purl.org/dc/terms/",
    "dc": "http://purl.org/dc/elements/1.1/",
    "vann": "http://purl.org/vocab/vann/",
    "void": "http://rdfs.org/ns/void#",
    "dcat": "http://www.w3.org/ns/dcat#",
    "sssom": "https://w3id.org/sssom/",
    "semapv": "https://w3id.org/semapv/vocab/",
    # Transformation / complex-alignment layer (projection specs; not reasoned)
    "fno": "https://w3id.org/function/ontology#",
    "fnom": "https://w3id.org/function/vocabulary/mapping#",
    "edoal": "http://ns.inria.org/edoal/1.0/#",
    "align": "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#",
    # Upper-ontology spine
    "gufo": "http://purl.org/nemo/gufo#",
    "umbel": "http://umbel.org/umbel#",
    "umbelrc": "http://umbel.org/umbel/rc/",
    "dul": "http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#",
    "bfo": "http://purl.obolibrary.org/obo/",
    # Peer schemas GMEOW supersets / aligns to
    "foaf": "http://xmlns.com/foaf/0.1/",
    "rel": "http://purl.org/vocab/relationship/",
    "doap": "http://usefulinc.com/ns/doap#",
    "prov": "http://www.w3.org/ns/prov#",
    "np": "http://www.nanopub.org/nschema#",
    "crm": "http://www.cidoc-crm.org/cidoc-crm/",
    "crminf": "http://www.ics.forth.gr/isl/CRMinf/",
    "oa": "http://www.w3.org/ns/oa#",
    "org": "http://www.w3.org/ns/org#",
    "time": "http://www.w3.org/2006/time#",
    "lode": "http://linkedevents.org/ontology/",
    "sem": "http://semanticweb.cs.vu.nl/2009/11/sem/",
    "ical": "http://www.w3.org/2002/12/cal/icaltzd#",
    "schema": "https://schema.org/",
    "gedcom": "http://www.w3.org/2000/10/swap/pim/gedcom#",
    "vcard": "http://www.w3.org/2006/vcard/ns#",
    "geo": "http://www.opengis.net/ont/geosparql#",
    "wgs84": "http://www.w3.org/2003/01/geo/wgs84_pos#",
    "tgn": "http://vocab.getty.edu/tgn/",
    "gvp": "http://vocab.getty.edu/ontology#",
    "bibo": "http://purl.org/ontology/bibo/",
    "bibframe": "http://id.loc.gov/ontologies/bibframe/",
    "sioc": "http://rdfs.org/sioc/ns#",
    "mads": "http://www.loc.gov/mads/rdf/v1#",
    "esco": "http://data.europa.eu/esco/model#",
    "nmo": "http://www.semanticdesktop.org/ontologies/2007/03/22/nmo#",
    "wot": "http://xmlns.com/wot/0.1/",
    # Gender / sexuality identity vocabularies
    "gsso": "http://purl.obolibrary.org/obo/GSSO_",
    "homosaurus": "https://homosaurus.org/v4/",
    "fhir": "http://hl7.org/fhir/",
    # Genealogy
    "bio": "http://purl.org/vocab/bio/0.1/",
    "gx": "http://gedcomx.org/",
    "gxv": "http://gedcomx.org/v1/",
    "gn": "http://www.geonames.org/ontology#",
    # Wikidata
    "wd": "http://www.wikidata.org/entity/",
    "wdt": "http://www.wikidata.org/prop/direct/",
    "wikibase": "http://wikiba.se/ontology#",
    "p": "http://www.wikidata.org/prop/",
    "ps": "http://www.wikidata.org/prop/statement/",
    "wds": "http://www.wikidata.org/entity/statement/",
    # Languages (the ISO 639 hub is Lexvo; Glottolog for genealogy/languoids)
    "lexvo": "http://lexvo.org/id/",
    "glottolog": "https://glottolog.org/resource/languoid/id/",
}

# --------------------------------------------------------------------------- #
# License-aware link policy
# --------------------------------------------------------------------------- #


class LinkPolicy(StrEnum):
    """Whether an external vocabulary's axioms may be copied into GMEOW.

    GMEOW is published under CC BY 4.0. Importing or extracting an external
    ontology *copies its axioms/labels* into GMEOW, which is only permissible
    for compatibly-licensed sources. Restrictive sources may still be *linked*
    by IRI (which copies nothing).
    """

    IMPORT_OK = "import-ok"
    REFERENCE_ONLY = "reference-only"


#: License-id tokens (uppercased) that block axiom copying into a CC-BY work.
#: Non-commercial, no-derivatives, conflicting share-alike, and copyleft
#: software licenses are reference-only.
_REFERENCE_ONLY_MARKERS: tuple[str, ...] = (
    "NC",  # non-commercial (CC-BY-NC, CC-BY-NC-SA, CC-BY-NC-ND)
    "ND",  # no-derivatives
    "SA",  # share-alike (CC-BY-SA conflicts with CC-BY republication)
    "GPL",  # GPL / LGPL / AGPL copyleft software licenses
    "EUPL",  # European Union Public License (copyleft)
    "PROPRIETARY",
    "INTERNAL",
    "ACADEMIC",
)

#: License-id tokens (uppercased) explicitly cleared for axiom copying.
_IMPORT_OK_LICENSES: frozenset[str] = frozenset(
    {
        "CC0",
        "CC0-1.0",
        "CC-BY",
        "CC-BY-1.0",
        "CC-BY-3.0",
        "CC-BY-4.0",
        "MIT",
        "APACHE-2.0",
        "BSD-2-CLAUSE",
        "BSD-3-CLAUSE",
        "PDDL-1.0",
        "PDDL",
        "ODC-BY-1.0",
        "ODC-BY",
        "PUBLIC-DOMAIN",
        "PUBLIC DOMAIN",
        "W3C",
        "W3C-DOCUMENT",
        "OGC",
        "NIST-PUBLIC-DOMAIN",
        "NIST PUBLIC DOMAIN",
        "UNLICENSE",
    }
)


def policy_for_license(license_id: str) -> LinkPolicy:
    """Classify a license string into a link policy.

    The classifier is conservative: a restrictive marker (NC/ND/SA/GPL/…)
    anywhere in the token forces ``REFERENCE_ONLY``, even if a permissive
    substring is also present (e.g. ``CC-BY-NC-SA`` contains ``CC-BY`` but is
    still reference-only). Unknown licenses default to ``REFERENCE_ONLY`` so a
    mistake fails safe (links allowed, copying refused).

    Args:
        license_id: A license identifier such as ``"CC-BY-4.0"`` or
            ``"CC-BY-NC-ND 4.0"``.

    Returns:
        The :class:`LinkPolicy` for the license.
    """
    token = license_id.strip().upper()
    # Restrictive markers win, regardless of any permissive substring.
    for marker in _REFERENCE_ONLY_MARKERS:
        # Match the marker as a hyphen/space/edge-delimited segment so that,
        # e.g. "ND" does not spuriously match inside "PUBLIC DOMAIN".
        if _has_marker_segment(token, marker):
            return LinkPolicy.REFERENCE_ONLY
    if token in _IMPORT_OK_LICENSES:
        return LinkPolicy.IMPORT_OK
    # Bare "CC-BY" with a version suffix not already listed.
    if token.startswith("CC-BY-") and "SA" not in token and "NC" not in token:
        return LinkPolicy.IMPORT_OK
    return LinkPolicy.REFERENCE_ONLY


def _has_marker_segment(token: str, marker: str) -> bool:
    """Return whether ``marker`` appears as a delimited segment of ``token``."""
    segments = token.replace("_", "-").replace(" ", "-").split("-")
    if marker in segments:
        return True
    # GPL family also appears as a prefix (e.g. "GPL-2.0", "LGPL", "AGPL-3.0").
    return marker == "GPL" and any(seg.endswith("GPL") for seg in segments)


@dataclass(frozen=True, slots=True)
class AlignmentTarget:
    """A curated external vocabulary GMEOW aligns to.

    Informed by — never parsed from — the local source registry. ``kind`` marks
    whether the target is a foundational *upper* ontology, a peer *schema*
    GMEOW supersets/aligns to, or a *concept_scheme* (value vocabulary).
    """

    name: str
    namespace: str
    license: str
    kind: str  # "upper" | "schema" | "concept_scheme"

    @property
    def policy(self) -> LinkPolicy:
        """Return the link policy implied by this target's license."""
        return policy_for_license(self.license)


#: Curated alignment targets. The spec authors extend this as alignment grows;
#: the policy is derived from each target's license automatically.
ALIGNMENT_TARGETS: dict[str, AlignmentTarget] = {
    "gufo": AlignmentTarget("gUFO", PREFIXES["gufo"], "MIT", "upper"),
    "umbel": AlignmentTarget("UMBEL", PREFIXES["umbel"], "CC-BY-3.0", "upper"),
    "dolce": AlignmentTarget("DOLCE/DUL", PREFIXES["dul"], "LGPL", "upper"),
    "bfo": AlignmentTarget(
        "BFO", "http://purl.obolibrary.org/obo/bfo.owl", "CC-BY-4.0", "upper"
    ),
    "foaf": AlignmentTarget("FOAF", PREFIXES["foaf"], "CC-BY-1.0", "schema"),
    "rel": AlignmentTarget(
        "REL (Relationship)", PREFIXES["rel"], "CC-BY-1.0", "schema"
    ),
    "doap": AlignmentTarget("DOAP", PREFIXES["doap"], "Public-Domain", "schema"),
    "prov": AlignmentTarget("PROV-O", PREFIXES["prov"], "W3C-Document", "schema"),
    "org": AlignmentTarget("ORG", PREFIXES["org"], "PDDL-1.0", "schema"),
    "time": AlignmentTarget("OWL-Time", PREFIXES["time"], "CC-BY-4.0", "schema"),
    "schema": AlignmentTarget(
        "Schema.org", PREFIXES["schema"], "CC-BY-SA-3.0", "schema"
    ),
    "gedcom": AlignmentTarget(
        "W3C GEDCOM", PREFIXES["gedcom"], "W3C-Document", "schema"
    ),
    "vcard": AlignmentTarget("vCard", PREFIXES["vcard"], "W3C-Document", "schema"),
    "geo": AlignmentTarget("GeoSPARQL", PREFIXES["geo"], "OGC", "schema"),
    "wgs84": AlignmentTarget(
        "WGS84 Geo Positioning", PREFIXES["wgs84"], "W3C-Document", "schema"
    ),
    "tgn": AlignmentTarget(
        "Getty TGN", PREFIXES["tgn"], "ODC-BY-1.0", "concept_scheme"
    ),
    "gvp": AlignmentTarget(
        "Getty Vocabulary Program", PREFIXES["gvp"], "ODC-BY-1.0", "concept_scheme"
    ),
    "bibo": AlignmentTarget("BIBO", PREFIXES["bibo"], "CC-BY-3.0", "schema"),
    "bibframe": AlignmentTarget("BIBFRAME", PREFIXES["bibframe"], "CC0-1.0", "schema"),
    "sioc": AlignmentTarget("SIOC", PREFIXES["sioc"], "W3C-Document", "schema"),
    "skos": AlignmentTarget("SKOS", PREFIXES["skos"], "W3C-Document", "concept_scheme"),
    "nmo": AlignmentTarget(
        "Nepomuk Message Ontology", PREFIXES["nmo"], "Unknown", "schema"
    ),
    "wot": AlignmentTarget("WOT Schema", PREFIXES["wot"], "Unknown", "schema"),
    "gsso": AlignmentTarget(
        "Gender, Sex, and Sexual Orientation Ontology",
        PREFIXES["gsso"],
        "CC-BY-NC-ND 4.0",  # per SOURCE_REGISTRY; reference-only (we only link)
        "concept_scheme",
    ),
    "homosaurus": AlignmentTarget(
        "Homosaurus", PREFIXES["homosaurus"], "CC-BY-4.0", "concept_scheme"
    ),
    "fhir": AlignmentTarget("HL7 FHIR", PREFIXES["fhir"], "CC0-1.0", "schema"),
    "bio": AlignmentTarget("BIO vocabulary", PREFIXES["bio"], "CC-BY-3.0", "schema"),
    "gedcomx": AlignmentTarget("GEDCOM X", PREFIXES["gx"], "Apache-2.0", "schema"),
    "geonames": AlignmentTarget(
        "GeoNames", PREFIXES["gn"], "CC-BY-4.0", "concept_scheme"
    ),
    "wikidata": AlignmentTarget(
        "Wikidata", PREFIXES["wd"], "CC0-1.0", "concept_scheme"
    ),
    "lexvo": AlignmentTarget(
        "Lexvo", PREFIXES["lexvo"], "CC-BY-SA-3.0", "concept_scheme"
    ),
    "glottolog": AlignmentTarget(
        "Glottolog", PREFIXES["glottolog"], "CC-BY-4.0", "concept_scheme"
    ),
}
