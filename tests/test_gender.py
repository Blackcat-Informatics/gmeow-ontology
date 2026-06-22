"""Structural + DL-safety guards for the gender building block.

Pins the shared gmeow:IdentityFacet base (a gufo:Relator), the GenderIdentity /
GenderExpression facets, the value-vs-subclass decisions (gender / expression /
sex-assigned-at-birth are OPEN value vocabularies of individuals, never per-value
Person subclasses), the functional-per-facet value properties, the
inclusive-without-overtyping invariant (no flat-literal gender shortcut — the
fresh-individual escape hatch is the single path), and the removal of the old
flat gmeow:sex. Cross-axis independence lives in test_identity_orthogonality.py.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_identity_facet_is_a_relator() -> None:
    graph = _graph()
    # IdentityFacet ⊑ gufo:Relator (the NameUsage idiom; Endurant-compatible so it
    # bears displayable / wasAttributedTo without clashing with Entity-domained props).
    assert (
        URIRef(GMEOW + "IdentityFacet"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
    # NOT grounded as a Situation (disjoint from Endurant in gUFO).
    assert (
        URIRef(GMEOW + "IdentityFacet"),
        RDFS.subClassOf,
        URIRef(GUFO + "Situation"),
    ) not in graph
    for facet in ("GenderIdentity", "GenderExpression"):
        assert (
            URIRef(GMEOW + facet),
            RDFS.subClassOf,
            URIRef(GMEOW + "IdentityFacet"),
        ) in graph


def test_gender_values_are_individuals_not_subclasses() -> None:
    graph = _graph()
    # The value classes are abstract value spaces (gufo:QualityValue), not endurants.
    qv = URIRef(GUFO + "QualityValue")
    for cls in ("Gender", "GenderExpressionStyle", "SexAssignedAtBirth"):
        assert (URIRef(GMEOW + cls), RDFS.subClassOf, qv) in graph
    # Seed values exist as INDIVIDUALS of gmeow:Gender.
    for ind in ("genderWoman", "genderMan", "genderNonBinary", "genderTwoSpirit"):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "Gender")) in graph
    # Per-value Person subclasses must NOT exist (no overtyping / class explosion).
    for rejected in ("Woman", "Man", "NonBinaryPerson", "TransPerson", "AgenderPerson"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_value_properties_are_functional_facets_nonfunctional() -> None:
    graph = _graph()
    # genderValue / expressionValue are functional PER FACET.
    for prop in ("genderValue", "expressionValue"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph
    # hasGenderIdentity / hasGenderExpression are NON-functional (co-equal).
    for prop in ("hasGenderIdentity", "hasGenderExpression"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_no_flat_gender_shortcut() -> None:
    """Greenfield: a flat-literal duplicate of the first-class value path is baggage.

    The gmeow:genderValue object property (→ a gmeow:Gender individual) is the SINGLE
    path; there is deliberately no flat datatype shortcut like gmeow:gender /
    genderLabel duplicating it.
    """
    graph = _graph()
    for banned in ("gender", "genderLabel", "genderString", "expressionLabel"):
        node = URIRef(GMEOW + banned)
        msg = f"{banned} is baggage"
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph, msg


def test_sex_assigned_at_birth_is_recorded_not_a_facet() -> None:
    graph = _graph()
    saab = URIRef(GMEOW + "sexAssignedAtBirth")
    assert (saab, RDF.type, OWL.ObjectProperty) in graph
    assert (saab, RDFS.range, URIRef(GMEOW + "SexAssignedAtBirth")) in graph
    # It is NOT an identity facet (recorded datum, not a self-assertion).
    assert (saab, RDFS.subPropertyOf, URIRef(GMEOW + "hasGenderIdentity")) not in graph
    # The old flat gmeow:sex literal is GONE (greenfield removal).
    sex = URIRef(GMEOW + "sex")
    assert (sex, RDF.type, OWL.DatatypeProperty) not in graph
    assert (sex, RDF.type, OWL.ObjectProperty) not in graph


def test_displayable_generalised_to_cover_identity() -> None:
    """displayable is now domain-free — it covers both Appellation and IdentityFacet."""
    graph = _graph()
    displayable = URIRef(GMEOW + "displayable")
    assert (displayable, RDF.type, OWL.DatatypeProperty) in graph
    # No narrow domain pinning it to Appellation only.
    assert (displayable, RDFS.domain, URIRef(GMEOW + "Appellation")) not in graph


def test_competency_gender_values_query() -> None:
    graph = _graph()
    query = (COMPETENCY_DIR / "gender-values.rq").read_text(encoding="utf-8")
    values: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        values.add(str(row[0]))
    for ind in ("genderWoman", "genderNonBinary", "genderAgender", "genderTwoSpirit"):
        assert GMEOW + ind in values
    assert len(values) >= 11
