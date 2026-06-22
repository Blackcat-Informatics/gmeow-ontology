"""Structural guards for the MessageParticipant relator and EmailAddress facets.

Issue #159: Email address occurrences — the relator binds a message, an address,
and a header/envelope role, keeping display-name and raw-value claims scoped to
the occurrence rather than asserting them as global facts about the EmailAddress.
"""

from __future__ import annotations

from pathlib import Path

import yaml
from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, XSD, Graph, URIRef
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_message_participant_class_exists() -> None:
    graph = _graph()
    assert (URIRef(GMEOW + "MessageParticipant"), RDF.type, OWL.Class) in graph


def test_message_participant_role_class_exists() -> None:
    graph = _graph()
    assert (URIRef(GMEOW + "MessageParticipantRole"), RDF.type, OWL.Class) in graph


def test_role_seed_individuals_exist() -> None:
    graph = _graph()
    for role in (
        "messageRoleFrom",
        "messageRoleSender",
        "messageRoleReplyTo",
        "messageRoleTo",
        "messageRoleCc",
        "messageRoleBcc",
        "messageRoleReturnPath",
        "messageRoleErrorsTo",
        "messageRoleEnvelopeFrom",
        "messageRoleEnvelopeTo",
        "messageRoleDeliveredTo",
        "messageRoleOriginalTo",
        "messageRoleResentFrom",
        "messageRoleResentTo",
        "messageRoleResentCc",
    ):
        node = URIRef(GMEOW + role)
        assert (node, RDF.type, URIRef(GMEOW + "MessageParticipantRole")) in graph, (
            f"{role} missing"
        )


def test_participant_properties_are_declared() -> None:
    graph = _graph()
    for prop, rng in (
        ("participantMessage", "EmailMessage"),
        ("participantAddress", "EmailAddress"),
        ("participantRole", "MessageParticipantRole"),
        ("participantHeader", "MessageHeader"),
    ):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph, (
            f"{prop} not an ObjectProperty"
        )
        assert (node, RDFS.domain, URIRef(GMEOW + "MessageParticipant")) in graph
        assert (node, RDFS.range, URIRef(GMEOW + rng)) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph, (
            f"{prop} not functional"
        )


def test_has_message_participant_inverse() -> None:
    graph = _graph()
    has_p = URIRef(GMEOW + "hasMessageParticipant")
    part_m = URIRef(GMEOW + "participantMessage")
    assert (has_p, RDF.type, OWL.ObjectProperty) in graph
    assert (has_p, OWL.inverseOf, part_m) in graph
    assert (has_p, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (has_p, RDFS.range, URIRef(GMEOW + "MessageParticipant")) in graph


def test_participant_ordinal_is_functional() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "participantOrdinal")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "MessageParticipant")) in graph
    assert (node, RDFS.range, XSD.nonNegativeInteger) in graph


def test_display_name_scoped_to_participant() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "displayName")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "MessageParticipant")) in graph
    # Must NOT be declared with domain EmailAddress
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailAddress")) not in graph


def test_raw_address_value_scoped_to_participant() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "rawAddressValue")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "MessageParticipant")) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailAddress")) not in graph


def test_stable_address_properties_on_email_address() -> None:
    graph = _graph()
    for prop in ("addressValue", "localPart", "domainPart"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.DatatypeProperty) in graph, f"{prop} missing"
        assert (node, RDF.type, OWL.FunctionalProperty) in graph, (
            f"{prop} not functional"
        )
        assert (node, RDFS.domain, URIRef(GMEOW + "EmailAddress")) in graph


def test_participant_group_is_literal() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "participantGroup")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "MessageParticipant")) in graph


def test_resent_date_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "resentDate")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, XSD.dateTime) in graph
    # Multiple resent blocks possible
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_resent_message_id_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "resentMessageId")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph
    # Multiple resent blocks possible
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_resent_properties_are_multivalued_in_linkml_schema() -> None:
    """Regression guard: non-functional datatype properties must compile to
    multivalued slots (issue #134 review feedback)."""
    linkml_path = (
        Path(__file__).parent.parent / "generated" / "schemas" / "gmeow.linkml.yaml"
    )
    with linkml_path.open() as f:
        schema = yaml.safe_load(f)
    slots = schema.get("slots", {})
    assert slots.get("resentDate", {}).get("multivalued") is True
    assert slots.get("resentMessageId", {}).get("multivalued") is True


def _fixture_path() -> str:
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


def test_fixture_binds_occurrence_correctly() -> None:
    """The coverage fixture shows alice@example.org in msg1 From and msg2 To
    with different display names."""
    graph = load_merged_graph(include_imports=False)
    # Load the fixture explicitly so the instance data is present.
    graph.parse(_fixture_path(), format="turtle")

    alice = URIRef("https://example.org/mail/addrAlice")
    msg1 = URIRef("https://example.org/mail/msg1")
    msg2 = URIRef("https://example.org/mail/msg2")
    role_from = URIRef(GMEOW + "messageRoleFrom")
    role_to = URIRef(GMEOW + "messageRoleTo")

    # Find the MessageParticipant nodes by querying
    q = """
    SELECT ?mp ?msg ?addr ?role ?display WHERE {
        ?mp a <https://blackcatinformatics.ca/gmeow/MessageParticipant> ;
            <https://blackcatinformatics.ca/gmeow/participantMessage> ?msg ;
            <https://blackcatinformatics.ca/gmeow/participantAddress> ?addr ;
            <https://blackcatinformatics.ca/gmeow/participantRole> ?role ;
            <https://blackcatinformatics.ca/gmeow/displayName> ?display .
    }
    """
    results = list(graph.query(q))
    assert len(results) >= 4, f"expected at least 4 participants, got {len(results)}"

    # msg1 has alice as From with displayName "Alice Smith"
    msg1_alice = []
    for r in results:
        assert isinstance(r, ResultRow)
        if (
            str(r[1]) == str(msg1)
            and str(r[2]) == str(alice)
            and str(r[3]) == str(role_from)
        ):
            msg1_alice.append((r[0], r[4]))
    assert len(msg1_alice) == 1
    assert str(msg1_alice[0][1]) == "Alice Smith"

    # msg2 has alice as To with displayName "Dr. Alice Smith"
    msg2_alice = []
    for r in results:
        assert isinstance(r, ResultRow)
        if (
            str(r[1]) == str(msg2)
            and str(r[2]) == str(alice)
            and str(r[3]) == str(role_to)
        ):
            msg2_alice.append((r[0], r[4]))
    assert len(msg2_alice) == 1
    assert str(msg2_alice[0][1]) == "Dr. Alice Smith"


def test_fixture_address_decomposition() -> None:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    alice = URIRef("https://example.org/mail/addrAlice")
    assert (alice, URIRef(GMEOW + "addressValue"), None) in graph
    assert (alice, URIRef(GMEOW + "localPart"), None) in graph
    assert (alice, URIRef(GMEOW + "domainPart"), None) in graph
