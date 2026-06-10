"""Tests for syntax checking and structural lint."""

from __future__ import annotations

from rdflib import RDFS, Graph, Literal, URIRef
from rdflib.namespace import OWL, SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.validate import check_syntax, structural_lint, validate_all


def test_check_syntax_on_sources() -> None:
    assert check_syntax().ok


def test_validate_all_passes_on_skeleton() -> None:
    # Full pure-Python validation (syntax + lint + SHACL) over the real sources.
    assert validate_all().ok


def test_structural_lint_flags_missing_annotations() -> None:
    graph = Graph()
    bad = URIRef(NAMESPACE + "Undocumented")
    graph.add((bad, RDFS.subClassOf, OWL.Thing))
    graph.add((bad, RDFS.label, Literal("x")))  # has label, missing definition
    graph.add((bad, RDFS.isDefinedBy, URIRef(NAMESPACE)))
    graph.add((bad, __import__("rdflib").RDF.type, OWL.Class))

    result = structural_lint(graph)
    assert any("skos:definition" in e for e in result.errors)


def test_structural_lint_clean_for_well_formed_term() -> None:
    graph = Graph()
    good = URIRef(NAMESPACE + "Documented")
    from rdflib import RDF

    graph.add((good, RDF.type, OWL.Class))
    graph.add((good, RDFS.label, Literal("Documented")))
    graph.add((good, SKOS.definition, Literal("A well-formed term.")))
    graph.add((good, RDFS.isDefinedBy, URIRef(NAMESPACE)))

    assert structural_lint(graph).ok


def test_structural_lint_accepts_mixed_case_private_language_tag() -> None:
    graph = Graph()
    graph.add(
        (
            URIRef("https://example.org/name"),
            URIRef(NAMESPACE + "fullName"),
            Literal("Japanese", lang="x-GMEOW-Japanese"),
        )
    )

    assert structural_lint(graph).ok


def test_structural_lint_rejects_external_language_tag_on_gmeow_property() -> None:
    graph = Graph()
    graph.add(
        (
            URIRef("https://example.org/name"),
            URIRef(NAMESPACE + "fullName"),
            Literal("Japanese", lang="ja"),
        )
    )

    result = structural_lint(graph)
    assert not result.ok
    assert any("external or invalid language tag" in err for err in result.errors)


def test_structural_lint_rejects_en_on_gmeow_label() -> None:
    graph = Graph()
    term = URIRef(NAMESPACE + "TestTerm")
    from rdflib import RDF

    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDFS.label, Literal("Name", lang="en")))
    graph.add((term, SKOS.definition, Literal("A test term.")))
    graph.add((term, RDFS.isDefinedBy, URIRef(NAMESPACE)))

    result = structural_lint(graph)
    assert not result.ok
    assert any(
        "external language tag 'en'" in err and "label" in err for err in result.errors
    )


def test_structural_lint_accepts_x_gmeow_english_on_label() -> None:
    graph = Graph()
    term = URIRef(NAMESPACE + "TestTerm")
    from rdflib import RDF

    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDFS.label, Literal("Name", lang="x-gmeow-english")))
    graph.add((term, SKOS.definition, Literal("A test term.", lang="x-gmeow-english")))
    graph.add((term, RDFS.isDefinedBy, URIRef(NAMESPACE)))

    assert structural_lint(graph).ok
