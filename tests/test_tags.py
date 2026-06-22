"""Structural + DL-safety guards for the tags building block.

Pins gmeow:Tag / gmeow:TagScheme (InformationObject subclasses), gmeow:Tagging
(gufo:Relator), the trichotomy of typing / aboutness / tagging, the flat+reified
duality, the anti-property-bag rule, and the tag-relation semantics (transitive
broaderTag, symmetric relatedTag).
"""

from __future__ import annotations

from itertools import combinations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_tag_is_information_object() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Tag"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    assert (
        URIRef(GMEOW + "TagScheme"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph


def test_tagging_is_a_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Tagging"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


# --------------------------------------------------------------------------- #
# Open value vocabulary (individuals, not subclasses)
# --------------------------------------------------------------------------- #


def test_tag_values_are_individuals_not_subclasses() -> None:
    graph = _graph()
    for ind in ("tagUrgent", "tagTodo", "tagReview"):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "Tag")) in graph
    for rejected in ("UrgentTag", "TodoTag", "ReviewTag"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


# --------------------------------------------------------------------------- #
# Property typing
# --------------------------------------------------------------------------- #


def test_has_tag_is_object_property_ranged_on_tag() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasTag")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "Tag")) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_is_about_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "isAbout")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_tagging_roles_are_functional() -> None:
    graph = _graph()
    for prop in ("taggingTagged", "taggingTag", "taggingScheme"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


def test_tagging_tagger_is_non_functional() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "taggingTagger")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


# --------------------------------------------------------------------------- #
# Anti-property-bag: no datatype/value property on Tag / Tagging
# --------------------------------------------------------------------------- #


def test_no_tag_value_datatype_property() -> None:
    """A tag is a label-bearing information object, never a property bag."""
    graph = _graph()
    for banned in ("tagValue", "tagColor", "tagPriority", "tagScore"):
        node = URIRef(GMEOW + banned)
        msg = f"{banned} is baggage"
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph, msg


# --------------------------------------------------------------------------- #
# Trichotomy guard: typing ⟂ aboutness ⟂ tagging
# --------------------------------------------------------------------------- #


def test_no_bridge_among_has_tag_is_about_and_rdf_type() -> None:
    """The three axes must remain orthogonal — no subPropertyOf or equivalentProperty
    bridge among them. OWL cannot express disjointness with rdf:type (not an
    ObjectProperty in the ontology), so the closed-world guard lives here."""
    graph = _graph()
    axes = {
        "hasTag": URIRef(GMEOW + "hasTag"),
        "isAbout": URIRef(GMEOW + "isAbout"),
        "rdf:type": RDF.type,
    }
    for a, b in combinations(axes, 2):
        na, nb = axes[a], axes[b]
        assert (na, RDFS.subPropertyOf, nb) not in graph, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in graph, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in graph, f"{a} ≡ {b} forbidden"
        assert (nb, OWL.equivalentProperty, na) not in graph, f"{b} ≡ {a} forbidden"


def test_has_tag_and_is_about_are_property_disjoint() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasTag"),
        OWL.propertyDisjointWith,
        URIRef(GMEOW + "isAbout"),
    ) in graph


# --------------------------------------------------------------------------- #
# Tag relation semantics
# --------------------------------------------------------------------------- #


def test_broader_tag_is_transitive() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "broaderTag"),
        RDF.type,
        OWL.TransitiveProperty,
    ) in graph


def test_narrower_tag_is_inverse_of_broader() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "narrowerTag"),
        OWL.inverseOf,
        URIRef(GMEOW + "broaderTag"),
    ) in graph


def test_related_tag_is_symmetric() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "relatedTag"),
        RDF.type,
        OWL.SymmetricProperty,
    ) in graph


# --------------------------------------------------------------------------- #
# Relator mediation (EL someValuesFrom)
# --------------------------------------------------------------------------- #


def test_tagging_has_el_existential_restrictions() -> None:
    graph = _graph()
    tagging = URIRef(GMEOW + "Tagging")
    restrictions = []
    for restr in graph.objects(tagging, RDFS.subClassOf):
        if (restr, RDF.type, OWL.Restriction) in graph:
            prop = graph.value(restr, OWL.onProperty)
            cls = graph.value(restr, OWL.someValuesFrom)
            restrictions.append((prop, cls))
    assert (URIRef(GMEOW + "taggingTagged"), URIRef(GMEOW + "Entity")) in restrictions
    assert (URIRef(GMEOW + "taggingTag"), URIRef(GMEOW + "Tag")) in restrictions
