# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Competency tests for the AI claim layer and the graphrag extension.

The examples gate validates both worked examples structurally; here the
CONSTITUTIONAL commitments are pinned — above all the thinness: the unified
observation stance (P9) means no parallel claim/evaluation construct may ever
exist, and the tombstone tests keep the mistaken parallel Claim constructs buried.

Migrated to declarative slicetest cells:
  - test_extraction_methods_are_observation_methods
      → slices/core/ai/tests/structural.ttl (saExtractionMethodsAreObservationMethods)
  - test_invocation_and_retrieval_are_activities
      → slices/core/ai/tests/structural.ttl (saModelInvocationIsActivity)
      → slices/extensions/graphrag/tests/structural.ttl (saRetrievalEventIsActivity,
        saRetrievalScoreIsAnnotationProperty)
  - test_vectors_stay_outside_the_graph
      → slices/extensions/graphrag/tests/structural.ttl
        (saVectorRefIsDatatypePropertyNoVectorValue, saNoVectorValueProperty)
  - test_extracted_descriptions_are_not_entities
      → slices/extensions/graphrag/tests/structural.ttl
        (saExtractedDescriptionsAreInformationObjects)
"""

from __future__ import annotations

from purrdf.compat.rdflib import RDF, RDFS, Graph, URIRef

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
    """gmeow:Observation IS the universal claim construct — the earlier
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
# Whole-graph / cross-slice guards that stay in pytest.
# --------------------------------------------------------------------------- #


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
