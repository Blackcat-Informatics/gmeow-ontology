"""Generate VoID and DCAT dataset descriptions.

VoID (with linksets) is what the LOD-Cloud submission consumes; DCAT gives a
FAIR-friendly dataset/distribution view. Linksets are pulled from the SSSOM
mappings so the cross-dataset links stay in sync with the alignment axioms.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

from rdflib import RDF, Graph, Literal, URIRef
from rdflib.namespace import DCAT, DCTERMS, FOAF, VOID, XSD

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    DCAT_FILE,
    MAPPINGS_DIR,
    MODULES_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    PREFIXES,
    PROJECT_ROOT,
    VOID_DATASET_IRI,
    VOID_FILE,
)
from gmeow_tools.generator import Generator, register, write_turtle
from gmeow_tools.graph import bind_prefixes
from gmeow_tools.mappings import build_linksets, load_mappings
from gmeow_tools.self_desc import load_self_description

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


def build_void_graph() -> Graph:
    """Build the VoID dataset description, including mapping linksets."""
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
            Literal(
                "A reasoning-centric, OWL 2 DL, upper-ontology-grounded "
                "super-vocabulary for entity, document, agreement and "
                "person-centric data.",
                lang="x-gmeow-english",
            ),
        )
    )
    graph.add((dataset, DCTERMS.license, _CC_BY))
    graph.add((dataset, DCTERMS.publisher, _PUBLISHER))
    graph.add((dataset, DCTERMS.creator, _PUBLISHER))
    graph.add((dataset, FOAF.homepage, URIRef(ONTOLOGY_IRI)))
    graph.add((dataset, DCTERMS.hasVersion, Literal(load_self_description().version)))
    graph.add((dataset, VOID.uriSpace, Literal(NAMESPACE)))
    graph.add((dataset, VOID.exampleResource, URIRef(NAMESPACE + "Person")))

    for ext, fmt_iri in _FORMAT_IRI.items():
        graph.add((dataset, VOID.feature, URIRef(fmt_iri)))
        graph.add((dataset, VOID.dataDump, URIRef(f"{ONTOLOGY_IRI}.{ext}")))

    # Vocabularies used / aligned to.
    core_vocabs = ("owl", "rdfs", "skos", "dcterms", "gufo")
    for prefix in core_vocabs:
        graph.add((dataset, VOID.vocabulary, URIRef(PREFIXES[prefix])))
    for target in ALIGNMENT_TARGETS.values():
        graph.add((dataset, VOID.vocabulary, URIRef(target.namespace)))

    # Linksets derived from the SSSOM mappings.
    linksets = build_linksets(load_mappings())
    for subj, _pred, _obj in linksets.triples((None, RDF.type, VOID.Linkset)):
        graph.add((dataset, VOID.subset, subj))
    graph += linksets
    return graph


def build_dcat_graph() -> Graph:
    """Build a DCAT dataset description with one distribution per format."""
    graph = Graph()
    bind_prefixes(graph)
    dataset = URIRef(ONTOLOGY_IRI)
    graph.add((dataset, RDF.type, DCAT.Dataset))
    graph.add((dataset, DCTERMS.title, Literal("GMEOW", lang="x-gmeow-english")))
    graph.add((dataset, DCTERMS.license, _CC_BY))
    graph.add((dataset, DCTERMS.publisher, _PUBLISHER))
    graph.add((dataset, DCAT.landingPage, URIRef(ONTOLOGY_IRI)))
    graph.add((dataset, DCTERMS.hasVersion, Literal(load_self_description().version)))
    for ext, media in _MEDIA_TYPE.items():
        dist = URIRef(f"{ONTOLOGY_IRI}#dist-{ext}")
        graph.add((dist, RDF.type, DCAT.Distribution))
        graph.add((dist, DCAT.downloadURL, URIRef(f"{ONTOLOGY_IRI}.{ext}")))
        graph.add((dist, DCAT.mediaType, Literal(media, datatype=XSD.string)))
        graph.add((dataset, DCAT.distribution, dist))
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
        """Canonical inputs for the metadata generator."""
        return [
            PROJECT_ROOT / "ontology" / "gmeow.ttl",
            *list(MODULES_DIR.glob("*.ttl")),
            *list(MAPPINGS_DIR.glob("*.sssom.tsv")),
        ]

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed outputs for the metadata generator."""
        return [VOID_FILE, DCAT_FILE]

    def render(self, staging: Path) -> None:
        """Render VoID and DCAT dataset descriptions."""
        void_path = staging / VOID_FILE.relative_to(PROJECT_ROOT)
        dcat_path = staging / DCAT_FILE.relative_to(PROJECT_ROOT)
        write_turtle(
            void_path,
            build_void_graph(),
            name=self.name,
            source_hash=getattr(self, "_source_hash", ""),
        )
        write_turtle(
            dcat_path,
            build_dcat_graph(),
            name=self.name,
            source_hash=getattr(self, "_source_hash", ""),
        )
