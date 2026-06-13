"""Music structural foundation gates (issue #307).

Principles 4, 5, 6, 9, 11, 15, 16.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, SH, Graph, Literal, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
SHACL = Namespace("http://www.w3.org/ns/shacl#")


def _graph() -> Graph:
    """Load the project's merged RDF graph without following owl:imports."""
    return load_merged_graph(include_imports=False)


def test_genre_is_never_subclassed() -> None:
    """Principle 9: Genre is a first-class Kind (the Language precedent),
    never a subclass tree."""
    graph = _graph()
    genre = GMEOW.Genre
    subclasses = {
        cls
        for cls in graph.transitive_subjects(RDFS.subClassOf, genre)
        if isinstance(cls, URIRef) and cls != genre
    }
    assert not subclasses, "Genre must not be subclassed; found: " + ", ".join(
        sorted(map(str, subclasses))
    )


def test_oral_tradition_guarantee() -> None:
    """No SHACL shape may require a Work to have a notated Expression
    (Principle 9 / #306 oral-tradition guarantee)."""
    # Load the committed SHACL shapes as a data graph for this structural scan.
    shapes_path = Path(__file__).resolve().parents[1] / "shapes" / "gmeow-shapes.ttl"
    shapes = Graph().parse(shapes_path, format="turtle")

    offending: list[URIRef] = []
    for shape in shapes.subjects(RDF.type, SH.NodeShape):
        if not isinstance(shape, URIRef):
            continue
        targets = set(shapes.objects(shape, SH.targetClass))
        if GMEOW.Work not in targets:
            continue
        for prop in shapes.objects(shape, SH.property):
            if (
                shapes.value(prop, SH.path) == GMEOW.realizationMode
                and shapes.value(prop, SH.hasValue) == GMEOW.realizationModeNotated
            ):
                offending.append(shape)
                break
            path = shapes.value(prop, SH.path)
            if path == GMEOW.realizationMode:
                min_count = shapes.value(prop, SH.minCount)
                if isinstance(min_count, Literal) and int(min_count) >= 1:
                    offending.append(shape)
                    break

    assert not offending, (
        "SHACL shapes must not require a Work to carry a notated realization mode: "
        + ", ".join(sorted(map(str, offending)))
    )


def test_dual_typed_music_roles() -> None:
    """performer/conductor/producer are one concept, one IRI, typed as both
    ContributionRole and ParticipantRole (Principle 5)."""
    graph = _graph()
    for role in (GMEOW.rolePerformer, GMEOW.roleConductor, GMEOW.roleProducer):
        assert (role, RDF.type, GMEOW.ContributionRole) in graph, (
            f"{role} missing ContributionRole"
        )
        assert (role, RDF.type, GMEOW.ParticipantRole) in graph, (
            f"{role} missing ParticipantRole"
        )


def test_music_properties_functionality() -> None:
    """Constitutive properties are functional; source-variable properties are not.

    A CreativeDerivation has exactly one source and one product; an Expression
    has exactly one realization mode. Derivation type and genre are open-valued
    and may be many."""
    graph = _graph()
    constitutive = [
        GMEOW.derivationSource,
        GMEOW.derivationProduct,
        GMEOW.realizationMode,
    ]
    source_variable = [
        GMEOW.derivationType,
        GMEOW.hasGenre,
    ]

    for prop in constitutive:
        assert (prop, RDF.type, OWL.FunctionalProperty) in graph, (
            f"Constitutive property {prop} must be functional"
        )

    for prop in source_variable:
        assert (prop, RDF.type, OWL.FunctionalProperty) not in graph, (
            f"Source-variable property {prop} must not be functional"
        )
