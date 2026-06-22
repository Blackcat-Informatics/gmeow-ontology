"""The rdflib ↔ gmeow_rdf engine-equivalence gate (#242).

This is the trust anchor that licenses the rest of the suite (and the projection
executor) to run on the fast gmeow_rdf engine: every committed query must return
the same answers under both engines. The negative test proves the gate actually
fires when the engines would disagree.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

# The engine-equivalence gate compares REAL upstream rdflib against gmeow_rdf — it
# is the one place that legitimately uses rdflib (the independent oracle). With
# rdflib dropped from the runtime (purrdf P0, #834) it is installed only via the
# `.[crosscheck]` extra, so skip-collect this whole module when rdflib is absent.
pytest.importorskip("rdflib")

from rdflib import RDF, XSD, Graph, Literal, URIRef

from gmeow_tools import sparql
from gmeow_tools.engine_crosscheck import (
    ENGINE_CROSSCHECK_STEM,
    RULE_AGREEMENT,
    RULE_DIVERGENCE,
    RULE_SKIPPED,
    CrosscheckResult,
    build_report,
    crosscheck_all,
    crosscheck_query,
    run,
)

_WIDGET = URIRef("https://example.org/Widget")


@pytest.fixture(scope="module")
def crosscheck_results() -> list[CrosscheckResult]:
    """Run the full engine cross-check once and share it across tests."""
    return crosscheck_all()


def test_every_committed_query_agrees_across_engines(
    crosscheck_results: list[CrosscheckResult],
) -> None:
    """rdflib and gmeow_rdf return identical answers for every committed query."""
    diverged = [r for r in crosscheck_results if not r.agree and not r.skipped]
    assert not diverged, "engine divergence:\n" + "\n".join(
        f"  [{r.form}] {r.name}: {r.detail}" for r in diverged
    )
    # Sanity: the gate actually exercised a meaningful number of queries.
    checked = [r for r in crosscheck_results if not r.skipped]
    assert len(checked) >= 50


def test_skips_are_only_multi_query_demo_files(
    crosscheck_results: list[CrosscheckResult],
) -> None:
    """Any skipped file is skipped because BOTH engines reject it (not one-sided)."""
    for result in crosscheck_results:
        if result.skipped:
            assert "both engines rejected" in result.detail


def test_crosscheck_detects_a_real_divergence() -> None:
    """A query whose answer depends on a deliberately diverged store fails the gate.

    We give the two engines *different* data for the same query: rdflib sees an
    extra triple gmeow_rdf does not. The cross-check must report disagreement —
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
    store = sparql.store_from_graph(g)  # gmeow_rdf canonicalizes to "645"
    result = crosscheck_query("synthetic/decimal.rq", query, g, store)
    assert result.agree


# --------------------------------------------------------------------------- #
# build_report / run — the surface is first-class output, not stdout (#667)
# --------------------------------------------------------------------------- #


def test_build_report_maps_each_outcome_to_its_severity() -> None:
    """A diverged/skipped/agree triad maps to error/note/info findings (#667)."""
    results = [
        CrosscheckResult("audit/a.rq", "SELECT", agree=True),
        CrosscheckResult("audit/b.rq", "ASK", agree=False, detail="rdflib=1 native=0"),
        CrosscheckResult(
            "audit/c.rq",
            "SELECT",
            agree=True,
            detail="both engines rejected: x",
            skipped=True,
        ),
    ]
    report = build_report(results)

    # The one real divergence is error-severity and fails the surface.
    assert report.error_count == 1
    assert not report.ok

    sarif = json.loads(report.to_sarif())
    sarif_results = sarif["runs"][0]["results"]
    rule_ids = {r["ruleId"] for r in sarif_results}
    assert {RULE_AGREEMENT, RULE_DIVERGENCE, RULE_SKIPPED} <= rule_ids

    # The agreement summary counts the surface (1 agreed of 2 checked, 1 skipped).
    messages = [r["message"]["text"] for r in sarif_results]
    assert any("1/2 queries agree" in m for m in messages)
    # The diverged query is named in its finding; agreeing queries are NOT spammed.
    assert any("audit/b.rq" in m for m in messages)
    assert sum("audit/a.rq" in m for m in messages) == 0


def test_build_report_all_agree_is_ok() -> None:
    """With no divergence the report is clean (info-only)."""
    results = [
        CrosscheckResult("audit/a.rq", "SELECT", agree=True),
        CrosscheckResult("audit/b.rq", "ASK", agree=True),
    ]
    report = build_report(results)
    assert report.ok
    assert report.error_count == 0


def test_run_writes_artifacts_and_passes_on_the_real_surface(
    tmp_path: Path,
) -> None:
    """``run`` cross-checks the committed queries and writes JSON/SARIF/HTML (#667)."""
    passed, results, _report = run(output_dir=tmp_path)

    assert passed, "engine cross-check unexpectedly diverged: " + "\n".join(
        f"  [{r.form}] {r.name}: {r.detail}"
        for r in results
        if not r.agree and not r.skipped
    )
    for kind in ("json", "sarif", "html"):
        artifact = tmp_path / f"{ENGINE_CROSSCHECK_STEM}.{kind}"
        assert artifact.exists(), f"missing {kind} artifact"
    assert (
        json.loads(
            (tmp_path / f"{ENGINE_CROSSCHECK_STEM}.sarif").read_text(encoding="utf-8")
        )["version"]
        == "2.1.0"
    )
