"""The centrepiece ethical invariant: the identity axes are ORTHOGONAL.

A person's address (pronouns, honorifics — names module), gender identity, gender
expression, sex-assigned-at-birth (gender module), and sexual + romantic
orientation (sexuality module) are independent axes. None may be inferred from
another. Since #281 the live enforcement is ANNOTATION-DRIVEN —
``gmeow:coequalFacet true`` marks each axis, and
:func:`gmeow_tools.reasoning_lint.coequal_facet_orthogonality` derives every
invariant from the annotation set inside ``make validate``, so a new axis is
enforced the moment it is declared. This file is the REGRESSION layer over that
machinery: it pins the historical seven-axis matrix explicitly (the annotation
set must cover it, the lint must hold on it, and seeded violations must be
caught), plus the co-equality guard: no preferred/primary marker anywhere.
"""

from __future__ import annotations

from itertools import combinations

from rdflib import OWL, RDF, RDFS, Graph, URIRef
from rdflib.collection import Collection

from gmeow_tools.graph import load_merged_graph

# Graph-accepting shim: serialize a synthetic rdflib graph and route it through
# the graph-free production reasoning check (#579).
from tests._graph_nt import coequal_facet_orthogonality

GMEOW = "https://blackcatinformatics.ca/gmeow/"

# The seven orthogonal axis properties and the value/facet class each ranges over.
# This is the REGRESSION pin — the live axis set is the gmeow:coequalFacet
# annotations in the modules, and must always cover (at least) these seven.
AXES: dict[str, str] = {
    "hasPronounSet": "PronounSet",  # address (names)
    "honorific": "Honorific",  # address (names)
    "hasGenderIdentity": "GenderIdentity",  # identity (gender)
    "hasGenderExpression": "GenderExpression",
    "sexAssignedAtBirth": "SexAssignedAtBirth",
    "hasSexualOrientation": "SexualOrientation",  # orientation (sexuality)
    "hasRomanticOrientation": "RomanticOrientation",
}


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _annotated_axes(graph: Graph) -> set[URIRef]:
    coequal = URIRef(GMEOW + "coequalFacet")
    return {
        s
        for s, o in graph.subject_objects(coequal)
        if isinstance(s, URIRef) and str(o) == "true"
    }


def test_annotation_set_covers_the_historical_seven_axes() -> None:
    """Every historical axis carries gmeow:coequalFacet true (#281).

    Superset, not equality: new co-equal facets (e.g. #263's
    SkillProficiency) extend the matrix and are enforced automatically.
    """
    annotated = _annotated_axes(_graph())
    expected = {URIRef(GMEOW + prop) for prop in AXES}
    missing = expected - annotated
    assert not missing, f"axes missing gmeow:coequalFacet true: {sorted(missing)}"


def test_coequal_facet_lint_holds_on_the_real_matrix() -> None:
    """The annotation-driven lint (the live enforcement) is clean."""
    problems = coequal_facet_orthogonality(_graph())
    assert not problems, "\n".join(problems)


def test_coequal_facet_lint_catches_seeded_violations() -> None:
    """Each violation class is detected when seeded into a copy of the graph.

    The lint is the live gate; this proves it actually fires — for a
    functional-property axis, a subPropertyOf bridge, and an axis missing
    from the joint disjointness axiom (a fresh axis with its own range).
    """
    base = _graph()
    a = URIRef(GMEOW + "hasGenderIdentity")
    b = URIRef(GMEOW + "hasGenderExpression")

    seeded = Graph()
    for triple in base:
        seeded.add(triple)
    seeded.add((a, RDF.type, OWL.FunctionalProperty))
    seeded.add((a, RDFS.subPropertyOf, b))
    fresh = URIRef(GMEOW + "hasTestFacet")
    seeded.add((fresh, RDF.type, OWL.ObjectProperty))
    seeded.add(
        (
            fresh,
            URIRef(GMEOW + "coequalFacet"),
            next(iter(seeded.objects(a, URIRef(GMEOW + "coequalFacet")))),
        )
    )
    seeded.add((fresh, RDFS.range, URIRef(GMEOW + "TestFacetValue")))

    problems = "\n".join(coequal_facet_orthogonality(seeded))
    assert "owl:FunctionalProperty" in problems
    assert "bridged" in problems
    assert "not jointly declared" in problems


def test_every_axis_property_exists_with_its_own_range() -> None:
    graph = _graph()
    ranges: set[URIRef] = set()
    for prop, rng in AXES.items():
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph or (
            node,
            RDF.type,
            OWL.DatatypeProperty,
        ) in graph, f"{prop} must be defined"
        # The axis ranges over its facet/value class EXCLUSIVELY — exactly one
        # range, not its own plus extras (a stray shared range would let two axes
        # collapse into the same value space and weaken the orthogonality guard).
        declared = set(graph.objects(node, RDFS.range))
        assert declared == {URIRef(GMEOW + rng)}, f"{prop} must range over only {rng}"
        ranges.add(URIRef(GMEOW + rng))
    # All seven ranges are distinct — no two axes share a value space.
    assert len(ranges) == len(AXES)


def test_no_axis_is_inferred_from_another() -> None:
    """For every ordered pair, no subProperty/equivalence bridge in either direction."""
    graph = _graph()
    for a, b in combinations(AXES, 2):
        na, nb = URIRef(GMEOW + a), URIRef(GMEOW + b)
        assert (na, RDFS.subPropertyOf, nb) not in graph, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in graph, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in graph, f"{a} ≡ {b} forbidden"
        assert (nb, OWL.equivalentProperty, na) not in graph, f"{b} ≡ {a} forbidden"


def _all_disjoint_member_sets(graph: Graph) -> list[set[URIRef]]:
    """Every owl:AllDisjointClasses, as the set of class IRIs it makes disjoint."""
    sets: list[set[URIRef]] = []
    for node in graph.subjects(RDF.type, OWL.AllDisjointClasses):
        members = graph.value(node, OWL.members)
        if members is not None:
            members_list = Collection(graph, members)
            sets.append({m for m in members_list if isinstance(m, URIRef)})
    return sets


def test_identity_axes_are_disjoint_classes_axiom() -> None:
    """The matrix is now also an OWL theorem (#38), not only a Python guard.

    The seven axis range classes are jointly disjoint via a single
    owl:AllDisjointClasses — so the four IdentityFacet siblings (the load-bearing
    case that gUFO does not already separate) are pairwise disjoint, and a
    reasoner rejects any individual placed in two axes. This complements, and does
    not replace, the subPropertyOf/equivalentProperty-absence guards above.
    """
    member_sets = _all_disjoint_member_sets(_graph())
    axis_classes = {URIRef(GMEOW + rng) for rng in AXES.values()}
    assert any(axis_classes <= s for s in member_sets), (
        "the seven identity axes must share one owl:AllDisjointClasses"
    )
    facets = {
        URIRef(GMEOW + c)
        for c in (
            "GenderIdentity",
            "GenderExpression",
            "SexualOrientation",
            "RomanticOrientation",
        )
    }
    assert any(facets <= s for s in member_sets), (
        "the four IdentityFacet siblings must be jointly disjoint"
    )


def test_no_preferred_or_primary_identity_term() -> None:
    """Co-equality across identity axes: no preferred/primary marker anywhere."""
    graph = _graph()
    property_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryGender",
        "preferredGender",
        "primaryOrientation",
        "preferredOrientation",
        "primaryIdentity",
        "preferredIdentity",
    ):
        node = URIRef(GMEOW + banned)
        for pt in property_types:
            assert (node, RDF.type, pt) not in graph, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in graph
