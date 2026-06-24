# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The statement-compile diagnostics surface (#809, #935).

Statement invariants and the RDF-1.2 ↔ OWL lossless round-trip are both checked
natively in Rust. The feedback surface calls the native statement pipeline and
unions the reports into the canonical ``Report``.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDF, Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL

from gmeow_tools import cli_dev

_GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _named(local: str) -> str:
    return f"{_GMEOW}{local}"


def _axiom(graph: Graph, ax: URIRef) -> None:
    graph.add((ax, RDF.type, OWL.Axiom))
    graph.add((ax, OWL.annotatedSource, URIRef(_named("Alice"))))
    graph.add((ax, OWL.annotatedProperty, URIRef(_named("knows"))))
    graph.add((ax, OWL.annotatedTarget, URIRef(_named("Bob"))))


def test_clean_committed_statements_compile_to_an_ok_report() -> None:
    """The committed statement DSL compiles with no error findings."""
    report = cli_dev._statement_compile_report()

    assert report.ok
    assert report.error_count == 0
    assert "statement-compile" in report.to_json()  # the tool stamp is present


def test_native_lossless_check_directions_a_dropped_triple() -> None:
    """A divergent RDF 1.2 form yields a directioned lossless finding (native diff)."""
    from gmeow_validate import check_statement_lossless

    ax = URIRef(_named("reifier/x"))
    owl = Graph()
    owl.add((URIRef(_named("Alice")), URIRef(_named("knows")), URIRef(_named("Bob"))))
    _axiom(owl, ax)

    # The normalized form LACKS the base triple — a lossy round-trip.
    divergent = Graph()
    _axiom(divergent, ax)

    report = check_statement_lossless(
        owl.serialize(format="turtle"),
        divergent.serialize(format="turtle"),
    )

    assert not report.ok
    assert report.error_count == 1
    item = report.findings[0]
    assert item["code"] == "statement-compile.lossless-round-trip"
    assert item["message"].startswith("OWL form has, RDF 1.2 lost:")
