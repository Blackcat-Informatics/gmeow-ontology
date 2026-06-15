# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the explanation skeleton emitter (issue #501, Task 6).

Covers:
* Transitive subClassOf derivation (Dog ⊑ Animal derived from Dog ⊑ Mammal,
  Mammal ⊑ Animal): explain() produces a skeleton whose cited IRIs are ALL in
  the proof trace; assert_explanation_faithful() passes.
* Hallucinated citation: inject an IRI not in the trace →
  assert_explanation_faithful() raises FaithfulnessError.
* Determinism: same input → identical explanation skeleton (cited_iris and
  step_skeleton are byte-for-byte identical across two runs).
* Asserted quad: explain() works for input (asserted) facts, not just derived.
* Markdown output: as_markdown() contains the cited-IRI skeleton header block
  and the step-skeleton header block.
* with_onto_graph: prose lines are populated when an ontology graph with
  rdfs:label and skos:definition is supplied.
"""

from __future__ import annotations

import pytest
from rdflib import ConjunctiveGraph, Graph, Literal, Namespace, URIRef
from rdflib.namespace import RDFS, SKOS

from gmeow_tools.logic_explain import (
    ExplainError,
    Explanation,
    FaithfulnessError,
    assert_explanation_faithful,
    explain,
)
from gmeow_tools.logic_ir import (
    ContextualScope,
    LogicAxiom,
    LogicProfile,
    LogicProgram,
    LogicRule,
    SemanticProfileId,
)
from gmeow_tools.logic_materialize import (
    DerivedQuad,
    MaterializationResult,
    materialize_program,
)

# --------------------------------------------------------------------------- #
# Shared namespaces / IRIs
# --------------------------------------------------------------------------- #

_EX = Namespace("http://example.org/")
_W = URIRef("http://world/Test")

# Individuals for subClassOf transitivity test
_DOG = URIRef("http://example.org/Dog")
_MAMMAL = URIRef("http://example.org/Mammal")
_ANIMAL = URIRef("http://example.org/Animal")

_SUB_CLASS_OF = URIRef("http://www.w3.org/2000/01/rdf-schema#subClassOf")

_TRANSITIVITY_RULE_IRI = (
    "https://blackcatinformatics.ca/logic/rules/subClassOfTransitivity"
)


# --------------------------------------------------------------------------- #
# Fixture helpers
# --------------------------------------------------------------------------- #


def _cg_with_quads(
    quads: list[tuple[URIRef | Literal, URIRef, URIRef | Literal, URIRef]],
) -> ConjunctiveGraph:
    cg: ConjunctiveGraph = ConjunctiveGraph()
    for s, p, o, g in quads:
        named_graph = cg.get_context(g)
        named_graph.add((s, p, o))
    return cg


def _sub_class_of_transitivity_program() -> LogicProgram:
    """rdfs:subClassOf transitivity rule: ?x ⊑ ?y, ?y ⊑ ?z → ?x ⊑ ?z."""
    rule = LogicRule(
        head=LogicAxiom(
            subject="?x",
            predicate=str(_SUB_CLASS_OF),
            obj="?z",
            obj_is_literal=False,
        ),
        body=(
            LogicAxiom(
                subject="?x",
                predicate=str(_SUB_CLASS_OF),
                obj="?y",
                obj_is_literal=False,
            ),
            LogicAxiom(
                subject="?y",
                predicate=str(_SUB_CLASS_OF),
                obj="?z",
                obj_is_literal=False,
            ),
        ),
        scope=ContextualScope(provenance=_TRANSITIVITY_RULE_IRI),
    )
    return LogicProgram(
        axioms=(),
        rules=(rule,),
        profiles=(LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN),),
    )


def _make_dog_mammal_animal_result() -> MaterializationResult:
    """Materialize: Dog ⊑ Mammal, Mammal ⊑ Animal → Dog ⊑ Animal."""
    cg = _cg_with_quads(
        [
            (_DOG, _SUB_CLASS_OF, _MAMMAL, _W),
            (_MAMMAL, _SUB_CLASS_OF, _ANIMAL, _W),
        ]
    )
    return materialize_program(_sub_class_of_transitivity_program(), cg)


def _find_derived_quad(
    result: MaterializationResult,
    subject: str,
    obj_n3: str,
) -> DerivedQuad:
    """Find a specific derived quad in the result by subject and object N3."""
    for dq in result.quads:
        if dq.subject == subject and dq.obj == obj_n3:
            return dq
    raise KeyError(f"Derived quad not found: subject={subject!r}, obj={obj_n3!r}")


# --------------------------------------------------------------------------- #
# Tests: basic derivation explain()
# --------------------------------------------------------------------------- #


class TestExplainTransitivity:
    def test_derived_quad_present_in_result(self) -> None:
        """Sanity: the materializer does produce Dog ⊑ Animal."""
        result = _make_dog_mammal_animal_result()
        dog_animal = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        assert dog_animal.rule_iri == _TRANSITIVITY_RULE_IRI

    def test_explain_returns_explanation(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert isinstance(expl, Explanation)

    def test_cited_iris_nonempty(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert len(expl.cited_iris) > 0

    def test_target_derivation_id_in_cited_iris(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert expl.target_derivation_id in expl.cited_iris

    def test_rule_iri_in_cited_iris(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert _TRANSITIVITY_RULE_IRI in expl.cited_iris

    def test_subject_iri_in_cited_iris(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert str(_DOG) in expl.cited_iris

    def test_predicate_iri_in_cited_iris(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert str(_SUB_CLASS_OF) in expl.cited_iris

    def test_object_iri_in_cited_iris(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert str(_ANIMAL) in expl.cited_iris

    def test_antecedent_quads_in_step_skeleton(self) -> None:
        """The derivation tree must include the two antecedent asserted facts."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        subjects = {step.subject_iri for step in expl.step_skeleton}
        # Dog ⊑ Mammal and Mammal ⊑ Animal are antecedents
        assert str(_DOG) in subjects
        assert str(_MAMMAL) in subjects

    def test_step_skeleton_has_derived_and_asserted_steps(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        has_derived = any(not s.is_asserted for s in expl.step_skeleton)
        has_asserted = any(s.is_asserted for s in expl.step_skeleton)
        assert has_derived, "Expected at least one derived step"
        assert has_asserted, "Expected at least one asserted step"

    def test_world_iri_correct(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert expl.world_iri == str(_W)

    def test_target_quad_reifier_in_cited_iris(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert expl.target_quad_reifier in expl.cited_iris


# --------------------------------------------------------------------------- #
# Tests: faithfulness gate — valid explanation passes
# --------------------------------------------------------------------------- #


class TestFaithfulnessGateValid:
    def test_assert_explanation_faithful_passes(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        # Must not raise
        assert_explanation_faithful(expl, result)

    def test_faithfulness_gate_on_asserted_quad(self) -> None:
        """explain() + faithfulness gate also work on asserted (input) facts."""
        result = _make_dog_mammal_animal_result()
        # Dog ⊑ Mammal is asserted
        target = _find_derived_quad(result, str(_DOG), _MAMMAL.n3())
        expl = explain(result, target)
        assert expl.step_skeleton[0].is_asserted
        assert_explanation_faithful(expl, result)


# --------------------------------------------------------------------------- #
# Tests: faithfulness gate — hallucinated citation raises
# --------------------------------------------------------------------------- #


class TestFaithfulnessGateHallucination:
    def _make_hallucinated_explanation(
        self,
    ) -> tuple[Explanation, MaterializationResult]:
        """Build a valid explanation then inject a foreign IRI into cited_iris."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)

        # Inject a completely foreign IRI that is NOT in the proof trace
        foreign_iri = "http://example.org/HALLUCINATED_FOREIGN_IRI_NOT_IN_TRACE"
        hallucinated_cited = expl.cited_iris | {foreign_iri}

        # Construct a new Explanation with the injected IRI
        hallucinated = Explanation(
            target_derivation_id=expl.target_derivation_id,
            target_quad_reifier=expl.target_quad_reifier,
            world_iri=expl.world_iri,
            step_skeleton=expl.step_skeleton,
            cited_iris=frozenset(hallucinated_cited),
            prose_lines=expl.prose_lines,
        )
        return hallucinated, result

    def test_hallucinated_iri_raises_faithfulness_error(self) -> None:
        hallucinated, result = self._make_hallucinated_explanation()
        with pytest.raises(FaithfulnessError) as exc_info:
            assert_explanation_faithful(hallucinated, result)
        err = exc_info.value
        assert "HALLUCINATED_FOREIGN_IRI_NOT_IN_TRACE" in err.cited_iri

    def test_faithfulness_error_carries_cited_iri(self) -> None:
        hallucinated, result = self._make_hallucinated_explanation()
        with pytest.raises(FaithfulnessError) as exc_info:
            assert_explanation_faithful(hallucinated, result)
        assert exc_info.value.cited_iri == (
            "http://example.org/HALLUCINATED_FOREIGN_IRI_NOT_IN_TRACE"
        )

    def test_faithfulness_error_carries_target(self) -> None:
        hallucinated, result = self._make_hallucinated_explanation()
        with pytest.raises(FaithfulnessError) as exc_info:
            assert_explanation_faithful(hallucinated, result)
        assert exc_info.value.explanation_target == hallucinated.target_derivation_id

    def test_clean_explanation_passes_gate(self) -> None:
        """The non-hallucinated explanation from the same result must pass."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        # This must NOT raise
        assert_explanation_faithful(expl, result)


# --------------------------------------------------------------------------- #
# Tests: determinism
# --------------------------------------------------------------------------- #


class TestDeterminism:
    def test_same_input_same_cited_iris(self) -> None:
        """Two identical runs produce identical cited_iris sets."""
        r1 = _make_dog_mammal_animal_result()
        r2 = _make_dog_mammal_animal_result()
        t1 = _find_derived_quad(r1, str(_DOG), _ANIMAL.n3())
        t2 = _find_derived_quad(r2, str(_DOG), _ANIMAL.n3())
        e1 = explain(r1, t1)
        e2 = explain(r2, t2)
        assert e1.cited_iris == e2.cited_iris

    def test_same_input_same_step_skeleton(self) -> None:
        """Two identical runs produce identical step_skeleton sequences."""
        r1 = _make_dog_mammal_animal_result()
        r2 = _make_dog_mammal_animal_result()
        t1 = _find_derived_quad(r1, str(_DOG), _ANIMAL.n3())
        t2 = _find_derived_quad(r2, str(_DOG), _ANIMAL.n3())
        e1 = explain(r1, t1)
        e2 = explain(r2, t2)
        assert e1.step_skeleton == e2.step_skeleton

    def test_same_input_same_target_derivation_id(self) -> None:
        r1 = _make_dog_mammal_animal_result()
        r2 = _make_dog_mammal_animal_result()
        t1 = _find_derived_quad(r1, str(_DOG), _ANIMAL.n3())
        t2 = _find_derived_quad(r2, str(_DOG), _ANIMAL.n3())
        e1 = explain(r1, t1)
        e2 = explain(r2, t2)
        assert e1.target_derivation_id == e2.target_derivation_id

    def test_same_input_same_markdown(self) -> None:
        """Two identical runs produce byte-for-byte identical Markdown output."""
        r1 = _make_dog_mammal_animal_result()
        r2 = _make_dog_mammal_animal_result()
        t1 = _find_derived_quad(r1, str(_DOG), _ANIMAL.n3())
        t2 = _find_derived_quad(r2, str(_DOG), _ANIMAL.n3())
        e1 = explain(r1, t1)
        e2 = explain(r2, t2)
        assert e1.as_markdown() == e2.as_markdown()


# --------------------------------------------------------------------------- #
# Tests: Markdown output shape
# --------------------------------------------------------------------------- #


class TestMarkdownOutput:
    def test_markdown_contains_cited_iri_skeleton_header(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        md = expl.as_markdown()
        assert "<!-- cited-iri-skeleton" in md
        assert "-->" in md

    def test_markdown_contains_step_skeleton_header(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        md = expl.as_markdown()
        assert "<!-- step-skeleton" in md

    def test_markdown_lists_transitivity_rule_in_skeleton(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        md = expl.as_markdown()
        assert _TRANSITIVITY_RULE_IRI in md

    def test_markdown_contains_world_iri(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        md = expl.as_markdown()
        assert str(_W) in md

    def test_markdown_no_term_annotations_when_no_graph(self) -> None:
        """Without an ontology graph, no term label/definition lines appear."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target, onto_graph=None)
        # Structural prose lines (step header + quad) are still emitted;
        # term annotation bullet lines ("- `<iri>` — label: defn") are absent.
        # No annotation lines should contain " — " followed by non-IRI text
        # (the fallback omits the label/definition suffix entirely)
        annotation_lines = [
            line
            for line in expl.prose_lines
            if line.strip().startswith("- `<") and " — " in line
        ]
        assert annotation_lines == [], (
            f"Expected no annotation lines without onto_graph; got {annotation_lines!r}"
        )

    def test_markdown_ends_with_newline(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert expl.as_markdown().endswith("\n")


# --------------------------------------------------------------------------- #
# Tests: prose via onto_graph
# --------------------------------------------------------------------------- #


class TestProseWithOntoGraph:
    def _make_onto_graph(self) -> Graph:
        g = Graph()
        # Annotate the transitivity rule
        rule_iri = URIRef(_TRANSITIVITY_RULE_IRI)
        g.add(
            (
                rule_iri,
                RDFS.label,
                Literal("subClassOf transitivity rule", lang="en"),
            )
        )
        g.add(
            (
                rule_iri,
                SKOS.definition,
                Literal(
                    "Derives transitive subclass relationships: "
                    "if X is a subclass of Y and Y is a subclass of Z, "
                    "then X is a subclass of Z.",
                    lang="en",
                ),
            )
        )
        # Annotate Dog, Mammal, Animal
        for term, label in (
            (_DOG, "Dog"),
            (_MAMMAL, "Mammal"),
            (_ANIMAL, "Animal"),
        ):
            g.add((term, RDFS.label, Literal(label, lang="en")))
            g.add(
                (
                    term,
                    SKOS.definition,
                    Literal(f"The class of all {label.lower()}s.", lang="en"),
                )
            )
        return g

    def test_prose_lines_populated_with_graph(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        onto_graph = self._make_onto_graph()
        expl = explain(result, target, onto_graph=onto_graph)
        assert len(expl.prose_lines) > 0

    def test_prose_lines_contain_rule_label(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        onto_graph = self._make_onto_graph()
        expl = explain(result, target, onto_graph=onto_graph)
        prose = "\n".join(expl.prose_lines)
        assert "transitivity" in prose.lower()

    def test_faithfulness_still_passes_with_graph(self) -> None:
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        onto_graph = self._make_onto_graph()
        expl = explain(result, target, onto_graph=onto_graph)
        # Must not raise
        assert_explanation_faithful(expl, result)


# --------------------------------------------------------------------------- #
# Tests: three-hop chain (Dog ⊑ Mammal ⊑ Animal ⊑ LivingThing)
# --------------------------------------------------------------------------- #


class TestThreeHopChain:
    _LIVING_THING = URIRef("http://example.org/LivingThing")

    def _make_three_hop_result(self) -> MaterializationResult:
        cg = _cg_with_quads(
            [
                (_DOG, _SUB_CLASS_OF, _MAMMAL, _W),
                (_MAMMAL, _SUB_CLASS_OF, _ANIMAL, _W),
                (_ANIMAL, _SUB_CLASS_OF, self._LIVING_THING, _W),
            ]
        )
        return materialize_program(_sub_class_of_transitivity_program(), cg)

    def test_dog_living_thing_is_derived(self) -> None:
        result = self._make_three_hop_result()
        target = _find_derived_quad(result, str(_DOG), self._LIVING_THING.n3())
        assert target.rule_iri == _TRANSITIVITY_RULE_IRI

    def test_explain_dog_living_thing_faithful(self) -> None:
        result = self._make_three_hop_result()
        target = _find_derived_quad(result, str(_DOG), self._LIVING_THING.n3())
        expl = explain(result, target)
        assert_explanation_faithful(expl, result)

    def test_all_term_iris_in_trace(self) -> None:
        result = self._make_three_hop_result()
        target = _find_derived_quad(result, str(_DOG), self._LIVING_THING.n3())
        expl = explain(result, target)
        # All terms cited across the skeleton must be in the proof trace
        for step in expl.step_skeleton:
            for term_iri in step.term_iris:
                assert term_iri in expl.cited_iris, (
                    f"Term IRI {term_iri!r} from step {step.derivation_id!r} "
                    "is not in cited_iris"
                )


# --------------------------------------------------------------------------- #
# Tests: ExplainError on unknown target
# --------------------------------------------------------------------------- #


class TestExplainErrorCases:
    def test_explain_raises_for_foreign_quad(self) -> None:
        """explain() raises ExplainError if the target is not in the result."""
        result = _make_dog_mammal_animal_result()
        # Build a DerivedQuad that is not in the result
        from gmeow_tools.config import PREFIXES
        from gmeow_tools.logic_materialize import derivation_id_iri, quad_reifier_iri

        logic_ns = PREFIXES["logic"]
        foreign_subject = URIRef("http://example.org/FOREIGN")
        foreign_pred = _SUB_CLASS_OF
        foreign_obj = URIRef("http://example.org/ALSO_FOREIGN")
        reifier = quad_reifier_iri(foreign_subject, foreign_pred, foreign_obj)
        deriv = derivation_id_iri(f"{logic_ns}assert", [reifier])

        foreign_quad = DerivedQuad(
            graph=str(_W),
            subject=str(foreign_subject),
            predicate=str(foreign_pred),
            obj=foreign_obj.n3(),
            graph_component=str(_W),
            derivation_id=deriv,
            rule_iri=f"{logic_ns}assert",
            source_quad_ids=[reifier],
            profile=f"{logic_ns}PositiveHornProfile",
            budget_status="ok",
        )
        with pytest.raises(ExplainError):
            explain(result, foreign_quad)


# --------------------------------------------------------------------------- #
# Regression tests: Gap 3 — literal-object quads (issue #501)
# --------------------------------------------------------------------------- #


_HAS_LABEL = URIRef("http://www.w3.org/2000/01/rdf-schema#label")
_W_LIT = URIRef("http://world/LiteralTest")


def _make_literal_object_result() -> MaterializationResult:
    """Materialize a graph that contains a literal-object quad.

    Input: Dog rdfs:label "Dog"@en (in _W_LIT)
    No rules — the result contains only the one asserted quad with a literal obj.
    This is the minimal case that triggered the crash in _build_reifier_index
    when it called _n3_to_term on a literal N3 string.
    """
    from gmeow_tools.logic_ir import (
        LogicProfile,
        LogicProgram,
        SemanticProfileId,
    )

    program = LogicProgram(
        axioms=(),
        rules=(),
        profiles=(LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN),),
    )
    cg: ConjunctiveGraph = ConjunctiveGraph()
    named_graph = cg.get_context(_W_LIT)
    named_graph.add((_DOG, _HAS_LABEL, Literal("Dog", lang="en")))
    return materialize_program(program, cg)


class TestLiteralObjectQuad:
    """Regression tests for Gap 3: explain() must not crash on literal-object quads."""

    def test_materialize_produces_literal_quad(self) -> None:
        """Sanity: the materializer produces the literal-object asserted quad."""
        result = _make_literal_object_result()
        lit_n3 = Literal("Dog", lang="en").n3()
        dq = _find_derived_quad(result, str(_DOG), lit_n3)
        assert dq.obj == lit_n3

    def test_explain_does_not_crash_on_literal_object(self) -> None:
        """explain() must succeed (not raise) when the target has a literal object.

        Previously _build_reifier_index called _n3_to_term(dq.obj) which raised
        ExplainError for any literal-valued object.  After the fix it uses
        _reifier_from_quad(dq) which handles literals correctly.
        """
        result = _make_literal_object_result()
        lit_n3 = Literal("Dog", lang="en").n3()
        target = _find_derived_quad(result, str(_DOG), lit_n3)
        expl = explain(result, target)
        assert isinstance(expl, Explanation)

    def test_literal_object_step_is_asserted(self) -> None:
        """The single asserted quad with a literal object must appear as is_asserted."""
        result = _make_literal_object_result()
        lit_n3 = Literal("Dog", lang="en").n3()
        target = _find_derived_quad(result, str(_DOG), lit_n3)
        expl = explain(result, target)
        assert expl.step_skeleton[0].is_asserted

    def test_literal_object_faithfulness_passes(self) -> None:
        """assert_explanation_faithful() must pass for a literal-object explanation."""
        result = _make_literal_object_result()
        lit_n3 = Literal("Dog", lang="en").n3()
        target = _find_derived_quad(result, str(_DOG), lit_n3)
        expl = explain(result, target)
        # Must not raise
        assert_explanation_faithful(expl, result)

    def test_literal_object_not_in_term_iris(self) -> None:
        """Literal objects must NOT appear in term_iris (only IRI objects do)."""
        result = _make_literal_object_result()
        lit_n3 = Literal("Dog", lang="en").n3()
        target = _find_derived_quad(result, str(_DOG), lit_n3)
        expl = explain(result, target)
        for step in expl.step_skeleton:
            for iri in step.term_iris:
                # No IRI should embed the literal string
                assert '"Dog"' not in iri

    def test_subject_and_predicate_in_cited_iris(self) -> None:
        """Subject and predicate IRIs must still be in cited_iris for literal quads."""
        result = _make_literal_object_result()
        lit_n3 = Literal("Dog", lang="en").n3()
        target = _find_derived_quad(result, str(_DOG), lit_n3)
        expl = explain(result, target)
        assert str(_DOG) in expl.cited_iris
        assert str(_HAS_LABEL) in expl.cited_iris


# --------------------------------------------------------------------------- #
# Regression tests: Gap 3b — world-scoped reifier index (issue #501)
# --------------------------------------------------------------------------- #

_W_A = URIRef("http://world/Alpha")
_W_B = URIRef("http://world/Beta")


def _make_two_world_result() -> MaterializationResult:
    """Materialize a two-world scenario where the SAME (S,P,O) appears in both.

    World A: Dog ⊑ Mammal (asserted), Mammal ⊑ Animal (asserted)
             → Dog ⊑ Animal (DERIVED by transitivity rule)
    World B: Dog ⊑ Animal (asserted directly — same triple, different provenance)

    Before the fix, keying the reifier index by reifier alone meant that the
    derived Dog⊑Animal in world A and the asserted Dog⊑Animal in world B shared
    one index slot, so antecedent resolution could return the wrong DerivedQuad.
    After the fix, (world_iri, reifier) keys disambiguate them correctly.
    """
    cg: ConjunctiveGraph = ConjunctiveGraph()
    # World A
    ctx_a = cg.get_context(_W_A)
    ctx_a.add((_DOG, _SUB_CLASS_OF, _MAMMAL))
    ctx_a.add((_MAMMAL, _SUB_CLASS_OF, _ANIMAL))
    # World B — same terminal triple, but asserted directly
    ctx_b = cg.get_context(_W_B)
    ctx_b.add((_DOG, _SUB_CLASS_OF, _ANIMAL))
    return materialize_program(_sub_class_of_transitivity_program(), cg)


class TestTwoWorldReifierIndex:
    """Regression tests for Gap 3b: antecedents must resolve in the correct world."""

    def test_both_worlds_have_dog_animal(self) -> None:
        """Both worlds produce a Dog⊑Animal quad but with different provenance."""
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        quads_by_graph: dict[str, DerivedQuad] = {}
        for dq in result.quads:
            if dq.subject == str(_DOG) and dq.obj == animal_n3:
                quads_by_graph[dq.graph] = dq
        assert str(_W_A) in quads_by_graph, "World A must have Dog⊑Animal"
        assert str(_W_B) in quads_by_graph, "World B must have Dog⊑Animal"

    def test_world_a_quad_is_derived(self) -> None:
        """In world A, Dog⊑Animal must be derived (by the transitivity rule)."""
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        for dq in result.quads:
            if (
                dq.subject == str(_DOG)
                and dq.obj == animal_n3
                and dq.graph == str(_W_A)
            ):
                assert dq.rule_iri == _TRANSITIVITY_RULE_IRI
                return
        pytest.fail("No Dog⊑Animal quad found in world A")

    def test_world_b_quad_is_asserted(self) -> None:
        """In world B, Dog⊑Animal must be asserted (no rule applied)."""
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        for dq in result.quads:
            if (
                dq.subject == str(_DOG)
                and dq.obj == animal_n3
                and dq.graph == str(_W_B)
            ):
                assert dq.rule_iri == "https://blackcatinformatics.ca/logic/assert"
                return
        pytest.fail("No Dog⊑Animal quad found in world B")

    def test_explain_world_a_derived_quad_faithful(self) -> None:
        """explain() on the DERIVED quad in world A must be faithful.

        This confirms antecedents (Dog⊑Mammal, Mammal⊑Animal) are resolved
        from world A, not collapsed with the asserted quad in world B.
        """
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        target_a: DerivedQuad | None = None
        for dq in result.quads:
            if (
                dq.subject == str(_DOG)
                and dq.obj == animal_n3
                and dq.graph == str(_W_A)
            ):
                target_a = dq
                break
        assert target_a is not None
        expl = explain(result, target_a)
        assert not expl.step_skeleton[0].is_asserted, (
            "Top step in world A must be derived (transitivity rule)"
        )
        assert_explanation_faithful(expl, result)

    def test_explain_world_b_asserted_quad_faithful(self) -> None:
        """explain() on the ASSERTED quad in world B must be faithful.

        World B has only the one asserted fact; its explanation must not
        pull in antecedents from world A.
        """
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        target_b: DerivedQuad | None = None
        for dq in result.quads:
            if (
                dq.subject == str(_DOG)
                and dq.obj == animal_n3
                and dq.graph == str(_W_B)
            ):
                target_b = dq
                break
        assert target_b is not None
        expl = explain(result, target_b)
        assert expl.step_skeleton[0].is_asserted, "Top step in world B must be asserted"
        assert_explanation_faithful(expl, result)

    def test_world_a_explanation_contains_antecedent_steps(self) -> None:
        """The derived quad in world A must cite its two antecedent asserted facts."""
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        for dq in result.quads:
            if (
                dq.subject == str(_DOG)
                and dq.obj == animal_n3
                and dq.graph == str(_W_A)
            ):
                expl = explain(result, dq)
                # Step skeleton must include Mammal as an intermediate node
                subjects = {step.subject_iri for step in expl.step_skeleton}
                assert str(_MAMMAL) in subjects, (
                    "Explanation for world A derived quad must include Mammal step"
                )
                return
        pytest.fail("No derived Dog⊑Animal found in world A")

    def test_world_b_explanation_has_only_one_step(self) -> None:
        """The asserted quad in world B has no antecedents beyond itself."""
        result = _make_two_world_result()
        animal_n3 = _ANIMAL.n3()
        for dq in result.quads:
            if (
                dq.subject == str(_DOG)
                and dq.obj == animal_n3
                and dq.graph == str(_W_B)
            ):
                expl = explain(result, dq)
                assert len(expl.step_skeleton) == 1, (
                    "World B asserted quad must produce exactly one step "
                    f"(no antecedents), got {len(expl.step_skeleton)}"
                )
                return
        pytest.fail("No asserted Dog⊑Animal found in world B")


# --------------------------------------------------------------------------- #
# Regression tests: Gap 5 — graph/world IRIs in faithfulness gate (issue #501)
# --------------------------------------------------------------------------- #


class TestGraphIriInFaithfulnessGate:
    """Regression tests for Gap 5: graph_iri and world_iri must be in the
    proof-trace IRI set so as_markdown() citations are faithfulness-checked."""

    def test_world_iri_in_cited_iris(self) -> None:
        """world_iri must appear in cited_iris (it is cited in as_markdown())."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        assert expl.world_iri in expl.cited_iris, (
            f"world_iri {expl.world_iri!r} must be in cited_iris"
        )

    def test_step_graph_iris_in_cited_iris(self) -> None:
        """Every step's graph_iri must appear in cited_iris."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        for step in expl.step_skeleton:
            assert step.graph_iri in expl.cited_iris, (
                f"Step graph_iri {step.graph_iri!r} must be in cited_iris "
                f"(step derivation_id={step.derivation_id!r})"
            )

    def test_hallucinated_world_iri_fails_faithfulness(self) -> None:
        """Injecting a foreign world_iri into the explanation raises FaithfulnessError.

        This verifies that the faithfulness gate actually CHECKS world_iri —
        a world IRI not in the proof trace must be caught.
        """
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)

        foreign_world = "http://world/HALLUCINATED_WORLD_IRI"
        hallucinated = Explanation(
            target_derivation_id=expl.target_derivation_id,
            target_quad_reifier=expl.target_quad_reifier,
            world_iri=foreign_world,
            step_skeleton=expl.step_skeleton,
            cited_iris=expl.cited_iris | {foreign_world},
            prose_lines=expl.prose_lines,
        )
        with pytest.raises(FaithfulnessError) as exc_info:
            assert_explanation_faithful(hallucinated, result)
        assert "HALLUCINATED_WORLD_IRI" in exc_info.value.cited_iri

    def test_markdown_world_iri_in_cited_iris(self) -> None:
        """The world_iri cited in as_markdown() prose must be in cited_iris."""
        result = _make_dog_mammal_animal_result()
        target = _find_derived_quad(result, str(_DOG), _ANIMAL.n3())
        expl = explain(result, target)
        md = expl.as_markdown()
        # as_markdown() cites world_iri on the "**World:**" line
        assert expl.world_iri in md
        # And it must be in cited_iris so faithfulness covers it
        assert expl.world_iri in expl.cited_iris
