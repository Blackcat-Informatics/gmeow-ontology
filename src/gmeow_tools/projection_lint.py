"""Cross-layer consistency checks for the projection stack.

The alignment stack represents the same mappings four ways — SSSOM (1:1 term
links), EDOAL (complex cells), FnO (the transform functions), and SPARQL CONSTRUCT
(the executors) — plus the ontology. Each can drift independently. Unit tests
verify each artifact in isolation; these checks verify them *against each other*:

* :func:`fno_type_mismatches` — an ``fno:Parameter``/``fno:Output`` whose
  ``fno:predicate`` is a GMEOW property with a declared ``rdfs:range`` must declare
  an ``fno:type`` equal to that range. (Catches e.g. ``fno:predicate gmeow:eventTime``
  — range ``xsd:dateTime`` — declared with a mismatched ``fno:type``.)
* :func:`fno_reference_integrity` — every FnO function an EDOAL cell invokes via
  ``edoal:transformation`` must be a defined ``fno:Function``.
* :func:`projection_spec_drift` — for each profile, every target-vocabulary term a
  CONSTRUCT executor emits must be declared in the spec (an EDOAL cell or an SSSOM
  alignment), and no EDOAL cell may be dead (declare a term the executor never
  emits).
"""

from __future__ import annotations

import re
from pathlib import Path

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import Namespace

from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    MAPPINGS_DIR,
    PREFIXES,
    PROJECTION_QUERY_DIR,
    PROJECTIONS_DIR,
)
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mappings import build_alignment_graph, load_mappings

FNO = Namespace("https://w3id.org/function/ontology#")
ALIGN = Namespace("http://knowledgeweb.semanticweb.org/heterogeneity/alignment#")
EDOAL = Namespace("http://ns.inria.org/edoal/1.0/#")

#: The FnO catalog files (projection transforms + the language conversion catalog).
_TRANSFORMS_NAME = "transforms.fno.ttl"
_FNO_FILES = ("functions.fno.ttl", _TRANSFORMS_NAME)

#: Each projection profile and the target-vocabulary prefixes it emits.
_PROFILE_TARGETS: dict[str, tuple[str, ...]] = {
    "schema-org": ("schema",),
    # vcardx: the RFC-9554 extension namespace (PRONOUNS) — checked alongside
    # vcard: so the new vcardx:* output stays under CONSTRUCT↔EDOAL↔SSSOM drift.
    "vcard": ("vcard", "vcardx"),
    "foaf": ("foaf", "wgs84"),
    "geosparql": ("geo",),
    "ical": ("ical",),
    "owl-time": ("time",),
    "bot": ("bot",),
    # Rights module (#21): the structural ODRL policy + CC REL licence projections.
    # Their target terms are declared in the SSSOM linkage layer (the structural
    # templateAtoms cells emit no EDOAL cell), so the drift check confirms every
    # emitted odrl:/cc: term is aligned by reference.
    "odrl": ("odrl",),
    "cc": ("cc",),
    "dcterms": ("dcterms",),
    "oai_dc": ("dc",),
    "spdx": ("spdx",),
    "sosa": ("sosa", "geo"),
    # Image super-ontology (#22)
    "iiif": ("iiif", "oa"),
    "exif": ("exif",),
    # Transpiler coverage profiles (#34 phases 2-3)
    "org": ("org",),
    "bibo": ("bibo",),
    # bibframe's minted identifier nodes carry their value via rdf:value, so
    # the rdf: prefix is scanned too — keeping that emission under the gate.
    "bibframe": ("bibframe", "rdf"),
    "gedcom": ("gedcom",),
    "sioc": ("sioc",),
}

#: Target terms a compose/decompose transform legitimately MINTS — intermediate
#: nodes, linking properties, composed literals and datatypes. These are outputs of
#: declared FnO transforms (fnComposeAddress, the name-part decomposition,
#: fnRetagWkt, fnRetagGeoJson, fnCoarsenToGranularityGeoJson, fnLatLongToGeoUri),
#: not standalone term correspondences, so they have no EDOAL/SSSOM cell. Listed
#: here so the drift check still catches genuinely-undeclared mappings.
_STRUCTURAL_OUTPUTS: frozenset[str] = frozenset(
    {
        # #34 minted structural nodes: the boundary Instant's value
        # carrier, the minted DOI-identifier node's type, and the
        # has_container projection's inverse row — outputs of declared
        # minting templates, not standalone term correspondences.
        "http://www.w3.org/2006/time#inXSDDateTime",
        # the per-review ClaimReview's minted Rating node link (#34)
        "https://schema.org/reviewRating",
        "http://id.loc.gov/ontologies/bibframe/Doi",
        "http://id.loc.gov/ontologies/bibframe/Identifier",
        # the minted identifier node's literal carrier (bibframe scans rdf:)
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#value",
        "http://rdfs.org/sioc/ns#container_of",
        # the minted gedcom:Marriage node's symmetric spouse rows
        "http://www.w3.org/2000/10/swap/pim/gedcom#spouseIn",
        "http://www.w3.org/2006/vcard/ns#hasName",
        "http://www.w3.org/2006/vcard/ns#Name",
        "http://www.w3.org/2006/vcard/ns#hasAddress",
        "http://www.w3.org/2006/vcard/ns#label",
        "http://www.w3.org/2006/vcard/ns#hasGeo",
        "http://www.w3.org/2006/vcard/ns#Geo",
        "http://www.w3.org/2006/vcard/ns#latitude",
        "http://www.w3.org/2006/vcard/ns#longitude",
        "http://www.opengis.net/ont/geosparql#wktLiteral",
        "http://www.opengis.net/ont/geosparql#geoJSONLiteral",
    }
)


def _fno_graph(projections_dir: Path = PROJECTIONS_DIR) -> Graph:
    graph = Graph()
    for name in _FNO_FILES:
        path = projections_dir / name
        if not path.exists() and name == _TRANSFORMS_NAME:
            # The hand-authored language-transform catalog lives in the DSL
            # tree (#287); staging trees copy it in, the committed tree
            # reads it from its authored home.
            path = MAPPING_DSL_DIR / name
        graph.parse(path, format="turtle")
    return graph


def fno_type_mismatches(projections_dir: Path = PROJECTIONS_DIR) -> list[str]:
    """Return FnO param/output types that disagree with their predicate's range."""
    onto = load_merged_graph(include_imports=False)
    fno = _fno_graph(projections_dir)
    problems: list[str] = []
    params = set(fno.subjects(RDF.type, FNO.Parameter)) | set(
        fno.subjects(RDF.type, FNO.Output)
    )
    for param in params:
        predicate = fno.value(param, FNO.predicate)
        ftype = fno.value(param, FNO.type)
        if not isinstance(predicate, URIRef) or ftype is None:
            continue
        ranges = set(onto.objects(predicate, RDFS.range))
        if not ranges:
            continue  # an external / projected predicate — no ontology range to check
        if ftype not in ranges:
            problems.append(
                f"{param}: predicate {predicate} has range "
                f"{sorted(str(r) for r in ranges)} but fno:type is {ftype}"
            )
    return problems


def fno_reference_integrity(projections_dir: Path = PROJECTIONS_DIR) -> list[str]:
    """Return EDOAL ``edoal:transformation`` references to undefined FnO functions."""
    defined = set(_fno_graph(projections_dir).subjects(RDF.type, FNO.Function))
    problems: list[str] = []
    for edoal in sorted(projections_dir.glob("*.edoal.ttl")):
        graph = Graph().parse(edoal, format="turtle")
        for cell in graph.subjects(RDF.type, ALIGN.Cell):
            for trans in graph.objects(cell, EDOAL.transformation):
                for ref in graph.objects(trans, RDFS.seeAlso):
                    local = str(ref).rsplit("/", 1)[-1]
                    if local.startswith("fn") and ref not in defined:
                        problems.append(f"{edoal.name}: undefined FnO function {ref}")
    return problems


def _target_terms_in_query(text: str, prefixes: tuple[str, ...]) -> set[str]:
    """Target-vocabulary IRIs mentioned in a CONSTRUCT query (comments stripped)."""
    pattern = re.compile(r"\b(" + "|".join(prefixes) + r"):([A-Za-z][\w-]*)")
    out: set[str] = set()
    for line in text.splitlines():
        for prefix, local in pattern.findall(line.split("#", 1)[0]):
            out.add(PREFIXES[prefix] + local)
    return out


def _edoal_targets(path: Path, namespaces: tuple[str, ...]) -> set[str]:
    """Target-vocabulary IRIs an EDOAL file declares (its cells' entity2)."""
    graph = Graph().parse(path, format="turtle")
    out: set[str] = set()
    for cell in graph.subjects(RDF.type, ALIGN.Cell):
        entity2 = graph.value(cell, ALIGN.entity2)
        if entity2 is None:
            continue
        uri = graph.value(entity2, EDOAL.uri)
        if isinstance(uri, URIRef) and str(uri).startswith(namespaces):
            out.add(str(uri))
    return out


def projection_spec_drift(
    projections_dir: Path = PROJECTIONS_DIR,
    query_dir: Path = PROJECTION_QUERY_DIR,
    mappings_dir: Path = MAPPINGS_DIR,
) -> list[str]:
    """Return CONSTRUCT↔EDOAL↔SSSOM inconsistencies, per profile."""
    # An SSSOM row may place the external term in subject OR object position
    # (e.g. "geo:Feature closeMatch gmeow:Place"), so collect both.
    aligned: set[str] = set()
    for subject, _predicate, obj in build_alignment_graph(load_mappings(mappings_dir)):
        for node in (subject, obj):
            if isinstance(node, URIRef):
                aligned.add(str(node))
    problems: list[str] = []
    for profile, prefixes in _PROFILE_TARGETS.items():
        namespaces = tuple(PREFIXES[p] for p in prefixes)
        emitted = _target_terms_in_query(
            (query_dir / f"{profile}.rq").read_text(encoding="utf-8"),
            prefixes,
        )
        edoal = _edoal_targets(projections_dir / f"{profile}.edoal.ttl", namespaces)
        declared = (
            edoal
            | {t for t in aligned if t.startswith(namespaces)}
            | _STRUCTURAL_OUTPUTS
        )
        for term in sorted(emitted - declared):
            problems.append(
                f"{profile}: {term} emitted by the executor but declared in "
                f"neither EDOAL nor SSSOM"
            )
        for term in sorted(edoal - emitted):
            problems.append(
                f"{profile}: {term} declared in EDOAL but never emitted by the "
                f"{profile}.rq executor (dead cell)"
            )
    return problems
