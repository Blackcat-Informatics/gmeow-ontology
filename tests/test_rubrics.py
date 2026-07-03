# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The rubrics facility, in the norms slice.

All structural invariants and the exemplar-polarity competency question have been
migrated to the slice-resident declarative test-DSL:

  - slices/extensions/norms/tests/structural.ttl
  - slices/extensions/norms/tests/competency.ttl

What remains here are tests that cannot be expressed as module-scoped SPARQL
ASK/SELECT cells:

  - test_no_preferred_assessment_machinery: dynamic sweep over the merged graph
    checking that no preferred/canonical/primary assessment term exists.
  - test_two_judges_disagree_without_contradiction: fixture-level assessment
    value check asserting two co-equal assessment cells stand.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import RDF, Graph, Literal, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
EX = Namespace("https://example.org/shapes/")

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Dynamic / fixture residue
# --------------------------------------------------------------------------- #


def test_no_preferred_assessment_machinery() -> None:
    """No preferredScore / canonicalAssessment selectors (Principle 9): two
    judges disagreeing are two coexisting cells."""
    g = _graph()
    banned = (
        "preferredscore",
        "canonicalassessment",
        "primaryassessment",
        "preferredassessment",
    )
    offenders = [
        str(s)
        for s in set(g.subjects())
        if str(s).startswith(GMEOW)
        and "/" not in str(s)[len(GMEOW) :]
        and str(s)[len(GMEOW) :].lower().startswith(banned)
    ]
    assert offenders == []


def test_two_judges_disagree_without_contradiction() -> None:
    """The LLM-judge doctrine in fixture form: one chunk, two vantages, two
    scores — both cells stand."""
    g = _fixture("rubrics-wellformed")
    scores: dict[object, float] = {}
    for a in g.subjects(RDF.type, GM.Assessment):
        value = g.value(a, GM.assessmentScoreValue)
        assert isinstance(value, Literal)
        scores[g.value(a, GM.vantage)] = float(value.toPython())
    assert scores[EX.judgeA] == 0.9
    assert scores[EX.judgeB] == 0.4
