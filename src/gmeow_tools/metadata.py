"""Generate VoID and DCAT dataset descriptions.

VoID (with linksets) is what the LOD-Cloud submission consumes; DCAT gives a
FAIR-friendly dataset/distribution view. Every data-derived value comes from
the GTS snapshot (the narrow waist, #267): the version from the ontology
header in the default graph, the linksets from the alignments named graph.
rdflib appears here ONLY as the output serializer for the freshly
constructed description graphs — never as a reader of canonical sources.
"""

from __future__ import annotations

import hashlib
from collections import defaultdict
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import DCAT, DCTERMS, FOAF, SKOS, VOID, XSD

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    DCAT_FILE,
    GTS_GRAPH_ALIGNMENTS,
    GTS_GRAPH_METADATA,
    GTS_SNAPSHOT_FILE,
    NAMESPACE,
    ONTOLOGY_IRI,
    PREFIXES,
    PROJECT_ROOT,
    VOID_DATASET_IRI,
    VOID_FILE,
)
from gmeow_tools.generator import Generator, register, write_turtle
from gmeow_tools.graph import bind_prefixes
from gmeow_tools.gts_views import FoldView, load_fold
from gmeow_tools.mappings import object_namespace
from gmeow_tools.self_desc import GMEOW

_CC_BY = URIRef("https://creativecommons.org/licenses/by/4.0/")
_PUBLISHER = URIRef("https://blackcatinformatics.ca/#bii")

#: W3C format registry IRIs by published file extension.
_FORMAT_IRI: dict[str, str] = {
    "ttl": "http://www.w3.org/ns/formats/Turtle",
    "rdf": "http://www.w3.org/ns/formats/RDF_XML",
    "nt": "http://www.w3.org/ns/formats/N-Triples",
    "jsonld": "http://www.w3.org/ns/formats/JSON-LD",
}
_MEDIA_TYPE: dict[str, str] = {
    "ttl": "text/turtle",
    "rdf": "application/rdf+xml",
    "nt": "application/n-triples",
    "jsonld": "application/ld+json",
}


_OWL_VERSION_INFO = "http://www.w3.org/2002/07/owl#versionInfo"
_DCTERMS_DESCRIPTION = "http://purl.org/dc/terms/description"


def _fold_version(view: FoldView) -> str:
    """The ontology version from the snapshot's header (owl:versionInfo)."""
    onto = view.tid_of_iri(ONTOLOGY_IRI)
    version = view.value(onto, _OWL_VERSION_INFO) if onto is not None else None
    if version is None:
        msg = f"snapshot lacks owl:versionInfo on {ONTOLOGY_IRI}"
        raise ValueError(msg)
    return view.lex(version)


def _fold_description(view: FoldView) -> str:
    """The canonical ontology description from the snapshot header.

    Read ``dcterms:description`` from the fold rather than hardcoded, so VoID/DCAT
    carry exactly the one canonical abstract authored in the ontology header
    (Principle 4 — one source).
    """
    onto = view.tid_of_iri(ONTOLOGY_IRI)
    desc = view.value(onto, _DCTERMS_DESCRIPTION) if onto is not None else None
    if desc is None:
        msg = f"snapshot lacks dcterms:description on {ONTOLOGY_IRI}"
        raise ValueError(msg)
    return view.lex(desc)


_WIKIDATA_PREFIX = "http://www.wikidata.org/entity/"


def _fold_wikidata_dataset_links(view: FoldView | None) -> list[URIRef]:
    """Wikidata authority/exact-match links for the Work, projected to VoID/DCAT."""
    if view is None:
        return []
    work_tid = view.tid_of_iri(ONTOLOGY_IRI)
    if work_tid is None:
        return []
    links: set[URIRef] = set()
    for predicate_iri in (str(GMEOW.authorityLink), str(SKOS.exactMatch)):
        for obj_tid in view.objects(work_tid, predicate_iri, scope=GTS_GRAPH_METADATA):
            if view.is_iri(obj_tid):
                obj_iri = view.lex(obj_tid)
                if obj_iri.startswith(_WIKIDATA_PREFIX):
                    links.add(URIRef(obj_iri))
    return sorted(links, key=str)


#: VoID statistics term IRIs, by rdf:type, for the published default graph.
_OWL = "http://www.w3.org/2002/07/owl#"
_RDFS = "http://www.w3.org/2000/01/rdf-schema#"
_RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
_CLASS_TYPES = (_OWL + "Class", _RDFS + "Class")
_PROPERTY_TYPES = (
    _OWL + "ObjectProperty",
    _OWL + "DatatypeProperty",
    _OWL + "AnnotationProperty",
    _RDF + "Property",
)


@dataclass(frozen=True, slots=True)
class _VoidStats:
    """VoID dataset statistics computed over the published default graph."""

    triples: int
    classes: int
    properties: int
    entities: int


def _fold_stats(view: FoldView) -> _VoidStats:
    """Compute VoID statistics over the published ontology (default graph).

    The ``triples`` count is the LOD-Cloud "size" (it scales the cloud bubble);
    it is the *asserted* published graph, not the reasoned closure (Principle 8 —
    the reasoner is QA, not a consumer prerequisite). ``entities`` counts distinct
    IRI subjects in the GMEOW namespace.
    """
    triples = 0
    entities: set[str] = set()
    for s_tid, _p, _o, _g in view.quads():
        triples += 1
        if view.is_iri(s_tid) and view.lex(s_tid).startswith(NAMESPACE):
            entities.add(view.lex(s_tid))
    classes: set[int] = set()
    for class_iri in _CLASS_TYPES:
        classes.update(view.subjects_by_type(class_iri))
    properties: set[int] = set()
    for property_iri in _PROPERTY_TYPES:
        properties.update(view.subjects_by_type(property_iri))
    return _VoidStats(triples, len(classes), len(properties), len(entities))


def _fold_linksets(view: FoldView) -> Graph:
    """VoID linksets from the snapshot's alignments graph.

    Same bucketing as the retired SSSOM-row path: one ``void:Linkset`` per
    (target namespace, predicate) pair with its triple count — but counted
    over the alignment AXIOMS the snapshot actually carries (§7.8 set
    semantics), which is what the published links are.
    """
    graph = Graph()
    bind_prefixes(graph)
    dataset = URIRef(VOID_DATASET_IRI)
    buckets: dict[tuple[str, str], int] = defaultdict(int)
    for _s, p, o, _g in view.quads(scope=GTS_GRAPH_ALIGNMENTS):
        if not (view.is_iri(p) and view.is_iri(o)):
            continue  # defensive: alignment axioms are IRI→IRI by construction
        target_ns = object_namespace(URIRef(view.lex(o)))
        buckets[(target_ns, view.lex(p))] += 1

    for (target_ns, predicate_iri), count in sorted(buckets.items()):
        predicate = URIRef(predicate_iri)
        predicate_id = view.curie(predicate_iri)
        slug = predicate_id.replace(":", "_")
        target_slug = target_ns.rstrip("#/").rsplit("/", 1)[-1] or "target"
        ns_hash = hashlib.sha256(target_ns.encode()).hexdigest()[:6]
        linkset = URIRef(f"{VOID_DATASET_IRI}-linkset-{target_slug}-{ns_hash}-{slug}")
        graph.add((linkset, RDF.type, VOID.Linkset))
        graph.add((linkset, VOID.subjectsTarget, dataset))
        graph.add((linkset, VOID.objectsTarget, URIRef(target_ns)))
        graph.add((linkset, VOID.linkPredicate, predicate))
        graph.add((linkset, VOID.triples, Literal(count)))
        graph.add(
            (
                linkset,
                RDFS.label,
                Literal(
                    f"GMEOW {predicate_id} links to {target_ns} ({count})",
                    lang="x-gmeow-english",
                ),
            )
        )
    return graph


def build_void_graph(view: FoldView | None = None) -> Graph:
    """Build the VoID dataset description, including mapping linksets."""
    if view is None:
        view = load_fold()
    graph = Graph()
    bind_prefixes(graph)
    dataset = URIRef(VOID_DATASET_IRI)

    graph.add((dataset, RDF.type, VOID.Dataset))
    graph.add(
        (
            dataset,
            DCTERMS.title,
            Literal(
                "GMEOW — Global Metadata and Entity Ontology for the Web",
                lang="x-gmeow-english",
            ),
        )
    )
    graph.add(
        (
            dataset,
            DCTERMS.description,
            Literal(_fold_description(view), lang="x-gmeow-english"),
        )
    )
    graph.add((dataset, DCTERMS.license, _CC_BY))
    graph.add((dataset, DCTERMS.publisher, _PUBLISHER))
    graph.add((dataset, DCTERMS.creator, _PUBLISHER))
    graph.add((dataset, FOAF.homepage, URIRef(ONTOLOGY_IRI)))
    for link in _fold_wikidata_dataset_links(view):
        graph.add((dataset, GMEOW.authorityLink, link))
        graph.add((dataset, SKOS.exactMatch, link))
    graph.add((dataset, DCTERMS.hasVersion, Literal(_fold_version(view))))
    graph.add((dataset, VOID.uriSpace, Literal(NAMESPACE)))
    graph.add((dataset, VOID.exampleResource, URIRef(NAMESPACE + "Person")))

    # VoID statistics. void:triples is the LOD-Cloud "size" (it scales the cloud
    # bubble); the others give the standard schema census. Counted from the fold,
    # so they regenerate accurately every release rather than rotting by hand.
    stats = _fold_stats(view)
    graph.add((dataset, VOID.triples, Literal(stats.triples)))
    graph.add((dataset, VOID.classes, Literal(stats.classes)))
    graph.add((dataset, VOID.properties, Literal(stats.properties)))
    graph.add((dataset, VOID.entities, Literal(stats.entities)))

    for ext, fmt_iri in _FORMAT_IRI.items():
        graph.add((dataset, VOID.feature, URIRef(fmt_iri)))
        graph.add((dataset, VOID.dataDump, URIRef(f"{ONTOLOGY_IRI}.{ext}")))

    # Vocabularies used / aligned to.
    core_vocabs = ("owl", "rdfs", "skos", "dcterms", "gufo")
    for prefix in core_vocabs:
        graph.add((dataset, VOID.vocabulary, URIRef(PREFIXES[prefix])))
    for target in ALIGNMENT_TARGETS.values():
        graph.add((dataset, VOID.vocabulary, URIRef(target.namespace)))

    # Linksets derived from the snapshot's alignments graph.
    linksets = _fold_linksets(view)
    for subj, _pred, _obj in linksets.triples((None, RDF.type, VOID.Linkset)):
        graph.add((dataset, VOID.subset, subj))
    graph += linksets
    return graph


def build_dcat_graph(view: FoldView | None = None) -> Graph:
    """Build a DCAT dataset description with one distribution per format."""
    if view is None:
        view = load_fold()
    graph = Graph()
    bind_prefixes(graph)
    dataset = URIRef(ONTOLOGY_IRI)
    graph.add((dataset, RDF.type, DCAT.Dataset))
    graph.add((dataset, DCTERMS.title, Literal("GMEOW", lang="x-gmeow-english")))
    graph.add(
        (
            dataset,
            DCTERMS.description,
            Literal(_fold_description(view), lang="x-gmeow-english"),
        )
    )
    graph.add((dataset, DCTERMS.license, _CC_BY))
    graph.add((dataset, DCTERMS.publisher, _PUBLISHER))
    graph.add((dataset, DCAT.landingPage, URIRef(ONTOLOGY_IRI)))
    graph.add((dataset, DCTERMS.hasVersion, Literal(_fold_version(view))))
    for link in _fold_wikidata_dataset_links(view):
        graph.add((dataset, GMEOW.authorityLink, link))
        graph.add((dataset, SKOS.exactMatch, link))
    for ext, media in _MEDIA_TYPE.items():
        dist = URIRef(f"{ONTOLOGY_IRI}#dist-{ext}")
        graph.add((dist, RDF.type, DCAT.Distribution))
        graph.add((dist, DCAT.downloadURL, URIRef(f"{ONTOLOGY_IRI}.{ext}")))
        graph.add((dist, DCAT.mediaType, Literal(media, datatype=XSD.string)))
        graph.add((dataset, DCAT.distribution, dist))

    # Profile IRIs are datasets too (#330): composition made discoverable.
    # The root IRI is the core profile; <…/full> aggregates it plus every
    # extension; named profiles are slim dependency-closed subsets of full.
    from gmeow_tools.config import FULL_PROFILE_IRI, NAMED_PROFILE_NS
    from gmeow_tools.profiles_gen import dependency_closure, group_named_profiles
    from gmeow_tools.slices import discover_slices

    slices = discover_slices()
    full = URIRef(FULL_PROFILE_IRI)
    graph.add((full, RDF.type, DCAT.Dataset))
    graph.add(
        (full, DCTERMS.title, Literal("GMEOW — full profile", lang="x-gmeow-english"))
    )
    graph.add((full, DCTERMS.hasPart, dataset))
    graph.add((full, DCAT.landingPage, full))
    for name, members in sorted(group_named_profiles(slices).items()):
        profile = URIRef(NAMED_PROFILE_NS + name)
        closure = dependency_closure(members, slices)
        graph.add((profile, RDF.type, DCAT.Dataset))
        graph.add(
            (
                profile,
                DCTERMS.title,
                Literal(f"GMEOW — {name} profile", lang="x-gmeow-english"),
            )
        )
        graph.add(
            (
                profile,
                DCTERMS.description,
                Literal(
                    f"Slim dependency-closed profile: {len(members)} declared "
                    f"slice(s), {len(closure)} in the import closure.",
                    lang="x-gmeow-english",
                ),
            )
        )
        graph.add((profile, DCTERMS.isPartOf, full))
        graph.add((profile, DCAT.landingPage, profile))
    return graph


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #


@register
class MetadataGenerator(Generator):
    """Generate VoID and DCAT dataset descriptions."""

    name: str = "metadata"

    @property
    def inputs(self) -> Sequence[Path]:
        """The snapshot plus the manifests that declare profile membership."""
        from gmeow_tools.config import SLICES_DIR

        return [GTS_SNAPSHOT_FILE, *sorted(SLICES_DIR.glob("*/*/manifest.ttl"))]

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed outputs for the metadata generator."""
        return [VOID_FILE, DCAT_FILE]

    def render(self, staging: Path) -> None:
        """Render VoID and DCAT dataset descriptions."""
        void_path = staging / VOID_FILE.relative_to(PROJECT_ROOT)
        dcat_path = staging / DCAT_FILE.relative_to(PROJECT_ROOT)
        view = load_fold()
        tag_map = view.tag_map()
        write_turtle(
            void_path,
            build_void_graph(view),
            name=self.name,
            source_hash=getattr(self, "_source_hash", ""),
            tag_map=tag_map,
        )
        write_turtle(
            dcat_path,
            build_dcat_graph(view),
            name=self.name,
            source_hash=getattr(self, "_source_hash", ""),
            tag_map=tag_map,
        )
