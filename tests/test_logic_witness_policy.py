# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Anti-rigidity witness policy (issue #503, Task 4).

Anti-rigidity (``Role``/``Phase``) formally requires, for each instantiation, a
world of existence in which the instance *lacks* the type (LOGIC-SEMANTICS.md
§Anti-rigidity needs a witness policy).  The per-case policy — declared in
``profile.json`` as ``"anti_rigidity_policy"`` — governs ONLY the instance-level
obligation facet, not any violation verdict:

* ``witness-obligation`` (DEFAULT) emits one ``logic:dischargeObligation`` per
  anti-rigid instantiation, in the typing world — an obligation, not a violation;
* ``schema-only`` emits NOTHING instance-level (the type-level FreeRole/MixIden
  verdicts from Task 2 are the whole story);
* ``witness-required`` emits ``logic:witnessRequiredViolation`` in the typing
  world UNLESS a materialized counter-world (the instance exists but is not typed
  the anti-rigid type) discharges it.

The construction of the counter-world itself is #505; this pass surfaces the
obligation only.  These are soundness tests over the Python oracle: each
hand-builds a multi-world EDB, materializes it, and asserts the exact set of
obligation/witness facts :func:`gmeow_tools.logic_foundation.anti_rigidity_obligations`
emits — and, critically, that the violation sets are policy-INVARIANT (P3).
"""

from __future__ import annotations

import pytest
from rdflib import RDF, ConjunctiveGraph, URIRef

from gmeow_tools.config import LOGIC_NAMESPACE
from gmeow_tools.logic_foundation import (
    anti_rigidity_obligations,
    cross_world_rigidity_violations,
    foundation_rules,
)
from gmeow_tools.logic_ir import LogicProgram
from gmeow_tools.logic_materialize import MaterializationResult, materialize_program

_LOGIC = LOGIC_NAMESPACE
_EX = "https://example.org/witness/"
_WORLD_A = URIRef("https://example.org/witness/worldA")
_WORLD_B = URIRef("https://example.org/witness/worldB")

_DISCHARGE_OBLIGATION = _LOGIC + "dischargeObligation"
_WITNESS_REQUIRED_VIOLATION = _LOGIC + "witnessRequiredViolation"
_VIOLATION = _LOGIC + "violation"
_RIGIDITY_VIOLATION = _LOGIC + "rigidityViolation"


def _ex(local: str) -> URIRef:
    return URIRef(_EX + local)


def _lg(local: str) -> URIRef:
    return URIRef(_LOGIC + local)


# Stereotype declarations make a *type* anti-rigid (schema is world-independent, so
# a single carrying world suffices).  ``SomeRole`` is anti-rigid; ``RigidKind`` is
# rigid (used to prove the rigidity set stays policy-invariant).
_STEREOTYPES: list[tuple[URIRef, URIRef, URIRef]] = [
    (_ex("SomeRole"), RDF.type, _lg("Role")),
    (_ex("RigidKind"), RDF.type, _lg("Kind")),
]


def _two_world_cg(
    world_a: list[tuple[URIRef, URIRef, URIRef]],
    world_b: list[tuple[URIRef, URIRef, URIRef]],
) -> ConjunctiveGraph:
    """Build a 2-world ConjunctiveGraph (worldA, worldB); seed stereotypes in A."""
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


def _materialize(
    cg: ConjunctiveGraph, *, with_rules: bool = False
) -> MaterializationResult:
    """Materialize a multi-world EDB.

    With ``with_rules=True`` the Task-2 foundation rules are injected (so the
    type-level ``logic:violation`` verdicts are present), mirroring how the runner
    augments the program under ``foundation_lowering``.
    """
    rules = foundation_rules(LogicProgram((), (), ())) if with_rules else ()
    program = LogicProgram(axioms=(), rules=rules, profiles=())
    return materialize_program(program, cg, enable_naf=True)


def _obligation_facts(
    result: MaterializationResult, policy: str
) -> set[tuple[str, str, str, str]]:
    """Return (world, instance, predicate, type) tuples the policy pass emits."""
    return {
        (
            q.graph,
            q.subject,
            q.predicate,
            q.obj[1:-1] if q.obj.startswith("<") else q.obj,
        )
        for q in anti_rigidity_obligations(result, policy)
    }


def _violation_set(result: MaterializationResult) -> set[tuple[str, str, str]]:
    """Return the COMBINED Task-2 + Task-3 violation set already in ``result``.

    This is the set the policy MUST NOT touch (P3): every ``logic:violation`` quad
    materialized by the foundation rules plus every ``logic:rigidityViolation`` quad
    the cross-world closure pass emits.  Returned as (graph, subject, type) tuples.
    """
    out: set[tuple[str, str, str]] = set()
    for q in result.quads:
        if q.predicate in (_VIOLATION, _RIGIDITY_VIOLATION):
            obj = q.obj[1:-1] if q.obj.startswith("<") else q.obj
            out.add((q.graph, q.subject, obj))
    for q in cross_world_rigidity_violations(result):
        obj = q.obj[1:-1] if q.obj.startswith("<") else q.obj
        out.add((q.graph, q.subject, obj))
    return out


# --------------------------------------------------------------------------- #
# witness-obligation (DEFAULT)
# --------------------------------------------------------------------------- #


def test_witness_obligation_emits_one_discharge_obligation() -> None:
    """An anti-rigid Role instantiation in A ⇒ exactly one dischargeObligation in A."""
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[],
    )
    result = _materialize(cg)
    assert _obligation_facts(result, "witness-obligation") == {
        (str(_WORLD_A), _EX + "x", _DISCHARGE_OBLIGATION, _EX + "SomeRole"),
    }


# --------------------------------------------------------------------------- #
# schema-only
# --------------------------------------------------------------------------- #


def test_schema_only_emits_nothing_instance_level() -> None:
    """Same EDB ⇒ ZERO obligation/witness facts under schema-only."""
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[],
    )
    result = _materialize(cg)
    assert anti_rigidity_obligations(result, "schema-only") == ()


def test_schema_only_preserves_type_level_verdicts() -> None:
    """schema-only does not disturb the Task-2 FreeRole/MixIden type-level verdicts.

    A bare Role type trips FreeRole + MixIden at the TYPE level (Task 2); those are
    materialized by the foundation rules independently of the policy pass, which adds
    nothing under schema-only.
    """
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[],
    )
    result = _materialize(cg, with_rules=True)
    type_level = {
        (q.subject, q.obj[1:-1] if q.obj.startswith("<") else q.obj)
        for q in result.quads
        if q.predicate == _VIOLATION
    }
    # SomeRole is a bare Role TYPE: FreeRole (no rigid ancestor) + MixIden (no Kind).
    assert (_EX + "SomeRole", _LOGIC + "FreeRole") in type_level
    assert (_EX + "SomeRole", _LOGIC + "MixIden") in type_level
    assert anti_rigidity_obligations(result, "schema-only") == ()


# --------------------------------------------------------------------------- #
# witness-required
# --------------------------------------------------------------------------- #


def test_witness_required_no_counter_world_fires() -> None:
    """Role-typed in A and B, no untyped world ⇒ witness-required fires in both.

    ``x`` is typed SomeRole in A and ALSO typed SomeRole in B — there is no world in
    which ``x`` exists but lacks the Role, so the strict obligation is undischarged.
    """
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[(_ex("x"), RDF.type, _ex("SomeRole"))],
    )
    result = _materialize(cg)
    assert _obligation_facts(result, "witness-required") == {
        (str(_WORLD_A), _EX + "x", _WITNESS_REQUIRED_VIOLATION, _EX + "SomeRole"),
        (str(_WORLD_B), _EX + "x", _WITNESS_REQUIRED_VIOLATION, _EX + "SomeRole"),
    }


def test_witness_required_single_world_fires() -> None:
    """A single typing world has no counter-world ⇒ witness-required fires."""
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx = cg.get_context(_WORLD_A)
    ctx.add((_ex("SomeRole"), RDF.type, _lg("Role")))
    ctx.add((_ex("x"), RDF.type, _ex("SomeRole")))
    result = _materialize(cg)
    assert _obligation_facts(result, "witness-required") == {
        (str(_WORLD_A), _EX + "x", _WITNESS_REQUIRED_VIOLATION, _EX + "SomeRole"),
    }


def test_witness_required_counter_world_discharges() -> None:
    """Role-typed in A, x exists in B untyped-by-SomeRole ⇒ NO witnessRequiredViolation.

    World B witnesses ``x`` existing (subject of a quad) but NOT typed SomeRole — the
    anti-rigidity witness — so the obligation is discharged in both worlds.
    """
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        # x exists in B (subject of a relation) but is NOT typed SomeRole there.
        world_b=[(_ex("x"), _ex("rel"), _ex("other"))],
    )
    result = _materialize(cg)
    assert _obligation_facts(result, "witness-required") == set()


def test_witness_required_per_world_discharge() -> None:
    """The discharge is per typing-world: A's typing is witnessed by B, B's by A.

    ``x`` is typed in A and typed in B; for A's obligation the counter-world is any
    world where x exists untyped — there is none here (both type it), so BOTH fire.
    Adding a third untyped world would discharge both.  This asserts the symmetric
    no-witness case stays symmetric.
    """
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[(_ex("x"), RDF.type, _ex("SomeRole"))],
    )
    result = _materialize(cg)
    facts = _obligation_facts(result, "witness-required")
    assert len(facts) == 2


# --------------------------------------------------------------------------- #
# Non-suppression invariant (P3) — the central guarantee
# --------------------------------------------------------------------------- #


def test_non_suppression_violation_set_is_policy_invariant() -> None:
    """P3: the violation set is byte-identical across all three policies.

    A fixture carrying BOTH a type-level violation (a bare Role TYPE ⇒ FreeRole +
    MixIden) AND a cross-world rigidity violation (a rigid instance that fails to
    persist) is materialized under all three policies.  The combined
    ``logic:violation`` + ``logic:rigidityViolation`` set MUST be identical across
    policies — only the dischargeObligation / witnessRequiredViolation facet differs.
    """
    cg = _two_world_cg(
        world_a=[
            # Anti-rigid instantiation (drives the policy facet).
            (_ex("x"), RDF.type, _ex("SomeRole")),
            # Rigid instance that will fail to persist into B (Task-3 violation).
            (_ex("k1"), RDF.type, _ex("RigidKind")),
        ],
        world_b=[
            # k1 exists in B (subject of a relation) but is NOT typed RigidKind there
            # ⇒ a cross-world rigidity violation in B.
            (_ex("k1"), _ex("rel"), _ex("other")),
        ],
    )
    # The materialized result (with the Task-2 rules) is policy-independent — the
    # policy is applied only by the post-pass — so a single materialization is the
    # shared baseline for the violation set.
    result = _materialize(cg, with_rules=True)
    base_violations = _violation_set(result)

    # Sanity: the fixture genuinely carries BOTH violation flavours.
    assert any(t == _LOGIC + "FreeRole" for (_g, _s, t) in base_violations)
    assert any(t == _LOGIC + "MixIden" for (_g, _s, t) in base_violations)
    assert (str(_WORLD_B), _EX + "k1", _EX + "RigidKind") in base_violations

    obligation_by_policy: dict[str, set[tuple[str, str, str, str]]] = {}
    for policy in ("witness-obligation", "schema-only", "witness-required"):
        # The violation set is computed from the SAME materialized result for every
        # policy (the policy pass never touches result.quads' violation facts), so it
        # is trivially invariant — assert it explicitly anyway as the contract.
        assert _violation_set(result) == base_violations
        obligation_by_policy[policy] = _obligation_facts(result, policy)

    # Only the obligation/witness facet differs across policies.
    assert obligation_by_policy["schema-only"] == set()
    assert obligation_by_policy["witness-obligation"] == {
        (str(_WORLD_A), _EX + "x", _DISCHARGE_OBLIGATION, _EX + "SomeRole"),
    }
    assert obligation_by_policy["witness-required"] == {
        (str(_WORLD_A), _EX + "x", _WITNESS_REQUIRED_VIOLATION, _EX + "SomeRole"),
    }
    # The three facets are mutually distinct ⇒ the policy genuinely varies the facet
    # while the violation set above stays fixed.
    assert (
        obligation_by_policy["witness-obligation"]
        != obligation_by_policy["witness-required"]
    )


# --------------------------------------------------------------------------- #
# Seam contract + determinism + hard-fail
# --------------------------------------------------------------------------- #


def test_emitted_obligation_quad_seam_contract_is_complete() -> None:
    """The emitted obligation quad carries the full seam contract, in the typing world.

    The witness-obligation facet is co-located with the typing fact in its world.
    """
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[],
    )
    result = _materialize(cg)
    quads = anti_rigidity_obligations(result, "witness-obligation")
    assert len(quads) == 1
    q = quads[0]
    assert q.graph == str(_WORLD_A)
    assert q.graph_component == str(_WORLD_A)
    assert q.subject == _EX + "x"
    assert q.predicate == _DISCHARGE_OBLIGATION
    assert q.obj == f"<{_EX}SomeRole>"
    assert q.rule_iri == _LOGIC + "rule/anti-rigidity-witness"
    # Closure-pass leaf: no in-world antecedent cited.
    assert q.source_quad_ids == []
    assert q.profile == result.profile
    assert q.budget_status == result.budget_status
    assert "derivation/" in q.derivation_id


def test_determinism_sorted_output() -> None:
    """Multiple obligations come back in canonical (graph, S, P, obj) order."""
    cg = _two_world_cg(
        world_a=[
            (_ex("alpha"), RDF.type, _ex("SomeRole")),
            (_ex("beta"), RDF.type, _ex("SomeRole")),
        ],
        world_b=[],
    )
    result = _materialize(cg)
    quads = anti_rigidity_obligations(result, "witness-obligation")
    keys = [(q.graph, q.subject, q.predicate, q.obj) for q in quads]
    assert keys == sorted(keys)
    assert _obligation_facts(result, "witness-obligation") == {
        (str(_WORLD_A), _EX + "alpha", _DISCHARGE_OBLIGATION, _EX + "SomeRole"),
        (str(_WORLD_A), _EX + "beta", _DISCHARGE_OBLIGATION, _EX + "SomeRole"),
    }


def test_no_anti_rigid_type_emits_nothing() -> None:
    """An EDB with no anti-rigid type ⇒ no obligation under any (emitting) policy."""
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx = cg.get_context(_WORLD_A)
    ctx.add((_ex("RigidKind"), RDF.type, _lg("Kind")))
    ctx.add((_ex("k1"), RDF.type, _ex("RigidKind")))
    result = _materialize(cg)
    assert anti_rigidity_obligations(result, "witness-obligation") == ()
    assert anti_rigidity_obligations(result, "witness-required") == ()
    assert anti_rigidity_obligations(result, "schema-only") == ()


def test_unknown_policy_raises() -> None:
    """An unknown policy string is a HARD FAILURE (closed enum, no silent default)."""
    cg = _two_world_cg(
        world_a=[(_ex("x"), RDF.type, _ex("SomeRole"))],
        world_b=[],
    )
    result = _materialize(cg)
    with pytest.raises(ValueError, match="Unknown anti_rigidity_policy"):
        anti_rigidity_obligations(result, "not-a-real-policy")
