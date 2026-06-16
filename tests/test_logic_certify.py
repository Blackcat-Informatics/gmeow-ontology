# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the static profile / decidability certifier (issue #502, Task 2).

Covers:
* A stratified rule set certifies under StratifiedNAF.
* A non-stratified set (recursion through a negated atom) is flagged with the
  predicate-cycle message.
* A DL-safety violation (head-only / under-negation variable) is detected.
* The StableModel advisory is always present unless the set is *also* stratified.
* The vacuous decidable-fragment checks (weak/joint acyclicity, guard, sticky)
  pass on a function-free program — asserted via a *certified verdict*, never a
  bare ``== []``, so a stub is never mistaken for an unexercised check (C3).
* The incompleteness statement is present in ``certify_program``'s docstring.
* ``CertificationVerdict.to_json()`` is deterministic and sorted-key.
* The PositiveHorn check forbids negation.

Fixtures are built directly from ``LogicProgram`` / ``LogicRule`` / ``LogicAxiom``
(the established logic-test idiom, e.g. tests/test_logic_materialize.py) so the
certifier is exercised without any frontend-parse dependency.
"""

from __future__ import annotations

import json

import pytest
from rdflib.namespace import RDF

from gmeow_tools.logic_certify import (
    CertificationVerdict,
    PredicateDepGraph,
    StratificationResult,
    certify_dl_safe,
    certify_guarded,
    certify_invariants,
    certify_joint_acyclicity,
    certify_positive_horn,
    certify_program,
    certify_stable_model,
    certify_sticky,
    certify_stratified_naf,
    certify_weak_acyclicity,
    stratify,
    tarjan_scc,
)
from gmeow_tools.logic_ir import (
    LogicAxiom,
    LogicProfile,
    LogicProgram,
    LogicRule,
    SemanticProfileId,
)

_RDF_TYPE = str(RDF.type)
_EX = "http://example.org/"
_P = _EX + "p"
_Q = _EX + "q"
_R = _EX + "r"


# --------------------------------------------------------------------------- #
# Fixture builders
# --------------------------------------------------------------------------- #


def _atom(
    pred: str, subj: str = "?x", obj: str = "?y", *, negated: bool = False
) -> LogicAxiom:
    return LogicAxiom(subject=subj, predicate=pred, obj=obj, negated=negated)


def _program(rules: tuple[LogicRule, ...], profile: SemanticProfileId) -> LogicProgram:
    return LogicProgram(
        axioms=(),
        rules=rules,
        profiles=(LogicProfile(profile_id=profile),),
    )


def _stratified_program() -> LogicProgram:
    """``r(x,y) :- p(x,y)`` and ``s(x) :- p(x,y), not q(x,y)`` — no negative cycle."""
    rule_pos = LogicRule(head=_atom(_R), body=(_atom(_P),))
    rule_neg = LogicRule(
        head=_atom(_EX + "s", subj="?x", obj="?x"),
        body=(_atom(_P), _atom(_Q, negated=True)),
    )
    return _program((rule_pos, rule_neg), SemanticProfileId.STRATIFIED_NAF)


def _non_stratified_program() -> LogicProgram:
    """``p(x,y) :- not q(x,y)`` and ``q(x,y) :- p(x,y)`` — cycle through negation."""
    rule_a = LogicRule(head=_atom(_P), body=(_atom(_Q, negated=True),))
    rule_b = LogicRule(head=_atom(_Q), body=(_atom(_P),))
    return _program((rule_a, rule_b), SemanticProfileId.STRATIFIED_NAF)


# --------------------------------------------------------------------------- #
# Tarjan / dependency graph
# --------------------------------------------------------------------------- #


def test_tarjan_finds_simple_cycle() -> None:
    sccs = tarjan_scc({"a": ["b"], "b": ["a"], "c": ["a"]})
    big = next(s for s in sccs if len(s) > 1)
    assert big == frozenset({"a", "b"})
    assert frozenset({"c"}) in sccs


def test_tarjan_is_deterministic() -> None:
    graph = {"a": ["b", "c"], "b": ["c"], "c": ["a"], "d": []}
    assert tarjan_scc(graph) == tarjan_scc(graph)


def test_dep_graph_records_negative_edge() -> None:
    graph = PredicateDepGraph.from_program(_non_stratified_program())
    labels = {label for (_h, _b, label) in graph.edges}
    assert "negative" in labels
    assert "positive" in labels


# --------------------------------------------------------------------------- #
# Stratification
# --------------------------------------------------------------------------- #


def test_stratified_set_certifies() -> None:
    program = _stratified_program()
    verdict = certify_program(program, SemanticProfileId.STRATIFIED_NAF)
    assert verdict.certified is True
    assert verdict.violations == ()
    assert verdict.decidability_class == "terminating/PTIME-data"


def test_stratify_result_type_and_strata() -> None:
    result = stratify(PredicateDepGraph.from_program(_stratified_program()))
    assert isinstance(result, StratificationResult)
    assert result.is_stratified is True
    assert result.offending_cycle is None
    assert result.strata  # non-empty partition


def test_non_stratified_set_is_flagged_with_cycle_message() -> None:
    program = _non_stratified_program()
    violations = certify_stratified_naf(program)
    assert len(violations) == 1
    msg = violations[0]
    assert "StratifiedNAFProfile violation" in msg
    assert "crosses a negated body atom" in msg
    assert "not stratifiable" in msg
    assert "LOGIC-SEMANTICS.md §Semantic profiles" in msg
    # The offending cycle is rendered deterministically as [P -> Q -> P].
    assert " -> " in msg
    assert certify_stratified_naf(program) == certify_stratified_naf(program)


def test_non_stratified_program_not_certified() -> None:
    verdict = certify_program(
        _non_stratified_program(), SemanticProfileId.STRATIFIED_NAF
    )
    assert verdict.certified is False
    assert any("not stratifiable" in v for v in verdict.violations)


# --------------------------------------------------------------------------- #
# PositiveHorn
# --------------------------------------------------------------------------- #


def test_positive_horn_rejects_negation() -> None:
    program = _program(
        (LogicRule(head=_atom(_P), body=(_atom(_Q, negated=True),)),),
        SemanticProfileId.POSITIVE_HORN,
    )
    violations = certify_positive_horn(program)
    assert len(violations) == 1
    assert "PositiveHornProfile violation" in violations[0]
    assert "no negation-as-failure" in violations[0]


def test_positive_horn_program_with_negation_not_certified() -> None:
    program = _program(
        (LogicRule(head=_atom(_P), body=(_atom(_Q, negated=True),)),),
        SemanticProfileId.POSITIVE_HORN,
    )
    verdict = certify_program(program, SemanticProfileId.POSITIVE_HORN)
    assert verdict.certified is False


def test_positive_program_certifies_under_positive_horn() -> None:
    rule = LogicRule(head=_atom(_R), body=(_atom(_P), _atom(_Q)))
    verdict = certify_program(
        _program((rule,), SemanticProfileId.POSITIVE_HORN),
        SemanticProfileId.POSITIVE_HORN,
    )
    assert verdict.certified is True


# --------------------------------------------------------------------------- #
# DL-safety
# --------------------------------------------------------------------------- #


def test_dl_safety_violation_detected() -> None:
    # ?z appears only in the head — unbound by any positive body atom.
    rule = LogicRule(
        head=_atom(_R, subj="?x", obj="?z"),
        body=(_atom(_P, subj="?x", obj="?y"),),
    )
    violations = certify_dl_safe(_program((rule,), SemanticProfileId.STRATIFIED_NAF))
    assert len(violations) == 1
    assert "DL-safety violation" in violations[0]
    assert "?z" in violations[0]
    assert "not DL-safe" in violations[0]


def test_dl_safety_under_negation_is_unsafe() -> None:
    # ?y appears only under negation, never in a positive body atom.
    rule = LogicRule(
        head=_atom(_R, subj="?x", obj="?x"),
        body=(
            _atom(_P, subj="?x", obj="?x"),
            _atom(_Q, subj="?x", obj="?y", negated=True),
        ),
    )
    violations = certify_dl_safe(_program((rule,), SemanticProfileId.STRATIFIED_NAF))
    assert any("?y" in v for v in violations)


def test_dl_safe_clean_rule_passes() -> None:
    rule = LogicRule(head=_atom(_R), body=(_atom(_P),))
    assert certify_dl_safe(_program((rule,), SemanticProfileId.STRATIFIED_NAF)) == []


# --------------------------------------------------------------------------- #
# StableModel advisory
# --------------------------------------------------------------------------- #


def test_stable_model_advisory_present_when_not_stratified() -> None:
    violations = certify_stable_model(_non_stratified_program())
    assert len(violations) == 1
    assert "StableModelProfile is NP-hard" in violations[0]
    assert "LOGIC-SEMANTICS.md §Decidability" in violations[0]


def test_stable_model_advisory_absent_when_stratified() -> None:
    # A stratified set is also stable=well-founded ⇒ no advisory.
    assert certify_stable_model(_stratified_program()) == []


def test_stable_model_verdict_carries_advisory() -> None:
    verdict = certify_program(_non_stratified_program(), SemanticProfileId.STABLE_MODEL)
    assert verdict.certified is False
    assert verdict.decidability_class == "NP-hard"
    assert any("NP-hard" in v for v in verdict.violations)


# --------------------------------------------------------------------------- #
# Vacuous decidable-fragment checks — assert CERTIFICATION, not bare == [] (C3)
# --------------------------------------------------------------------------- #


def test_function_free_program_certifies_weak_acyclicity() -> None:
    # A function-free program has no existential head variables, so weak
    # acyclicity holds; assert the certified verdict, not a bare empty list.
    program = _stratified_program()
    assert certify_weak_acyclicity(program) == []
    verdict = certify_program(program, SemanticProfileId.STRATIFIED_NAF)
    assert verdict.certified is True


def test_function_free_program_certifies_joint_acyclicity() -> None:
    program = _stratified_program()
    # Vacuity is the correct answer for the existential-free fragment.
    assert certify_joint_acyclicity(program) == []
    assert certify_program(program, SemanticProfileId.STRATIFIED_NAF).certified is True


def test_function_free_program_certifies_guarded_and_sticky() -> None:
    program = _stratified_program()
    assert certify_guarded(program) == []
    assert certify_sticky(program) == []
    # The whole StratifiedNAF check set (which runs guard + sticky) certifies.
    assert certify_invariants(program, SemanticProfileId.STRATIFIED_NAF) == []


# --------------------------------------------------------------------------- #
# Incompleteness statement + verdict serialization
# --------------------------------------------------------------------------- #


def test_certify_program_docstring_states_incompleteness() -> None:
    doc = certify_program.__doc__ or ""
    lowered = doc.lower()
    assert "sufficient" in lowered
    assert "incomplete" in lowered
    assert "undecidable" in lowered


def test_verdict_to_json_is_deterministic_and_sorted() -> None:
    verdict = CertificationVerdict(
        profile_id=SemanticProfileId.STRATIFIED_NAF,
        decidability_class="terminating/PTIME-data",
        certified=False,
        violations=("zeta", "alpha", "mu"),
    )
    out = verdict.to_json()
    # Keys are sorted; profile_id is the string value.
    assert list(out.keys()) == sorted(out.keys())
    assert out["profile_id"] == "StratifiedNAFProfile"
    # Violations are sorted regardless of construction order.
    assert out["violations"] == ["alpha", "mu", "zeta"]
    # JSON-serializable and stable across calls.
    assert json.dumps(out, sort_keys=True) == json.dumps(
        verdict.to_json(), sort_keys=True
    )


def test_verdict_to_json_from_real_certification() -> None:
    verdict = certify_program(_non_stratified_program(), SemanticProfileId.STABLE_MODEL)
    out = verdict.to_json()
    assert out["certified"] is False
    assert out["decidability_class"] == "NP-hard"
    assert out["profile_id"] == "StableModelProfile"
    assert isinstance(out["violations"], list)


# --------------------------------------------------------------------------- #
# Aggregator + class-level recursion visibility
# --------------------------------------------------------------------------- #


def test_certify_invariants_matches_verdict_violations() -> None:
    program = _non_stratified_program()
    flat = certify_invariants(program, SemanticProfileId.STRATIFIED_NAF)
    verdict = certify_program(program, SemanticProfileId.STRATIFIED_NAF)
    assert flat == list(verdict.violations)


# --------------------------------------------------------------------------- #
# Type-guard negative-path tests (issue #502, Gap 3)
# --------------------------------------------------------------------------- #


def test_certify_program_rejects_non_logic_program() -> None:
    """certify_program raises TypeError when the first arg is not a LogicProgram."""
    with pytest.raises(TypeError, match="program"):
        certify_program("not a program", SemanticProfileId.STRATIFIED_NAF)  # type: ignore[arg-type]


def test_certify_program_rejects_bad_profile() -> None:
    """certify_program raises TypeError for a non-SemanticProfileId declared_profile."""
    program = _stratified_program()
    with pytest.raises(TypeError, match="declared_profile"):
        certify_program(program, "PositiveHorn")  # type: ignore[arg-type]


def test_certify_program_rejects_none_profile() -> None:
    """certify_program raises TypeError when declared_profile is None."""
    program = _stratified_program()
    with pytest.raises(TypeError, match="declared_profile"):
        certify_program(program, None)  # type: ignore[arg-type]


def test_rdf_type_class_level_self_cycle_through_negation_flagged() -> None:
    # ?x rdf:type C :- ?x rdf:type C (negated)  — a class-level negative self-loop.
    cls = _EX + "C"
    rule = LogicRule(
        head=LogicAxiom(subject="?x", predicate=_RDF_TYPE, obj=cls),
        body=(LogicAxiom(subject="?x", predicate=_RDF_TYPE, obj=cls, negated=True),),
    )
    program = _program((rule,), SemanticProfileId.STRATIFIED_NAF)
    violations = certify_stratified_naf(program)
    assert len(violations) == 1
    assert "crosses a negated body atom" in violations[0]
