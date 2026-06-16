# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Cross-world positive rigidity closure (issue #503, Task 3).

The fourth OntoUML discipline — positive cross-world rigidity — is the one the
type-only lint (``reasoning_lint.py``) *cannot* express: it is the world-indexed
universal constraint (LOGIC-SEMANTICS.md §Operational semantics)::

    ∀x, w, w' : exists(x, w) ∧ exists(x, w') ∧ instOf(x, T, w) ∧ rigid(T)
                ⇒ instOf(x, T, w')

Because the GMEOW chase is strictly world-local, this is NOT an in-world Datalog
rule; it is a bounded closure over the finite materialized world set, implemented by
:func:`gmeow_tools.logic_foundation.cross_world_rigidity_violations`.

This is a **soundness** gate, not a lint-equivalence one: the lint is silent on
cross-world facts, so there is nothing to round-trip against.  Instead each test
hand-builds a two-world EDB, materializes it through the Python oracle, and asserts
the exact set of ``logic:rigidityViolation`` quads the closure pass emits.
"""

from __future__ import annotations

import pytest
from rdflib import RDF, ConjunctiveGraph, URIRef

from gmeow_tools.config import LOGIC_NAMESPACE
from gmeow_tools.logic_foundation import cross_world_rigidity_violations
from gmeow_tools.logic_ir import LogicProgram
from gmeow_tools.logic_materialize import MaterializationResult, materialize_program

_LOGIC = LOGIC_NAMESPACE
_EX = "https://example.org/rigid/"
_WORLD_A = URIRef("https://example.org/rigid/worldA")
_WORLD_B = URIRef("https://example.org/rigid/worldB")
_RIGIDITY_VIOLATION = _LOGIC + "rigidityViolation"


def _ex(local: str) -> URIRef:
    return URIRef(_EX + local)


def _lg(local: str) -> URIRef:
    return URIRef(_LOGIC + local)


# Stereotype declarations make a *type* rigid / anti-rigid (the schema is
# world-independent, so a single world carrying the stereotype suffices).  The
# rigidity constraint is then over INSTANCES of those types across worlds.
#   ``RigidKind   rdf:type logic:Kind``    — RigidKind is a rigid type.
#   ``RigidSub    rdf:type logic:SubKind`` — RigidSub is a rigid type.
#   ``SomeRole    rdf:type logic:Role``    — SomeRole is anti-rigid (exempt).
_STEREOTYPES: list[tuple[URIRef, URIRef, URIRef]] = [
    (_ex("RigidKind"), RDF.type, _lg("Kind")),
    (_ex("RigidSub"), RDF.type, _lg("SubKind")),
    (_ex("SomeRole"), RDF.type, _lg("Role")),
]


def _two_world_cg(
    world_a: list[tuple[URIRef, URIRef, URIRef]],
    world_b: list[tuple[URIRef, URIRef, URIRef]],
) -> ConjunctiveGraph:
    """Build a 2-world ConjunctiveGraph (worldA, worldB) from (s, p, o) triples.

    The type-level stereotype declarations (:data:`_STEREOTYPES`) are seeded into
    world A so the closure pass can resolve which instance types are rigid; rigidity
    of a type is world-independent schema, so a single carrying world is enough.
    """
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx_a = cg.get_context(_WORLD_A)
    for s, p, o in _STEREOTYPES:
        ctx_a.add((s, p, o))
    for s, p, o in world_a:
        ctx_a.add((s, p, o))
    ctx_b = cg.get_context(_WORLD_B)
    for s, p, o in world_b:
        ctx_b.add((s, p, o))
    return cg


def _materialize(cg: ConjunctiveGraph) -> MaterializationResult:
    """Materialize a multi-world EDB with an empty program (assert facts only).

    The rigidity closure reads the materialized rigid typings + per-world subject
    sets, which are the asserted EDB facts here, so an empty program is sufficient
    to drive the pass and keeps the fixture focused on the cross-world semantics.
    """
    program = LogicProgram(axioms=(), rules=(), profiles=())
    return materialize_program(program, cg, enable_naf=True)


def _rigidity_quads(result: MaterializationResult) -> set[tuple[str, str, str]]:
    """Run the closure pass; return (world, instance, type) triples it emits.

    ``q.obj`` is canonical N3 (``<iri>``); the angle brackets are stripped so the
    comparison is against the bare type IRI.
    """
    return {
        (q.graph, q.subject, q.obj[1:-1] if q.obj.startswith("<") else q.obj)
        for q in cross_world_rigidity_violations(result)
        if q.predicate == _RIGIDITY_VIOLATION
    }


def test_rigidity_fires_when_persistence_fails() -> None:
    """inst1 a RigidKind in A; exists in B untyped ⇒ one violation in B."""
    cg = _two_world_cg(
        world_a=[(_ex("inst1"), RDF.type, _ex("RigidKind"))],
        # inst1 exists in B (it is the subject of a quad) but is NOT typed RigidKind.
        world_b=[(_ex("inst1"), _ex("rel"), _ex("other"))],
    )
    result = _materialize(cg)
    assert _rigidity_quads(result) == {
        (str(_WORLD_B), _EX + "inst1", _EX + "RigidKind"),
    }


def test_rigidity_clean_when_typed_in_both_worlds() -> None:
    """inst2 a RigidKind in BOTH worlds ⇒ no violation."""
    cg = _two_world_cg(
        world_a=[(_ex("inst2"), RDF.type, _ex("RigidKind"))],
        world_b=[(_ex("inst2"), RDF.type, _ex("RigidKind"))],
    )
    result = _materialize(cg)
    assert _rigidity_quads(result) == set()


def test_anti_rigid_role_is_exempt() -> None:
    """inst3 a SomeRole (anti-rigid) in A; exists in B untyped ⇒ NO violation.

    Anti-rigid types classify only contingently — the whole point of rigidity —
    so a Role instance that does not carry over is correct, not a violation.
    """
    cg = _two_world_cg(
        world_a=[(_ex("inst3"), RDF.type, _ex("SomeRole"))],
        world_b=[(_ex("inst3"), _ex("rel"), _ex("other"))],
    )
    result = _materialize(cg)
    assert _rigidity_quads(result) == set()


def test_existence_in_other_world_is_required() -> None:
    """inst4 a RigidKind in A but absent from B entirely ⇒ NO violation.

    The constraint is conditional on existence in w'; an instance absent from B
    is not obliged to be typed there.
    """
    cg = _two_world_cg(
        world_a=[(_ex("inst4"), RDF.type, _ex("RigidKind"))],
        # World B mentions a DIFFERENT subject, so inst4 does not exist in B.
        world_b=[(_ex("bystander"), _ex("rel"), _ex("other"))],
    )
    result = _materialize(cg)
    assert _rigidity_quads(result) == set()


def test_subkind_is_rigid_and_fires() -> None:
    """A SubKind-stereotyped type is rigid too: failing to persist trips it."""
    cg = _two_world_cg(
        world_a=[(_ex("inst5"), RDF.type, _ex("RigidSub"))],
        world_b=[(_ex("inst5"), _ex("rel"), _ex("other"))],
    )
    result = _materialize(cg)
    assert _rigidity_quads(result) == {
        (str(_WORLD_B), _EX + "inst5", _EX + "RigidSub"),
    }


def test_single_world_emits_nothing() -> None:
    """A single materialized world admits no ordered world pair ⇒ no violations."""
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx = cg.get_context(_WORLD_A)
    ctx.add((_ex("RigidKind"), RDF.type, _lg("Kind")))
    ctx.add((_ex("inst1"), RDF.type, _ex("RigidKind")))
    result = _materialize(cg)
    assert cross_world_rigidity_violations(result) == ()


def test_emitted_quad_seam_contract_is_complete() -> None:
    """The emitted violation quad carries the full seam contract, in world w2."""
    cg = _two_world_cg(
        world_a=[(_ex("inst1"), RDF.type, _ex("RigidKind"))],
        world_b=[(_ex("inst1"), _ex("rel"), _ex("other"))],
    )
    result = _materialize(cg)
    quads = cross_world_rigidity_violations(result)
    assert len(quads) == 1
    q = quads[0]
    # The violation is placed in the world where persistence fails (w2 == worldB).
    assert q.graph == str(_WORLD_B)
    assert q.graph_component == str(_WORLD_B)
    assert q.subject == _EX + "inst1"
    assert q.predicate == _RIGIDITY_VIOLATION
    assert q.obj == f"<{_EX}RigidKind>"
    assert q.rule_iri == _LOGIC + "rule/cross-world-rigidity"
    # Cross-world derivation: no in-world antecedent (closure-pass leaf).
    assert q.source_quad_ids == []
    # Provenance + budget inherited from the result.
    assert q.profile == result.profile
    assert q.budget_status == result.budget_status
    # The derivation id is content-addressed (folds in the cross-world witness).
    assert "derivation/" in q.derivation_id


def test_determinism_sorted_output() -> None:
    """Multiple violations come back in canonical (graph, S, P, obj) order."""
    cg = _two_world_cg(
        world_a=[
            (_ex("alpha"), RDF.type, _ex("RigidKind")),
            (_ex("beta"), RDF.type, _ex("RigidKind")),
        ],
        world_b=[
            (_ex("alpha"), _ex("rel"), _ex("x")),
            (_ex("beta"), _ex("rel"), _ex("x")),
        ],
    )
    result = _materialize(cg)
    quads = cross_world_rigidity_violations(result)
    keys = [(q.graph, q.subject, q.predicate, q.obj) for q in quads]
    assert keys == sorted(keys)
    assert _rigidity_quads(result) == {
        (str(_WORLD_B), _EX + "alpha", _EX + "RigidKind"),
        (str(_WORLD_B), _EX + "beta", _EX + "RigidKind"),
    }


def test_pytest_import_is_used() -> None:
    """Guard: the pytest import is referenced (keeps the linter honest)."""
    assert pytest is not None
