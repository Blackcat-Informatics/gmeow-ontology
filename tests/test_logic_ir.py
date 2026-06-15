# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the typed IR layer (issue #500, Task 1 — logic_ir.py).

Covers:
* Enum values match the local names in ``slices/core/logic/module.ttl`` verbatim.
* Frozen dataclass construction and equality.
* ``LogicAxiom`` / ``LogicRule`` / ``LogicProfile`` validation (post_init).
* ``LogicProgram`` order-independence: two programs with the same content but
  different insertion order compare equal and produce identical canonical dicts.
* ``ContextualScope`` confidence range validation.
* ``ComplexityClass`` empty-label guard.
* ``LogicRule`` body canonicalization.
"""

from __future__ import annotations

import pytest

from gmeow_tools.logic_ir import (
    ComplexityClass,
    ContextualScope,
    LogicAxiom,
    LogicModality,
    LogicProfile,
    LogicProgram,
    LogicRule,
    PreservationKind,
    SemanticProfileId,
)

LOGIC = "https://blackcatinformatics.ca/logic/"


# --------------------------------------------------------------------------- #
# Enum surface checks (local names match module.ttl verbatim)
# --------------------------------------------------------------------------- #


def test_semantic_profile_ids_match_module_ttl() -> None:
    """SemanticProfileId values must match the local names in module.ttl."""
    expected = {
        "PositiveHornProfile",
        "StratifiedNAFProfile",
        "WellFoundedProfile",
        "StableModelProfile",
        "ProceduralPrologProfile",
        "ProbabilisticProfile",
    }
    assert {p.value for p in SemanticProfileId} == expected


def test_preservation_kind_values_match_module_ttl() -> None:
    """PreservationKind values must match the local names in module.ttl."""
    expected = {
        "ExactPreservation",
        "SoundUnderApproximation",
        "CompleteOverApproximation",
        "ValidationOnly",
        "InconsistencyPreserving",
        "InconsistencyReflecting",
    }
    assert {k.value for k in PreservationKind} == expected


def test_logic_modality_has_none_sentinel() -> None:
    assert LogicModality.NONE.value == "none"
    assert len(list(LogicModality)) == 8  # NONE + 7 world types


# --------------------------------------------------------------------------- #
# ComplexityClass
# --------------------------------------------------------------------------- #


def test_complexity_class_round_trips() -> None:
    cc = ComplexityClass("PTIME")
    assert str(cc) == "PTIME"
    assert cc.label == "PTIME"


def test_complexity_class_rejects_empty_label() -> None:
    with pytest.raises(ValueError, match="non-empty"):
        ComplexityClass("")


def test_complexity_class_rejects_whitespace_only() -> None:
    with pytest.raises(ValueError, match="non-empty"):
        ComplexityClass("   ")


# --------------------------------------------------------------------------- #
# ContextualScope validation
# --------------------------------------------------------------------------- #


def test_contextual_scope_defaults_are_none() -> None:
    scope = ContextualScope()
    assert scope.standpoint is None
    assert scope.time is None
    assert scope.confidence is None
    assert scope.modality is LogicModality.NONE
    assert scope.provenance is None


def test_contextual_scope_confidence_valid_bounds() -> None:
    assert ContextualScope(confidence=0.0).confidence == 0.0
    assert ContextualScope(confidence=1.0).confidence == 1.0
    assert ContextualScope(confidence=0.5).confidence == 0.5


@pytest.mark.parametrize("bad", [-0.1, 1.01, 2.0, -1.0])
def test_contextual_scope_confidence_out_of_range(bad: float) -> None:
    with pytest.raises(ValueError, match="confidence"):
        ContextualScope(confidence=bad)


# --------------------------------------------------------------------------- #
# LogicAxiom
# --------------------------------------------------------------------------- #


def _axiom(
    subj: str = "ex:s", pred: str = LOGIC + "Kind", obj: str = "ex:o"
) -> LogicAxiom:
    return LogicAxiom(subject=subj, predicate=pred, obj=obj)


def test_logic_axiom_frozen() -> None:
    from dataclasses import FrozenInstanceError

    a = _axiom()
    with pytest.raises(FrozenInstanceError):
        a.subject = "new"  # type: ignore[misc]


def test_logic_axiom_equality() -> None:
    a1 = LogicAxiom(subject="ex:s", predicate=LOGIC + "Kind", obj="ex:o")
    a2 = LogicAxiom(subject="ex:s", predicate=LOGIC + "Kind", obj="ex:o")
    assert a1 == a2


def test_logic_axiom_hashable() -> None:
    a = _axiom()
    s = {a, a}
    assert len(s) == 1


def test_logic_axiom_rejects_empty_subject() -> None:
    with pytest.raises(ValueError, match="subject"):
        LogicAxiom(subject="", predicate=LOGIC + "Kind", obj="ex:o")


def test_logic_axiom_rejects_empty_predicate() -> None:
    with pytest.raises(ValueError, match="predicate"):
        LogicAxiom(subject="ex:s", predicate="", obj="ex:o")


def test_logic_axiom_literal_flag() -> None:
    a = LogicAxiom(
        subject="ex:s",
        predicate=LOGIC + "confidence",
        obj="0.9",
        obj_is_literal=True,
    )
    assert a.obj_is_literal is True


# --------------------------------------------------------------------------- #
# LogicRule body canonicalization
# --------------------------------------------------------------------------- #


def test_logic_rule_body_is_canonicalized() -> None:
    """Rule body is sorted regardless of construction order."""
    a1 = LogicAxiom(subject="ex:a", predicate=LOGIC + "rigidlyAppliesTo", obj="ex:b")
    a2 = LogicAxiom(subject="ex:c", predicate=LOGIC + "mediates", obj="ex:d")
    head = LogicAxiom(subject="ex:s", predicate=LOGIC + "Kind", obj="ex:o")

    rule_ab = LogicRule(head=head, body=(a1, a2))
    rule_ba = LogicRule(head=head, body=(a2, a1))

    assert rule_ab == rule_ba
    assert rule_ab.body == rule_ba.body  # same canonical order


def test_logic_rule_frozen() -> None:
    from dataclasses import FrozenInstanceError

    head = _axiom()
    rule = LogicRule(head=head, body=())
    with pytest.raises(FrozenInstanceError):
        rule.head = _axiom("ex:other")  # type: ignore[misc]


# --------------------------------------------------------------------------- #
# LogicProfile
# --------------------------------------------------------------------------- #


def test_logic_profile_with_complexity() -> None:
    p = LogicProfile(
        profile_id=SemanticProfileId.POSITIVE_HORN,
        complexity=ComplexityClass("PTIME"),
    )
    assert p.profile_id is SemanticProfileId.POSITIVE_HORN
    assert p.complexity is not None
    assert str(p.complexity) == "PTIME"


def test_logic_profile_without_complexity() -> None:
    p = LogicProfile(profile_id=SemanticProfileId.STABLE_MODEL)
    assert p.complexity is None


# --------------------------------------------------------------------------- #
# LogicProgram order-independence (the core canonicalization contract)
# --------------------------------------------------------------------------- #


def _make_program(
    axiom_order: list[LogicAxiom], profile_order: list[LogicProfile]
) -> LogicProgram:
    return LogicProgram(
        axioms=tuple(axiom_order),
        rules=(),
        profiles=tuple(profile_order),
    )


def test_logic_program_order_independence_equality() -> None:
    """Two programs with the same content in different order must be equal."""
    a1 = LogicAxiom(subject="ex:x", predicate=LOGIC + "Kind", obj="ex:o")
    a2 = LogicAxiom(subject="ex:y", predicate=LOGIC + "Role", obj="ex:o")
    p1 = LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN)
    p2 = LogicProfile(profile_id=SemanticProfileId.STRATIFIED_NAF)

    prog_ab = _make_program([a1, a2], [p1, p2])
    prog_ba = _make_program([a2, a1], [p2, p1])

    assert prog_ab == prog_ba


def test_logic_program_canonical_is_stable() -> None:
    """canonical() produces the same dict regardless of construction order."""
    a1 = LogicAxiom(subject="ex:x", predicate=LOGIC + "Kind", obj="ex:o")
    a2 = LogicAxiom(subject="ex:y", predicate=LOGIC + "Phase", obj="ex:o")

    prog1 = LogicProgram(axioms=(a1, a2), rules=(), profiles=())
    prog2 = LogicProgram(axioms=(a2, a1), rules=(), profiles=())

    assert prog1.canonical() == prog2.canonical()


def test_logic_program_canonical_contains_expected_keys() -> None:
    prog = LogicProgram(
        axioms=(), rules=(), profiles=(), source_iri="https://example.org/prog"
    )
    canon = prog.canonical()
    assert set(canon.keys()) == {"axioms", "rules", "profiles", "source_iri"}
    assert canon["source_iri"] == "https://example.org/prog"
    assert canon["axioms"] == []
    assert canon["rules"] == []
    assert canon["profiles"] == []


def test_logic_program_canonical_round_trips_scope() -> None:
    """Scope metadata survives canonical() serialisation."""
    scope = ContextualScope(
        standpoint="https://example.org/sp",
        confidence=0.8,
        modality=LogicModality.EPISTEMIC,
        provenance="https://example.org/agent",
    )
    axiom = LogicAxiom(
        subject="ex:s",
        predicate=LOGIC + "rigidlyAppliesTo",
        obj="ex:o",
        scope=scope,
    )
    prog = LogicProgram(axioms=(axiom,), rules=(), profiles=())
    canon = prog.canonical()
    ax_canon = canon["axioms"][0]
    assert ax_canon["scope"]["confidence"] == 0.8
    assert ax_canon["scope"]["modality"] == "epistemic"
    assert ax_canon["scope"]["standpoint"] == "https://example.org/sp"
    assert ax_canon["scope"]["provenance"] == "https://example.org/agent"


def test_logic_program_with_rules_order_independence() -> None:
    """Rules are also canonically ordered."""
    head = LogicAxiom(subject="ex:s", predicate=LOGIC + "Kind", obj="ex:o")
    body1 = LogicAxiom(subject="ex:a", predicate=LOGIC + "rigidlyAppliesTo", obj="ex:b")
    body2 = LogicAxiom(subject="ex:c", predicate=LOGIC + "mediates", obj="ex:d")
    rule1 = LogicRule(head=head, body=(body1,))
    rule2 = LogicRule(head=head, body=(body2,))

    prog_12 = LogicProgram(axioms=(), rules=(rule1, rule2), profiles=())
    prog_21 = LogicProgram(axioms=(), rules=(rule2, rule1), profiles=())

    assert prog_12 == prog_21
    assert prog_12.canonical() == prog_21.canonical()


def test_logic_program_frozen() -> None:
    from dataclasses import FrozenInstanceError

    prog = LogicProgram(axioms=(), rules=(), profiles=())
    with pytest.raises(FrozenInstanceError):
        prog.axioms = ()  # type: ignore[misc]
