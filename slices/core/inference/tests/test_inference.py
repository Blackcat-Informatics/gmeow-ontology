# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: CC-BY-4.0
"""Structural + closed-world guards for the inference module (issue #581).

Exercises Peirce's tetrad as the epistemic face of logic:: the endurant/occurrent
split (InferenceProcess ⊑ MentalProcess vs InferenceCommitment ⊑ logic:Relator), the
exactly-one-logic-master invariant, the property domains/ranges/characteristics, the
mode + defeater value vocabularies, and the SHACL shapes (a well-formed commitment
conforms; a malformed one — premise == conclusion, self-competesWith — is flagged).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef
from tests._graph_nt import run_shacl

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
LOGIC = "https://blackcatinformatics.ca/logic/"

_SLICE = Path(__file__).resolve().parent.parent
_MODULE = _SLICE / "module.ttl"
_SHAPES = _SLICE / "shapes.ttl"
_SLICE_IRI = URIRef(GMEOW + "slices/inference")

# The full set of allowed logic: master metaclasses (one per class — the invariant).
# After the #694 migration, stereotype authoring is in the logic: namespace.
_LOGIC_MASTERS = {
    URIRef(LOGIC + m)
    for m in (
        "Kind",
        "Category",
        "Relator",
        "Mode",
        "QualityValue",
        "AbstractIndividualType",
        "Phase",
        "Role",
        "SubKind",
        "RoleMixin",
        "PhaseMixin",
        "Mixin",
        "Event",  # gufo:EventType → logic:Event
        "Situation",  # gufo:SituationType → logic:Situation
        "Disposition",
    )
}

_CLASSES = (
    "InferenceProcess",
    "InferenceCommitment",
    "Analogy",
    "Correspondence",
    "InferenceMode",
    "DefeaterKind",
    "InferenceTenure",
)


def _graph() -> Graph:
    """Merged ontology graph without imports — fast TBox checks."""
    return load_merged_graph(include_imports=False)


def _g(local: str) -> URIRef:
    return URIRef(GMEOW + local)


def _logic(local: str) -> URIRef:
    """Return a logic:-namespaced term URI (stereotype namespace after #694)."""
    return URIRef(LOGIC + local)


# --------------------------------------------------------------------------- #
# Exactly-one-gUFO-master invariant (the acceptance gate).
# --------------------------------------------------------------------------- #


def test_every_class_has_exactly_one_gufo_metaclass() -> None:
    graph = _graph()
    for cls in _CLASSES:
        types = set(graph.objects(_g(cls), RDF.type))
        meta = types & _LOGIC_MASTERS
        assert len(meta) == 1, (
            f"{cls} must carry exactly one gUFO master metaclass, got {meta}"
        )


def test_all_terms_defined_by_inference_slice() -> None:
    graph = _graph()
    for cls in _CLASSES:
        assert (_g(cls), RDFS.isDefinedBy, _SLICE_IRI) in graph


# --------------------------------------------------------------------------- #
# The endurant/occurrent split — the mandated architecture correction.
# --------------------------------------------------------------------------- #


def test_inference_process_is_eventtype_under_mental_process() -> None:
    """The OCCURRENT face: InferenceProcess ⊑ MentalProcess (a perdurant).
    After #694 migration: stereotype is logic:Event (renamed from gufo:EventType)."""
    graph = _graph()
    ip = _g("InferenceProcess")
    assert (ip, RDF.type, _logic("Event")) in graph
    assert (ip, RDFS.subClassOf, _g("MentalProcess")) in graph
    # It must NOT also be a Relator — that was the rejected double-typing.
    assert (ip, RDFS.subClassOf, _logic("Relator")) not in graph


def test_inference_commitment_is_relator_kind() -> None:
    """The ENDURANT face: InferenceCommitment ⊑ logic:Relator, master logic:Kind.
    After #694 migration: stereotype namespace is logic: not gufo:."""
    graph = _graph()
    ic = _g("InferenceCommitment")
    assert (ic, RDF.type, _logic("Kind")) in graph
    assert (ic, RDFS.subClassOf, _logic("Relator")) in graph
    # It must NOT be a MentalProcess — the split keeps it off the occurrent side.
    assert (ic, RDFS.subClassOf, _g("MentalProcess")) not in graph


def test_relator_classes_carry_relator_only_via_subclassof() -> None:
    """Analogy/Correspondence are Relators by subClassOf, master logic:Kind.
    After #694 migration: stereotype namespace is logic: not gufo:."""
    graph = _graph()
    for cls in ("Analogy", "Correspondence"):
        assert (_g(cls), RDF.type, _logic("Kind")) in graph
        assert (_g(cls), RDFS.subClassOf, _logic("Relator")) in graph
        assert (_g(cls), RDF.type, _logic("Relator")) not in graph


def test_inference_tenure_is_situation_under_timescoped() -> None:
    """After #694 migration: gufo:SituationType → logic:Situation."""
    graph = _graph()
    it = _g("InferenceTenure")
    assert (it, RDF.type, _logic("Situation")) in graph
    assert (it, RDFS.subClassOf, _g("TimeScopedRelation")) in graph


def test_value_vocabs_are_abstract_individual_types() -> None:
    """After #694 migration: stereotype namespace is logic: not gufo:."""
    graph = _graph()
    for cls in ("InferenceMode", "DefeaterKind"):
        assert (_g(cls), RDF.type, _logic("AbstractIndividualType")) in graph
        assert (_g(cls), RDFS.subClassOf, _logic("QualityValue")) in graph


# --------------------------------------------------------------------------- #
# Value individuals — Peirce's tetrad + Pollock's defeaters.
# --------------------------------------------------------------------------- #


def test_mode_individuals_typed() -> None:
    graph = _graph()
    for mode in ("modeDeduction", "modeInduction", "modeAbduction", "modeAnalogical"):
        assert (_g(mode), RDF.type, _g("InferenceMode")) in graph


def test_defeater_kind_individuals_typed() -> None:
    graph = _graph()
    for kind in ("defeaterRebutting", "defeaterUndercutting"):
        assert (_g(kind), RDF.type, _g("DefeaterKind")) in graph


# --------------------------------------------------------------------------- #
# Property domains / ranges / characteristics.
# --------------------------------------------------------------------------- #


def test_flat_spine_properties_domain_claim() -> None:
    """The 80% case hangs on the conclusion-claim, not on a reified node."""
    graph = _graph()
    for prop in ("inferredFrom", "inferenceMode"):
        assert (_g(prop), RDFS.domain, _g("StandpointClaim")) in graph


def test_reified_slots_domain_commitment() -> None:
    graph = _graph()
    for prop in ("premise", "conclusion", "inferenceModeOf", "warrant"):
        assert (_g(prop), RDFS.domain, _g("InferenceCommitment")) in graph


def test_bridge_links_process_to_commitment() -> None:
    graph = _graph()
    assert (_g("hasInferenceCommitment"), RDFS.domain, _g("InferenceProcess")) in graph
    assert (
        _g("hasInferenceCommitment"),
        RDFS.range,
        _g("InferenceCommitment"),
    ) in graph


def test_functional_properties() -> None:
    graph = _graph()
    for prop in (
        "conclusion",
        "inferenceModeOf",
        "correspondingSource",
        "correspondingTarget",
        "tenureOf",
    ):
        assert (_g(prop), RDF.type, OWL.FunctionalProperty) in graph, prop


def test_competes_with_is_symmetric_claim_to_claim() -> None:
    graph = _graph()
    cw = _g("competesWith")
    assert (cw, RDF.type, OWL.SymmetricProperty) in graph
    assert (cw, RDFS.domain, _g("StandpointClaim")) in graph
    assert (cw, RDFS.range, _g("StandpointClaim")) in graph
    # Irreflexivity is enforced in SHACL, NOT as an OWL axiom (DL-clean).
    assert (cw, RDF.type, OWL.IrreflexiveProperty) not in graph


def test_conclusion_ranges_over_standpoint_claim() -> None:
    graph = _graph()
    assert (_g("conclusion"), RDFS.range, _g("StandpointClaim")) in graph


def test_solver_layer_scores_are_decimal() -> None:
    graph = _graph()
    xsd_decimal = URIRef("http://www.w3.org/2001/XMLSchema#decimal")
    for prop in ("explanatoryScore", "systematicity"):
        assert (_g(prop), RDFS.range, xsd_decimal) in graph


# --------------------------------------------------------------------------- #
# SHACL — well-formed conforms, malformed is flagged (against slice shapes).
# --------------------------------------------------------------------------- #


def _data(instance_ttl: str) -> Graph:
    """Slice module (for class/property defs) + an inline instance graph."""
    g = Graph()
    g.parse(_MODULE, format="turtle")
    g.parse(data=instance_ttl, format="turtle")
    return g


# Self-contained fixtures: the ObservationMethod individual is typed locally so
# the central CoreObservationMethodShape (which requires every StandpointClaim to
# carry exactly one gmeow:observationMethod of class gmeow:ObservationMethod) is
# satisfied without loading the full merged graph — keeping these guards fast.
# Example conformance against the real method individuals is covered by
# `make validate` (check_examples over the merged graph), the cognition precedent.
_PRELUDE = """
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <http://example.org/inf/> .
ex:methodReason a gmeow:ObservationMethod .
"""

_WELLFORMED = (
    _PRELUDE
    + """
ex:p1 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:concl a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:commit a gmeow:InferenceCommitment ;
    gmeow:premise ex:p1 ;
    gmeow:conclusion ex:concl ;
    gmeow:inferenceModeOf gmeow:modeDeduction .
ex:h1 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason ;
    gmeow:competesWith ex:h2 .
ex:h2 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
"""
)

_MALFORMED = (
    _PRELUDE
    + """
ex:claimX a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
# premise == conclusion: an argument cannot assume what it proves
ex:badCommit a gmeow:InferenceCommitment ;
    gmeow:premise ex:claimX ;
    gmeow:conclusion ex:claimX .
# self-competition: competesWith must be irreflexive
ex:selfRival a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason ;
    gmeow:competesWith ex:selfRival .
"""
)


def test_wellformed_commitment_conforms() -> None:
    result = run_shacl(_data(_WELLFORMED), shapes_path=_SHAPES)
    assert result.ok, result.errors


def test_malformed_commitment_is_flagged() -> None:
    result = run_shacl(_data(_MALFORMED), shapes_path=_SHAPES)
    assert not result.ok
    blob = " ".join(result.errors)
    assert "assume what it proves" in blob  # premise == conclusion
    assert "irreflexive" in blob  # self-competition


def test_all_examples_parse() -> None:
    """Every worked example is syntactically valid Turtle (full SHACL
    conformance against the real method individuals is enforced by
    `make validate` / check_examples over the merged graph)."""
    examples = sorted((_SLICE / "examples").glob("*.ttl"))
    assert len(examples) == 5
    for example in examples:
        Graph().parse(example, format="turtle")
