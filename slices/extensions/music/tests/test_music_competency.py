"""Competency query guard for the music stress-corpus fixtures.

Loads the merged ontology plus every fixture under
``slices/extensions/music/fixtures/`` and asserts that the
``queries/competency/music.rq`` SPARQL bundle returns the expected
work/evidence rows for each of the 15 competency questions.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")


def _load_fixture_graph() -> Graph:
    """Return the merged ontology closed with all music fixtures."""
    graph = load_merged_graph(include_imports=False)
    fixture_dir = Path(__file__).parent.parent / "fixtures"
    for fixture in sorted(fixture_dir.glob("*.ttl")):
        graph.parse(fixture, format="turtle")
    return graph


def test_music_competency_query() -> None:
    graph = _load_fixture_graph()
    query_path = (
        Path(__file__).parent.parent.parent.parent.parent
        / "queries"
        / "competency"
        / "music.rq"
    )
    query = query_path.read_text(encoding="utf-8")

    results: set[tuple[str, str, str]] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        results.add((str(row[0]), str(row[1]), str(row[2])))

    expected = {
        (
            "Q1: nested rational tuplets",
            str(GMEOW.fixtureFerneyhoughWork),
            str(GMEOW.fixtureFerneyhoughTuplet54),
        ),
        (
            "Q2: irrational tempo canon",
            str(GMEOW.fixtureNancarrowTempoCanonWork),
            str(GMEOW.fixtureNancarrowSqrt2Mapping),
        ),
        *[
            (
                "Q3: complete DegreeOfFreedom profile",
                str(GMEOW.fixtureFourThirtyThreeWork),
                str(ev),
            )
            for ev in (
                GMEOW.dofFourThirtyThreeDuration,
                GMEOW.dofFourThirtyThreeDynamics,
                GMEOW.dofFourThirtyThreeInstrumentation,
                GMEOW.dofFourThirtyThreeLocation,
                GMEOW.dofFourThirtyThreeOrder,
                GMEOW.dofFourThirtyThreePerformerCount,
                GMEOW.dofFourThirtyThreePitch,
                GMEOW.dofFourThirtyThreeSoundContent,
                GMEOW.dofFourThirtyThreeTacet,
                GMEOW.dofFourThirtyThreeTempo,
            )
        ],
        (
            "Q4: fragment graph + TraversalConstraint + PerformanceDecisions",
            str(GMEOW.fixtureStockhausenKlavierstuckXIWork),
            str(GMEOW.fixtureStockhausenTraversalConstraint),
        ),
        *[
            (
                "Q5: 43-tone just intonation with integer-pair ratios",
                str(GMEOW.fixturePartch43Work),
                str(ev),
            )
            for ev in (
                GMEOW.fixturePartchRatio1_1,
                GMEOW.fixturePartchRatio2_1,
                GMEOW.fixturePartchRatio3_2,
                GMEOW.fixturePartchRatio4_3,
                GMEOW.fixturePartchRatio5_3,
                GMEOW.fixturePartchRatio5_4,
                GMEOW.fixturePartchRatio9_8,
                GMEOW.fixturePartchRatio11_8,
            )
        ],
        (
            "Q6: stochastic glissando field with graphic notation",
            str(GMEOW.fixtureXenakisGlissandoWork),
            str(GMEOW.fixtureXenakisGlissandoProcess),
        ),
        (
            "Q7: spectrum-derived PitchCollection with CMN projection loss",
            str(GMEOW.fixtureGriseyPartielsWork),
            str(GMEOW.fixtureGriseyPartielsPitches),
        ),
        *[
            (
                "Q8: graphic score with standpointed symbolic interpretations",
                str(GMEOW.fixtureCardewTreatiseWork),
                str(ev),
            )
            for ev in (
                GMEOW.fixtureCardewTranscriptionA,
                GMEOW.fixtureCardewTranscriptionB,
            )
        ],
        *[
            (
                "Q9: mensural notation with unequal talea and color cycles",
                str(GMEOW.fixtureArsSubtiliorWork),
                str(ev),
            )
            for ev in (
                GMEOW.fixtureArsSubtiliorTaleaSegment,
                GMEOW.fixtureArsSubtiliorColorSegment,
            )
        ],
        (
            "Q10: added-value MetricGroups + non-retrogradable identity "
            "+ mode of limited transposition",
            str(GMEOW.fixtureMessiaenExcerptWork),
            str(GMEOW.fixtureMessiaenModeClaim),
        ),
        *[
            (
                "Q11: unsynchronized ad-lib spans bounded by cue anchors",
                str(GMEOW.fixtureLutoslawskiAdLibWork),
                str(ev),
            )
            for ev in (
                GMEOW.fixtureLutoslawskiMappingA,
                GMEOW.fixtureLutoslawskiMappingB,
            )
        ],
        (
            "Q12: score-less oral tradition with ornament profile "
            "and transmission lineage",
            str(GMEOW.fixtureOralRagaYamanWork),
            str(GMEOW.fixtureRagaYamanAlapOrnamentProfile),
        ),
        *[
            (
                "Q13: additive aksak MetricGroups with changing meters",
                str(GMEOW.fixtureAksakFolkTuneWork),
                str(ev),
            )
            for ev in (
                GMEOW.fixtureAksakMeter5,
                GMEOW.fixtureAksakMeter7,
                GMEOW.fixtureAksakMeter9,
            )
        ],
        *[
            (
                "Q14: polymeter + contested meter + riff transformations "
                "+ drop-D + refuted genre",
                str(GMEOW.fixtureMathRockTrackWork),
                str(ev),
            )
            for ev in (
                GMEOW.fixtureMathRockBar17SixEightClaim,
                GMEOW.fixtureMathRockBar17TwelveEightClaim,
            )
        ],
        (
            "Q15: phasing generative process with realizations",
            str(GMEOW.fixtureReichPhasingWork),
            str(GMEOW.fixtureReichPhasingProcess),
        ),
    }

    assert results == expected, (
        f"Competency query results do not match expected.\n"
        f"Only in results: {results - expected}\n"
        f"Missing from results: {expected - results}"
    )
