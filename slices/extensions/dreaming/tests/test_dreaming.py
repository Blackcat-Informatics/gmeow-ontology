"""Competency query guard for the #589 dreaming extension examples.

Loads the merged ontology plus every example under
``slices/extensions/dreaming/examples/`` and asserts that the dreaming slice
composes dreams, dream reports, dream elements, lucid-dream awareness modes,
and memory-consolidation replay with analogical transfer as documented.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")


def _load_dreaming_graph() -> Graph:
    """Return the merged ontology closed with all dreaming examples."""
    graph = load_merged_graph(include_imports=False)
    example_dir = Path(__file__).parent.parent / "examples"
    for example in sorted(example_dir.glob("*.ttl")):
        graph.parse(example, format="turtle")
    return graph


def test_dream_experience_composition() -> None:
    """Dreams are composed Experiences with dreaming process and imagined origin."""
    graph = _load_dreaming_graph()
    query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?dream
        WHERE {
            ?dream a gmeow:Experience ;
                   gmeow:mentalProcessType gmeow:processDreaming ;
                   gmeow:contentOrigin gmeow:originImagined ;
                   gmeow:awarenessMode ?mode .
            FILTER (?mode IN (
                gmeow:modeDreaming,
                gmeow:modeREM,
                gmeow:modeLucidDreaming
            ))
        }
    """
    results = {str(row[0]) for row in graph.query(query)}
    assert results, "Expected at least one composed dream Experience."


def test_dream_report_composition() -> None:
    """Dream reports are DreamReport recollections with imagined content origin."""
    graph = _load_dreaming_graph()
    query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?report
        WHERE {
            ?report a gmeow:DreamReport ;
                    gmeow:mentalProcessType gmeow:processRecollection ;
                    gmeow:contentOrigin gmeow:originImagined .
        }
    """
    results = {str(row[0]) for row in graph.query(query)}
    assert results, "Expected at least one composed DreamReport."


def test_dream_element_links() -> None:
    """Dream experiences link to imagined constituents via gmeow:dreamElement."""
    graph = _load_dreaming_graph()
    query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?dream ?element
        WHERE {
            ?dream gmeow:dreamElement ?element .
        }
    """
    results = {str(row[0]) for row in graph.query(query)}
    assert results, "Expected at least one dream-to-element link."


def test_lucid_dream_uses_mode_lucid_dreaming() -> None:
    """Exactly one example experience is a dream with lucid-dreaming mode."""
    graph = _load_dreaming_graph()
    query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?dream
        WHERE {
            ?dream a gmeow:Experience ;
                   gmeow:mentalProcessType gmeow:processDreaming ;
                   gmeow:awarenessMode gmeow:modeLucidDreaming .
        }
    """
    results = {str(row[0]) for row in graph.query(query)}
    assert len(results) == 1, (
        f"Expected exactly one lucid dreaming Experience, "
        f"found {len(results)}: {results}"
    )


def test_memory_consolidation_replay() -> None:
    """AI replay is a consolidation/concept-formation LearningEvent with Analogy."""
    graph = _load_dreaming_graph()

    replay_query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?replay
        WHERE {
            ?replay a gmeow:LearningEvent ;
                    gmeow:learningType gmeow:learningConsolidation ;
                    gmeow:learningType gmeow:learningConceptFormation .
        }
    """
    replays = {str(row[0]) for row in graph.query(replay_query)}
    assert replays, (
        "Expected at least one LearningEvent with consolidation and concept formation."
    )

    analogy_query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?analogy
        WHERE {
            ?replay a gmeow:LearningEvent ;
                    gmeow:learnedFrom ?analogy .
            ?analogy a gmeow:Analogy .
        }
    """
    analogies = {str(row[0]) for row in graph.query(analogy_query)}
    assert analogies, (
        "Expected at least one Analogy linked to a LearningEvent "
        "via a provenance property."
    )
