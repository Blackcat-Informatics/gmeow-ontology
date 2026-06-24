"""SHACL + cross-slice structural guards for the notes & annotation building block.

TBox invariants for notes-module subjects (Note/Annotation/Highlight/Bookmark/
Comment hierarchy, annotation roles, comment threading, backlink graph, motivation
vocabulary, orthogonality and selector-reuse guards) have been migrated to the
declarative slicetest DSL:
    slices/extensions/notes/tests/structural.ttl

Retained here (not migratable as module-scoped ASK cells):
  - test_evidence_span_is_information_object: EvidenceSpan subject is in the
    evidencespan slice, not in notes/module.ttl (cross-slice).
  - test_selector_sub_class_of_evidence_span: Selector subject is in the
    evidencespan slice (cross-slice).
  - test_notes_are_standpoint_indexed: accordingTo subject is in the standpoint
    slice (cross-slice).
  - test_motivation_values_are_individuals: the len(...)==10 count assertion is
    a dynamic whole-graph numeric check; the 10 seed individuals and banned-class
    guards are covered in structural.ttl cells sa10MotivationSeeds and
    saMotivationNotClasses.
  - All projection SPARQL parse tests (generated-artifact, numeric/parse check).

SHACL instance tests migrated to crates/validate/tests/conformance_notes.rs (#867).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.config import PROJECTION_QUERY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
_PROJ_Q = PROJECTION_QUERY_DIR


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# =========================================================================== #
# Cross-slice structural guards (subjects not in notes/module.ttl)
# =========================================================================== #


def test_evidence_span_is_information_object() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "EvidenceSpan"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph


def test_selector_sub_class_of_evidence_span() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Selector"),
        RDFS.subClassOf,
        URIRef(GMEOW + "EvidenceSpan"),
    ) in graph


# =========================================================================== #
# Open value vocabulary (dynamic count -- not expressible as scopeModule ASK)
# =========================================================================== #


def test_motivation_values_are_individuals() -> None:
    # Seed-existence and not-class guards are covered in structural.ttl cells
    # sa10MotivationSeeds and saMotivationNotClasses. Retained here only for
    # the len(...)==10 dynamic count gate.
    graph = _graph()
    motivation_class = URIRef(GMEOW + "AnnotationMotivation")
    assert len(set(graph.subjects(RDF.type, motivation_class))) == 10


# =========================================================================== #
# Standpoint availability (accordingTo is cross-slice -- standpoint module)
# =========================================================================== #


def test_notes_are_standpoint_indexed() -> None:
    """The standpoint machinery (accordingTo) is available on notes via the
    statement/provenance layer; the TBox does not forbid it."""
    g = _graph()
    assert (URIRef(GMEOW + "accordingTo"), RDF.type, OWL.AnnotationProperty) in g


# =========================================================================== #
# Projection round-trip / parse tests
# =========================================================================== #


def _sparql_parse(path: Path) -> None:
    """Minimal parse guard — the query must be syntactically valid SPARQL."""
    import sys

    from gmeow_rdf.compat.rdflib.plugins.sparql import prepareQuery

    # Large projection CONSTRUCT queries push pyparsing past the default limit.
    old = sys.getrecursionlimit()
    sys.setrecursionlimit(3000)
    try:
        text = path.read_text()
        prepareQuery(text)
    finally:
        sys.setrecursionlimit(old)


def test_notes_oa_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "web-annotation.rq")


def test_notes_schema_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "schema-org.rq")


def test_notes_as_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "activitystreams.rq")


def test_notes_markdown_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "markdown.rq")
