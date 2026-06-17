# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Concepts slice — category, categorization, concept structure, and tenure.

These structural assertions guard the load-bearing shape of the concepts slice:

* ``gmeow:Concept`` is a socially sustained representational category, a
  ``gufo:Kind`` under ``gmeow:SocialObject`` — explicitly not the gUFO universal.
* ``gmeow:instanceOfConcept`` is the flat shortcut from any ``gmeow:Entity`` to a
  ``gmeow:Concept``; it is non-functional (many concepts may apply).
* ``gmeow:ConceptCategorization`` is the reified, standpoint-indexed promotion of
  that shortcut: a ``gufo:SubKind`` of ``gmeow:StandpointClaim`` whose
  ``observedFeature`` is the categorized entity and whose ``observationResult`` is
  the concept, optionally graded by ``gmeow:typicality`` in ``[0,1]``.
* ``gmeow:subsumes`` and ``gmeow:composedOf`` structure concepts, but are not
  declared transitive at the core (Principle 12: transitive closure is solver work).
* ``gmeow:ConceptTenure`` is a ``gufo:SituationType`` / ``gmeow:TimeScopedRelation``
  that records a concept's applicability over an interval, linked functionally by
  ``gmeow:conceptHoldsFor``.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS, XSD
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/concepts")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"
_SHAPES = Path(__file__).resolve().parents[1] / "shapes.ttl"

# Every locally-declared term, by name (8 total).
_DECLARED_TERMS = (
    "Concept",
    "instanceOfConcept",
    "ConceptCategorization",
    "typicality",
    "subsumes",
    "composedOf",
    "ConceptTenure",
    "conceptHoldsFor",
)


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _gufo(name: str) -> URIRef:
    """A gufo-namespaced term URI."""
    return URIRef(GUFO + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def test_concept_is_social_object_kind() -> None:
    """gmeow:Concept is an owl:Class, a gufo:Kind, and a subclass of
    gmeow:SocialObject."""
    g = _graph()
    concept = _t("Concept")
    assert (concept, RDF.type, OWL.Class) in g
    assert (concept, RDF.type, _gufo("Kind")) in g
    assert (concept, RDFS.subClassOf, _t("SocialObject")) in g


def test_concept_categorization_subkind_of_standpoint_claim() -> None:
    """gmeow:ConceptCategorization is an owl:Class, a gufo:SubKind, and a
    subclass of gmeow:StandpointClaim."""
    g = _graph()
    cc = _t("ConceptCategorization")
    assert (cc, RDF.type, OWL.Class) in g
    assert (cc, RDF.type, _gufo("SubKind")) in g
    assert (cc, RDFS.subClassOf, _t("StandpointClaim")) in g


def test_instance_of_concept_property() -> None:
    """gmeow:instanceOfConcept is an object property from gmeow:Entity to
    gmeow:Concept, and is non-functional."""
    g = _graph()
    prop = _t("instanceOfConcept")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.domain, _t("Entity")) in g
    assert (prop, RDFS.range, _t("Concept")) in g
    assert (prop, RDF.type, OWL.FunctionalProperty) not in g


def test_typicality_property() -> None:
    """gmeow:typicality is a datatype property on gmeow:ConceptCategorization
    with range xsd:decimal, and is non-functional."""
    g = _graph()
    prop = _t("typicality")
    assert (prop, RDF.type, OWL.DatatypeProperty) in g
    assert (prop, RDFS.domain, _t("ConceptCategorization")) in g
    assert (prop, RDFS.range, XSD.decimal) in g
    assert (prop, RDF.type, OWL.FunctionalProperty) not in g


def test_concept_structure_properties() -> None:
    """gmeow:subsumes and gmeow:composedOf are object properties between concepts,
    non-functional, and not declared transitive (Principle 12)."""
    g = _graph()
    for name in ("subsumes", "composedOf"):
        prop = _t(name)
        assert (prop, RDF.type, OWL.ObjectProperty) in g
        assert (prop, RDFS.domain, _t("Concept")) in g
        assert (prop, RDFS.range, _t("Concept")) in g
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g
        assert (prop, RDF.type, OWL.TransitiveProperty) not in g


def test_concept_tenure_is_time_scoped() -> None:
    """gmeow:ConceptTenure is a gufo:SituationType subclass of
    gmeow:TimeScopedRelation; gmeow:conceptHoldsFor is functional from tenure to
    concept."""
    g = _graph()
    tenure = _t("ConceptTenure")
    assert (tenure, RDF.type, OWL.Class) in g
    assert (tenure, RDF.type, _gufo("SituationType")) in g
    assert (tenure, RDFS.subClassOf, _t("TimeScopedRelation")) in g

    holds = _t("conceptHoldsFor")
    assert (holds, RDF.type, OWL.ObjectProperty) in g
    assert (holds, RDF.type, OWL.FunctionalProperty) in g
    assert (holds, RDFS.domain, tenure) in g
    assert (holds, RDFS.range, _t("Concept")) in g


def test_every_declared_term_is_annotated() -> None:
    """Annotation-completeness (Principle 8): each locally-declared term carries
    an rdfs:label, a skos:definition, and rdfs:isDefinedBy the concepts slice IRI."""
    g = _graph()
    assert len(_DECLARED_TERMS) == 8
    for name in _DECLARED_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, (
            f"{name} missing rdfs:isDefinedBy slice IRI"
        )


# --------------------------------------------------------------------------- #
# SHACL — gmeow:ConceptCategorizationShape and gmeow:ConceptTenureShape.
# --------------------------------------------------------------------------- #


def _data(instance_ttl: str) -> Graph:
    """Slice module plus an inline instance graph for SHACL fixtures."""
    g = Graph()
    g.parse(_MODULE, format="turtle")
    g.parse(data=instance_ttl, format="turtle")
    return g


_PRELUDE = """
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <http://example.org/cnc/> .
ex:whale a gmeow:Entity .
ex:mammal a gmeow:Concept .
ex:vehicle a gmeow:Entity .
ex:interval a gmeow:TimeInterval .
ex:method a gmeow:ObservationMethod .
"""

_WELLFORMED_CATEGORIZATION = (
    _PRELUDE
    + """
ex:cat1 a gmeow:ConceptCategorization ;
    gmeow:observedFeature ex:whale ;
    gmeow:observationResult ex:mammal ;
    gmeow:observationMethod ex:method ;
    gmeow:typicality "0.95"^^xsd:decimal .
"""
)

_CATEGORIZATION_MISSING_FEATURE = (
    _PRELUDE
    + """
ex:bad1 a gmeow:ConceptCategorization ;
    gmeow:observationResult ex:mammal ;
    gmeow:observationMethod ex:method ;
    gmeow:typicality "0.5"^^xsd:decimal .
"""
)

_CATEGORIZATION_RESULT_NOT_CONCEPT = (
    _PRELUDE
    + """
ex:bad2 a gmeow:ConceptCategorization ;
    gmeow:observedFeature ex:whale ;
    gmeow:observationResult ex:vehicle ;
    gmeow:observationMethod ex:method ;
    gmeow:typicality "0.5"^^xsd:decimal .
"""
)

_CATEGORIZATION_TYPICALITY_OUT_OF_RANGE = (
    _PRELUDE
    + """
ex:bad3 a gmeow:ConceptCategorization ;
    gmeow:observedFeature ex:whale ;
    gmeow:observationResult ex:mammal ;
    gmeow:observationMethod ex:method ;
    gmeow:typicality "1.5"^^xsd:decimal .
"""
)

_WELLFORMED_TENURE = (
    _PRELUDE
    + """
ex:tenure1 a gmeow:ConceptTenure ;
    gmeow:conceptHoldsFor ex:mammal ;
    gmeow:duringInterval ex:interval .
"""
)

_TENURE_MISSING_INTERVAL = (
    _PRELUDE
    + """
ex:tenure2 a gmeow:ConceptTenure ;
    gmeow:conceptHoldsFor ex:mammal .
"""
)


def test_wellformed_concept_categorization_conforms() -> None:
    result = run_shacl(_data(_WELLFORMED_CATEGORIZATION), shapes_path=_SHAPES)
    assert result.ok, result.errors


def test_categorization_missing_feature_is_flagged() -> None:
    result = run_shacl(_data(_CATEGORIZATION_MISSING_FEATURE), shapes_path=_SHAPES)
    assert not result.ok
    assert "exactly one observed feature" in " ".join(result.errors).lower()


def test_categorization_result_not_concept_is_flagged() -> None:
    result = run_shacl(_data(_CATEGORIZATION_RESULT_NOT_CONCEPT), shapes_path=_SHAPES)
    assert not result.ok
    assert "exactly one observation result" in " ".join(result.errors).lower()


def test_categorization_typicality_out_of_range_is_flagged() -> None:
    result = run_shacl(
        _data(_CATEGORIZATION_TYPICALITY_OUT_OF_RANGE), shapes_path=_SHAPES
    )
    assert not result.ok
    assert "xsd:decimal in the closed interval [0,1]" in " ".join(result.errors)


def test_wellformed_concept_tenure_conforms() -> None:
    result = run_shacl(_data(_WELLFORMED_TENURE), shapes_path=_SHAPES)
    assert result.ok, result.errors


def test_tenure_missing_interval_is_flagged() -> None:
    result = run_shacl(_data(_TENURE_MISSING_INTERVAL), shapes_path=_SHAPES)
    assert not result.ok
    assert "exactly one gmeow:TimeInterval" in " ".join(result.errors)
