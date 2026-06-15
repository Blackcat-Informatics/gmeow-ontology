"""Tests for syntax checking and structural lint."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDFS, Graph, Literal, URIRef
from rdflib.namespace import OWL, SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.validate import (
    ValidationResult,
    _read_cached_result,
    _write_cached_result,
    check_sameas_ban,
    check_syntax,
    structural_lint,
    validate_all,
)


def test_check_syntax_on_sources() -> None:
    assert check_syntax().ok


def test_validate_all_passes_on_skeleton() -> None:
    # Full pure-Python validation (syntax + lint + SHACL) over the real sources.
    assert validate_all().ok


def test_cached_validation_result_write_replaces_cleanly(tmp_path: Path) -> None:
    path = tmp_path / "result.json"

    _write_cached_result(path, ValidationResult(errors=["old"], warnings=[]))
    _write_cached_result(path, ValidationResult(errors=[], warnings=["new"]))

    cached = _read_cached_result(path)
    assert cached is not None
    assert cached.errors == []
    assert cached.warnings == ["new"]
    assert not list(tmp_path.glob("*.tmp"))


def test_cached_validation_result_ignores_non_object_payload(tmp_path: Path) -> None:
    path = tmp_path / "result.json"
    path.write_text("[]", encoding="utf-8")

    assert _read_cached_result(path) is None


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


# --------------------------------------------------------------------------- #
# owl:sameAs ban (Principle 5)
# --------------------------------------------------------------------------- #


def test_check_sameas_ban_rejects_external_sameas(tmp_path: Path) -> None:
    path = tmp_path / "bad.ttl"
    path.write_text(
        """
    @prefix ex: <https://example.org/> .
    @prefix owl: <http://www.w3.org/2002/07/owl#> .
    ex:a owl:sameAs ex:b .
    """,
        encoding="utf-8",
    )
    result = check_sameas_ban([path])
    assert not result.ok
    assert any("banned owl:sameAs to external entity" in e for e in result.errors)


def test_check_sameas_ban_allows_internal_sameas(tmp_path: Path) -> None:
    path = tmp_path / "ok.ttl"
    path.write_text(
        f"""
    @prefix gmeow: <{NAMESPACE}> .
    @prefix owl: <http://www.w3.org/2002/07/owl#> .
    gmeow:A owl:sameAs gmeow:B .
    """,
        encoding="utf-8",
    )
    result = check_sameas_ban([path])
    assert result.ok


def test_check_sameas_ban_respects_allowlist(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "allowed.ttl"
    path.write_text(
        """
    @prefix ex: <https://example.org/> .
    @prefix owl: <http://www.w3.org/2002/07/owl#> .
    ex:a owl:sameAs ex:b .
    """,
        encoding="utf-8",
    )
    monkeypatch.setattr(
        "gmeow_tools.validate._SAMEAS_ALLOWLIST",
        frozenset({("https://example.org/a", "https://example.org/b")}),
    )
    result = check_sameas_ban([path])
    assert result.ok


def test_check_sameas_ban_rejects_empty_paths() -> None:
    """An explicitly empty paths list is a caller bug — fail fast, not silently."""
    with pytest.raises(ValueError, match="must not be empty"):
        check_sameas_ban([])
