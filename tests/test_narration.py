# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The narration seam (#360, EPIC #358).

NOnt's reference function between text and story: neither mereology nor
participation. Flat narrates/narratedIn by default (one quad per link — the
efficiency doctrine at 38k-link corpus scale); a reified Depiction ONLY when
mode/vantage/confidence rides on the link, and then it must say why (mode
required). No inverseOf between the orientations — EL-clean, query both.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Namespace
from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
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


def test_seam_links_specialize_one_ancestor() -> None:
    g = _graph()
    assert (GM.narrationLink, RDF.type, OWL.ObjectProperty) in g
    assert (GM.narrates, RDFS.subPropertyOf, GM.narrationLink) in g
    assert (GM.narratedIn, RDFS.subPropertyOf, GM.narrationLink) in g
    # The ancestor is domain- and range-free for media-specific seams.
    assert g.value(GM.narrationLink, RDFS.domain) is None
    assert g.value(GM.narrationLink, RDFS.range) is None


def test_orientations_are_not_inverse_axioms() -> None:
    """No owl:inverseOf between narrates and narratedIn: EL stays clean and
    either orientation is usable without entailing the other (the connectsTo
    convention). Consumers query both."""
    g = _graph()
    assert g.value(GM.narrates, OWL.inverseOf) is None
    assert g.value(GM.narratedIn, OWL.inverseOf) is None
    assert (GM.narrates, RDFS.domain, GM.ContentSegment) in g
    assert (GM.narratedIn, RDFS.range, GM.ContentSegment) in g
    # Open on the content side, per the seam doctrine.
    assert g.value(GM.narrates, RDFS.range) is None
    assert g.value(GM.narratedIn, RDFS.domain) is None


def test_narration_usage_is_a_reified_relator_with_open_subject() -> None:
    g = _graph()
    assert (GM.NarrationUsage, RDF.type, GUFO.Kind) in g
    assert (GM.NarrationUsage, RDFS.subClassOf, GUFO.Relator) in g
    for prop in (GM.narrationSegment, GM.narrationSubject):
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
    assert g.value(GM.narrationSubject, RDFS.range) is None
    # Mode is deliberately NOT functional: a flashback can also be a dream.
    assert (GM.narrationMode, RDF.type, OWL.FunctionalProperty) not in g


def test_narration_mode_vocab_seeds() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.NarrationMode))
    assert {
        GM.narrationDirect,
        GM.narrationMentioned,
        GM.narrationFlashback,
        GM.narrationDream,
        GM.narrationHypothetical,
        GM.narrationUnreliable,
    } <= members


def test_no_truth_bridge_from_unreliable_mode() -> None:
    """depictionUnreliable is a plain vocabulary individual — no axiom links
    it to the deception module (documented bridge only, #212)."""
    g = _graph()
    types = set(g.objects(GM.narrationUnreliable, RDF.type))
    assert types == {GM.NarrationMode}


# --------------------------------------------------------------------------- #
# Closed-world SHACL shapes + the efficiency doctrine in fixture form
# --------------------------------------------------------------------------- #


def test_wellformed_narration_fixture_conforms() -> None:
    result = run_shacl(_fixture("narration-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_narration_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("narration-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "at least one gmeow:narrationMode" in errors
    assert "exactly one gmeow:narrationSubject" in errors
    assert "exactly one gmeow:narrationSegment" in errors


def test_fixture_obeys_the_efficiency_budget() -> None:
    """The chapter-scale fixture demonstrates the doctrine: many flat links,
    exactly one promoted NarrationUsage (the one with a reason), and the promoted
    link is not duplicated as a flat quad."""
    g = _fixture("narration-wellformed")
    flat = list(g.subject_objects(GM.narrates)) + list(g.subject_objects(GM.narratedIn))
    reified = list(g.subjects(RDF.type, GM.NarrationUsage))
    assert len(flat) >= 14
    assert len(reified) == 1
    # No duplication: the reified subject has no flat depicts edge.
    promoted_subject = g.value(reified[0], GM.narrationSubject)
    assert (EX.chapter31, GM.narrates, promoted_subject) not in g


def test_competency_cooccurrence_query_over_fixture() -> None:
    """The DraCor primitive: co-occurrence pairs reachable through all three
    seam forms (flat narrates, flat narratedIn, promoted NarrationUsage)."""
    query = (COMPETENCY_DIR / "narrative-narration-cooccurrence.rq").read_text(
        encoding="utf-8"
    )
    rows = list(_fixture("narration-wellformed").query(query))
    pairs = set()
    for row in rows:
        assert isinstance(row, ResultRow)
        pairs.add((row[1], row[2]))
    # Guy entered via appearsIn; the oath event via the promoted NarrationUsage —
    # both must still pair with flat-linked Phèdre.
    flat_pairs = {tuple(sorted([str(EX.phedre), str(EX.guy)]))}
    seen = {tuple(sorted([str(a), str(b)])) for a, b in pairs}
    assert flat_pairs <= seen
    assert tuple(sorted([str(EX.phedre), str(EX.evtOath)])) in seen
