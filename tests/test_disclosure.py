"""Tests for the consumer projection policy / disclosure control facility.

Covers the ontology structure (ProjectionContext and DisclosurePolicy as universal
QualityValue types, eligibleForConsumer and hasDisclosurePolicy as domain-free
non-functional ObjectProperties), orthogonality to other axes, no preferred/primary
term, the projectWhen compiler guard, SHACL leak shapes, and per-consumer projection
gating.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from purrdf.compat.rdflib import Graph, Namespace, URIRef, Variable
from purrdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.slices import module_path

GM = Namespace(NAMESPACE)
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")
SCHEMA = Namespace("https://schema.org/")
EX = Namespace("https://example.org/disclosure/")

COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _projection_source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(COVERAGE_FIXTURES / "disclosure.ttl", format="turtle")
    return graph


# --------------------------------------------------------------------------- #
# No preferred / primary (Principle 9)
# --------------------------------------------------------------------------- #


def test_no_preferred_or_primary_disclosure_term() -> None:
    """No gmeow:primary* / gmeow:preferred* disclosure term."""
    module = Graph().parse(
        module_path("kernel"),
        format="turtle",
    )
    offenders = []
    for s in set(module.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(NAMESPACE):
            continue
        local = str(s)[len(NAMESPACE) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], offenders


# --------------------------------------------------------------------------- #
# Projections — per-consumer gating
# --------------------------------------------------------------------------- #


def test_project_when_in_sparql_query() -> None:
    """The schema-org SPARQL projection contains the projectWhen FILTER EXISTS guard."""
    from gmeow_tools.config import PROJECTION_QUERY_DIR

    query = (PROJECTION_QUERY_DIR / "schema-org.rq").read_text(encoding="utf-8")
    assert (
        "FILTER EXISTS { ?ent gmeow:eligibleForConsumer gmeow:consumerPublicSite . }"
        in query
    )


# --------------------------------------------------------------------------- #
# Competency queries (graph-query answerability)
# --------------------------------------------------------------------------- #


def test_public_candidates_query_runnable() -> None:
    """public-candidates.rq must execute without error against the coverage fixture.

    Binds ?consumer to consumerPublicSite to verify parameterised filtering.
    """
    from gmeow_tools.config import QUERIES_DIR

    query_path = QUERIES_DIR / "competency" / "public-candidates.rq"
    if not query_path.exists():
        pytest.fail(f"Missing required competency query: {query_path}")
    src = _projection_source()
    result = src.query(
        query_path.read_text(encoding="utf-8"),
        initBindings={Variable("consumer"): GM.consumerPublicSite},
    )
    # Must return at least the public-safe alice.
    rows = list(result)
    assert any(
        isinstance(r, ResultRow) and any(str(v) == str(EX["alice"]) for v in r)
        for r in rows
    ), rows


def test_privacy_leaks_query_runnable() -> None:
    """privacy-leaks.rq must execute without error and find the leak case."""
    from gmeow_tools.config import QUERIES_DIR

    query_path = QUERIES_DIR / "competency" / "privacy-leaks.rq"
    if not query_path.exists():
        pytest.fail(f"Missing required competency query: {query_path}")
    src = _projection_source()
    result = src.query(query_path.read_text(encoding="utf-8"))
    rows = list(result)
    assert any(
        isinstance(r, ResultRow) and any(str(v) == str(EX["secretPlace"]) for v in r)
        for r in rows
    ), rows
