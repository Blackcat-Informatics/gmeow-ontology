# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Narrative time frames (#359, EPIC #358).

The chapter sequence is the syuzhet projection of frame-relative story content.
Discourse time (order of telling) and story time (fabula — order of happening)
are distinct NarrativeTimeFrames; the same diegetic event may carry coexisting
positions in both, and their disagreement is the flashback made queryable
rather than a contradiction (Principle 9). A position without its frame is the
bare-integer anti-pattern, forbidden by SHACL.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
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
# Structural invariants
# --------------------------------------------------------------------------- #


def test_frame_properties_are_functional_with_correct_anchors() -> None:
    g = _graph()
    for prop, rng in [
        (GM.narrativeTimeAxis, GM.NarrativeTimeAxis),
        (GM.discourseTimeOf, GM.CreativeWork),
        (GM.storyTimeOf, GM.NarrativeReferenceFrame),
        (GM.positionFrame, GM.NarrativeTimeFrame),
    ]:
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
        assert (prop, RDFS.range, rng) in g, prop


def test_at_narrative_position_is_domain_free_and_not_functional() -> None:
    """The one anchor reused by the seam (#360), arcs (#361), motifs (#363).

    NOT functional: an event holds coexisting positions in discourse and story
    frames — the flashback IS that coexistence.
    """
    g = _graph()
    assert (GM.atNarrativePosition, RDF.type, OWL.ObjectProperty) in g
    assert g.value(GM.atNarrativePosition, RDFS.domain) is None
    assert (GM.atNarrativePosition, RDF.type, OWL.FunctionalProperty) not in g
    assert (GM.atNarrativePosition, RDFS.range, GM.NarrativePosition) in g


# --------------------------------------------------------------------------- #
# The flashback round-trip: two orderings, no contradiction
# --------------------------------------------------------------------------- #


def test_flashback_fixture_carries_coexisting_orders() -> None:
    g = _fixture("narrative-time-wellformed")
    positions = list(g.objects(EX.betrayalEvent, GM.atNarrativePosition))
    assert len(positions) == 2
    by_frame: dict[object, int] = {}
    for p in positions:
        ordinal = g.value(p, GM.positionOrdinal)
        assert isinstance(ordinal, Literal)
        by_frame[g.value(p, GM.positionFrame)] = int(ordinal.toPython())
    # Discourse says 31; story says 1. Both stand.
    assert by_frame[EX.discourseFrame] == 31
    assert by_frame[EX.storyFrame] == 1


def test_competency_narrative_time_axes_query() -> None:
    query = (COMPETENCY_DIR / "narrative-time-axes.rq").read_text(encoding="utf-8")
    axes: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        axes.add(row[0])
    assert axes == {GM.axisDiscourseTime, GM.axisStoryTime}
