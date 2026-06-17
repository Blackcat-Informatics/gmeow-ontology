"""Tests for the UFO/OntoUML anti-pattern checks (gmeow_tools.reasoning_lint).

A positive end-to-end test asserts the real, meta-grounded ontology is clean
(every class stereotyped, no anti-patterns). Negative tests seed one anti-pattern
each into a minimal in-memory graph and assert the matching check fires — the same
fixture idiom as :mod:`tests.test_validate` and :mod:`tests.test_statements`.
"""

from __future__ import annotations

import gmeow_validate
from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL, Namespace

from gmeow_tools.config import NAMESPACE, PREFIXES
from gmeow_tools.graph import iter_source_files

# Graph-accepting test shims: serialize a synthetic rdflib graph to N-Triples and
# route it through the graph-free production reasoning checks (#579).
from tests._graph_nt import (
    anti_rigidity_discipline,
    exactly_one_stereotype,
    identity_overlap,
    relator_mediation,
)

GUFO = Namespace(PREFIXES["gufo"])


def _cls(
    graph: Graph, name: str, *stereotypes: URIRef, parent: URIRef | None = None
) -> URIRef:
    """Declare a minimal GMEOW class punned with the given gUFO stereotype(s)."""
    iri = URIRef(NAMESPACE + name)
    graph.add((iri, RDF.type, OWL.Class))
    for stereotype in stereotypes:
        graph.add((iri, RDF.type, stereotype))
    if parent is not None:
        graph.add((iri, RDFS.subClassOf, parent))
    return iri


# --------------------------------------------------------------------------- #
# Positive: the real ontology is fully and correctly meta-grounded.
# --------------------------------------------------------------------------- #


def test_real_ontology_is_clean() -> None:
    # The real ontology is checked graph-free over its source paths (#579).
    report = gmeow_validate.reasoning_invariants(
        [str(p) for p in iter_source_files()], str(NAMESPACE)
    )
    assert list(report["errors"]) == []


# --------------------------------------------------------------------------- #
# Negative: one seeded anti-pattern per check.
# --------------------------------------------------------------------------- #


def test_missing_stereotype_is_flagged() -> None:
    graph = Graph()
    graph.add((URIRef(NAMESPACE + "Bare"), RDF.type, OWL.Class))
    problems = exactly_one_stereotype(graph)
    assert any("carries no gUFO meta-class" in p for p in problems)


def test_conflicting_stereotypes_are_flagged() -> None:
    graph = Graph()
    _cls(graph, "TwoFaced", GUFO.Kind, GUFO.Role)
    problems = exactly_one_stereotype(graph)
    assert any("conflicting gUFO meta-classes" in p for p in problems)


def test_kind_under_kind_is_flagged_mixiden() -> None:
    graph = Graph()
    animal = _cls(graph, "Animal", GUFO.Kind)
    _cls(graph, "Dog", GUFO.Kind, parent=animal)  # a Kind specializing a Kind
    problems = identity_overlap(graph)
    assert any("MixIden" in p and "gmeow:Dog" in p for p in problems)


def test_free_role_is_flagged() -> None:
    graph = Graph()
    _cls(graph, "Wanderer", GUFO.Role)  # a Role specializing no rigid sortal
    problems = anti_rigidity_discipline(graph)
    assert any("FreeRole" in p for p in problems)


def test_rigid_under_anti_rigid_is_flagged_mixrig() -> None:
    graph = Graph()
    student = _cls(graph, "Student", GUFO.Role)
    _cls(graph, "HonorsStudent", GUFO.SubKind, parent=student)  # rigid ⊑ anti-rigid
    problems = anti_rigidity_discipline(graph)
    # The message names both the rigid class and the offending anti-rigid ancestor.
    assert any(
        "MixRig" in p and "gmeow:HonorsStudent" in p and "gmeow:Student" in p
        for p in problems
    )


def test_under_mediated_relator_is_flagged_relcomp() -> None:
    graph = Graph()
    relator = _cls(graph, "LonelyBond", GUFO.Kind, parent=GUFO.Relator)
    # exactly one functional mediation = one end < the required two
    prop = URIRef(NAMESPACE + "bondParty")
    graph.add((prop, RDF.type, OWL.ObjectProperty))
    graph.add((prop, RDF.type, OWL.FunctionalProperty))
    graph.add((prop, RDFS.domain, relator))
    graph.add((prop, RDFS.range, URIRef(NAMESPACE + "Person")))
    problems = relator_mediation(graph)
    assert any("RelComp" in p and "gmeow:LonelyBond" in p for p in problems)


def test_well_formed_relator_passes() -> None:
    graph = Graph()
    relator = _cls(graph, "Bond", GUFO.Kind, parent=GUFO.Relator)
    for role in ("bondLeft", "bondRight"):
        prop = URIRef(NAMESPACE + role)
        graph.add((prop, RDF.type, OWL.ObjectProperty))
        graph.add((prop, RDF.type, OWL.FunctionalProperty))
        graph.add((prop, RDFS.domain, relator))
        graph.add((prop, RDFS.range, URIRef(NAMESPACE + "Person")))
    assert relator_mediation(graph) == []


def test_abstract_relator_base_is_exempt() -> None:
    graph = Graph()
    base = _cls(graph, "AbstractBond", GUFO.Kind, parent=GUFO.Relator)
    _cls(graph, "ConcreteBond", GUFO.SubKind, parent=base)  # base has a gmeow subclass
    # the base carries no mediations of its own, but it is abstract → not flagged
    assert not any("gmeow:AbstractBond" in p for p in relator_mediation(graph))
