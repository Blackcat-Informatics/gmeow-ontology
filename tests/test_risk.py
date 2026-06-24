# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The risk slice (#354, EPIC #348) — retained pytest tests.

Structural TBox invariants have been migrated to the declarative slicetest
DSL in slices/extensions/risk/tests/structural.ttl. Only tests that cannot
be expressed as module-scoped SPARQL ASK cells are retained here:
  - test_no_occurrence_gate: multi-file ABox dynamic check
  - test_wellformed_risk_fixture_conforms: ExampleConformance via run_shacl
  - test_malformed_risk_fixture_is_flagged: ExampleConformance via run_shacl
  - test_competency_severity_order_query: external .rq file / result-set count
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


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
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_risk_fixture_conforms() -> None:
    result = run_shacl(_fixture("risk-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_risk_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("risk-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "exactly one gmeow:hazardBearer" in errors
    assert "at least one feared gmeow:manifestedAsType" in errors
    assert "antecedent and consequent must be distinct" in errors
    assert "exactly one gmeow:causalModality" in errors
    assert "no causal link may reach itself" in errors
    assert "an ungraded cascade is just a story" in errors
    assert "at least one gmeow:mitigationMeasure" in errors
    assert "CausalLink (barrier on the chain) or a Hazard" in errors


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
