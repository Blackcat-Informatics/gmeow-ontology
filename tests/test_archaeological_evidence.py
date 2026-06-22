"""Structural + DL-safety guards for the archaeological evidence layer (#173).

Pins the layer separation: physical carrier, inscription, reading,
transliteration, translation, dating, find context, and linguistic interpretation
are all distinct nodes. Competing readings, transliterations, translations, and
script/language attributions coexist without a single winner (Principle 9).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
GM = Namespace(GMEOW)
GUFO_NS = Namespace(GUFO)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Class hierarchy
# --------------------------------------------------------------------------- #


def test_inscription_is_information_object() -> None:
    g = _graph()
    assert (GM.Inscription, RDF.type, OWL.Class) in g
    assert (GM.Inscription, RDFS.subClassOf, GM.InformationObject) in g


def test_inscription_reading_is_observation_relator() -> None:
    g = _graph()
    assert (GM.InscriptionReading, RDF.type, OWL.Class) in g
    assert (GM.InscriptionReading, RDFS.subClassOf, GM.Observation) in g
    assert (GM.InscriptionReading, RDFS.subClassOf, GUFO_NS.Relator) in g


def test_inscription_transliteration_is_observation_relator() -> None:
    g = _graph()
    assert (GM.InscriptionTransliteration, RDF.type, OWL.Class) in g
    assert (GM.InscriptionTransliteration, RDFS.subClassOf, GM.Observation) in g
    assert (GM.InscriptionTransliteration, RDFS.subClassOf, GUFO_NS.Relator) in g


def test_inscription_translation_is_observation_relator() -> None:
    g = _graph()
    assert (GM.InscriptionTranslation, RDF.type, OWL.Class) in g
    assert (GM.InscriptionTranslation, RDFS.subClassOf, GM.Observation) in g
    assert (GM.InscriptionTranslation, RDFS.subClassOf, GUFO_NS.Relator) in g


def test_script_language_attribution_is_observation_relator() -> None:
    g = _graph()
    assert (GM.ScriptLanguageAttribution, RDF.type, OWL.Class) in g
    assert (GM.ScriptLanguageAttribution, RDFS.subClassOf, GM.Observation) in g
    assert (GM.ScriptLanguageAttribution, RDFS.subClassOf, GUFO_NS.Relator) in g


def test_archaeological_find_context_is_observation_relator() -> None:
    g = _graph()
    assert (GM.ArchaeologicalFindContext, RDF.type, OWL.Class) in g
    assert (GM.ArchaeologicalFindContext, RDFS.subClassOf, GM.Observation) in g
    assert (GM.ArchaeologicalFindContext, RDFS.subClassOf, GUFO_NS.Relator) in g


# --------------------------------------------------------------------------- #
# Inscription ↔ carrier properties
# --------------------------------------------------------------------------- #


def test_inscription_carrier_is_functional() -> None:
    g = _graph()
    assert (GM.inscriptionCarrier, RDF.type, OWL.FunctionalProperty) in g


def test_carrier_inscription_is_non_functional() -> None:
    g = _graph()
    assert (GM.carrierInscription, RDF.type, OWL.ObjectProperty) in g
    assert (GM.carrierInscription, RDF.type, OWL.FunctionalProperty) not in g


def test_carrier_type_is_non_functional() -> None:
    g = _graph()
    assert (GM.carrierType, RDF.type, OWL.ObjectProperty) in g
    assert (GM.carrierType, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# Reading / transliteration / translation properties
# --------------------------------------------------------------------------- #


def test_reading_of_is_functional() -> None:
    g = _graph()
    assert (GM.readingOf, RDF.type, OWL.FunctionalProperty) in g


def test_reading_result_is_non_functional() -> None:
    g = _graph()
    assert (GM.readingResult, RDF.type, OWL.ObjectProperty) in g
    assert (GM.readingResult, RDF.type, OWL.FunctionalProperty) not in g


def test_transliteration_of_is_functional() -> None:
    g = _graph()
    assert (GM.transliterationOf, RDF.type, OWL.FunctionalProperty) in g


def test_transliteration_result_is_non_functional() -> None:
    g = _graph()
    assert (GM.transliterationResult, RDF.type, OWL.ObjectProperty) in g
    assert (GM.transliterationResult, RDF.type, OWL.FunctionalProperty) not in g


def test_translation_of_is_functional() -> None:
    g = _graph()
    assert (GM.translationOf, RDF.type, OWL.FunctionalProperty) in g


def test_translation_result_is_non_functional() -> None:
    g = _graph()
    assert (GM.translationResult, RDF.type, OWL.ObjectProperty) in g
    assert (GM.translationResult, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# ScriptLanguageAttribution properties
# --------------------------------------------------------------------------- #


def test_attribution_target_is_functional() -> None:
    g = _graph()
    assert (GM.attributionTarget, RDF.type, OWL.FunctionalProperty) in g


def test_attributed_language_is_non_functional() -> None:
    g = _graph()
    assert (GM.attributedLanguage, RDF.type, OWL.ObjectProperty) in g
    assert (GM.attributedLanguage, RDF.type, OWL.FunctionalProperty) not in g


def test_attributed_script_is_non_functional() -> None:
    g = _graph()
    assert (GM.attributedScript, RDF.type, OWL.ObjectProperty) in g
    assert (GM.attributedScript, RDF.type, OWL.FunctionalProperty) not in g


def test_attributed_notation_is_non_functional() -> None:
    g = _graph()
    assert (GM.attributedNotation, RDF.type, OWL.ObjectProperty) in g
    assert (GM.attributedNotation, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# ArchaeologicalFindContext properties
# --------------------------------------------------------------------------- #


def test_find_context_target_is_functional() -> None:
    g = _graph()
    assert (GM.findContextTarget, RDF.type, OWL.FunctionalProperty) in g


def test_find_context_place_is_non_functional() -> None:
    g = _graph()
    assert (GM.findContextPlace, RDF.type, OWL.ObjectProperty) in g
    assert (GM.findContextPlace, RDF.type, OWL.FunctionalProperty) not in g


def test_find_context_stratigraphy_is_non_functional() -> None:
    g = _graph()
    assert (GM.findContextStratigraphy, RDF.type, OWL.ObjectProperty) in g
    assert (GM.findContextStratigraphy, RDF.type, OWL.FunctionalProperty) not in g


def test_find_context_dating_is_non_functional() -> None:
    g = _graph()
    assert (GM.findContextDating, RDF.type, OWL.ObjectProperty) in g
    assert (GM.findContextDating, RDF.type, OWL.FunctionalProperty) not in g


def test_find_context_event_is_non_functional() -> None:
    g = _graph()
    assert (GM.findContextEvent, RDF.type, OWL.ObjectProperty) in g
    assert (GM.findContextEvent, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# Lexicon hook
# --------------------------------------------------------------------------- #


def test_attested_on_carrier_exists() -> None:
    g = _graph()
    assert (GM.attestedOnCarrier, RDF.type, OWL.ObjectProperty) in g
    assert (GM.attestedOnCarrier, RDF.type, OWL.FunctionalProperty) not in g
    assert (GM.attestedOnCarrier, RDFS.domain, GM.UsageAttestation) in g
    assert (GM.attestedOnCarrier, RDFS.range, GM.PhysicalObject) in g


# --------------------------------------------------------------------------- #
# Observation bridge subproperties
# --------------------------------------------------------------------------- #


def test_reading_of_subsumed_under_observed_feature() -> None:
    g = _graph()
    assert (GM.readingOf, RDFS.subPropertyOf, GM.observedFeature) in g


def test_reading_result_subsumed_under_observation_result() -> None:
    g = _graph()
    assert (GM.readingResult, RDFS.subPropertyOf, GM.observationResult) in g


def test_attribution_target_subsumed_under_observed_feature() -> None:
    g = _graph()
    assert (GM.attributionTarget, RDFS.subPropertyOf, GM.observedFeature) in g


def test_find_context_target_subsumed_under_observed_feature() -> None:
    g = _graph()
    assert (GM.findContextTarget, RDFS.subPropertyOf, GM.observedFeature) in g


# --------------------------------------------------------------------------- #
# Value vocabularies
# --------------------------------------------------------------------------- #


def test_physical_carrier_type_value_vocabulary() -> None:
    g = _graph()
    assert (GM.PhysicalCarrierType, RDFS.subClassOf, GUFO_NS.QualityValue) in g
    for individual in (
        "carrierTablet",
        "carrierOstracon",
        "carrierSeal",
        "carrierCoin",
        "carrierManuscript",
        "carrierWallInscription",
        "carrierStela",
        "carrierPapyrus",
        "carrierPotterySherd",
        "carrierBone",
        "carrierMetal",
        "carrierWood",
    ):
        assert (GM[individual], RDF.type, GM.PhysicalCarrierType) in g, (
            f"{individual} must be a PhysicalCarrierType"
        )


# --------------------------------------------------------------------------- #
# No preferred/primary terms (Principle 9)
# --------------------------------------------------------------------------- #


def test_no_primary_or_preferred_archaeological_terms() -> None:
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if isinstance(s, Namespace) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"
