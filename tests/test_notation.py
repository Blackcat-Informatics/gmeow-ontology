"""Retained cross-slice and dynamic guards for the notation and symbolic
systems building block (#172).

The asserted-TBox MUST / MUST-NOT invariants whose subjects are defined in
slices/core/notation/module.ttl have been migrated to declarative slicetest
cells in slices/core/notation/tests/structural.ttl (19 cells, issue #867).

RETAINED here (not migratable as module-scoped ASK cells):
- test_writing_system_is_sibling_not_subclass — gmeow:WritingSystem is a
  subject defined in another slice.
- test_language_is_sibling_not_subclass_of_symbolic — gmeow:Language is a
  subject defined in another slice.
- test_formal_language_not_subclass_of_notation — gmeow:FormalLanguage is a
  subject defined in another slice.
- test_value_vocabularies_not_subclasses — the graph.subjects() loop that
  sweeps the whole merged graph for unexpected subclasses of SymbolicSystemKind
  and NotationUsageRole is a dynamic whole-graph sweep; the static positive
  class+QualityValue assertions are covered by DSL cells 6 and 7.
- test_ambiguous_cases_co_modelable — gmeow:originFormal and
  gmeow:LanguageOrigin are subjects defined in
  slices/extensions/languages/module.ttl, not in the notation module.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Hierarchy — cross-slice siblings (subjects defined in other slices)
# --------------------------------------------------------------------------- #


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
# Boundary — cross-slice subject
# --------------------------------------------------------------------------- #


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
# Value vocabularies — dynamic whole-graph subclass sweep
# --------------------------------------------------------------------------- #


def test_value_vocabularies_not_subclasses() -> None:
    """No unexpected subclasses of SymbolicSystemKind or NotationUsageRole.

    The static positive assertions (owl:Class + gufo:QualityValue subClassOf)
    are covered by DSL cells saSymbolicSystemKindIsValueVocab and
    saNotationUsageRoleIsValueVocab in slices/core/notation/tests/structural.ttl.
    Only the dynamic whole-graph subjects sweep is retained here.
    """
    graph = _graph()
    for value_type in ("SymbolicSystemKind", "NotationUsageRole"):
        node = URIRef(GMEOW + value_type)
        # No subclasses of the value vocab (dynamic sweep — whole merged graph)
        for sub in graph.subjects(RDFS.subClassOf, node):
            assert sub == node, f"unexpected subclass of {value_type}: {sub}"


# --------------------------------------------------------------------------- #
# Ambiguous boundary cases — cross-slice subjects
# --------------------------------------------------------------------------- #


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
