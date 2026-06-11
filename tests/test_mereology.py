"""Universal mereology spine and propagation rules (#76)."""

from __future__ import annotations

import owlrl
from rdflib import OWL, RDF, RDFS, Graph, Namespace
from rdflib.term import Node

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.slices import module_path

GM = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/mereology/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _materialize(*modules: str, abox: tuple[tuple[Node, Node, Node], ...]) -> Graph:
    """Close real authored modules + a tiny A-Box under OWL 2 RL."""
    graph = Graph()
    for module in modules:
        graph.parse(module_path(module), format="turtle")
    for triple in abox:
        graph.add(triple)
    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    return graph


def test_universal_part_properties_are_broad_transitive_inverses() -> None:
    g = _graph()
    for prop in (GM.partOf, GM.hasPart):
        assert (prop, RDF.type, OWL.ObjectProperty) in g
        assert (prop, RDF.type, OWL.TransitiveProperty) in g
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g
        assert not list(g.objects(prop, RDFS.domain))
        assert not list(g.objects(prop, RDFS.range))

    assert (GM.partOf, OWL.inverseOf, GM.hasPart) in g
    assert (GM.hasPart, OWL.inverseOf, GM.partOf) in g


def test_existing_part_like_relations_specialize_the_spine() -> None:
    g = _graph()
    part_subproperties = {
        GM.containedInLocation,
        GM.containedInPlace,
        GM.rcc8tpp,
        GM.rcc8ntpp,
        GM.subOrganizationOf,
        GM.subEventOf,
        GM.partOfThread,
    }
    has_part_subproperties = {
        GM.rcc8tppi,
        GM.rcc8ntppi,
        GM.hasSubEvent,
        GM.hasNamePart,
        GM.hasBodyPart,
        GM.hasAttachment,
    }

    for prop in part_subproperties:
        assert (prop, RDFS.subPropertyOf, GM.partOf) in g
    for prop in has_part_subproperties:
        assert (prop, RDFS.subPropertyOf, GM.hasPart) in g


def test_specialized_part_relations_entail_generic_parthood() -> None:
    g = _materialize(
        "core",
        "places",
        "organization",
        "events",
        "email",
        abox=(
            (EX.room, GM.containedInPlace, EX.building),
            (EX.team, GM.subOrganizationOf, EX.division),
            (EX.talk, GM.subEventOf, EX.session),
            (EX.message, GM.hasBodyPart, EX.mimePart),
        ),
    )
    assert (EX.room, GM.partOf, EX.building) in g
    assert (EX.team, GM.partOf, EX.division) in g
    assert (EX.talk, GM.partOf, EX.session) in g
    assert (EX.message, GM.hasPart, EX.mimePart) in g


def test_member_of_propagates_through_suborganization() -> None:
    g = _materialize(
        "core",
        "organization",
        abox=(
            (EX.alex, GM.memberOf, EX.team),
            (EX.team, GM.subOrganizationOf, EX.division),
            (EX.division, GM.subOrganizationOf, EX.company),
        ),
    )
    assert (EX.alex, GM.memberOf, EX.division) in g
    assert (EX.alex, GM.memberOf, EX.company) in g


def test_event_location_propagates_through_spatial_containment_only() -> None:
    g = _materialize(
        "core",
        "places",
        "events",
        abox=(
            (EX.meeting, GM.eventLocation, EX.room),
            (EX.room, GM.containedInPlace, EX.building),
            (EX.building, GM.containedInPlace, EX.city),
        ),
    )
    assert (EX.meeting, GM.eventLocation, EX.building) in g
    assert (EX.meeting, GM.eventLocation, EX.city) in g


def test_no_winner_or_cardinality_terms_for_parts() -> None:
    g = _graph()
    forbidden_locals = {
        "primaryPart",
        "preferredPart",
        "primaryWhole",
        "preferredWhole",
    }
    locals_seen = {
        str(s).removeprefix(str(GM)) for s in g.subjects() if str(s).startswith(str(GM))
    }
    assert forbidden_locals.isdisjoint(locals_seen)

    for prop in (GM.partOf, GM.hasPart, GM.subOrganizationOf, GM.subEventOf):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g
