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
#: SHACL shapes for the mapping DSL source (gmeow_tools.dsl_validate).
MAPPING_DSL_SHAPES_FILE = SHAPES_DIR / "mapping-dsl-shapes.ttl"
#: SHACL shapes for the statement DSL source (gmeow_tools.dsl_validate).
STATEMENT_DSL_SHAPES_FILE = SHAPES_DIR / "statement-dsl-shapes.ttl"
QUERIES_DIR = PROJECT_ROOT / "queries"
COMPETENCY_DIR = QUERIES_DIR / "competency"
QC_DIR = QUERIES_DIR / "qc"
#: Reasoned-graph negative-test queries (ROBOT ``verify``; any returned row is a
#: violation — the OBO QC pattern). Run over the reasoned merged ontology.
VERIFY_DIR = QUERIES_DIR / "verify"
#: The Temporal Query Language (TQL) toolkit — parameterized SPARQL 1.1 temporal
#: queries (Allen-relation closures, timeline, overlap, bitemporal four-clocks)
#: over the events model. A query algebra realized in standard SPARQL, not a
#: bespoke engine (Principle 5: align to T-SPARQL/stSPARQL by reference).
TEMPORAL_QUERY_DIR = QUERIES_DIR / "temporal"
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
    "dqv": "http://www.w3.org/ns/dqv#",
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
    "prof": "http://www.w3.org/ns/dx/prof/",
    "sosa": "http://www.w3.org/ns/sosa/",
    "ssn": "http://www.w3.org/ns/ssn/",
    "sweet": "http://sweetontology.net/",
    "om": "http://www.wurvoc.org/vocabularies/om-1.8/",
    "qb": "http://purl.org/linked-data/cube#",
    "mf": "http://www.opengis.net/ont/movingfeatures#",
    "sta": "http://www.opengis.net/def/ont/sensorthings/1.1/",
    "iso19156": "http://www.isotc211.org/iso19156/",
    "oboe": "http://ecoinformatics.org/oboe/oboe.1.2/oboe-core.owl#",
    "obi": "http://purl.obolibrary.org/obo/OBI_",
    "iao": "http://purl.obolibrary.org/obo/IAO_",
    "pato": "http://purl.obolibrary.org/obo/PATO_",
    "crmarc": "http://www.cidoc-crm.org/crmarchaeo/",
    "iptc": "http://iptc.org/std/NewsML-G2/",
    "bbc": "http://www.bbc.co.uk/ontologies/news/",
    "obscore": "http://www.ivoa.net/rdf/ObsCore#",
    "ppsr": "https://purl.org/ppsr/core#",
    "loinc": "http://loinc.org/rdf/",
    "np": "http://www.nanopub.org/nschema#",
    "crm": "http://www.cidoc-crm.org/cidoc-crm/",
    "crminf": "http://www.ics.forth.gr/isl/CRMinf/",
    "oa": "http://www.w3.org/ns/oa#",
    "org": "http://www.w3.org/ns/org#",
    "moat": "http://moat-project.org/ns#",
    "tags": "http://www.holygoat.co.uk/owl/redwood/0.1/tags/",
    "time": "http://www.w3.org/2006/time#",
    "teo": "https://sbmi.uth.edu/bsdi/TEO_1.0.0.owl#",
    # Robotics / pose alignment (#78)
    "pos": "http://purl.org/ieee1872-owl/pos#",
    "cora": "http://purl.org/ieee1872-owl/cora#",
    "knowrob": "http://knowrob.org/kb/knowrob.owl#",
    "soma": "http://www.ease-crc.org/ont/SOMA.owl#",
    # Temporal / geologic / measurement alignment (#67)
    "qudt": "http://qudt.org/schema/qudt/",
    "unit": "http://qudt.org/vocab/unit/",
    "edtf": "http://id.loc.gov/datatypes/edtf/",
    "periodo": "http://n2t.net/ark:/99152/",
    "gts": "http://resource.geosciml.org/ontology/timescale/gts#",
    "ivoa": "http://www.ivoa.net/rdf/",
    "crmgeo": "http://www.ics.forth.gr/isl/CRMgeo/",
    "lode": "http://linkedevents.org/ontology/",
    "sem": "http://semanticweb.cs.vu.nl/2009/11/sem/",
    "ical": "http://www.w3.org/2002/12/cal/icaltzd#",
    "schema": "https://schema.org/",
    "gedcom": "http://www.w3.org/2000/10/swap/pim/gedcom#",
    "vcard": "http://www.w3.org/2006/vcard/ns#",
    # Building ontologies: Brick (building systems), BOT (building topology),
    # ifcOWL (IFC4)
    "brick": "https://brickschema.org/schema/Brick#",
    "bot": "https://w3id.org/bot#",
    "ifc": "http://www.buildingsmart-tech.org/ifcOWL/IFC4#",
    # vCard 4 RFC-9554 extension terms (PRONOUNS, …) that the W3C vCard RDF
    # ontology — based on RFC 6350 — never minted an IRI for. Deliberately a
    # vCard-extension namespace OUTSIDE the gmeow/ term space, so the projection
    # neither fabricates a vcard: term nor leaks a GMEOW term into a pure profile.
    "vcardx": "https://blackcatinformatics.ca/vcard-ext/",
    "geo": "http://www.opengis.net/ont/geosparql#",
    "sf": "http://www.opengis.net/ont/sf#",
    "wgs84": "http://www.w3.org/2003/01/geo/wgs84_pos#",
    # Transit / network (#80)
    "gtfs": "http://vocab.gtfs.org/terms#",
    "tgn": "http://vocab.getty.edu/tgn/",
    # Gazetteers / place hubs (#82)
    "lgdo": "http://linkedgeodata.org/ontology/",
    "pleiades": "http://pleiades.stoa.org/places/vocab#",
    "whg": "https://whgazetteer.org/",
    "gvp": "http://vocab.getty.edu/ontology#",
    "mrg": "http://marineregions.org/ns/ontology#",
    "bibo": "http://purl.org/ontology/bibo/",
    "bibframe": "http://id.loc.gov/ontologies/bibframe/",
    "dpv": "https://w3id.org/dpv#",
    "sioc": "http://rdfs.org/sioc/ns#",
    "mads": "http://www.loc.gov/mads/rdf/v1#",
    "esco": "http://data.europa.eu/esco/model#",
    "nmo": "http://www.semanticdesktop.org/ontologies/2007/03/22/nmo#",
    "wot": "http://xmlns.com/wot/0.1/",
    # Verifiable Credentials / DID (the attestation module, #162)
    "vc": "https://www.w3.org/2018/credentials#",
    "did": "https://www.w3.org/ns/did#",
    # Rights / IP / licensing (the rights module, #21)
    "odrl": "http://www.w3.org/ns/odrl/2/",
    "cc": "http://creativecommons.org/ns#",
    "premis": "http://www.loc.gov/premis/rdf/v3/",
    "rstmt": "https://rightsstatements.org/vocab/",
    "ccpd": "https://creativecommons.org/publicdomain/",
    "spdx": "http://spdx.org/rdf/terms#",
    "spdxlic": "http://spdx.org/licenses/",
    "ma": "http://www.w3.org/ns/ma-ont#",
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
    "lvont": "http://lexvo.org/ontology#",
    "glottolog": "https://glottolog.org/resource/languoid/id/",
    "ontolex": "http://www.w3.org/ns/lemon/ontolex#",
    "lime": "http://www.w3.org/ns/lemon/lime#",
    # Currency (FIBO CurrencyAmount)
    "fibo-fnd-acc-cur": "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/CurrencyAmount/",
    # Machine Learning (ML-Schema)
    "mls": "http://www.w3.org/ns/mls#",
    # Biological-sequence realm (FALDO, Sequence Ontology)
    "faldo": "http://biohackathon.org/resource/faldo#",
    "so": "http://purl.obolibrary.org/obo/SO_",
    # Cadastral / land administration (ISO 19152 LADM, INSPIRE CP)
    "ladm": "http://www.opengis.net/ont/ladm#",
    "cp": "http://inspire.ec.europa.eu/ont/cp#",
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
    "dqv": AlignmentTarget("W3C DQV", PREFIXES["dqv"], "W3C-Document", "schema"),
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
    # Rights / IP / licensing (the rights module, #21). We only ever LINK to
    # these (Principle 5); the policy below documents copy-eligibility, not intent.
    "odrl": AlignmentTarget("ODRL 2.2", PREFIXES["odrl"], "W3C-Document", "schema"),
    "cc": AlignmentTarget("CC REL", PREFIXES["cc"], "CC-BY-4.0", "schema"),
    "premis": AlignmentTarget("PREMIS 3", PREFIXES["premis"], "CC-BY-4.0", "schema"),
    "rstmt": AlignmentTarget(
        "RightsStatements.org", PREFIXES["rstmt"], "CC0-1.0", "concept_scheme"
    ),
    "spdx": AlignmentTarget("SPDX", PREFIXES["spdx"], "CC-BY-3.0", "schema"),
    "spdxlic": AlignmentTarget(
        "SPDX License List", PREFIXES["spdxlic"], "CC0-1.0", "concept_scheme"
    ),
    "ma": AlignmentTarget(
        "Ontology for Media Resources", PREFIXES["ma"], "W3C-Document", "schema"
    ),
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
    "ontolex": AlignmentTarget(
        "OntoLex-Lemon", PREFIXES["ontolex"], "W3C-Document", "schema"
    ),
    "lime": AlignmentTarget("LIME", PREFIXES["lime"], "W3C-Document", "schema"),
    "qudt": AlignmentTarget("QUDT", PREFIXES["qudt"], "CC-BY-4.0", "schema"),
    "gtfs": AlignmentTarget("GTFS", PREFIXES["gtfs"], "CC-BY-3.0", "schema"),
    "fibo-fnd-acc-cur": AlignmentTarget(
        "FIBO CurrencyAmount", PREFIXES["fibo-fnd-acc-cur"], "MIT", "schema"
    ),
    "brick": AlignmentTarget("Brick", PREFIXES["brick"], "BSD-3-Clause", "schema"),
    "bot": AlignmentTarget(
        "BOT (Building Topology Ontology)", PREFIXES["bot"], "BSD-3-Clause", "schema"
    ),
    "ifc": AlignmentTarget("ifcOWL (IFC4)", PREFIXES["ifc"], "Proprietary", "schema"),
    "lvont": AlignmentTarget(
        "Lexvo Ontology", PREFIXES["lvont"], "CC-BY-SA-3.0", "schema"
    ),
    "moat": AlignmentTarget("MOAT", PREFIXES["moat"], "Unknown", "schema"),
    "tags": AlignmentTarget("Tag Ontology", PREFIXES["tags"], "Unknown", "schema"),
    "qb": AlignmentTarget("RDF Data Cube", PREFIXES["qb"], "W3C-Document", "schema"),
    "mf": AlignmentTarget("OGC Moving Features", PREFIXES["mf"], "OGC", "schema"),
    # Biological-sequence realm (#90)
    "faldo": AlignmentTarget("FALDO", PREFIXES["faldo"], "Unknown", "schema"),
    "so": AlignmentTarget(
        "Sequence Ontology", PREFIXES["so"], "CC-BY-SA-4.0", "concept_scheme"
    ),
}
