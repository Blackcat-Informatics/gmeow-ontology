"""Focused tests for annotation completeness across all GMEOW terms.

Issue #221 strengthened the annotation-completeness gate so that every
GMEOW-namespaced ontology header, class, property, annotation property,
datatype, and individual must carry rdfs:label, skos:definition, and
rdfs:isDefinedBy.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, RDFS, SKOS, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL

from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    PROJECT_ROOT,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import load_merged_graph

# Graph-accepting shim: serialize a synthetic (or the real merged) rdflib graph
# and route it through the graph-free production structural lint (#579).
from tests._graph_nt import structural_lint

_TEST_ROLE = URIRef("https://example.org/boxTBox")


def _add_test_role(graph: Graph, term: URIRef) -> None:
    graph.add((_TEST_ROLE, RDF.type, URIRef(NAMESPACE + "GraphBoxRole")))
    graph.add((term, URIRef(NAMESPACE + "graphBoxRole"), _TEST_ROLE))


def _assert_lint_ok(graph: Graph, message: str) -> None:
    """Run structural_lint on *graph* and assert it passes with a readable msg."""
    result = structural_lint(graph)
    assert result.ok, (
        f"{message}\nErrors:\n" + "\n".join(result.errors) if result.errors else message
    )


def _dsl_vocabulary_graph(path: Path) -> Graph:
    """Parse a DSL vocabulary with the kernel role definitions it references."""
    graph = Graph()
    graph.parse(PROJECT_ROOT / "slices" / "core" / "kernel" / "module.ttl")
    graph.parse(path, format="turtle")
    return graph


def test_merged_ontology_has_no_missing_annotations() -> None:
    """Every GMEOW term in the merged ontology carries the required triple."""
    _assert_lint_ok(load_merged_graph(), "Merged ontology has missing annotations")


def test_structural_lint_flags_missing_label_definition_and_isdefinedby() -> None:
    """Missing any of the three required annotations is an error (issue #221)."""
    graph = Graph()
    bad = URIRef(NAMESPACE + "Undocumented")
    graph.add((bad, RDF.type, OWL.Class))
    # deliberately omit all three annotation properties

    result = structural_lint(graph)
    assert not result.ok
    messages = "\n".join(result.errors)
    assert "rdfs:label" in messages
    assert "skos:definition" in messages
    assert "rdfs:isDefinedBy" in messages


def test_structural_lint_covers_individuals() -> None:
    """Individuals are in scope for the annotation-completeness gate."""
    graph = Graph()
    individual = URIRef(NAMESPACE + "SampleIndividual")
    graph.add((individual, RDF.type, URIRef(NAMESPACE + "SomeClass")))
    graph.add((individual, RDFS.label, Literal("Sample")))
    graph.add((individual, SKOS.definition, Literal("A sample individual.")))
    _add_test_role(graph, individual)
    # missing rdfs:isDefinedBy

    result = structural_lint(graph)
    assert not result.ok
    assert any("rdfs:isDefinedBy" in e and "individual" in e for e in result.errors)


def test_structural_lint_covers_annotation_properties() -> None:
    """Annotation properties are in scope for the annotation-completeness gate."""
    graph = Graph()
    prop = URIRef(NAMESPACE + "sampleAnnotationProperty")
    graph.add((prop, RDF.type, OWL.AnnotationProperty))
    graph.add((prop, RDFS.label, Literal("Sample")))
    graph.add((prop, SKOS.definition, Literal("A sample annotation property.")))
    graph.add((prop, RDFS.isDefinedBy, URIRef(ONTOLOGY_IRI)))
    _add_test_role(graph, prop)

    assert structural_lint(graph).ok


def test_mapping_dsl_vocabulary_has_no_missing_annotations() -> None:
    """Every vocabulary term in mapping-dsl/vocabulary.ttl is fully annotated.

    Only the vocabulary file is checked; example and mapping cells elsewhere in
    the DSL are not vocabulary terms and are therefore out of scope.
    """
    graph = _dsl_vocabulary_graph(MAPPING_DSL_DIR / "vocabulary.ttl")
    _assert_lint_ok(graph, "Mapping DSL vocabulary has missing annotations")


def test_statement_dsl_vocabulary_has_no_missing_annotations() -> None:
    """Every vocabulary term in dsl/statements/vocabulary.ttl is fully annotated.

    Only the vocabulary file is checked; example cells elsewhere in the DSL are
    not vocabulary terms and are therefore out of scope.
    """
    graph = _dsl_vocabulary_graph(STATEMENT_DSL_DIR / "vocabulary.ttl")
    _assert_lint_ok(graph, "Statement DSL vocabulary has missing annotations")
