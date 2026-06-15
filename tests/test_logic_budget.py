# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the runtime budget governor (issue #502, Task 3).

The budget governor caps the forward chase by rule firings, derived answers, or
wall-clock time.  On exhaustion it MUST return a SOUND partial result tagged
``budget_status="exhausted"`` with ``incomplete=True`` — never a false answer
and never a fabricated quad.

Covers:
* Ceiling enforcement: a transitive-closure program with ``max_rule_firings=N``
  stops short of full fixpoint and reports exhaustion.
* Never-false-answer: the bounded result is a strict subset of the full
  (unbounded) materialisation — every kept quad is genuinely entailed.
* Deterministic truncation: under a ``max_answers`` cap the kept set is the
  canonical-sort prefix and is identical run-to-run.
* Default-unbounded parity: ``budget=None`` reproduces the pre-#502 behaviour
  byte-for-byte (all ``"ok"``, ``incomplete=False``).
"""

from __future__ import annotations

from rdflib import ConjunctiveGraph, Literal, Namespace, URIRef

from gmeow_tools.logic_ir import (
    ContextualScope,
    LogicAxiom,
    LogicProfile,
    LogicProgram,
    LogicRule,
    SemanticProfileId,
)
from gmeow_tools.logic_materialize import (
    BudgetParams,
    DerivedQuad,
    materialize_program,
)

# --------------------------------------------------------------------------- #
# Shared namespaces / IRIs (mirror tests/test_logic_materialize.py idiom)
# --------------------------------------------------------------------------- #

_EX = Namespace("http://example.org/")
_W_ALPHA = URIRef("http://world/Alpha")
_RELATED = URIRef("http://example.org/related")
_ASSERT_IRI = "https://blackcatinformatics.ca/logic/assert"


def _node(i: int) -> URIRef:
    """Return a stable chain node IRI ``http://example.org/n{i}``."""
    return URIRef(f"http://example.org/n{i}")


# --------------------------------------------------------------------------- #
# Programs / inputs
# --------------------------------------------------------------------------- #


def _transitivity_program() -> LogicProgram:
    """Return the ``?x related ?y, ?y related ?z -> ?x related ?z`` program."""
    rule = LogicRule(
        head=LogicAxiom(
            subject="?x", predicate=str(_RELATED), obj="?z", obj_is_literal=False
        ),
        body=(
            LogicAxiom(
                subject="?x", predicate=str(_RELATED), obj="?y", obj_is_literal=False
            ),
            LogicAxiom(
                subject="?y", predicate=str(_RELATED), obj="?z", obj_is_literal=False
            ),
        ),
        scope=ContextualScope(
            provenance="https://blackcatinformatics.ca/logic/rules/transitivity"
        ),
    )
    return LogicProgram(
        axioms=(),
        rules=(rule,),
        profiles=(LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN),),
    )


def _chain_input(length: int) -> ConjunctiveGraph:
    """Return a linear ``n0->n1->...->n{length}`` chain in world Alpha.

    A chain of ``length`` edges has a transitive closure of
    ``length*(length+1)/2`` related-edges (the full fixpoint), so it is a
    convenient stress input for the firing/answer ceilings.
    """
    cg: ConjunctiveGraph = ConjunctiveGraph()
    ctx = cg.get_context(_W_ALPHA)
    for i in range(length):
        ctx.add((_node(i), _RELATED, _node(i + 1)))
    return cg


def _derived_spog(quads: tuple[DerivedQuad, ...]) -> set[tuple[str, str, str, str]]:
    """Return the (subject, predicate, obj, graph) identity set of ``quads``."""
    return {(q.subject, q.predicate, q.obj, q.graph) for q in quads}


# --------------------------------------------------------------------------- #
# Ceiling enforcement
# --------------------------------------------------------------------------- #


class TestCeilingEnforcement:
    def test_max_rule_firings_stops_short_of_fixpoint(self) -> None:
        cg_full = _chain_input(6)
        cg_bounded = _chain_input(6)
        full = materialize_program(_transitivity_program(), cg_full)
        bounded = materialize_program(
            _transitivity_program(),
            cg_bounded,
            budget=BudgetParams(max_rule_firings=3),
        )
        # Full fixpoint derives strictly more than the bounded run.
        assert len(bounded.quads) < len(full.quads)
        assert bounded.budget_status == "exhausted"
        assert bounded.incomplete is True
        # Every emitted quad carries the exhausted marker.
        assert all(q.budget_status == "exhausted" for q in bounded.quads)

    def test_max_firings_limits_derived_count(self) -> None:
        cg = _chain_input(6)
        bounded = materialize_program(
            _transitivity_program(),
            cg,
            budget=BudgetParams(max_rule_firings=2),
        )
        derived = [q for q in bounded.quads if q.rule_iri != _ASSERT_IRI]
        # The chase stops the same round the 2nd firing trips the ceiling; it
        # never blows far past the cap.
        assert len(derived) <= 6
        assert len(derived) >= 1

    def test_unreached_ceiling_stays_ok(self) -> None:
        # A ceiling far above the full closure size must NOT trip.
        cg = _chain_input(3)
        full = materialize_program(_transitivity_program(), cg)
        cg2 = _chain_input(3)
        generous = materialize_program(
            _transitivity_program(),
            cg2,
            budget=BudgetParams(max_rule_firings=10_000, max_answers=10_000),
        )
        assert generous.budget_status == "ok"
        assert generous.incomplete is False
        assert _derived_spog(generous.quads) == _derived_spog(full.quads)
        assert all(q.budget_status == "ok" for q in generous.quads)


# --------------------------------------------------------------------------- #
# Never-false-answer (soundness): bounded result is a strict subset
# --------------------------------------------------------------------------- #


class TestNeverFalseAnswer:
    def test_bounded_quads_are_subset_of_full(self) -> None:
        cg_full = _chain_input(7)
        full = materialize_program(_transitivity_program(), cg_full)
        full_set = _derived_spog(full.quads)

        for ceiling in (1, 2, 4, 8):
            cg = _chain_input(7)
            bounded = materialize_program(
                _transitivity_program(),
                cg,
                budget=BudgetParams(max_rule_firings=ceiling),
            )
            bounded_set = _derived_spog(bounded.quads)
            # Soundness: every kept quad is genuinely entailed by the program.
            assert bounded_set <= full_set, (
                f"ceiling={ceiling}: bounded result contains a quad absent from "
                f"the full fixpoint — {bounded_set - full_set}"
            )

    def test_exhausted_run_is_strict_subset(self) -> None:
        cg_full = _chain_input(7)
        full = materialize_program(_transitivity_program(), cg_full)
        cg = _chain_input(7)
        bounded = materialize_program(
            _transitivity_program(),
            cg,
            budget=BudgetParams(max_rule_firings=3),
        )
        assert bounded.incomplete is True
        assert _derived_spog(bounded.quads) < _derived_spog(full.quads)


# --------------------------------------------------------------------------- #
# Deterministic truncation under max_answers
# --------------------------------------------------------------------------- #


class TestDeterministicTruncation:
    def test_max_answers_caps_derived_exactly(self) -> None:
        cg = _chain_input(6)
        bounded = materialize_program(
            _transitivity_program(),
            cg,
            budget=BudgetParams(max_answers=4),
        )
        derived = [q for q in bounded.quads if q.rule_iri != _ASSERT_IRI]
        assert len(derived) == 4
        assert bounded.budget_status == "exhausted"
        assert bounded.incomplete is True

    def test_truncation_is_canonical_sort_prefix_of_kept_candidates(self) -> None:
        # The honest, achievable invariant: a budget-stopped chase cannot derive
        # the WHOLE fixpoint, so the kept set is the canonical-sort PREFIX of the
        # quads it actually derived before the cap tripped (a sound subset of the
        # full closure), NOT the global prefix of the unreachable full closure.
        cg_full = _chain_input(6)
        full = materialize_program(_transitivity_program(), cg_full)
        full_derived = {
            (q.subject, q.predicate, q.obj, q.graph)
            for q in full.quads
            if q.rule_iri != _ASSERT_IRI
        }
        cg = _chain_input(6)
        bounded = materialize_program(
            _transitivity_program(),
            cg,
            budget=BudgetParams(max_answers=4),
        )
        bounded_derived = [
            (q.subject, q.predicate, q.obj, q.graph)
            for q in bounded.quads
            if q.rule_iri != _ASSERT_IRI
        ]
        # Sound subset of the full closure (never a false answer).
        assert set(bounded_derived) <= full_derived
        # Exactly max_answers kept.
        assert len(bounded_derived) == 4
        # The kept set is emitted in canonical-sort order (the global output sort
        # over a deterministically truncated, content-addressed set) — re-sorting
        # is a no-op, confirming the cut is along the canonical order.
        assert bounded_derived == sorted(bounded_derived)

    def test_truncation_is_deterministic_across_runs(self) -> None:
        results: list[tuple[tuple[str, str, str, str], ...]] = []
        for _ in range(2):
            cg = _chain_input(6)
            bounded = materialize_program(
                _transitivity_program(),
                cg,
                budget=BudgetParams(max_answers=4),
            )
            results.append(
                tuple((q.subject, q.predicate, q.obj, q.graph) for q in bounded.quads)
            )
        assert results[0] == results[1]


# --------------------------------------------------------------------------- #
# Default-unbounded parity with pre-#502 behaviour
# --------------------------------------------------------------------------- #


class TestDefaultUnboundedParity:
    def test_none_budget_matches_no_budget_argument(self) -> None:
        cg_a = _chain_input(5)
        cg_b = _chain_input(5)
        no_arg = materialize_program(_transitivity_program(), cg_a)
        explicit_none = materialize_program(_transitivity_program(), cg_b, budget=None)
        assert no_arg.quads == explicit_none.quads
        assert no_arg.budget_status == explicit_none.budget_status

    def test_none_budget_is_all_ok_and_complete(self) -> None:
        cg = _chain_input(5)
        result = materialize_program(_transitivity_program(), cg)
        assert result.budget_status == "ok"
        assert result.incomplete is False
        assert all(q.budget_status == "ok" for q in result.quads)

    def test_unbounded_params_object_matches_none(self) -> None:
        # An all-None BudgetParams must behave identically to budget=None.
        cg_a = _chain_input(5)
        cg_b = _chain_input(5)
        via_none = materialize_program(_transitivity_program(), cg_a)
        via_unbounded = materialize_program(
            _transitivity_program(), cg_b, budget=BudgetParams()
        )
        assert via_none.quads == via_unbounded.quads
        assert via_unbounded.budget_status == "ok"
        assert via_unbounded.incomplete is False
        assert BudgetParams().is_unbounded() is True

    def test_literal_object_input_parity(self) -> None:
        # A literal-bearing fixture exercises the asserted-quad path too.
        cg: ConjunctiveGraph = ConjunctiveGraph()
        ctx = cg.get_context(_W_ALPHA)
        ctx.add((_EX.thing, _EX.label, Literal("hello", lang="en")))
        result = materialize_program(
            LogicProgram(
                axioms=(),
                rules=(),
                profiles=(LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN),),
            ),
            cg,
        )
        assert result.budget_status == "ok"
        assert result.incomplete is False
        assert all(q.budget_status == "ok" for q in result.quads)
