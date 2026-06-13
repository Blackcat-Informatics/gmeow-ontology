# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Tests for the ontology-docs generator (#440)."""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.generator import registry
from gmeow_tools.ontology_docs import build_ontology_docs


def test_ontology_docs_generator_is_registered() -> None:
    assert "ontology-docs" in registry()
    gen = registry()["ontology-docs"]
    assert gen.is_directory_output


@pytest.mark.ci_only
def test_build_ontology_docs_creates_expected_tree(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    docs = out / "docs"
    assert (out / "mkdocs.yml").exists()
    assert (out / "site" / "index.html").exists()
    assert (docs / "index.md").exists()
    assert (docs / "reference" / "classes" / "index.md").exists()
    assert (docs / "reference" / "properties" / "index.md").exists()
    assert (docs / "reference" / "individuals" / "index.md").exists()
    assert (docs / "reference" / "datatypes" / "index.md").exists()
    assert (docs / "slices" / "index.md").exists()
    assert (docs / "profiles" / "index.md").exists()
    assert (docs / "visualization" / "index.md").exists()
    assert (docs / "quality" / "oops-report.md").exists()
    assert (docs / "about.md").exists()
    assert (docs / "changelog.md").exists()
    assert (docs / "stylesheets" / "extra.css").exists()


@pytest.mark.ci_only
def test_build_ontology_docs_is_deterministic(tmp_path: Path) -> None:
    out1 = tmp_path / "tree1"
    out2 = tmp_path / "tree2"
    build_ontology_docs(out1)
    build_ontology_docs(out2)

    # Compare deterministic source Markdown (site/ may contain timestamps).
    docs1 = out1 / "docs"
    docs2 = out2 / "docs"
    files1 = sorted(p.relative_to(docs1) for p in docs1.rglob("*") if p.is_file())
    files2 = sorted(p.relative_to(docs2) for p in docs2.rglob("*") if p.is_file())
    assert files1 == files2
    for rel in files1:
        assert (docs1 / rel).read_bytes() == (docs2 / rel).read_bytes()


@pytest.mark.ci_only
def test_index_contains_ontology_header_and_slice_stats(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)
    index = (out / "docs" / "index.md").read_text(encoding="utf-8")

    assert "GMEOW" in index
    assert "Namespace:" in index
    assert "## Profiles" in index
    assert "## Slices" in index
    assert "## Reference" in index


@pytest.mark.ci_only
def test_slice_index_lists_manifest_metadata(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)
    slices_index = (out / "docs" / "slices" / "index.md").read_text(encoding="utf-8")

    assert "# Slices" in slices_index
    assert "| Slice | Tier | Profiles | Dependencies | Consumer |" in slices_index
    assert "[kernel](kernel.md)" in slices_index


@pytest.mark.ci_only
def test_reference_pages_have_term_metadata(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    # gmeow:Person is a core class that should always exist.
    person = out / "docs" / "reference" / "classes" / "gmeow-Person.md"
    assert person.exists()
    text = person.read_text(encoding="utf-8")
    assert "gmeow:Person" in text
    assert "**IRI:**" in text
