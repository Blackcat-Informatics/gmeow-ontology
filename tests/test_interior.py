# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The interior parcel: affect (#356, EPIC #348) + arcs/roles/motifs
(#361/#362/#363, EPIC #358).

Affect is the thinnest slice by commitment (Emotion ⊑ IntrinsicMode, open
Plutchik-seeded types, Appraisal ⊑ Observation with PAD + aesthetic
qualities). The narrative interior anchors to what exists: ArcSamples
observe characters AT NarrativePositions (#359, the PitchTrajectory move);
RoleInNarrative is scoped and interpretive (no primaryProtagonist, ever);
Motif occurrences ride the narration seam (#360). Cross-slice state
references are soft (IRIs, never dependencies — P16); every reading is a
vantage-indexed cell (P9).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace
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
# Affect (#356)
# --------------------------------------------------------------------------- #


def test_emotion_is_an_intrinsic_mode() -> None:
    g = _graph()
    assert (GM.Emotion, RDF.type, GUFO.Kind) in g
    assert (GM.Emotion, RDFS.subClassOf, GUFO.IntrinsicMode) in g
    assert (GM.emotionBearer, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.emotionBearer, RDFS.range, GM.Agent) in g
    # Blends and rival traditions coexist (P9).
    assert (GM.emotionType, RDF.type, OWL.FunctionalProperty) not in g


def test_plutchik_seeds_are_present_and_open() -> None:
    g = _graph()
    seeds = {
        GM.emotionJoy,
        GM.emotionTrust,
        GM.emotionFear,
        GM.emotionSurprise,
        GM.emotionSadness,
        GM.emotionDisgust,
        GM.emotionAnger,
        GM.emotionAnticipation,
    }
    members = set(g.subjects(RDF.type, GM.EmotionType))
    assert seeds <= members


def test_appraisal_is_a_vantage_indexed_observation() -> None:
    g = _graph()
    assert (GM.Appraisal, RDFS.subClassOf, GM.Observation) in g
    assert (GM.appraisalOf, RDFS.subPropertyOf, GM.observedFeature) in g
    # Constitutive constituent: one appraisal, one subject.
    assert (GM.appraisalOf, RDF.type, OWL.FunctionalProperty) in g
    # Per-cell readings are NOT OWL-functional (the #385 convention):
    # single-valuedness per cell is SHACL's job; rival cells coexist (P9).
    for prop in (GM.appraisalDimension, GM.appraisalValue, GM.appraisalQuality):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop


def test_no_emotion_tenure_class_exists() -> None:
    """Thin means thin: episodic scope rides validFrom/validUntil; a tenure
    class arrives only on consumer demand (docs record the bar)."""
    g = _graph()
    assert (GM.EmotionTenure, RDF.type, OWL.Class) not in g


# --------------------------------------------------------------------------- #
# Arc samples (#361)
# --------------------------------------------------------------------------- #


def test_arc_sample_constituents() -> None:
    g = _graph()
    assert (GM.ArcSample, RDFS.subClassOf, GM.Observation) in g
    assert (GM.sampleSubject, RDFS.subPropertyOf, GM.observedFeature) in g
    for prop in (GM.sampleSubject, GM.samplePosition, GM.sampleState):
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
    assert (GM.samplePosition, RDFS.range, GM.NarrativePosition) in g
    # State is a soft cross-slice reference: range-open by design (P16).
    assert g.value(GM.sampleState, RDFS.range) is None
    # Localizable evidence prose (the #376 convention).
    assert (GM.developmentSignalText, RDF.type, OWL.FunctionalProperty) not in g
    assert (GM.developmentSignalEvent, RDFS.range, GM.Event) in g


def test_character_arc_extension_is_additive() -> None:
    g = _graph()
    assert (GM.arcSample, RDFS.subPropertyOf, GM.hasPart) in g
    assert (GM.arcSample, RDFS.domain, GM.CharacterArc) in g
    # The pre-existing arc machinery is untouched.
    assert (GM.arcType, RDF.type, OWL.ObjectProperty) in g


# --------------------------------------------------------------------------- #
# Roles (#362)
# --------------------------------------------------------------------------- #


def test_role_in_narrative_is_scoped_and_interpretive() -> None:
    g = _graph()
    assert (GM.RoleInNarrative, RDFS.subClassOf, GUFO.Relator) in g
    for prop in (GM.narrativeRoleBearer, GM.narrativeRoleScope, GM.narrativeRoleValue):
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
    assert (GM.narrativeRoleScope, RDFS.range, GM.NarrativeScope) in g
    # The scope graft, extension-side (the Rule ⊑ Norm direction).
    for cls in (GM.CreativeWork, GM.ContentSegment, GM.NarrativeReferenceFrame):
        assert (cls, RDFS.subClassOf, GM.NarrativeScope) in g, cls


def test_no_primary_protagonist_machinery() -> None:
    g = _graph()
    banned = (
        "primaryprotagonist",
        "preferredprotagonist",
        "primaryrole",
        "preferredrole",
    )
    offenders = [
        str(s)
        for s in set(g.subjects())
        if str(s).startswith(GMEOW)
        and "/" not in str(s)[len(GMEOW) :]
        and str(s)[len(GMEOW) :].lower().startswith(banned)
    ]
    assert offenders == []


# --------------------------------------------------------------------------- #
# Motifs (#363)
# --------------------------------------------------------------------------- #


def test_motif_rides_the_seam() -> None:
    g = _graph()
    assert (GM.Motif, RDFS.subClassOf, GM.SocialObject) in g
    assert (GM.motifOccursIn, RDFS.subPropertyOf, GM.narratedIn) in g
    assert (GM.motifOccursIn, RDFS.range, GM.ContentSegment) in g
    assert (GM.motifKind, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_interior_fixture_conforms() -> None:
    result = run_shacl(_fixture("interior-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_interior_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("interior-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "exactly one gmeow:samplePosition" in errors
    assert "exactly one gmeow:sampleState" in errors
    assert "protagonist-of-WHAT is half the claim" in errors
    assert "an unnameable recurring unit is a tag" in errors
    assert "rides the narration seam into a ContentSegment" in errors
    assert "exactly one gmeow:emotionBearer" in errors
    assert "at least one gmeow:emotionType" in errors
    assert "must read SOMETHING" in errors
    assert "half a reading is no reading" in errors


# --------------------------------------------------------------------------- #
# The trajectory query — disagreement visible, never resolved
# --------------------------------------------------------------------------- #


def test_trajectory_query_orders_and_surfaces_disagreement() -> None:
    query_path = COMPETENCY_DIR / "narrative-arc-trajectory.rq"
    query = query_path.read_text(encoding="utf-8")
    rows = list(_fixture("interior-wellformed").query(query))
    readings = []
    for row in rows:
        assert isinstance(row, ResultRow)
        ordinal = row[1]
        assert isinstance(ordinal, Literal)
        readings.append((row[0], int(ordinal.toPython()), row[2]))
    # Analyzer A: anticipation@3 then fear@31; analyzer B: anticipation@31.
    assert (EX.modelA, 3, GM.emotionAnticipation) in readings
    assert (EX.modelA, 31, GM.emotionFear) in readings
    assert (EX.modelB, 31, GM.emotionAnticipation) in readings
    # Both 31-readings stand — disagreement is data (P9).
    at_31 = {r for r in readings if r[1] == 31}
    assert len(at_31) == 2
