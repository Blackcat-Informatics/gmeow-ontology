"""Universal identity/coreference guards (#74)."""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"
SKOS = "http://www.w3.org/2004/02/skos/core#"
EX = "https://example.org/coref/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_authority_link_is_universal_open_and_not_sameas() -> None:
    graph = _graph()
    authority = URIRef(GMEOW + "authorityLink")
    assert (authority, RDF.type, OWL.ObjectProperty) in graph
    assert (authority, RDFS.domain, URIRef(GMEOW + "Entity")) in graph
    assert not list(graph.objects(authority, RDFS.range))
    assert (authority, RDF.type, OWL.FunctionalProperty) not in graph
    assert (authority, OWL.equivalentProperty, OWL.sameAs) not in graph
    assert (authority, RDFS.subPropertyOf, OWL.sameAs) not in graph


def test_counterpart_preserves_link_without_identity_merge() -> None:
    graph = _graph()
    counterpart = URIRef(GMEOW + "counterpartOf")
    assert (counterpart, RDF.type, OWL.ObjectProperty) in graph
    assert (counterpart, RDF.type, OWL.SymmetricProperty) in graph
    assert (counterpart, RDF.type, OWL.TransitiveProperty) not in graph
    assert (counterpart, OWL.equivalentProperty, OWL.sameAs) not in graph
    assert (counterpart, RDFS.subPropertyOf, OWL.sameAs) not in graph


def test_universal_version_and_edition_lineage_terms() -> None:
    graph = _graph()
    for prop in ("versionOf", "editionOf", "supersedes"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (URIRef(GMEOW + "versionOf"), RDF.type, OWL.FunctionalProperty) in graph
    assert (URIRef(GMEOW + "editionOf"), RDF.type, OWL.FunctionalProperty) in graph
    assert (URIRef(GMEOW + "supersedes"), RDF.type, OWL.FunctionalProperty) not in graph
    assert (URIRef(GMEOW + "versionOf"), RDFS.domain, URIRef(GMEOW + "Entity")) in graph


def test_no_preferred_or_primary_coreference_terms() -> None:
    graph = _graph()
    for banned in (
        "primaryAuthority",
        "preferredAuthority",
        "primaryCoreference",
        "preferredCoreference",
        "primaryIdentity",
        "preferredIdentity",
    ):
        node = URIRef(GMEOW + banned)
        assert (node, RDF.type, OWL.Class) not in graph
        assert (node, RDF.type, OWL.ObjectProperty) not in graph
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph


def test_authority_link_without_match_strength_warns_only() -> None:
    bare = Graph()
    bare.add(
        (
            URIRef(EX + "entity"),
            URIRef(GMEOW + "authorityLink"),
            URIRef(EX + "authority"),
        )
    )
    result = run_shacl(bare)
    assert result.ok
    assert any("authority link should also assert" in w for w in result.warnings)


def test_schema_sameas_projection_requires_exact_authority_match() -> None:
    src = load_merged_graph(include_imports=False)
    src.parse(FIXTURES_DIR / "coreference.ttl", format="turtle")
    projected = project_graph("schema-org", src)

    subject = URIRef(EX + "recordedPerson")
    exact = URIRef(EX + "authority/person-123")
    close = URIRef(EX + "authority/person-near")
    assert (subject, URIRef(SCHEMA + "sameAs"), exact) in projected
    assert (subject, URIRef(SCHEMA + "sameAs"), close) not in projected

    # Source keeps the SKOS distinction; projection does not leak GMEOW predicates.
    assert (subject, URIRef(SKOS + "closeMatch"), close) in src
    assert not any(str(p).startswith(GMEOW) for _, p, _ in projected)
