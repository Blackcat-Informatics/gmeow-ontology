"""Tests that the competency and QC SPARQL queries behave as expected."""

from __future__ import annotations

from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR, NAMESPACE, QC_DIR
from gmeow_tools.graph import load_merged_graph


def test_competency_agents_query() -> None:
    graph = load_merged_graph(include_imports=False)
    query = (COMPETENCY_DIR / "agents.rq").read_text(encoding="utf-8")
    results: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        results.add(str(row[0]))
    # Agent and its skeleton subclasses must be returned.
    for term in ("Agent", "Person", "Organization"):
        assert NAMESPACE + term in results


def _query_terms(filename: str) -> set[str]:
    graph = load_merged_graph(include_imports=False)
    query = (COMPETENCY_DIR / filename).read_text(encoding="utf-8")
    terms: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        terms.add(str(row[0]))
    return terms


def test_competency_works_query() -> None:
    terms = _query_terms("works.rq")
    for term in ("CreativeWork", "Article", "Patent", "Dataset", "SoftwareProject"):
        assert NAMESPACE + term in terms


def test_competency_kinship_query() -> None:
    terms = _query_terms("kinship.rq")
    for term in ("hasParent", "hasChild", "hasSpouse", "hasSibling"):
        assert NAMESPACE + term in terms


def test_competency_life_events_query() -> None:
    terms = _query_terms("life-events.rq")
    # A comprehensive genealogy slice models many life-event types.
    for term in ("Birth", "Death", "Marriage", "Burial", "Census", "Adoption"):
        assert NAMESPACE + term in terms
    assert len(terms) >= 25


def test_competency_email_participants_query() -> None:
    terms = _query_terms("email-participants.rq")
    # Every RFC 5322 role property routes through the EmailAddress seam.
    for term in ("from", "sender", "replyTo", "to", "cc", "bcc"):
        assert NAMESPACE + term in terms


def test_competency_message_trust_query() -> None:
    terms = _query_terms("message-trust.rq")
    for term in (
        "CryptographicSignature",
        "DKIMSignature",
        "SMIMESignature",
        "PGPSignature",
    ):
        assert NAMESPACE + term in terms


def test_qc_missing_definitions_is_empty() -> None:
    # The skeleton is fully annotated, so the QC check returns no offenders.
    graph = load_merged_graph(include_imports=False)
    query = (QC_DIR / "missing-definitions.rq").read_text(encoding="utf-8")
    offenders = list(graph.query(query))
    assert offenders == [], f"classes missing definitions: {offenders}"
