# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""NAF activation for stratifiable negation programs (issue #503 review, PR #605).

The v1/v2 oracle only evaluated ``logic:negatedBody`` as real negation-as-failure
under the ``foundation_lowering`` opt-in; every other case materialized with NAF
OFF, silently joining a negated atom *positively*.  The runner now activates NAF
for ANY stratifiable negation program (:func:`_program_has_stratifiable_negation`),
while non-stratifiable programs — and StableModel / WellFounded sets the stratified
oracle cannot compute — keep their lossy positive materialization (loss recorded).

Covered here:

1. The gate flips on for a stratifiable negation program, off for a positive-only
   program, and off for a non-stratifiable (negative-cycle) program.
2. End-to-end the gate produces NAF-correct materialization (a derivation gated by
   ``NOT`` fires only when the negated fact is ABSENT), and differs from the old
   positive treatment — the concrete behaviour the fix corrects.
"""

from __future__ import annotations

from rdflib import ConjunctiveGraph, Namespace, URIRef

from gmeow_tools.logic_ir import (
    ContextualScope,
    LogicAxiom,
    LogicProgram,
    LogicRule,
    SemanticProfileId,
)
from gmeow_tools.logic_materialize import materialize_program
from gmeow_tools.logic_runner import _program_has_stratifiable_negation

EX = Namespace("https://example.org/naf/")


def _married_single_program() -> LogicProgram:
    """The canonical two-stratum StratifiedNAF shape.

    * Stratum 0: ``?X married ?X :- ?X spouseOf ?Y`` (positive).
    * Stratum 1: ``?X single ?X :- ?X person ?X , NOT ?X married ?X`` (NAF over the
      lower stratum — the negation crosses a stratum boundary, so it stratifies).
    """
    married = LogicRule(
        head=LogicAxiom(subject="?X", predicate=str(EX.married), obj="?X"),
        body=(LogicAxiom(subject="?X", predicate=str(EX.spouseOf), obj="?Y"),),
        scope=ContextualScope(provenance=str(EX.ruleMarried)),
    )
    single = LogicRule(
        head=LogicAxiom(subject="?X", predicate=str(EX.single), obj="?X"),
        body=(
            LogicAxiom(subject="?X", predicate=str(EX.person), obj="?X"),
            LogicAxiom(subject="?X", predicate=str(EX.married), obj="?X", negated=True),
        ),
        scope=ContextualScope(provenance=str(EX.ruleSingle)),
    )
    return LogicProgram(axioms=(), rules=(married, single), profiles=())


def _world() -> ConjunctiveGraph:
    """alice is a spouse of bob; alice and bob are both persons."""
    cg = ConjunctiveGraph()
    ctx = cg.get_context(URIRef(str(EX.world)))
    ctx.add((URIRef(str(EX.alice)), URIRef(str(EX.spouseOf)), URIRef(str(EX.bob))))
    ctx.add((URIRef(str(EX.alice)), URIRef(str(EX.person)), URIRef(str(EX.alice))))
    ctx.add((URIRef(str(EX.bob)), URIRef(str(EX.person)), URIRef(str(EX.bob))))
    return cg


# --------------------------------------------------------------------------- #
# (1) The activation gate
# --------------------------------------------------------------------------- #


def test_gate_on_for_stratifiable_negation() -> None:
    """A program with stratum-crossing negation enables NAF."""
    assert _program_has_stratifiable_negation(_married_single_program()) is True


def test_gate_off_without_negation() -> None:
    """A positive-only program never enables NAF (byte-stable for every old case)."""
    positive = LogicProgram(
        axioms=(),
        rules=(
            LogicRule(
                head=LogicAxiom(subject="?X", predicate=str(EX.married), obj="?X"),
                body=(LogicAxiom(subject="?X", predicate=str(EX.spouseOf), obj="?Y"),),
            ),
        ),
        profiles=(),
    )
    assert _program_has_stratifiable_negation(positive) is False


def test_gate_off_for_non_stratifiable_negation() -> None:
    """A negation cycle (p :- NOT q ; q :- NOT p) is NOT stratifiable → NAF off.

    The stratified oracle must not be asked to chase a non-stratifiable set; it
    falls back to the lossy positive materialization with the loss recorded.
    """
    p_rule = LogicRule(
        head=LogicAxiom(subject="?X", predicate=str(EX.p), obj="?X"),
        body=(
            LogicAxiom(subject="?X", predicate=str(EX.q), obj="?X", negated=True),
            LogicAxiom(subject="?X", predicate=str(EX.thing), obj="?X"),
        ),
    )
    q_rule = LogicRule(
        head=LogicAxiom(subject="?X", predicate=str(EX.q), obj="?X"),
        body=(
            LogicAxiom(subject="?X", predicate=str(EX.p), obj="?X", negated=True),
            LogicAxiom(subject="?X", predicate=str(EX.thing), obj="?X"),
        ),
    )
    prog = LogicProgram(axioms=(), rules=(p_rule, q_rule), profiles=())
    assert _program_has_stratifiable_negation(prog) is False


# --------------------------------------------------------------------------- #
# (2) End-to-end NAF correctness vs the old positive treatment
# --------------------------------------------------------------------------- #


def _single_subjects(program: LogicProgram, *, enable_naf: bool) -> set[str]:
    result = materialize_program(
        program,
        _world(),
        profile=SemanticProfileId.POSITIVE_HORN,
        enable_naf=enable_naf,
    )
    return {q.subject for q in result.quads if q.predicate == str(EX.single)}


def test_naf_on_is_correct() -> None:
    """With NAF active: bob (unmarried) is single; alice (married) is NOT single."""
    singles = _single_subjects(_married_single_program(), enable_naf=True)
    assert singles == {str(EX.bob)}, (
        f"Stratified NAF: only the unmarried person is single; got {singles!r}"
    )


def test_naf_off_overderives_documenting_the_bug() -> None:
    """NAF OFF (the pre-fix path) joins ``NOT married`` positively → alice over-derived.

    This pins the exact defect the runner gate fixes: with NAF off the negated
    atom is treated as a positive ``?X married ?X`` join, so the married person is
    wrongly derived ``single`` and the unmarried person is not — the inverse of the
    sound result.
    """
    singles = _single_subjects(_married_single_program(), enable_naf=False)
    assert singles == {str(EX.alice)}, (
        "Positive treatment of the negated atom mis-derives single(alice); "
        f"got {singles!r}"
    )
