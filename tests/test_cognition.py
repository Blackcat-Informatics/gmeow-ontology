"""Structural + closed-world guards for the cognition module (issues #556/#557/#558).

Exercises the MentalMoment umbrella, the relocated proficiency value vocab, the
reified KnowledgeProficiency relator + CognitiveState mode + KnowledgeLevel ordinal
axis (with transitive deeperThan ON LEVELS ONLY), the teleology IntentionalMode
reparent, the attention/interest/objectual-memory relations, and the
KnowledgeProficiency SHACL shape.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    """Merged ontology graph without imports — fast TBox checks."""
    return load_merged_graph(include_imports=False)


def _g(local: str) -> URIRef:
    return URIRef(GMEOW + local)


def _gufo(local: str) -> URIRef:
    return URIRef(GUFO + local)


# --------------------------------------------------------------------------- #
# MentalMoment umbrella (#556) and its members.
# --------------------------------------------------------------------------- #


def test_mental_moment_is_category_under_intrinsic_mode() -> None:
    """MentalMoment is a gufo:Category placed under gufo:IntrinsicMode (kernel)."""
    graph = _graph()
    mm = _g("MentalMoment")
    assert (mm, RDF.type, _gufo("Category")) in graph
    assert (mm, RDFS.subClassOf, _gufo("IntrinsicMode")) in graph
    assert (mm, RDFS.isDefinedBy, _g("slices/kernel")) in graph


def test_mental_moment_has_exactly_one_gufo_metaclass() -> None:
    """Acceptance: each new class carries exactly one gUFO metaclass."""
    graph = _graph()
    gufo_meta = {
        _gufo(m)
        for m in (
            "Kind",
            "Category",
            "Relator",
            "Mode",
            "IntrinsicMode",
            "QualityValue",
            "AbstractIndividualType",
            "Phase",
            "Role",
            "SubKind",
            "RoleMixin",
            "PhaseMixin",
            "Mixin",
        )
    }
    for cls in (
        "MentalMoment",
        "CognitiveState",
        "KnowledgeProficiency",
        "KnowledgeLevel",
    ):
        types = set(graph.objects(_g(cls), RDF.type))
        meta = types & gufo_meta
        assert len(meta) == 1, (
            f"{cls} must carry exactly one gUFO metaclass, got {meta}"
        )


def test_cognitive_state_is_kind_under_mental_moment() -> None:
    """CognitiveState (the knowing mode) is a gufo:Kind under MentalMoment."""
    graph = _graph()
    cs = _g("CognitiveState")
    assert (cs, RDF.type, _gufo("Kind")) in graph
    assert (cs, RDFS.subClassOf, _g("MentalMoment")) in graph


def test_intentional_mode_reparented_under_mental_moment() -> None:
    """teleology:IntentionalMode now subclasses MentalMoment (the reparent)."""
    graph = _graph()
    im = _g("IntentionalMode")
    assert (im, RDFS.subClassOf, _g("MentalMoment")) in graph
    # It must NOT keep a redundant direct gufo:IntrinsicMode parent assertion.
    assert (im, RDFS.subClassOf, _gufo("IntrinsicMode")) not in graph


# --------------------------------------------------------------------------- #
# Proficiency value vocab relocated to kernel (#556) — no cycle.
# --------------------------------------------------------------------------- #


def test_proficiency_vocab_relocated_to_kernel() -> None:
    """ProficiencyScale/Level/Modality are defined by the kernel slice now."""
    graph = _graph()
    for cls in ("ProficiencyScale", "ProficiencyLevel", "ProficiencyModality"):
        node = _g(cls)
        assert (node, RDFS.subClassOf, _gufo("QualityValue")) in graph
        assert (node, RDFS.isDefinedBy, _g("slices/kernel")) in graph
        assert (node, RDFS.isDefinedBy, _g("slices/expertise")) not in graph


# --------------------------------------------------------------------------- #
# Reified KnowledgeProficiency relator (#556), mirroring SkillProficiency.
# --------------------------------------------------------------------------- #


def test_knowledge_proficiency_is_relator_with_functional_roles() -> None:
    graph = _graph()
    assert (_g("KnowledgeProficiency"), RDFS.subClassOf, _gufo("Relator")) in graph
    assert (_g("KnowledgeProficiency"), RDF.type, _gufo("Kind")) in graph
    for role in (
        "knowledgeProficiencyAgent",
        "knowledgeProficiencySubject",
        "knowledgeProficiencyLevel",
        "knowledgeProficiencyScale",
    ):
        node = _g(role)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


def test_knowledge_proficiency_interval_is_optional() -> None:
    graph = _graph()
    interval = _g("knowledgeProficiencyInterval")
    assert (interval, RDF.type, OWL.ObjectProperty) in graph
    assert (interval, RDF.type, OWL.FunctionalProperty) not in graph


def test_knowledge_proficiency_some_values_from_axioms() -> None:
    graph = _graph()
    relator = _g("KnowledgeProficiency")
    for prop, cls in (
        ("knowledgeProficiencyAgent", "Agent"),
        ("knowledgeProficiencySubject", "Entity"),
        ("knowledgeProficiencyLevel", "KnowledgeLevel"),
        ("knowledgeProficiencyScale", "ProficiencyScale"),
    ):
        restrictions = list(graph.objects(relator, RDFS.subClassOf))
        assert any(
            (rest, OWL.onProperty, _g(prop)) in graph
            and (rest, OWL.someValuesFrom, _g(cls)) in graph
            for rest in restrictions
        ), f"KnowledgeProficiency missing someValuesFrom {prop} -> {cls}"


def test_cognitive_state_and_proficiency_are_not_double_typed() -> None:
    """Principle 12: the mode and the relator are distinct classes, never linked."""
    graph = _graph()
    cs, kp = _g("CognitiveState"), _g("KnowledgeProficiency")
    assert (kp, RDFS.subClassOf, cs) not in graph
    assert (cs, RDFS.subClassOf, kp) not in graph
    assert (kp, RDF.type, cs) not in graph


# --------------------------------------------------------------------------- #
# KnowledgeLevel ordinal axis + transitive deeperThan ON LEVELS ONLY (#556).
# --------------------------------------------------------------------------- #


def test_knowledge_level_is_quality_value() -> None:
    graph = _graph()
    assert (_g("KnowledgeLevel"), RDFS.subClassOf, _gufo("QualityValue")) in graph


def test_deeper_than_is_transitive_on_levels_only() -> None:
    graph = _graph()
    dt = _g("deeperThan")
    assert (dt, RDF.type, OWL.TransitiveProperty) in graph
    assert (dt, RDFS.domain, _g("KnowledgeLevel")) in graph
    assert (dt, RDFS.range, _g("KnowledgeLevel")) in graph


def test_knowledge_levels_are_ordinally_chained() -> None:
    graph = _graph()
    for level in (
        "knowledgeAware",
        "knowledgeKnowsAbout",
        "knowledgeUnderstands",
        "knowledgeMastered",
    ):
        assert (_g(level), RDF.type, _g("KnowledgeLevel")) in graph
    chain = (
        ("knowledgeMastered", "knowledgeUnderstands"),
        ("knowledgeUnderstands", "knowledgeKnowsAbout"),
        ("knowledgeKnowsAbout", "knowledgeAware"),
    )
    for deeper, shallower in chain:
        assert (_g(deeper), _g("deeperThan"), _g(shallower)) in graph


def test_spectrum_pairs_with_relator() -> None:
    graph = _graph()
    for prop in ("isAwareOf", "knowsAbout", "understands", "hasMastered"):
        assert (_g(prop), _g("pairsWith"), _g("KnowledgeProficiency")) in graph


# --------------------------------------------------------------------------- #
# Attention / interest / objectual memory (#557).
# --------------------------------------------------------------------------- #


def test_attention_and_interest_relations_exist() -> None:
    graph = _graph()
    for prop in ("attendsTo", "interestedIn", "curiousAbout", "remembers"):
        node = _g(prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDFS.domain, _g("Agent")) in graph
        assert (node, RDFS.range, _g("Entity")) in graph


def test_curious_about_specializes_interested_in() -> None:
    graph = _graph()
    assert (_g("curiousAbout"), RDFS.subPropertyOf, _g("interestedIn")) in graph


def test_remembers_is_objectual_memory_under_awareness() -> None:
    """remembers ⊑ isAwareOf, and NO axiom bridge to the ai MemoryItem construct."""
    graph = _graph()
    assert (_g("remembers"), RDFS.subPropertyOf, _g("isAwareOf")) in graph
    # The bridge to ai:memoryOf is documented prose, never an OWL coupling.
    assert (_g("remembers"), RDFS.subPropertyOf, _g("memoryOf")) not in graph


def test_no_axiom_bridge_to_teleology_desire() -> None:
    """Principle 9: interest/attention are not subproperties of any teleology term."""
    graph = _graph()
    for prop in ("attendsTo", "interestedIn", "curiousAbout"):
        assert (_g(prop), RDFS.subPropertyOf, _g("hasGoal")) not in graph


# --------------------------------------------------------------------------- #
# Closed-world SHACL: KnowledgeProficiency well-formedness (#558).
# --------------------------------------------------------------------------- #


def _fixture(name: str) -> Graph:
    from pathlib import Path

    path = Path(__file__).parent / "fixtures" / "shapes" / f"{name}.ttl"
    return Graph().parse(path, format="turtle")


def test_wellformed_knowledge_proficiency_conforms() -> None:
    result = run_shacl(_fixture("cognition-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_knowledge_proficiency_is_flagged() -> None:
    result = run_shacl(_fixture("cognition-malformed"))
    assert not result.ok
    assert result.errors
    errors = "\n".join(result.errors)
    assert "must reference exactly one subject" in errors
    assert "must carry exactly one KnowledgeLevel" in errors
    assert "at most one scale" in errors
