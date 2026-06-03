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


def test_qc_missing_definitions_is_empty() -> None:
    # The skeleton is fully annotated, so the QC check returns no offenders.
    graph = load_merged_graph(include_imports=False)
    query = (QC_DIR / "missing-definitions.rq").read_text(encoding="utf-8")
    offenders = list(graph.query(query))
    assert offenders == [], f"classes missing definitions: {offenders}"
