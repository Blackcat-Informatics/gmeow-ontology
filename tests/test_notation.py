"""Structural + DL-safety guards for the notation and symbolic systems building
block (#172).

Pins the sibling hierarchy (SymbolicSystem and NotationSystem alongside
Language and WritingSystem), the reified NotationSystemUsage relator and its
functional roles, the value-vs-subclass decisions, and the boundary invariants:
- a notation system is not a language by default
- a formal language is not a notation system by default
- mathematical and musical notation are not natural languages
- stenography and cryptographic encoding are notation systems
- ambiguous cases (IPA, MusicXML, MathML, MIDI) are co-modelable via standpoint
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Hierarchy
# --------------------------------------------------------------------------- #


def test_symbolic_system_hierarchy() -> None:
    graph = _graph()
    # SymbolicSystem is a Kind under InformationObject
    assert (
        URIRef(GMEOW + "SymbolicSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    assert (
        URIRef(GMEOW + "SymbolicSystem"),
        RDF.type,
        URIRef(GUFO + "Kind"),
    ) in graph
    # NotationSystem is a SubKind under SymbolicSystem
    assert (
        URIRef(GMEOW + "NotationSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "SymbolicSystem"),
    ) in graph
    assert (
        URIRef(GMEOW + "NotationSystem"),
        RDF.type,
        URIRef(GUFO + "SubKind"),
    ) in graph


def test_writing_system_is_sibling_not_subclass() -> None:
    """WritingSystem remains a sibling to SymbolicSystem, not a subclass."""
    graph = _graph()
    # WritingSystem is under InformationObject
    assert (
        URIRef(GMEOW + "WritingSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    # But NOT under SymbolicSystem
    assert (
        URIRef(GMEOW + "WritingSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "SymbolicSystem"),
    ) not in graph


def test_language_is_sibling_not_subclass_of_symbolic() -> None:
    """Language remains a sibling to SymbolicSystem, not a subclass."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "Language"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    assert (
        URIRef(GMEOW + "Language"),
        RDFS.subClassOf,
        URIRef(GMEOW + "SymbolicSystem"),
    ) not in graph


# --------------------------------------------------------------------------- #
# Reified relator and functional roles
# --------------------------------------------------------------------------- #


def test_notation_system_usage_relor() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "NotationSystemUsage"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
    for role in (
        "notationUsageTarget",
        "notationUsageNotationSystem",
        "notationUsageRole",
        "notationUsageInterval",
    ):
        node = URIRef(GMEOW + role)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


# --------------------------------------------------------------------------- #
# Boundary: notation system is NOT a language by default
# --------------------------------------------------------------------------- #


def test_notation_system_not_subclass_of_language() -> None:
    """A NotationSystem is not a Language — the boundary is explicit."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "NotationSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Language"),
    ) not in graph


def test_formal_language_not_subclass_of_notation() -> None:
    """A FormalLanguage is not a NotationSystem — grammar-defined languages are
    structurally distinct from representational notations."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "FormalLanguage"),
        RDFS.subClassOf,
        URIRef(GMEOW + "NotationSystem"),
    ) not in graph


# --------------------------------------------------------------------------- #
# Value vocabularies are not subclasses
# --------------------------------------------------------------------------- #


def test_value_vocabularies_not_subclasses() -> None:
    graph = _graph()
    for value_type in ("SymbolicSystemKind", "NotationUsageRole"):
        node = URIRef(GMEOW + value_type)
        assert (node, RDF.type, OWL.Class) in graph
        assert (node, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
        # No subclasses of Entity
        for sub in graph.subjects(RDFS.subClassOf, node):
            assert sub == node, f"unexpected subclass of {value_type}: {sub}"


# --------------------------------------------------------------------------- #
# Edge cases: stenography and cryptographic encoding
# --------------------------------------------------------------------------- #


def test_stenography_is_notation_not_language() -> None:
    """Stenography is modeled as a notation system (shorthand for a language),
    not as a language itself."""
    graph = _graph()
    # The kind individual exists
    assert (
        URIRef(GMEOW + "symbolicKindStenographic"),
        RDF.type,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) in graph
    # Stenography is under SymbolicSystemKind, not LanguageOrigin
    assert (
        URIRef(GMEOW + "symbolicKindStenographic"),
        RDF.type,
        URIRef(GMEOW + "LanguageOrigin"),
    ) not in graph


def test_cryptography_is_notation_not_language() -> None:
    """Cryptographic ciphers are notation systems (transform schemes), not
    languages by default."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "symbolicKindCryptographic"),
        RDF.type,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) in graph
    assert (
        URIRef(GMEOW + "symbolicKindCryptographic"),
        RDF.type,
        URIRef(GMEOW + "LanguageOrigin"),
    ) not in graph


# --------------------------------------------------------------------------- #
# Ambiguous boundary cases
# --------------------------------------------------------------------------- #


def test_ipa_is_notation_not_natural_language() -> None:
    """IPA is a transcription notation system, not a natural language."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "symbolicKindTranscription"),
        RDF.type,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) in graph


def test_math_notation_not_natural_language() -> None:
    """Mathematical notation is a notation system, not inferred as a natural
    language."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "symbolicKindMathematical"),
        RDF.type,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) in graph
    # Mathematical notation is NOT a LanguageOrigin
    assert (
        URIRef(GMEOW + "symbolicKindMathematical"),
        RDF.type,
        URIRef(GMEOW + "LanguageOrigin"),
    ) not in graph


def test_musical_notation_not_natural_language() -> None:
    """Musical notation is a notation system, not inferred as a natural
    language."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "symbolicKindMusical"),
        RDF.type,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) in graph
    assert (
        URIRef(GMEOW + "symbolicKindMusical"),
        RDF.type,
        URIRef(GMEOW + "LanguageOrigin"),
    ) not in graph


def test_ambiguous_cases_co_modelable() -> None:
    """Ambiguous systems (MusicXML, MathML, MIDI, ABC) can be co-modeled as
    both FormalLanguage and NotationSystem through standpoint-indexed claims
    (Principle 9). The ontology provides the classes; the standpoint layer
    resolves the classification."""
    graph = _graph()
    # Both formal-language and notation-system kinds exist
    assert (
        URIRef(GMEOW + "originFormal"),
        RDF.type,
        URIRef(GMEOW + "LanguageOrigin"),
    ) in graph
    assert (
        URIRef(GMEOW + "symbolicKindMusical"),
        RDF.type,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) in graph
    # They are distinct value vocabularies — no inferential bridge
    assert (
        URIRef(GMEOW + "LanguageOrigin"),
        RDFS.subClassOf,
        URIRef(GMEOW + "SymbolicSystemKind"),
    ) not in graph
    assert (
        URIRef(GMEOW + "SymbolicSystemKind"),
        RDFS.subClassOf,
        URIRef(GMEOW + "LanguageOrigin"),
    ) not in graph


# --------------------------------------------------------------------------- #
# Bridging properties
# --------------------------------------------------------------------------- #


def test_has_notation_system_property_exists() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasNotationSystem")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (
        node,
        RDFS.domain,
        URIRef(GMEOW + "Language"),
    ) in graph
    assert (
        node,
        RDFS.range,
        URIRef(GMEOW + "NotationSystem"),
    ) in graph


def test_writing_system_as_notation_property_exists() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "writingSystemAsNotation")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (
        node,
        RDFS.domain,
        URIRef(GMEOW + "WritingSystem"),
    ) in graph
    assert (
        node,
        RDFS.range,
        URIRef(GMEOW + "NotationSystem"),
    ) in graph


def test_notation_system_kind_is_subproperty() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "notationSystemKind"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "symbolicSystemKind"),
    ) in graph
