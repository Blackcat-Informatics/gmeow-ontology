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
  - All run_shacl() SHACL instance tests (ExampleConformance).
  - All projection SPARQL parse tests (generated-artifact, numeric/parse check).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef

from gmeow_tools.config import PROJECTION_QUERY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
EX = Namespace("https://example.org/test/")
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
# SHACL instance tests
# =========================================================================== #


def test_note_with_content_passes_shacl() -> None:
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))
    g.add((EX.note, URIRef(GMEOW + "noteContent"), Literal("A test note.")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_note_with_label_passes_shacl() -> None:
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))
    g.add((EX.note, RDFS.label, Literal("Test Note")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_note_without_content_or_label_fails_shacl() -> None:
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))

    result = run_shacl(g)
    assert not result.ok
    assert any(
        "note content" in e.lower() or "rdfs:label" in e.lower() for e in result.errors
    )


def test_annotation_without_target_fails_shacl() -> None:
    g = Graph()
    g.add((EX.ann, RDF.type, URIRef(GMEOW + "Annotation")))
    g.add(
        (
            EX.ann,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationCommenting"),
        )
    )

    result = run_shacl(g)
    assert not result.ok
    assert any("annotationtarget" in e.lower() for e in result.errors)


def test_annotation_with_target_passes_shacl() -> None:
    g = Graph()
    g.add((EX.ann, RDF.type, URIRef(GMEOW + "Annotation")))
    g.add((EX.ann, URIRef(GMEOW + "annotationTarget"), EX.doc))
    g.add(
        (
            EX.ann,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationCommenting"),
        )
    )
    g.add((EX.doc, RDF.type, URIRef(GMEOW + "Entity")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_highlight_without_selector_fails_shacl() -> None:
    g = Graph()
    g.add((EX.hl, RDF.type, URIRef(GMEOW + "Highlight")))
    g.add((EX.hl, URIRef(GMEOW + "annotationTarget"), EX.doc))
    g.add(
        (
            EX.hl,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationHighlighting"),
        )
    )
    g.add((EX.doc, RDF.type, URIRef(GMEOW + "Entity")))

    result = run_shacl(g)
    assert not result.ok
    assert any("selector" in e.lower() for e in result.errors)


def test_highlight_with_selector_passes_shacl() -> None:
    g = Graph()
    g.add((EX.hl, RDF.type, URIRef(GMEOW + "Highlight")))
    g.add((EX.hl, URIRef(GMEOW + "annotationTarget"), EX.doc))
    g.add((EX.hl, URIRef(GMEOW + "annotationTargetSpan"), EX.span))
    g.add(
        (
            EX.hl,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationHighlighting"),
        )
    )
    g.add((EX.doc, RDF.type, URIRef(GMEOW + "Entity")))
    g.add((EX.span, RDF.type, URIRef(GMEOW + "EvidenceSpan")))
    g.add((EX.span, URIRef(GMEOW + "selectorTextQuote"), Literal("highlighted text")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


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


def test_retracted_note_displayable_false() -> None:
    """A retracted note/comment sets displayable false (Principle 10)."""
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))
    g.add((EX.note, URIRef(GMEOW + "noteContent"), Literal("A retracted note.")))
    g.add(
        (
            EX.note,
            URIRef(GMEOW + "displayable"),
            Literal(
                "false", datatype=URIRef("http://www.w3.org/2001/XMLSchema#boolean")
            ),
        )
    )

    result = run_shacl(g)
    # displayable false is valid; the shape only warns when a reply's
    # parent is suppressed.
    assert result.ok, "\n".join(result.errors)
