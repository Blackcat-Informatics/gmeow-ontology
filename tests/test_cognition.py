"""Retained guards for the cognition module.

Asserted-TBox structural invariants whose ASK subjects live in the cognition
module graph have been migrated to the declarative test-DSL cell file at
slices/core/cognition/tests/structural.ttl.

Cross-slice asserted-TBox invariants migrated to their owning slices:
  - test_mental_moment_is_category_under_intrinsic_mode
      → slices/core/kernel/tests/structural.ttl (saMentalMomentIsCategoryUnderMode)
  - test_intentional_mode_reparented_under_mental_moment
      → slices/core/teleology/tests/structural.ttl (saIntentionalModeIsCategory,
        saIntentionalModeNotDirectlyIntrinsicMode)
  - test_proficiency_vocab_relocated_to_kernel
      → slices/core/kernel/tests/structural.ttl (saProficiencyVocabInKernel)
  - test_intrinsic_modes_are_grounded (from tests/test_teleology.py)
      → slices/core/kernel/tests/structural.ttl (saMentalMomentGroundsIntrinsicModes)

RETAINED here (not expressible as module-scoped declarative cells):

  test_mental_moment_has_exactly_one_gufo_metaclass --
    Whole-graph dynamic sweep: iterates all four classes and counts
    metaclass hits from an open gufo:/logic: set. The "exactly-one"
    cardinality check cannot be faithfully encoded as a module-scoped
    ASK without narrowing the assertion to a finite list.

  test_cognition_sssom_* --
    load_mappings() reads of gmeow-cognition.sssom.tsv; MAP-flag ledger
    checks, not module-scoped TBox assertions.
"""

from __future__ import annotations

from purrdf.compat.rdflib import RDF, Graph, URIRef

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mappings import load_mappings

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
LOGIC = "https://blackcatinformatics.ca/logic/"


def _graph() -> Graph:
    """Merged ontology graph without imports — fast TBox checks."""
    return load_merged_graph(include_imports=False)


def _g(local: str) -> URIRef:
    return URIRef(GMEOW + local)


def _gufo(local: str) -> URIRef:
    return URIRef(GUFO + local)


def _logic(local: str) -> URIRef:
    return URIRef(LOGIC + local)


# --------------------------------------------------------------------------- #
# MentalMoment umbrella — dynamic sweep; RETAINED.
# --------------------------------------------------------------------------- #


def test_mental_moment_has_exactly_one_gufo_metaclass() -> None:
    """Each new class carries exactly one ontological metaclass.

    Checks that each term has exactly one metaclass annotation from either
    the gufo: or logic: namespace. Retained because the "exactly one"
    cardinality check over an open metaclass set is a dynamic sweep that
    cannot be faithfully encoded as a module-scoped ASK.
    """
    graph = _graph()
    metaclass_locals = (
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
    known_meta = {_gufo(m) for m in metaclass_locals} | {
        _logic(m) for m in metaclass_locals
    }
    for cls in (
        "MentalMoment",
        "CognitiveState",
        "KnowledgeProficiency",
        "KnowledgeLevel",
    ):
        types = set(graph.objects(_g(cls), RDF.type))
        meta = types & known_meta
        assert len(meta) == 1, (
            f"{cls} must carry exactly one ontological metaclass, got {meta}"
        )


# --------------------------------------------------------------------------- #
# SSSOM alignment ledger coverage.
# --------------------------------------------------------------------------- #


def _cognition_sssom_rows() -> set[tuple[str, str, str]]:
    """Return the (subject_id, predicate_id, object_id) rows from the cognition
    mapping set."""
    return {
        (m.subject_id, m.predicate_id, m.object_id)
        for m in load_mappings()
        if m.source.name == "gmeow-cognition.sssom.tsv"
    }


def test_cognition_sssom_rows_include_expected_alignments() -> None:
    """The cognition SSSOM ledger contains the expected cross-ontology rows."""
    rows = _cognition_sssom_rows()
    expected = {
        # Knowledge spectrum surface vocabulary anchors.
        ("gmeow:knowsAbout", "skos:exactMatch", "schema:knowsAbout"),
        ("gmeow:knowsAbout", "skos:relatedMatch", "sumo:knows"),
        ("gmeow:knowsAbout", "skos:relatedMatch", "wd:Q9081"),
        ("gmeow:isAwareOf", "skos:relatedMatch", "sumo:knows"),
        ("gmeow:attendsTo", "skos:closeMatch", "foaf:focus"),
        ("gmeow:interestedIn", "skos:closeMatch", "foaf:interest"),
        # Corrected Wikidata QIDs for attention, curiosity, and mastery.
        ("gmeow:attendsTo", "skos:relatedMatch", "wd:Q6501338"),
        ("gmeow:curiousAbout", "skos:relatedMatch", "wd:Q366791"),
        ("gmeow:hasMastered", "skos:relatedMatch", "wd:Q12770764"),
        # New OpenCyc objectual-knows anchor.
        ("gmeow:knowsAbout", "skos:relatedMatch", "cyc:knowsAbout"),
        # Competency / knowledge-depth framework references.
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "ctdlasn:"),
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "esco-base:"),
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "onet:"),
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "wd:Q1774565"),
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "wd:Q5307365"),
        (
            "gmeow:scaleKnowledgeDepth",
            "skos:relatedMatch",
            "https://en.wikipedia.org/wiki/Structure_of_observed_learning_outcome",
        ),
    }
    missing = expected - rows
    assert not missing, f"Missing cognition SSSOM rows: {missing}"


def test_cognition_sssom_includes_corrected_wikidata_qids() -> None:
    """The issue-supplied QIDs were rejected and replaced with verified entities."""
    rows = _cognition_sssom_rows()
    qids = {obj for _subj, _pred, obj in rows if obj.startswith("wd:")}
    assert "wd:Q6501338" in qids, "attention QID expected"
    assert "wd:Q366791" in qids, "curiosity QID expected"
    assert "wd:Q12770764" in qids, "mastery QID expected"
    # Rejected issue QIDs must not have crept back in.
    assert "wd:Q327954" not in qids, "rejected 'torch' QID"
    assert "wd:Q179637" not in qids, "rejected 'prisoner of war' QID"
    assert "wd:Q1016098" not in qids, "rejected 'Mautes' QID"


def test_cognition_sssom_includes_opencyc_knows_about() -> None:
    """OpenCyc knowsAbout is present as a relatedMatch anchor."""
    rows = _cognition_sssom_rows()
    assert ("gmeow:knowsAbout", "skos:relatedMatch", "cyc:knowsAbout") in rows
