"""Structural guards for mailing-list header ontology. Issue #131."""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, SKOS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"

_cached_graph: Graph | None = None


def _graph() -> Graph:
    """Load the merged ontology graph without imports, cached."""
    global _cached_graph
    if _cached_graph is None:
        _cached_graph = load_merged_graph(include_imports=False)
    return _cached_graph


def test_mailing_list_class_exists() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "MailingList")
    assert (node, RDF.type, OWL.Class) in graph
    assert (node, RDFS.subClassOf, URIRef(GMEOW + "InformationObject")) in graph
    assert (node, RDF.type, URIRef(GUFO + "Kind")) in graph


def test_has_mailing_list_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasMailingList")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "MailingList")) in graph
    # Non-functional: cross-posting possible
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_list_subscribe_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "listSubscribe")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, OWL.Thing) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_list_unsubscribe_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "listUnsubscribe")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, OWL.Thing) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_list_post_is_datatype_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "listPost")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


def test_list_help_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "listHelp")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, OWL.Thing) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_list_archive_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "listArchive")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, OWL.Thing) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_list_owner_is_object_property_to_email_address() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "listOwner")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "EmailAddress")) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_list_id_domain_unchanged() -> None:
    """Regression guard: listId stays on Message; MailingList uses identifier."""
    graph = _graph()
    node = URIRef(GMEOW + "listId")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Message")) in graph
    # Must NOT acquire MailingList as an additional domain (would create
    # intersection semantics in OWL, violating EL profile)
    assert (node, RDFS.domain, URIRef(GMEOW + "MailingList")) not in graph


def test_annotation_completeness() -> None:
    """Every new term carries label, definition, and isDefinedBy (Principle 8)."""
    graph = _graph()
    for term in (
        "MailingList",
        "hasMailingList",
        "listSubscribe",
        "listUnsubscribe",
        "listPost",
        "listHelp",
        "listArchive",
        "listOwner",
    ):
        node = URIRef(GMEOW + term)
        assert (node, RDFS.label, None) in graph, f"{term} missing rdfs:label"
        assert (
            node,
            SKOS.definition,
            None,
        ) in graph, f"{term} missing skos:definition"
        assert (
            node,
            RDFS.isDefinedBy,
            URIRef(GMEOW + "slices/email"),
        ) in graph, f"{term} missing rdfs:isDefinedBy"
