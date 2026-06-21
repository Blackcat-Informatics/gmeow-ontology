"""Entailment & consistency tests for the axiomatized doctrine (#38).

Phase 2 of the reasoning-depth epic (#35) turns invariants that were only *tested*
in Python over the asserted graph into reasoner *theorems*. This module proves
they actually bite, using two reasoners for the two halves of the OWL-infers /
SHACL-validates split (Principle 8):

* **native RL** (``gmeow_tools.native_rl.native_rl_closure``, the native
  ``gmeow_logic`` OWL 2 RL engine) materializes POSITIVE entailments — derived
  ancestry, location-through-containment, sub-organization transitivity —
  Java/Docker-free, on every run. Each test loads the *real authored* module so
  it pins the shipped axioms, not a hand-built fixture. (The legacy ``owlrl``
  baseline is now the classic-cross-check lane's agreement oracle, issue #666.)
* The live HermiT/ROBOT inconsistency and fixture-coherence cases run through a
  repo-local script so Make/CI can schedule Docker outside pytest.
  This module keeps Docker-free coverage of the pure entailments and the
  reasoning-case orchestration.
"""

from __future__ import annotations

from functools import cache

import pytest
from rdflib import RDF, RDFS, Graph, Namespace
from rdflib.term import Node

from gmeow_tools import reasoning_cases
from gmeow_tools.native_rl_rdflib import native_rl_closure
from gmeow_tools.slices import module_path

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


# --------------------------------------------------------------------------- #
# Positive entailments — owlrl (pure Python, no Docker)
# --------------------------------------------------------------------------- #


@cache
def _parsed_module(module: str) -> Graph:
    """Parse a single authored module (cached) — closure runs with A-Box present."""
    graph = Graph()
    graph.parse(module_path(module), format="turtle")
    return graph


def _materialize(module: str, *abox: tuple[Node, Node, Node]) -> Graph:
    """Close a real authored module + a tiny A-Box under OWL 2 RL."""
    graph = Graph()
    for triple in _parsed_module(module):
        graph.add(triple)
    for triple in abox:
        graph.add(triple)
    native_rl_closure(graph)
    return graph


def test_ancestry_is_derived_not_asserted() -> None:
    """hasParent ∘ hasParent ⊑ hasAncestor (transitive sub-property), DERIVED."""
    graph = _materialize(
        "genealogy",
        (EX.a, GMEOW.hasParent, EX.b),
        (EX.b, GMEOW.hasParent, EX.c),
    )
    # The grandparent edge is asserted nowhere yet is entailed.
    assert (EX.a, GMEOW.hasAncestor, EX.c) in graph
    # Parentage feeds ancestry; the transitive inverse closes descendants too.
    assert (EX.a, GMEOW.hasAncestor, EX.b) in graph
    assert (EX.c, GMEOW.hasDescendant, EX.a) in graph


def test_location_propagates_through_containment() -> None:
    """locatedAt ∘ containedInPlace ⊑ locatedAt: in your room means in your city."""
    graph = _materialize(
        "places",
        (EX.thing, GMEOW.locatedAt, EX.room),
        (EX.room, GMEOW.containedInPlace, EX.city),
    )
    assert (EX.thing, GMEOW.locatedAt, EX.city) in graph


def test_suborganization_is_transitive() -> None:
    """subOrganizationOf is transitive — a team is part of the parent company."""
    graph = _materialize(
        "organization",
        (EX.team, GMEOW.subOrganizationOf, EX.div),
        (EX.div, GMEOW.subOrganizationOf, EX.corp),
    )
    assert (EX.team, GMEOW.subOrganizationOf, EX.corp) in graph


def test_proximity_measurement_is_a_measurement() -> None:
    """ProximityMeasurement ⊑ Measurement is asserted and survives materialization."""
    graph = _materialize(
        "places",
        (EX.commute, RDF.type, GMEOW.ProximityMeasurement),
        (EX.commute, GMEOW.proximityTo, EX.home),
        (EX.commute, GMEOW.observationResult, EX.dist),
        (EX.dist, RDF.type, GMEOW.ScalarQuantity),
    )
    # The asserted subclassOf is preserved through materialization.
    assert (GMEOW.ProximityMeasurement, RDFS.subClassOf, GMEOW.Measurement) in graph
    # And the instance is typed in both the asserted and reasoned graph.
    assert (EX.commute, RDF.type, GMEOW.ProximityMeasurement) in graph
    assert (EX.commute, RDF.type, GMEOW.Measurement) in graph


# --------------------------------------------------------------------------- #
# Negative & coherence orchestration — mocked here, live in `scripts/reasoning_cases.py`
# --------------------------------------------------------------------------- #


def test_two_axis_case_expects_inconsistency(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The Docker lane fails if the two-axis bad individual is coherent."""
    calls: list[tuple[str, str]] = []

    def fake_consistent(extra: Graph, name: str, *, reasoner: str = "hermit") -> bool:
        calls.append((name, reasoner))
        assert (
            reasoning_cases.EX.x,
            RDF.type,
            reasoning_cases.GMEOW.GenderIdentity,
        ) in extra
        assert (
            reasoning_cases.EX.x,
            RDF.type,
            reasoning_cases.GMEOW.GenderExpression,
        ) in extra
        return False

    monkeypatch.setattr(reasoning_cases, "_is_consistent", fake_consistent)
    reasoning_cases.assert_two_axis_individual_is_inconsistent()
    assert calls == [("two-axis", "hermit")]


def test_two_kind_case_expects_inconsistency(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The Docker lane fails if the two-kind bad individual is coherent."""
    calls: list[tuple[str, str]] = []

    def fake_consistent(extra: Graph, name: str, *, reasoner: str = "hermit") -> bool:
        calls.append((name, reasoner))
        assert (reasoning_cases.EX.y, RDF.type, reasoning_cases.GMEOW.Person) in extra
        assert (
            reasoning_cases.EX.y,
            RDF.type,
            reasoning_cases.GMEOW.Organization,
        ) in extra
        return False

    monkeypatch.setattr(reasoning_cases, "_is_consistent", fake_consistent)
    reasoning_cases.assert_two_kind_individual_is_inconsistent()
    assert calls == [("two-kind", "hermit")]


def test_reasoning_cases_run_all_order(monkeypatch: pytest.MonkeyPatch) -> None:
    """The repo script lane keeps the intended Docker cases in one order."""
    calls: list[str] = []
    monkeypatch.setattr(reasoning_cases, "merge_release", lambda *_a, **_kw: None)
    monkeypatch.setattr(
        reasoning_cases,
        "assert_two_axis_individual_is_inconsistent",
        lambda: calls.append("axis"),
    )
    monkeypatch.setattr(
        reasoning_cases,
        "assert_two_kind_individual_is_inconsistent",
        lambda: calls.append("kind"),
    )
    monkeypatch.setattr(
        reasoning_cases,
        "assert_worked_fixtures_stay_coherent_under_disjointness",
        lambda: calls.append("fixtures"),
    )

    completed = reasoning_cases.run_all()

    assert calls == ["axis", "kind", "fixtures"]
    assert completed == [
        "two-axis inconsistency",
        "two-kind inconsistency",
        "worked-fixture coherence",
    ]
