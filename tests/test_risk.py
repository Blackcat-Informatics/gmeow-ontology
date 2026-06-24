# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The risk slice (#354, EPIC #348) — retained pytest tests.

Structural TBox invariants have been migrated to the declarative slicetest
DSL in slices/extensions/risk/tests/structural.ttl. SHACL fixture tests have
been migrated to crates/validate/tests/conformance_risk.rs (#867). Only tests
that cannot be expressed as module-scoped SPARQL ASK cells or Rust conformance
twins are retained here:
  - test_no_occurrence_gate: multi-file ABox dynamic check
  - test_competency_severity_order_query: external .rq file / result-set count
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_no_occurrence_gate() -> None:
    """The EPIC #358-pattern gate: loading the risk fixtures and worked
    example entails ZERO gmeow:Event instances — cascades are expressible
    without anything having happened."""
    g = Graph()
    g.parse(FIXTURES / "risk-wellformed.ttl", format="turtle")
    g.parse(
        Path(__file__).parent.parent
        / "slices"
        / "extensions"
        / "risk"
        / "examples"
        / "trust-collapse.ttl",
        format="turtle",
    )
    assert list(g.subjects(RDF.type, GM.Event)) == []
    # And the feared kinds ARE present — as types.
    assert len(list(g.subjects(RDF.type, GM.EventType))) >= 3


# --------------------------------------------------------------------------- #
# Competency
# --------------------------------------------------------------------------- #


def test_competency_severity_order_query() -> None:
    query_path = COMPETENCY_DIR / "risk-severity-order.rq"
    query = query_path.read_text(encoding="utf-8")
    tops: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        tops.add(row[0])
    assert tops == {GM.severityCatastrophic}
