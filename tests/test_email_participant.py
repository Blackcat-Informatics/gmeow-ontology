"""Structural guards for the MessageParticipant relator and EmailAddress facets.

Issue #159: Email address occurrences — the relator binds a message, an address,
and a header/envelope role, keeping display-name and raw-value claims scoped to
the occurrence rather than asserting them as global facts about the EmailAddress.
"""

from __future__ import annotations

from pathlib import Path

import yaml
from purrdf.compat.rdflib import Graph, URIRef
from purrdf.compat.rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


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
    graph.parse(_fixture_path(), format="turtle")
    alice = URIRef("https://example.org/mail/addrAlice")
    msg1 = URIRef("https://example.org/mail/msg1")
    msg2 = URIRef("https://example.org/mail/msg2")
    role_from = URIRef(GMEOW + "messageRoleFrom")
    role_to = URIRef(GMEOW + "messageRoleTo")
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
    msg1_alice = []
    for r in results:
        assert isinstance(r, ResultRow)
        if (
            str(r[1]) == str(msg1)
            and str(r[2]) == str(alice)
            and (str(r[3]) == str(role_from))
        ):
            msg1_alice.append((r[0], r[4]))
    assert len(msg1_alice) == 1
    assert str(msg1_alice[0][1]) == "Alice Smith"
    msg2_alice = []
    for r in results:
        assert isinstance(r, ResultRow)
        if (
            str(r[1]) == str(msg2)
            and str(r[2]) == str(alice)
            and (str(r[3]) == str(role_to))
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
