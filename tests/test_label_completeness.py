"""Focused tests for annotation completeness across all GMEOW terms.

Issue #221 strengthened the annotation-completeness gate so that every
GMEOW-namespaced ontology header, class, property, annotation property,
datatype, and individual must carry rdfs:label, skos:definition, and
rdfs:isDefinedBy.
"""

from __future__ import annotations

from rdflib import RDF, RDFS, SKOS, Graph, Literal, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import structural_lint


def _gmeow_terms(graph: Graph) -> set[URIRef]:
    """Return every GMEOW-namespaced term that has an rdf:type."""
    terms: set[URIRef] = set()
    for term in set(graph.subjects()):
        if not isinstance(term, URIRef):
            continue
        s = str(term)
        if not (s.startswith(NAMESPACE) or s == ONTOLOGY_IRI):
            continue
        if any(graph.objects(term, RDF.type)):
            terms.add(term)
    return terms


def test_merged_ontology_has_no_missing_annotations() -> None:
    """Every GMEOW term in the merged ontology carries the required triple."""
    graph = load_merged_graph()
    missing: list[str] = []
    for term in _gmeow_terms(graph):
        if (term, RDFS.label, None) not in graph:
            missing.append(f"{term} missing rdfs:label")
        if (term, SKOS.definition, None) not in graph:
            missing.append(f"{term} missing skos:definition")
        if (term, RDFS.isDefinedBy, None) not in graph:
            missing.append(f"{term} missing rdfs:isDefinedBy")
    assert not missing, "Missing annotations:\n" + "\n".join(missing)


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

    assert structural_lint(graph).ok


def test_mapping_dsl_vocabulary_has_no_missing_annotations() -> None:
    """Every vocabulary term in mapping-dsl/vocabulary.ttl is fully annotated.

    Only the vocabulary file is checked; example and mapping cells elsewhere in
    the DSL are not vocabulary terms and are therefore out of scope.
    """
    graph = Graph()
    graph.parse(MAPPING_DSL_DIR / "vocabulary.ttl", format="turtle")

    missing: list[str] = []
    for term in _gmeow_terms(graph):
        if (term, RDFS.label, None) not in graph:
            missing.append(f"{term} missing rdfs:label")
        if (term, SKOS.definition, None) not in graph:
            missing.append(f"{term} missing skos:definition")
        if (term, RDFS.isDefinedBy, None) not in graph:
            missing.append(f"{term} missing rdfs:isDefinedBy")
    assert not missing, "Missing annotations in mapping DSL vocabulary:\n" + "\n".join(
        missing
    )


def test_statement_dsl_vocabulary_has_no_missing_annotations() -> None:
    """Every vocabulary term in statement-dsl/vocabulary.ttl is fully annotated.

    Only the vocabulary file is checked; example cells elsewhere in the DSL are
    not vocabulary terms and are therefore out of scope.
    """
    graph = Graph()
    graph.parse(STATEMENT_DSL_DIR / "vocabulary.ttl", format="turtle")

    missing: list[str] = []
    for term in _gmeow_terms(graph):
        if (term, RDFS.label, None) not in graph:
            missing.append(f"{term} missing rdfs:label")
        if (term, SKOS.definition, None) not in graph:
            missing.append(f"{term} missing skos:definition")
        if (term, RDFS.isDefinedBy, None) not in graph:
            missing.append(f"{term} missing rdfs:isDefinedBy")
    assert not missing, (
        "Missing annotations in statement DSL vocabulary:\n" + "\n".join(missing)
    )
