"""Tests for syntax checking and structural lint."""

from __future__ import annotations

from pathlib import Path

import gmeow_docs
import gmeow_validate
import pytest
from gmeow_rdf.compat.rdflib import RDFS, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF

import gmeow_tools.validate as validate_mod
from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    NAMESPACE,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.validate import (
    ValidationResult,
    _read_cached_result,
    _write_cached_result,
    check_sameas_ban,
    check_syntax,
)

# Graph-accepting structural-lint shim: serializes a synthetic rdflib graph to
# N-Triples and routes it through the graph-free production lint (#579).
from tests._graph_nt import structural_lint


def test_check_syntax_on_sources() -> None:
    assert check_syntax().ok


def test_validate_all_delegates_to_rust_native(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """validate_all wraps gmeow_validate.validate_all_native (#634)."""
    captured: dict[str, object] = {}
    source_paths = ["/fake/a.ttl", "/fake/b.ttl"]
    declared_terms = ["https://example.org/gmeow/TermA"]

    def fake_validate_all_native(
        paths: list[str],
        shapes_ttl: str,
        mapping_dsl_dir: str,
        statement_dsl_dir: str,
        config: object,
        options: object,
    ) -> dict[str, object]:
        captured["paths"] = paths
        captured["shapes_ttl"] = shapes_ttl
        captured["mapping_dsl_dir"] = mapping_dsl_dir
        captured["statement_dsl_dir"] = statement_dsl_dir
        captured["config"] = config
        captured["options"] = options
        return {
            "errors": [],
            "warnings": ["rust-warning"],
            "declared_terms": declared_terms,
            "timings": [{"phase": "test", "elapsed_ms": 1, "metadata": "ok"}],
        }

    anchor_calls: list[tuple[list[str], list[str]]] = []

    def fake_guide_anchor_lint(
        paths: list[str],
        root: Path | None = None,
        *,
        declared_terms: list[str] | None = None,
    ) -> ValidationResult:
        anchor_calls.append((paths, list(declared_terms or [])))
        return ValidationResult(warnings=["anchor-warning"])

    monkeypatch.setattr(
        gmeow_validate,
        "validate_all_native",
        fake_validate_all_native,
    )
    monkeypatch.setattr(
        validate_mod,
        "iter_source_files",
        lambda: [Path(p) for p in source_paths],
    )
    monkeypatch.setattr(
        validate_mod,
        "guide_anchor_lint",
        fake_guide_anchor_lint,
    )
    monkeypatch.setattr(
        gmeow_docs,
        "i18n_lint_po_files",
        lambda *args, **kwargs: {
            "errors": [],
            "warnings": [],
            "total_counts": {},
            "fuzzy_counts": {},
        },
    )

    validation = validate_mod.validate_all(timings=True)

    assert validation.ok
    assert validation.warnings == ["rust-warning", "anchor-warning"]
    assert validation.timings == [{"phase": "test", "elapsed_ms": 1, "metadata": "ok"}]
    assert captured["paths"] == source_paths
    shapes_ttl = captured["shapes_ttl"]
    assert isinstance(shapes_ttl, str)
    assert "sh:Shape" in shapes_ttl or "@prefix" in shapes_ttl
    assert captured["mapping_dsl_dir"] == str(MAPPING_DSL_DIR)
    assert captured["statement_dsl_dir"] == str(STATEMENT_DSL_DIR)
    assert captured["options"] is not None
    assert captured["config"] is not None
    assert anchor_calls == [(source_paths, declared_terms)]


def test_validate_all_skips_guide_anchor_on_rust_errors(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """When Rust reports errors, Python skips the guide-anchor lint."""

    def fake_validate_all_native(*args: object, **kwargs: object) -> dict[str, object]:
        return {
            "errors": ["rust-error"],
            "warnings": [],
            "declared_terms": ["https://example.org/gmeow/TermA"],
            "timings": [],
        }

    def failing_guide_anchor_lint(*args: object, **kwargs: object) -> ValidationResult:
        pytest.fail("guide_anchor_lint should not run when Rust reports errors")

    monkeypatch.setattr(
        gmeow_validate,
        "validate_all_native",
        fake_validate_all_native,
    )
    monkeypatch.setattr(
        validate_mod,
        "guide_anchor_lint",
        failing_guide_anchor_lint,
    )

    validation = validate_mod.validate_all()

    assert not validation.ok
    assert validation.errors == ["rust-error"]


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
    graph.add((bad, RDF.type, OWL.Class))

    result = structural_lint(graph)
    assert any("skos:definition" in e for e in result.errors)


# The remaining structural-lint cases (graphBoxRole presence/typing, language-tag
# discipline, well-formed-term clean pass) are exercised natively in
# crates/validate/src/lint.rs (structural_* #[test]s); this file keeps only the
# single FFI-contract smoke above, proving the structural_lint binding marshals
# an rdflib-derived N-Triples graph through the Rust path (#786 / T5 of #781).


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
