# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Tests for ``gmeow create-docs`` (#439)."""

from __future__ import annotations

import io
import tarfile
from pathlib import Path

import pytest

from gmeow_tools.config import NAMESPACE, ONTOLOGY_IRI
from gmeow_tools.create_docs import (
    _safe_filename,
    create_docs,
    resolve_doc_language,
)
from gts import Writer
from gts.model import Term, TermKind

RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
RDFS = "http://www.w3.org/2000/01/rdf-schema#"
OWL = "http://www.w3.org/2002/07/owl#"
SKOS = "http://www.w3.org/2004/02/skos/core#"
DCTERMS = "http://purl.org/dc/terms/"


def _make_tar(language: str, files: dict[str, bytes]) -> bytes:
    """Build a deterministic uncompressed tar archive."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as tar:
        for name in sorted(files):
            data = files[name]
            info = tarfile.TarInfo(name=f"{language}/{name}")
            info.size = len(data)
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def _build_test_gts(tmp_path: Path) -> Path:
    """Handcraft a minimal GTS snapshot with terms, guides, and doc archives."""
    w = Writer(profile="dist")

    terms = [
        Term(TermKind.IRI, ONTOLOGY_IRI),
        Term(TermKind.IRI, RDF + "type"),
        Term(TermKind.IRI, OWL + "Class"),
        Term(TermKind.IRI, OWL + "ObjectProperty"),
        Term(TermKind.IRI, OWL + "FunctionalProperty"),
        Term(TermKind.IRI, RDFS + "label"),
        Term(TermKind.IRI, SKOS + "definition"),
        Term(TermKind.IRI, RDFS + "isDefinedBy"),
        Term(TermKind.IRI, RDFS + "subClassOf"),
        Term(TermKind.IRI, RDFS + "domain"),
        Term(TermKind.IRI, RDFS + "range"),
        Term(TermKind.IRI, DCTERMS + "title"),
        Term(TermKind.IRI, OWL + "versionInfo"),
        Term(TermKind.IRI, NAMESPACE + "TestConcept"),
        Term(TermKind.IRI, NAMESPACE + "TestParent"),
        Term(TermKind.IRI, NAMESPACE + "hasName"),
        Term(TermKind.IRI, NAMESPACE + "TestIndividual"),
        Term(TermKind.IRI, NAMESPACE + "Language"),
        Term(TermKind.IRI, NAMESPACE + "languageTag"),
        Term(TermKind.IRI, NAMESPACE + "bcp47Tag"),
        Term(TermKind.LITERAL, "Test Ontology"),
        Term(TermKind.LITERAL, "1.0.0-test"),
        Term(TermKind.LITERAL, "Test Concept", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "A concept for testing.", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "Test Parent", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "A parent concept.", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "has name", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "A name property.", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "Test Individual", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "An individual.", lang="x-gmeow-english"),
        Term(TermKind.LITERAL, "x-gmeow-english"),
        Term(TermKind.LITERAL, "en"),
        Term(TermKind.LITERAL, "core/test"),
    ]
    w.add_terms(terms)

    # Ontology header.
    w.add_quads(
        [
            (
                0,
                1,
                2,
                None,
            ),  # ontology a owl:Class? actually ontology header; use owl:Ontology?
            (0, 11, 20, None),  # dcterms:title
            (0, 12, 21, None),  # owl:versionInfo
        ]
    )
    # fold_meta only needs the ontology IRI subject and its title/versionInfo values.
    # Add language mapping.
    w.add_quads(
        [
            (17, 1, 2, None),  # Language a owl:Class
            (17, 18, 30, None),  # languageTag x-gmeow-english
            (17, 19, 31, None),  # bcp47Tag en
        ]
    )
    # Class hierarchy.
    w.add_quads(
        [
            (13, 1, 2, None),  # TestConcept a owl:Class
            (14, 1, 2, None),  # TestParent a owl:Class
            (13, 5, 22, None),  # label
            (13, 6, 23, None),  # definition
            (13, 7, 0, None),  # isDefinedBy ontology
            (13, 8, 14, None),  # subClassOf TestParent
            (14, 1, 2, None),
            (14, 5, 24, None),
            (14, 6, 25, None),
        ]
    )
    # Property.
    w.add_quads(
        [
            (15, 1, 3, None),  # hasName a owl:ObjectProperty
            (15, 1, 4, None),  # functional
            (15, 5, 26, None),
            (15, 6, 27, None),
            (15, 9, 13, None),  # domain TestConcept
            (15, 10, 14, None),  # range TestParent
        ]
    )
    # Individual.
    w.add_quads(
        [
            (16, 1, 13, None),  # TestIndividual a TestConcept
            (16, 5, 28, None),
            (16, 6, 29, None),
        ]
    )

    # Slice guide blob.
    guide_payload = b"# Test Guide\n\nProse for the test slice.\n"
    w.add_blob(
        guide_payload, mt="text/markdown", rep="docs:core/test", transform=["zstd"]
    )

    # Project-docs tar archive.
    project_tar = _make_tar(
        "x-gmeow-english", {"RATIONALE.md": b"# Rationale\n\nIt works.\n"}
    )
    w.add_blob(
        project_tar, mt="application/x-tar", rep="project-docs", transform=["zstd"]
    )

    # Ontology-docs tar archive.
    ontology_tar = _make_tar(
        "x-gmeow-english", {"index.md": b"# Ontology Docs\n\nHello.\n"}
    )
    w.add_blob(
        ontology_tar, mt="application/x-tar", rep="ontology-docs", transform=["zstd"]
    )

    path = tmp_path / "test.gts"
    path.write_bytes(w.to_bytes())
    return path


def test_resolve_doc_language_defaults_to_english() -> None:
    assert resolve_doc_language() == "x-gmeow-english"


def test_safe_filename() -> None:
    assert _safe_filename("gmeow:Person") == "gmeow-Person.md"


def test_create_docs_writes_expected_tree(tmp_path: Path) -> None:
    gts_path = _build_test_gts(tmp_path)
    out = tmp_path / "docs-tree"
    create_docs(gts_path, out)

    assert (out / "index.md").exists()
    assert (out / "terms" / "classes" / "gmeow-TestConcept.md").exists()
    assert (out / "terms" / "properties" / "gmeow-hasName.md").exists()
    assert (out / "terms" / "individuals" / "gmeow-TestIndividual.md").exists()
    assert (out / "slices" / "core" / "test" / "docs.md").exists()
    assert (out / "project_docs" / "RATIONALE.md").exists()
    assert (out / "ontology-docs" / "index.md").exists()
    assert (out / "alignments.md").exists()
    assert (out / "statements.md").exists()


def test_create_docs_retags_internal_language_tags(tmp_path: Path) -> None:
    gts_path = _build_test_gts(tmp_path)
    out = tmp_path / "docs-tree"
    create_docs(gts_path, out)

    concept = (out / "terms" / "classes" / "gmeow-TestConcept.md").read_text(
        encoding="utf-8"
    )
    assert "Test Concept" in concept
    # The Markdown file itself is plain text; the language boundary is honoured
    # by selecting the public BCP-47-mapped literal, not by emitting tags.
    assert "@x-gmeow-english" not in concept


def test_create_docs_is_deterministic(tmp_path: Path) -> None:
    gts_path = _build_test_gts(tmp_path)
    out1 = tmp_path / "tree1"
    out2 = tmp_path / "tree2"
    create_docs(gts_path, out1)
    create_docs(gts_path, out2)

    files1 = sorted(p.relative_to(out1) for p in out1.rglob("*") if p.is_file())
    files2 = sorted(p.relative_to(out2) for p in out2.rglob("*") if p.is_file())
    assert files1 == files2
    for rel in files1:
        assert (out1 / rel).read_bytes() == (out2 / rel).read_bytes()


def test_create_docs_refuses_non_empty_directory(tmp_path: Path) -> None:
    gts_path = _build_test_gts(tmp_path)
    out = tmp_path / "docs-tree"
    out.mkdir()
    (out / "existing.txt").write_text("hello", encoding="utf-8")
    with pytest.raises(FileExistsError):
        create_docs(gts_path, out)


def test_create_docs_force_overwrites_non_empty_directory(tmp_path: Path) -> None:
    gts_path = _build_test_gts(tmp_path)
    out = tmp_path / "docs-tree"
    out.mkdir()
    (out / "existing.txt").write_text("hello", encoding="utf-8")
    create_docs(gts_path, out, force=True)
    assert (out / "index.md").exists()
