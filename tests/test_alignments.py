"""Tests for the canonical-term alignments (the superset mechanism)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import ALIGNMENT_TARGETS, LinkPolicy
from gmeow_tools.mappings import build_alignment_graph, load_mappings

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return build_alignment_graph(load_mappings())


def test_person_unifies_across_vocabularies() -> None:
    graph = _graph()
    person = URIRef(GMEOW + "Person")
    equivalents = {str(o) for o in graph.objects(person, OWL.equivalentClass)}
    assert "http://xmlns.com/foaf/0.1/Person" in equivalents
    assert "https://schema.org/Person" in equivalents
    assert "http://www.w3.org/2000/10/swap/pim/gedcom#Individual" in equivalents


def test_software_project_aligned_to_doap() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "SoftwareProject"),
        OWL.equivalentClass,
        URIRef("http://usefulinc.com/ns/doap#Project"),
    ) in graph


def test_kinship_property_alignment() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasParent"),
        OWL.equivalentProperty,
        URIRef("http://purl.org/vocab/relationship/childOf"),
    ) in graph


def test_genealogy_events_aligned_to_bio() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Birth"),
        OWL.equivalentClass,
        URIRef("http://purl.org/vocab/bio/0.1/Birth"),
    ) in graph


def test_parentchild_relationship_typed() -> None:
    # The reified parent-child relationship aligns to the GEDCOM X type.
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "BiologicalParentChild"),
        SKOS.closeMatch,
        URIRef("http://gedcomx.org/BiologicalParent"),
    ) in graph


def test_email_message_equivalent_to_schema() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "EmailMessage"),
        OWL.equivalentClass,
        URIRef("https://schema.org/EmailMessage"),
    ) in graph


def test_email_participants_aligned_to_schema() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    # The author/recipient role properties close-match their schema.org peers.
    assert (
        URIRef(GMEOW + "from"),
        SKOS.closeMatch,
        URIRef("https://schema.org/sender"),
    ) in graph
    assert (
        URIRef(GMEOW + "to"),
        SKOS.closeMatch,
        URIRef("https://schema.org/toRecipient"),
    ) in graph


def test_trust_aligned_to_wot_schema() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "CryptographicKey"),
        SKOS.closeMatch,
        URIRef("http://xmlns.com/wot/0.1/PubKey"),
    ) in graph
    assert (
        URIRef(GMEOW + "fingerprint"),
        SKOS.closeMatch,
        URIRef("http://xmlns.com/wot/0.1/fingerprint"),
    ) in graph


def test_relationships_aligned_to_rel_vocab() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "hasMet"),
        SKOS.closeMatch,
        URIRef("http://purl.org/vocab/relationship/hasMet"),
    ) in graph


def test_wot_is_reference_only() -> None:
    # The WOT schema's license is unknown → fails safe to reference-only (linked,
    # never imported).
    assert ALIGNMENT_TARGETS["wot"].policy is LinkPolicy.REFERENCE_ONLY


def test_import_provenance_aligned_to_prov_and_dcterms() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "sourceModifiedAt"),
        SKOS.closeMatch,
        URIRef("http://purl.org/dc/terms/modified"),
    ) in graph
    assert (
        URIRef(GMEOW + "assertedAt"),
        SKOS.closeMatch,
        URIRef("http://www.w3.org/ns/prov#generatedAtTime"),
    ) in graph


def test_schema_is_reference_only() -> None:
    # schema.org (CC-BY-SA) may be linked but never imported.
    assert ALIGNMENT_TARGETS["schema"].policy is LinkPolicy.REFERENCE_ONLY


def test_all_mappings_expand() -> None:
    # No CURIE in any mapping row fails to expand (would raise MappingError).
    graph = _graph()
    assert len(graph) >= 80
