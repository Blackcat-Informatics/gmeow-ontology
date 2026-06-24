"""Music analysis and genre standpoint gates (issue #315).

Principles 2, 4, 5, 9, 10, 11, 14, 16.
"""

from __future__ import annotations

import gmeow_native.pipeline as native_pipeline
from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/music/")


def _graph() -> Graph:
    """Load the project's merged RDF graph without following owl:imports."""
    return load_merged_graph(include_imports=False)


def _statement_owl_graph() -> Graph:
    """Compile the statement OWL downcast through the native Rust stage."""
    artifacts = native_pipeline.compile_statements(str(PROJECT_ROOT))
    graph = Graph()
    graph.parse(data=artifacts["owl_ttl"], format="turtle")
    return graph


def test_music_analysis_claim_subclass_of_observation() -> None:
    graph = _graph()
    assert (GMEOW.MusicAnalysisClaim, RDFS.subClassOf, GMEOW.Observation) in graph


def test_analysis_target_subproperty_of_observed_feature() -> None:
    graph = _graph()
    assert (GMEOW.analysisTarget, RDFS.subPropertyOf, GMEOW.observedFeature) in graph


def test_analysis_frame_subproperty_of_has_reference_frame() -> None:
    graph = _graph()
    assert (GMEOW.analysisFrame, RDFS.subPropertyOf, GMEOW.hasReferenceFrame) in graph


def test_analysis_claim_constitutive_properties_are_functional() -> None:
    graph = _graph()
    for prop in (
        GMEOW.analysisTarget,
        GMEOW.analysisProperty,
        GMEOW.analysisResult,
        GMEOW.analysisFrame,
    ):
        assert (prop, RDF.type, OWL.FunctionalProperty) in graph, (
            f"{prop} must be functional"
        )


def test_theory_frames_are_reference_frames() -> None:
    graph = _graph()
    for iri in (
        "theoryFrameRomanNumeral",
        "theoryFrameSchenkerian",
        "theoryFramePitchClassSet",
        "theoryFrameMaqamTheory",
        "theoryFrameRagaGrammar",
        "theoryFramePathet",
        "theoryFrameTransformational",
        "theoryFrameCorpusStatistical",
        "theoryFrameAIModel",
    ):
        frame = URIRef(GMEOW + iri)
        assert (frame, RDF.type, GMEOW.ReferenceFrame) in graph, (
            f"{iri} is not a ReferenceFrame"
        )
        assert (frame, GMEOW.frameRealm, GMEOW.frameRealmMusicAnalysis) in graph, (
            f"{iri} missing realm"
        )


def test_genre_seeds_coexist() -> None:
    graph = _graph()
    for iri in (
        "genreRock",
        "genreJazz",
        "genreBlues",
        "genreFunk",
        "genreSoul",
        "genreElectronic",
        "genreClassical",
        "genreMathRock",
        "genreProgressiveRock",
        "genrePostRock",
        "genreFusion",
        "genreDisco",
        "genreHipHop",
    ):
        assert (URIRef(GMEOW + iri), RDF.type, GMEOW.Genre) in graph, f"missing {iri}"


def test_genre_derivation_links_exist() -> None:
    graph = _graph()
    assert (GMEOW.genreMathRock, GMEOW.wasDerivedFrom, GMEOW.genreRock) in graph
    assert (GMEOW.genreFusion, GMEOW.wasDerivedFrom, GMEOW.genreJazz) in graph
    assert (GMEOW.genreFusion, GMEOW.wasDerivedFrom, GMEOW.genreRock) in graph


def test_statement_cells_include_contested_meter_pair() -> None:
    owl = _statement_owl_graph()
    subjects = set(owl.objects(None, OWL.annotatedSource))
    assert URIRef(EX + "bar17") in subjects


def test_statement_cells_emit_owl_axioms_with_standpoints() -> None:
    owl = _statement_owl_graph()
    # Both meter claims should appear as owl:Axiom nodes.
    claim_7_8 = URIRef(EX + "claim-bar17-meter-7_8")
    claim_4_4 = URIRef(EX + "claim-bar17-meter-4_4-plus-3_8")
    assert (claim_7_8, RDF.type, OWL.Axiom) in owl
    assert (claim_4_4, RDF.type, OWL.Axiom) in owl
    # Each carries a different accordingTo value.
    a7_8 = set(owl.objects(claim_7_8, GMEOW.accordingTo))
    a4_4 = set(owl.objects(claim_4_4, GMEOW.accordingTo))
    assert a7_8 != a4_4
