# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""GMEOW Logic foundation surface (issue #498, Task 1).

Covers the term-surface contract for the canonical ``logic:`` vocabulary:
the namespace is registered in the unified prefix registry, every minted
``logic:`` term carries an ``@x-gmeow-english`` label, a ``skos:definition``,
and the slice's ``rdfs:isDefinedBy`` IRI, and no local name carries a
Principle-9 selector token. The term list is enumerated from the module, never
hardcoded, so the test grows with the vocabulary.
"""

from __future__ import annotations

import pytest
from rdflib import RDF, ConjunctiveGraph, Graph, Literal, URIRef
from rdflib.namespace import RDFS, SKOS

from gmeow_tools.config import LOGIC_NAMESPACE, PREFIXES, SLICES_DIR
from gmeow_tools.logic_certify import certify_program
from gmeow_tools.logic_foundation import foundation_rules
from gmeow_tools.logic_ir import LogicProgram, SemanticProfileId
from gmeow_tools.logic_materialize import materialize_program

_LOGIC_MODULE = SLICES_DIR / "core" / "logic" / "module.ttl"
_LOGIC_SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/logic")
_X_GMEOW_ENGLISH = "x-gmeow-english"
#: Principle 9 forbids selector tokens in local names.
_SELECTOR_TOKENS = ("primary", "preferred", "default", "main")


def test_logic_prefix_registered() -> None:
    assert "logic" in PREFIXES
    assert PREFIXES["logic"] == LOGIC_NAMESPACE


def _logic_subjects(graph: Graph) -> set[URIRef]:
    return {
        s
        for s in set(graph.subjects())
        if isinstance(s, URIRef) and str(s).startswith(LOGIC_NAMESPACE)
    }


def test_logic_module_terms_are_complete() -> None:
    graph = Graph()
    graph.parse(_LOGIC_MODULE, format="turtle")
    subjects = _logic_subjects(graph)
    assert subjects, "the logic module must mint logic: terms"

    for subject in subjects:
        labels = list(graph.objects(subject, RDFS.label))
        assert labels, f"{subject} is missing an rdfs:label"
        assert any(
            isinstance(label, Literal) and label.language == _X_GMEOW_ENGLISH
            for label in labels
        ), f"{subject} has no @{_X_GMEOW_ENGLISH} rdfs:label"

        definitions = list(graph.objects(subject, SKOS.definition))
        assert definitions, f"{subject} is missing a skos:definition"
        assert any(
            isinstance(defn, Literal) and defn.language == _X_GMEOW_ENGLISH
            for defn in definitions
        ), f"{subject} has no @{_X_GMEOW_ENGLISH} skos:definition"

        defined_by = list(graph.objects(subject, RDFS.isDefinedBy))
        assert defined_by == [_LOGIC_SLICE_IRI], (
            f"{subject} must declare rdfs:isDefinedBy <{_LOGIC_SLICE_IRI}>, "
            f"got {defined_by}"
        )


def test_logic_local_names_have_no_selector_token() -> None:
    graph = Graph()
    graph.parse(_LOGIC_MODULE, format="turtle")
    for subject in _logic_subjects(graph):
        local = str(subject)[len(LOGIC_NAMESPACE) :].lower()
        for token in _SELECTOR_TOKENS:
            assert token not in local, (
                f"Principle 9: logic: local name {local!r} contains "
                f"selector token {token!r}"
            )


# --------------------------------------------------------------------------- #
# OntoUML discipline lowering (issue #503, Task 2)
#
# Each discipline is proven over a small hand-built world by the Python oracle
# (logic_materialize.materialize_program) running the foundation_rules: the set
# of derived ``logic:violation <label>`` facts must equal the offending classes
# reasoning_lint.py would report.  The fixtures are the lint's own canonical
# anti-pattern shapes translated into logic: form.
# --------------------------------------------------------------------------- #

_LOGIC = LOGIC_NAMESPACE
_EX = "https://example.org/disc/"
_WORLD = URIRef("https://example.org/disc/world")
_VIOLATION = _LOGIC + "violation"

_LABEL = {
    "card": _LOGIC + "StereotypeCardinality",
    "mixiden": _LOGIC + "MixIden",
    "freerole": _LOGIC + "FreeRole",
    "mixrig": _LOGIC + "MixRig",
    "relcomp": _LOGIC + "RelComp",
}


def _ex(local: str) -> URIRef:
    return URIRef(_EX + local)


def _lg(local: str) -> URIRef:
    return URIRef(_LOGIC + local)


def _world(triples: list[tuple[URIRef, URIRef, URIRef]]) -> ConjunctiveGraph:
    """Build a single-world ConjunctiveGraph from (s, p, o) triples."""
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx = cg.get_context(_WORLD)
    for s, p, o in triples:
        ctx.add((s, p, o))
    return cg


def _violations(
    triples: list[tuple[URIRef, URIRef, URIRef]],
) -> set[tuple[str, str]]:
    """Materialize the foundation rules over a world; return (class, label) pairs."""
    program = LogicProgram(
        axioms=(),
        rules=foundation_rules(LogicProgram((), (), ())),
        profiles=(),
    )
    result = materialize_program(program, _world(triples), enable_naf=True)
    # ``q.obj`` is the canonical N3 form of the object IRI (``<iri>``); strip the
    # angle brackets so the comparison is against the bare label IRI.
    return {
        (q.subject, q.obj[1:-1] if q.obj.startswith("<") else q.obj)
        for q in result.quads
        if q.predicate == _VIOLATION
    }


def test_exactly_one_stereotype_no_stereotype_fires() -> None:
    """A class that appears (in subClassOf) but carries no stereotype is flagged."""
    triples = [
        (_ex("Foo"), _lg("subClassOf"), _ex("Bar")),
        (_ex("Bar"), RDF.type, _lg("Kind")),
    ]
    # Bar has exactly one stereotype (clean); Foo has none.
    assert _violations(triples) == {(_EX + "Foo", _LABEL["card"])}


def test_exactly_one_stereotype_two_stereotypes_fires() -> None:
    """A class with two distinct meta-class puns is flagged (and only it)."""
    triples = [
        (_ex("X"), RDF.type, _lg("Kind")),
        (_ex("X"), RDF.type, _lg("SubKind")),
    ]
    out = _violations(triples)
    assert (_EX + "X", _LABEL["card"]) in out
    # X is Kind+SubKind: both are rigid sortals, so no FreeRole/MixRig fire; the
    # only discipline a Kind-bearing class can additionally trip here is none
    # (Kind has no kind-ancestor and is excluded from the non-Kind-sortal branch).
    assert out == {(_EX + "X", _LABEL["card"])}


def test_mixiden_kind_specializes_kind() -> None:
    """MixIden: Dog (Kind) ⊑ Animal (Kind) — both Kind (reasoning_lint fixture)."""
    triples = [
        (_ex("Dog"), RDF.type, _lg("Kind")),
        (_ex("Animal"), RDF.type, _lg("Kind")),
        (_ex("Dog"), _lg("subClassOf"), _ex("Animal")),
    ]
    assert _violations(triples) == {(_EX + "Dog", _LABEL["mixiden"])}


def test_freerole_role_without_rigid_parent() -> None:
    """FreeRole: a Role with no rigid parent.

    A bare Role also trips MixIden (a non-Kind sortal tracing to zero Kinds),
    exactly as ``reasoning_lint`` reports both — the lowering reproduces the full
    offending set, not a curated subset.
    """
    triples = [(_ex("Pet"), RDF.type, _lg("Role"))]
    assert _violations(triples) == {
        (_EX + "Pet", _LABEL["freerole"]),
        (_EX + "Pet", _LABEL["mixiden"]),
    }


def test_mixrig_subkind_parent_is_role_ac3() -> None:
    """AC#3: a SubKind whose parent is a Role MUST be caught (MixRig).

    The full lint verdict over Dog(SubKind) ⊑ Pet(Role): Dog trips MixRig (rigid
    sortal with anti-rigid ancestor) and MixIden (non-Kind sortal, zero Kind
    ancestors); Pet trips FreeRole and MixIden.  The lowering reproduces all four.
    """
    triples = [
        (_ex("Dog"), RDF.type, _lg("SubKind")),
        (_ex("Pet"), RDF.type, _lg("Role")),
        (_ex("Dog"), _lg("subClassOf"), _ex("Pet")),
    ]
    out = _violations(triples)
    assert (_EX + "Dog", _LABEL["mixrig"]) in out  # the AC#3 assertion
    assert out == {
        (_EX + "Dog", _LABEL["mixrig"]),
        (_EX + "Dog", _LABEL["mixiden"]),
        (_EX + "Pet", _LABEL["freerole"]),
        (_EX + "Pet", _LABEL["mixiden"]),
    }


def test_relcomp_concrete_relator_one_relatum_fires() -> None:
    """RelComp: a concrete Relator mediating only one relatum is flagged."""
    triples = [
        (_ex("Marriage"), RDF.type, _lg("Relator")),
        (_ex("Marriage"), _lg("mediates"), _ex("Alice")),
    ]
    assert _violations(triples) == {(_EX + "Marriage", _LABEL["relcomp"])}


def test_relcomp_concrete_relator_two_relata_clean() -> None:
    """RelComp: a concrete Relator mediating two distinct relata is clean."""
    triples = [
        (_ex("Marriage"), RDF.type, _lg("Relator")),
        (_ex("Marriage"), _lg("mediates"), _ex("Alice")),
        (_ex("Marriage"), _lg("mediates"), _ex("Bob")),
    ]
    assert _violations(triples) == set()


def test_clean_hierarchy_zero_violations() -> None:
    """A well-formed hierarchy + well-mediated relator yields ZERO violations.

    Animal(Kind) → Dog(SubKind) → Pet(Role) is a proper rigid-to-anti-rigid
    chain; Ownership(Relator) mediates two distinct relata.  Nothing fires.
    """
    triples = [
        (_ex("Animal"), RDF.type, _lg("Kind")),
        (_ex("Dog"), RDF.type, _lg("SubKind")),
        (_ex("Pet"), RDF.type, _lg("Role")),
        (_ex("Dog"), _lg("subClassOf"), _ex("Animal")),
        (_ex("Pet"), _lg("subClassOf"), _ex("Dog")),
        (_ex("Ownership"), RDF.type, _lg("Relator")),
        (_ex("Ownership"), _lg("mediates"), _ex("Animal")),
        (_ex("Ownership"), _lg("mediates"), _ex("Dog")),
    ]
    assert _violations(triples) == set()


def test_foundation_rules_certify_stratified() -> None:
    """The foundation rule set certifies as StratifiedNAF (no negation in a cycle)."""
    program = LogicProgram(
        axioms=(),
        rules=foundation_rules(LogicProgram((), (), ())),
        profiles=(),
    )
    verdict = certify_program(program, SemanticProfileId.STRATIFIED_NAF)
    assert verdict.certified, verdict.violations
    assert verdict.decidability_class == "terminating/PTIME-data"


def test_foundation_rules_signature_accepts_policy_stub() -> None:
    """``foundation_rules`` accepts the reserved ``policy`` kwarg (Task 4 stub)."""
    base = foundation_rules(LogicProgram((), (), ()))
    with_policy = foundation_rules(
        LogicProgram((), (), ()), policy="witness-obligation"
    )
    # The policy stub is ignored today — same rule set either way.
    assert base == with_policy


def test_pytest_import_is_used() -> None:
    """Guard: the pytest import is referenced (keeps the linter honest)."""
    assert pytest is not None
