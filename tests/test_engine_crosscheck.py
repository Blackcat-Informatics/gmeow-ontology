"""The rdflib ↔ pyoxigraph engine-equivalence gate (#242).

This is the trust anchor that licenses the rest of the suite (and the projection
executor) to run on the fast pyoxigraph engine: every committed query must return
the same answers under both engines. The negative test proves the gate actually
fires when the engines would disagree.
"""

from __future__ import annotations

from rdflib import RDF, XSD, Graph, Literal, URIRef

from gmeow_tools import sparql
from gmeow_tools.engine_crosscheck import crosscheck_all, crosscheck_query

_WIDGET = URIRef("https://example.org/Widget")


def test_every_committed_query_agrees_across_engines() -> None:
    """rdflib and pyoxigraph return identical answers for every committed query."""
    results = crosscheck_all()
    diverged = [r for r in results if not r.agree and not r.skipped]
    assert not diverged, "engine divergence:\n" + "\n".join(
        f"  [{r.form}] {r.name}: {r.detail}" for r in diverged
    )
    # Sanity: the gate actually exercised a meaningful number of queries.
    checked = [r for r in results if not r.skipped]
    assert len(checked) >= 50


def test_skips_are_only_multi_query_demo_files() -> None:
    """Any skipped file is skipped because BOTH engines reject it (not one-sided)."""
    for result in crosscheck_all():
        if result.skipped:
            assert "both engines rejected" in result.detail


def test_crosscheck_detects_a_real_divergence() -> None:
    """A query whose answer depends on a deliberately diverged store fails the gate.

    We give the two engines *different* data for the same query: rdflib sees an
    extra triple pyoxigraph does not. The cross-check must report disagreement —
    proving the gate is not vacuously green.
    """
    query = "SELECT ?s WHERE { ?s a <https://example.org/Widget> }"
    rdflib_graph = Graph()
    rdflib_graph.add((URIRef("https://example.org/w1"), RDF.type, _WIDGET))
    empty_store = sparql.store_with()  # merged ontology only — no Widget
    result = crosscheck_query(
        "synthetic/divergent.rq", query, rdflib_graph, empty_store
    )
    assert not result.agree
    assert not result.skipped


def test_crosscheck_decimal_values_compare_equal() -> None:
    """Value-based comparison: ``645.0`` and ``645`` (xsd:decimal) are equal."""
    query = "SELECT ?o WHERE { ?s <https://example.org/p> ?o }"
    g = Graph()
    g.add(
        (
            URIRef("https://example.org/s"),
            URIRef("https://example.org/p"),
            Literal("645.0", datatype=XSD.decimal),
        )
    )
    store = sparql.store_from_graph(g)  # pyoxigraph canonicalizes to "645"
    result = crosscheck_query("synthetic/decimal.rq", query, g, store)
    assert result.agree
