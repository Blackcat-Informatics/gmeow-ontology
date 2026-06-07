"""Data-quality layer structural tests (#99).

The quality module adds a cross-cutting ISO 19157 / W3C DQV-aligned layer that
refines confidence and provenance across every realm. These tests verify:

1. The TBox is well-formed (classes, properties, value vocabularies).
2. QualityAssessment is a gufo:SubKind of Observation.
3. QualityDimension is a gufo:AbstractIndividualType / QualityValue.
4. assessedEntity bridges to the universal Observation stack.
5. All seven ISO 19157 + lineage seeds exist.
6. No preferred/primary quality term is declared (Principle 9).
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.config import ONTOLOGY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_quality_assessment_class_structure() -> None:
    g = _graph()
    assert (GM.QualityAssessment, RDF.type, OWL.Class) in g
    assert (GM.QualityAssessment, RDF.type, URIRef(GUFO + "SubKind")) in g
    assert (GM.QualityAssessment, RDFS.subClassOf, GM.Observation) in g


def test_quality_dimension_class_structure() -> None:
    g = _graph()
    assert (GM.QualityDimension, RDF.type, OWL.Class) in g
    assert (GM.QualityDimension, RDF.type, URIRef(GUFO + "AbstractIndividualType")) in g
    assert (GM.QualityDimension, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in g


def test_assessed_entity_property_structure() -> None:
    g = _graph()
    assert (GM.assessedEntity, RDF.type, OWL.ObjectProperty) in g
    assert (GM.assessedEntity, RDFS.domain, GM.QualityAssessment) in g
    assert (GM.assessedEntity, RDFS.range, GM.Entity) in g
    assert (GM.assessedEntity, RDFS.subPropertyOf, GM.observedFeature) in g


def test_quality_dimension_property_structure() -> None:
    g = _graph()
    assert (GM.qualityDimension, RDF.type, OWL.ObjectProperty) in g
    assert (GM.qualityDimension, RDFS.domain, GM.QualityAssessment) in g
    assert (GM.qualityDimension, RDFS.range, GM.QualityDimension) in g
    # NOT functional: a single assessment may cover several dimensions (Principle 9).
    assert (GM.qualityDimension, RDF.type, OWL.FunctionalProperty) not in g


def test_dimension_seeds_exist() -> None:
    g = _graph()
    for term in (
        "qualityDimensionPositionalAccuracy",
        "qualityDimensionTemporalAccuracy",
        "qualityDimensionThematicAccuracy",
        "qualityDimensionCompleteness",
        "qualityDimensionLogicalConsistency",
        "qualityDimensionTopologicalConsistency",
        "qualityDimensionLineage",
    ):
        assert (GM[term], RDF.type, GM.QualityDimension) in g


def test_quality_assessment_specialises_observation() -> None:
    """A QualityAssessment individual is inferred as an Observation."""
    import owlrl

    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "quality.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.add((EX.qa1, RDF.type, GM.QualityAssessment))
    graph.add((EX.qa1, GM.assessedEntity, EX.place1))
    graph.add((EX.place1, RDF.type, GM.Place))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.qa1, RDF.type, GM.Observation) in graph


def test_no_preferred_or_primary_term_is_declared() -> None:
    """No GMEOW vocabulary term is a preferred/primary selector (Principle 9)."""
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"
