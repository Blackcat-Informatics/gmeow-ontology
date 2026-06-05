"""Structural guards for the audited contact-field terms (the small wins).

The 3 already-modelled concepts (nickname, birthDate, jobTitle) stay structured —
their flat schema.org/vCard forms are projection DOWNCASTS, never canonical flat
terms (greenfield rule). The genuine small gaps gain precisely-scoped terms:
gmeow:description (the one legitimately-flat note), gmeow:hasWebPage (structured web
presence), gmeow:subOrganizationOf (department/org hierarchy), and the completed
gmeow:Membership relator (member + organization roles).
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_new_small_terms_exist() -> None:
    graph = _graph()
    assert (URIRef(GMEOW + "description"), RDF.type, OWL.DatatypeProperty) in graph
    web = URIRef(GMEOW + "hasWebPage")
    assert (web, RDF.type, OWL.ObjectProperty) in graph
    assert (web, RDFS.range, URIRef(GMEOW + "WebPage")) in graph
    sub = URIRef(GMEOW + "subOrganizationOf")
    assert (sub, RDF.type, OWL.ObjectProperty) in graph
    assert (sub, RDF.type, OWL.TransitiveProperty) in graph
    assert (sub, RDFS.range, URIRef(GMEOW + "Organization")) in graph


def test_membership_relator_completed() -> None:
    graph = _graph()
    for role, rng in (
        ("membershipMember", "Agent"),
        ("membershipOrganization", "Organization"),
    ):
        node = URIRef(GMEOW + role)
        assert (node, RDFS.domain, URIRef(GMEOW + "Membership")) in graph
        assert (node, RDFS.range, URIRef(GMEOW + rng)) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


def test_no_flat_contact_terms() -> None:
    """nickname / birthDate / jobTitle / url / image are downcasts or deferred —
    never canonical flat terms."""
    graph = _graph()
    property_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "nickname",
        "nick",
        "birthDate",
        "jobTitle",
        "url",
        "image",
        "depiction",
        "depicts",
    ):
        node = URIRef(GMEOW + banned)
        for pt in property_types:
            assert (node, RDF.type, pt) not in graph, f"{banned} must not be canonical"
