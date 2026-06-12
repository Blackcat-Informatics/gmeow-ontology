# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Competency tests for the AI claim layer (#54, slices/core/ai).

The worked example is validated structurally by the examples gate (#332);
here the MODELING commitments are pinned: claim-not-truth, roles-not-kinds,
contradiction-surfaced-never-ranked, vectors-by-reference.
"""

from __future__ import annotations

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph

GUFO = "http://purl.org/nemo/gufo#"


def _g() -> Graph:
    return load_merged_graph(include_imports=False)


def _n(local: str) -> URIRef:
    return URIRef(NAMESPACE + local)


def test_the_pipeline_is_one_provenance_graph() -> None:
    """Every pipeline stage exists and the activities specialize Activity."""
    g = _g()
    for cls in ("Corpus", "Chunk", "Embedding", "VectorIndex", "Prompt", "Claim"):
        assert (_n(cls), RDF.type, OWL.Class) in g, cls
    for activity in ("RetrievalEvent", "ModelInvocation", "ToolCall", "EvaluationRun"):
        assert (_n(activity), RDFS.subClassOf, _n("Activity")) in g, activity


def test_generated_and_memory_are_roles_not_kinds() -> None:
    """Being machine-generated or remembered is contingent — gufo:Role.

    The same claim re-asserted by a person sheds the role, never its
    identity; a Kind here would make machine provenance an essence.
    """
    g = _g()
    for role in ("GeneratedClaim", "ExtractedClaim", "MemoryItem"):
        assert (_n(role), RDF.type, URIRef(GUFO + "Role")) in g, role
        assert (_n(role), RDFS.subClassOf, _n("Claim")) in g, role


def test_claim_epistemics_stay_at_the_statement_layer() -> None:
    """No flattened truth: retrievalScore is an AnnotationProperty (P3, P9)."""
    g = _g()
    assert (_n("retrievalScore"), RDF.type, OWL.AnnotationProperty) in g
    # And the claim class itself never asserts a truth-valued property.
    assert (_n("isTrue"), RDF.type, OWL.DatatypeProperty) not in g


def test_contradiction_is_a_relator_that_never_ranks() -> None:
    g = _g()
    assert (_n("Contradiction"), RDFS.subClassOf, URIRef(GUFO + "Relator")) in g
    # Surfaced and attributed:
    assert (_n("detectedBy"), RDFS.domain, _n("Contradiction")) in g
    # ...but no winner-selection machinery exists.
    for banned in ("resolvedBy", "winningClaim", "reviewRating"):
        assert (_n(banned), RDF.type, OWL.ObjectProperty) not in g, banned


def test_vectors_live_outside_the_graph() -> None:
    """gmeow:vectorRef is a locator, and no vector-payload property exists."""
    g = _g()
    assert (_n("vectorRef"), RDF.type, OWL.DatatypeProperty) in g
    assert (_n("vectorValue"), RDF.type, OWL.DatatypeProperty) not in g


def test_no_identity_axis_was_minted() -> None:
    """The slice adds no coequalFacet axes — claims about people stay in the
    identity slices; the AI layer carries WHO SAID, never WHO IS."""
    g = _g()
    coequal = _n("coequalFacet")
    ai_terms = {
        s
        for s in g.subjects(RDFS.isDefinedBy, URIRef(NAMESPACE + "slices/ai"))
        if isinstance(s, URIRef)
    }
    assert ai_terms, "the ai slice defines terms"
    assert not [t for t in ai_terms if (t, coequal, None) in g]


def test_metric_vocabulary_covers_the_ragas_ais_family() -> None:
    g = _g()
    for metric in (
        "metricFaithfulness",
        "metricAnswerRelevancy",
        "metricContextPrecision",
        "metricContextRecall",
        "metricAttributionPrecision",
        "metricAttributionRecall",
        "metricHallucinationRate",
    ):
        assert (_n(metric), RDF.type, _n("EvaluationMetric")) in g, metric
