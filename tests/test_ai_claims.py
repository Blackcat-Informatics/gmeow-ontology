# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Competency tests for the AI claim layer (#54) and the graphrag extension.

The examples gate (#332) validates both worked examples structurally; here the
CONSTITUTIONAL commitments are pinned — above all the thinness: the unified
observation stance (P9) means no parallel claim/evaluation construct may ever
exist, and the tombstone tests keep PR #388's mistakes buried.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL

from gmeow_tools.config import FIXTURES_DIR, NAMESPACE
from gmeow_tools.graph import load_merged_graph


def _g() -> Graph:
    return load_merged_graph(include_imports=False)


def _n(local: str) -> URIRef:
    return URIRef(NAMESPACE + local)


# --------------------------------------------------------------------------- #
# Thinness: the unified observation stance, enforced as tombstones (P9, P5).
# --------------------------------------------------------------------------- #


def test_no_parallel_claim_construct_exists() -> None:
    """gmeow:Observation IS the universal claim construct — PR #388's parallel
    Claim/GeneratedClaim/ExtractedClaim classes must never return."""
    g = _g()
    for tombstone in ("Claim", "GeneratedClaim", "ExtractedClaim", "claimText"):
        assert (_n(tombstone), None, None) not in g, tombstone


def test_no_parallel_evaluation_construct_exists() -> None:
    """Evaluation is the norms extension's Assessment (judge-as-vantage)."""
    g = _g()
    for tombstone in (
        "MetricObservation",
        "EvaluationRun",
        "EvaluationMetric",
        "metricValue",
        "observesMetric",
        "scoresSubject",
    ):
        assert (_n(tombstone), None, None) not in g, tombstone


def test_no_duplicate_provenance_properties() -> None:
    """Outputs hang off the EXISTING wasGeneratedBy — no forward duplicates."""
    g = _g()
    for tombstone in ("producedOutput", "builtBy", "extractionMethod"):
        assert (_n(tombstone), None, None) not in g, tombstone


def test_no_winner_machinery_anywhere() -> None:
    """Contradictions surface; nothing ranks them (P9)."""
    g = _g()
    for banned in ("resolvedBy", "winningClaim", "reviewRating"):
        assert (_n(banned), None, None) not in g, banned


# --------------------------------------------------------------------------- #
# The seams: what the slice DOES mint sits on existing machinery.
# --------------------------------------------------------------------------- #


def test_extraction_methods_are_observation_methods() -> None:
    """Claims are observations; HOW they were produced is observationMethod."""
    g = _g()
    for method in ("methodLlmExtraction", "methodNliDerivation"):
        assert (_n(method), RDF.type, _n("ObservationMethod")) in g, method


def test_invocation_and_retrieval_are_activities() -> None:
    g = _g()
    assert (_n("ModelInvocation"), RDFS.subClassOf, _n("Activity")) in g
    assert (_n("RetrievalEvent"), RDFS.subClassOf, _n("Activity")) in g  # graphrag
    assert (_n("retrievalScore"), RDF.type, OWL.AnnotationProperty) in g


def test_vectors_stay_outside_the_graph() -> None:
    """graphrag: vectorRef is a locator; no payload property exists (P12)."""
    g = _g()
    assert (_n("vectorRef"), RDF.type, OWL.DatatypeProperty) in g
    assert (_n("vectorValue"), None, None) not in g


def test_extracted_descriptions_are_not_entities() -> None:
    """GraphRAG artifacts are InformationObjects ABOUT putative entities (P5)."""
    g = _g()
    for cls in ("ExtractedEntity", "ExtractedRelationship", "Community"):
        assert (_n(cls), RDFS.subClassOf, _n("InformationObject")) in g, cls
    assert (_n("CommunitySummary"), RDFS.subClassOf, _n("Summary")) in g


def test_no_new_identity_axes_were_minted() -> None:
    """The AI layer carries WHO SAID, never WHO IS."""
    g = _g()
    coequal = _n("coequalFacet")
    for slice_iri in ("slices/ai", "slices/graphrag"):
        terms = {
            s
            for s in g.subjects(RDFS.isDefinedBy, URIRef(NAMESPACE + slice_iri))
            if isinstance(s, URIRef)
        }
        assert terms, slice_iri
        assert not [t for t in terms if (t, coequal, None) in g], slice_iri


def test_assessment_seam_is_the_norms_extensions() -> None:
    """The fixture's evaluator is an Assessment — the judge is just a vantage."""
    g = Graph().parse(FIXTURES_DIR / "ai-normative.ttl", format="turtle")
    ex = "https://blackcatinformatics.ca/gmeow/examples/ai-normative/"
    assert (URIRef(ex + "assessment-1"), RDF.type, _n("Assessment")) in g
    assert (URIRef(ex + "assessment-1"), _n("vantage"), URIRef(ex + "judge")) in g
