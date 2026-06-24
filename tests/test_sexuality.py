"""Retained dynamic guards for the sexuality building block.

Asserted-TBox structural invariants (orientation facet subclassing, split-
attraction axis independence, value-vs-subclass decisions, functional/non-
functional property shapes, and the absence of flat-literal orientation
shortcuts) have been migrated to declarative slicetest cells in
slices/core/sexuality/tests/structural.ttl (#867).

This file retains only tests that cannot be expressed as module-scoped
SPARQL ASK cells:
  - test_competency_orientation_values_query: reads an external .rq file
    from COMPETENCY_DIR and asserts len(values) >= 16 (generated-artifact
    read + numeric count guard).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_competency_orientation_values_query() -> None:
    graph = _graph()
    query = (COMPETENCY_DIR / "orientation-values.rq").read_text(encoding="utf-8")
    values: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        values.add(str(row[0]))
    for ind in (
        "orientAsexual",
        "orientBisexual",
        "romanticAromantic",
        "romanticBiromantic",
    ):
        assert GMEOW + ind in values
    assert len(values) >= 16
