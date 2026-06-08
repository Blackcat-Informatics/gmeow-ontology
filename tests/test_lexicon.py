"""Structural + DL-safety guards for the lexicon layer (#171).

Pins the evidence-vs-claim separation: UsageAttestation is evidence, not truth;
EtymologicalDerivation is a standpointed claim graph, not a flat property.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
GM = Namespace(GMEOW)
GUFO_NS = Namespace(GUFO)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# LexicalItem hierarchy
# --------------------------------------------------------------------------- #


def test_lexical_item_is_information_object() -> None:
    g = _graph()
    assert (GM.LexicalItem, RDFS.subClassOf, GM.InformationObject) in g


def test_lexical_form_is_information_object() -> None:
    g = _graph()
    assert (GM.LexicalForm, RDFS.subClassOf, GM.InformationObject) in g


def test_has_lexical_form_is_non_functional() -> None:
    g = _graph()
    assert (GM.hasLexicalForm, RDF.type, OWL.ObjectProperty) in g
    assert (GM.hasLexicalForm, RDF.type, OWL.FunctionalProperty) not in g


def test_lexical_item_language_is_functional() -> None:
    g = _graph()
    assert (GM.lexicalItemLanguage, RDF.type, OWL.FunctionalProperty) in g


# --------------------------------------------------------------------------- #
# LexicalForm properties
# --------------------------------------------------------------------------- #


def test_form_representation_is_functional() -> None:
    g = _graph()
    assert (GM.formRepresentation, RDF.type, OWL.FunctionalProperty) in g


def test_form_type_is_non_functional() -> None:
    g = _graph()
    assert (GM.formType, RDF.type, OWL.ObjectProperty) in g
    assert (GM.formType, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# UsageAttestation relator pattern
# --------------------------------------------------------------------------- #


def test_usage_attestation_is_observation_relator() -> None:
    g = _graph()
    assert (GM.UsageAttestation, RDF.type, OWL.Class) in g
    assert (GM.UsageAttestation, RDFS.subClassOf, GM.Observation) in g
    assert (GM.UsageAttestation, RDFS.subClassOf, GUFO_NS.Relator) in g


def test_attested_form_is_functional() -> None:
    g = _graph()
    assert (GM.attestedForm, RDF.type, OWL.FunctionalProperty) in g


# --------------------------------------------------------------------------- #
# EtymologicalDerivation relator pattern
# --------------------------------------------------------------------------- #


def test_etymological_derivation_is_observation_relator() -> None:
    g = _graph()
    assert (GM.EtymologicalDerivation, RDF.type, OWL.Class) in g
    assert (GM.EtymologicalDerivation, RDFS.subClassOf, GM.Observation) in g
    assert (GM.EtymologicalDerivation, RDFS.subClassOf, GUFO_NS.Relator) in g


def test_derivation_source_target_are_functional() -> None:
    g = _graph()
    assert (GM.derivationSource, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.derivationTarget, RDF.type, OWL.FunctionalProperty) in g


def test_derivation_kind_is_non_functional() -> None:
    g = _graph()
    assert (GM.derivationKind, RDF.type, OWL.ObjectProperty) in g
    assert (GM.derivationKind, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# Value vocabularies
# --------------------------------------------------------------------------- #


def test_form_type_value_vocabulary() -> None:
    g = _graph()
    assert (GM.LexicalFormType, RDFS.subClassOf, GUFO_NS.QualityValue) in g
    for individual in (
        "formWritten",
        "formSpoken",
        "formReconstructed",
        "formTransliterated",
    ):
        assert (GM[individual], RDF.type, GM.LexicalFormType) in g


def test_derivation_kind_value_vocabulary() -> None:
    g = _graph()
    assert (GM.DerivationKind, RDFS.subClassOf, GUFO_NS.QualityValue) in g
    for individual in (
        "derivationBorrowing",
        "derivationSemanticShift",
        "derivationCompounding",
        "derivationUnknownOrigin",
    ):
        assert (GM[individual], RDF.type, GM.DerivationKind) in g
