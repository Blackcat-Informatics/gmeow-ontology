"""Structural + DL-safety guards for the accessibility building block.

Pins gmeow:AccessibilityFacet / gmeow:AccessibilityPolarity (QualityValue
individuals), gmeow:AccessibilityAssertion (gufo:Relator), the flat+reified
duality, the orthogonality of hasAccessibilityFeature and hasBarrier, and the
relator mediation EL axioms.
"""

from __future__ import annotations

from itertools import combinations

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_accessibility_facet_is_quality_value() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "AccessibilityFacet"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph


def test_accessibility_polarity_is_quality_value() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "AccessibilityPolarity"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph


def test_accessibility_assertion_is_a_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "AccessibilityAssertion"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


# --------------------------------------------------------------------------- #
# Open value vocabulary (individuals, not subclasses)
# --------------------------------------------------------------------------- #


def test_facet_values_are_individuals_not_subclasses() -> None:
    graph = _graph()
    for ind in (
        "facetWheelchair",
        "facetStepFree",
        "facetVisual",
        "facetAuditory",
        "facetCognitive",
        "facetClearance",
        "facetLifeSupport",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "AccessibilityFacet"),
        ) in graph
    for rejected in (
        "WheelchairFacet",
        "StepFreeFacet",
        "VisualFacet",
    ):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_polarity_values_are_individuals_not_subclasses() -> None:
    graph = _graph()
    for ind in ("polarityFeature", "polarityBarrier", "polarityLimited"):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "AccessibilityPolarity"),
        ) in graph
    for rejected in ("PositivePolarity", "NegativePolarity", "LimitedPolarity"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


# --------------------------------------------------------------------------- #
# Property typing
# --------------------------------------------------------------------------- #


def test_has_accessibility_feature_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasAccessibilityFeature")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "AccessibilityFacet")) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_has_barrier_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasBarrier")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "AccessibilityFacet")) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_has_accessibility_need_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasAccessibilityNeed")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "AccessibilityFacet")) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_assertion_roles_are_functional() -> None:
    graph = _graph()
    for prop in ("assertionSubject", "assertionFacet", "assertionPolarity"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


# --------------------------------------------------------------------------- #
# Orthogonality guard: feature and barrier must not collapse into one property
# --------------------------------------------------------------------------- #


def test_no_bridge_between_feature_and_barrier() -> None:
    """The two axes must remain orthogonal — no subPropertyOf or equivalentProperty
    bridge among them."""
    graph = _graph()
    axes = {
        "hasAccessibilityFeature": URIRef(GMEOW + "hasAccessibilityFeature"),
        "hasBarrier": URIRef(GMEOW + "hasBarrier"),
    }
    for a, b in combinations(axes, 2):
        na, nb = axes[a], axes[b]
        assert (na, RDFS.subPropertyOf, nb) not in graph, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in graph, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in graph, f"{a} ≡ {b} forbidden"
        assert (nb, OWL.equivalentProperty, na) not in graph, f"{b} ≡ {a} forbidden"


# --------------------------------------------------------------------------- #
# Relator mediation (EL someValuesFrom)
# --------------------------------------------------------------------------- #


def test_accessibility_assertion_has_el_existential_restrictions() -> None:
    graph = _graph()
    assertion = URIRef(GMEOW + "AccessibilityAssertion")
    restrictions = []
    for restr in graph.objects(assertion, RDFS.subClassOf):
        if (restr, RDF.type, OWL.Restriction) in graph:
            prop = graph.value(restr, OWL.onProperty)
            cls = graph.value(restr, OWL.someValuesFrom)
            restrictions.append((prop, cls))
    assert (
        URIRef(GMEOW + "assertionSubject"),
        URIRef(GMEOW + "Entity"),
    ) in restrictions
    assert (
        URIRef(GMEOW + "assertionFacet"),
        URIRef(GMEOW + "AccessibilityFacet"),
    ) in restrictions
    assert (
        URIRef(GMEOW + "assertionPolarity"),
        URIRef(GMEOW + "AccessibilityPolarity"),
    ) in restrictions
