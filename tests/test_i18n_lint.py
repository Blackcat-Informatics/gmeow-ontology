"""Tests for the PO i18n fuzzy/staleness lint gate (#572)."""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.i18n_lint import I18nLintReport, lint_po_files

# Real term and predicate from the merged ontology, used for valid entries.
_VALID_TERM_LABEL = (
    "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
    "adoption",
)
_VALID_TERM_DEF = (
    "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
    "chain id",
)

_MINIMAL_ONTOLOGY_TTL = """\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:eventTypeAdoption rdfs:label "adoption"@x-gmeow-english .
gmeow:chainId rdfs:label "chain id"@x-gmeow-english .
gmeow:placeTypeCity rdfs:label "city"@x-gmeow-english .
"""


def _setup_minimal_ontology(tmp_path: Path) -> None:
    """Write a tiny English ontology to *tmp_path* so entries can resolve."""
    ontology_path = tmp_path / "ontology" / "gmeow.ttl"
    ontology_path.parent.mkdir(parents=True)
    ontology_path.write_text(_MINIMAL_ONTOLOGY_TTL, encoding="utf-8")


def _po_body(entries: list[tuple[str, str, str, bool]]) -> str:
    """Build PO file text from (msgctxt, msgid, msgstr, fuzzy) tuples."""
    lines = [
        "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc.",
        "# SPDX-License-Identifier: CC-BY-4.0",
        'msgid ""',
        'msgstr ""',
        '"Project-Id-Version: gmeow\\n"',
        '"Language: fr\\n"',
        '"MIME-Version: 1.0\\n"',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Content-Transfer-Encoding: 8bit\\n"',
        "",
    ]
    for msgctxt, msgid, msgstr, fuzzy in entries:
        if fuzzy:
            lines.append("#, fuzzy")
        lines.append(f'msgctxt "{msgctxt}"')
        lines.append(f'msgid "{msgid}"')
        lines.append(f'msgstr "{msgstr}"')
        lines.append("")
    return "\n".join(lines)


def test_lint_no_po_files_returns_empty_report(tmp_path: Path) -> None:
    """When no PO files exist the lint returns an empty report."""
    report = lint_po_files(tmp_path)
    assert report.errors == []
    assert report.warnings == []
    assert report.total_counts == {}
    assert report.fuzzy_counts == {}


def test_lint_valid_catalog_reports_no_errors(tmp_path: Path) -> None:
    """A catalog whose entries all resolve to current English literals passes."""
    _setup_minimal_ontology(tmp_path)
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "valid_fr.po"
    po_path.parent.mkdir(parents=True)
    po_path.write_text(
        _po_body(
            [
                (*_VALID_TERM_LABEL, "adoption", False),
                (*_VALID_TERM_DEF, "identifiant de chaîne", False),
            ]
        ),
        encoding="utf-8",
    )

    report = lint_po_files(tmp_path)
    assert report.errors == []
    assert report.warnings == []
    assert report.total_counts.get("x-gmeow-french") == 2
    assert report.fuzzy_counts.get("x-gmeow-french", 0) == 0


def test_lint_orphaned_entries_are_warnings(tmp_path: Path) -> None:
    """Entries pointing to missing terms or stale msgids are warnings."""
    _setup_minimal_ontology(tmp_path)
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "orphaned_fr.po"
    po_path.parent.mkdir(parents=True)
    po_path.write_text(
        _po_body(
            [
                (*_VALID_TERM_LABEL, "adoption", False),
                (
                    "https://blackcatinformatics.ca/gmeow/NonExistentLintTerm|rdfs:label",
                    "missing term",
                    "terme manquant",
                    False,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/placeTypeCity|rdfs:label",
                    "old city label",
                    "ancienne étiquette",
                    False,
                ),
            ]
        ),
        encoding="utf-8",
    )

    report = lint_po_files(tmp_path)
    assert report.errors == []
    assert len(report.warnings) == 2
    assert any("orphaned" in w for w in report.warnings)
    assert any("stale" in w for w in report.warnings)


def test_lint_all_fuzzy_catalog_produces_error(tmp_path: Path) -> None:
    """A catalog whose every entry is fuzzy produces an all-fuzzy error."""
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "all_fuzzy_fr.po"
    po_path.parent.mkdir(parents=True)
    po_path.write_text(
        _po_body(
            [
                (*_VALID_TERM_LABEL, "adoption", True),
                (*_VALID_TERM_DEF, "identifiant de chaîne", True),
            ]
        ),
        encoding="utf-8",
    )

    report = lint_po_files(tmp_path)
    assert len(report.errors) == 1
    assert "x-gmeow-french has only fuzzy entries" in report.errors[0]


def test_lint_fuzzy_ratio_exceeding_max_produces_error(tmp_path: Path) -> None:
    """A catalog whose fuzzy ratio exceeds the configured limit errors."""
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "ratio_fr.po"
    po_path.parent.mkdir(parents=True)
    po_path.write_text(
        _po_body(
            [
                (*_VALID_TERM_LABEL, "adoption", True),
                (*_VALID_TERM_DEF, "identifiant de chaîne", False),
            ]
        ),
        encoding="utf-8",
    )

    report = lint_po_files(tmp_path, max_fuzzy_ratio=30.0)
    assert len(report.errors) == 1
    assert "50.0% fuzzy" in report.errors[0]
    assert "30.0% limit" in report.errors[0]


def test_lint_parse_error_is_reported_as_error(tmp_path: Path) -> None:
    """A structurally invalid PO file surfaces as a lint error."""
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "broken_fr.po"
    po_path.parent.mkdir(parents=True)
    po_path.write_text(
        "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc.\n"
        'msgid ""\n'
        'msgstr ""\n'
        '"Language: fr\\n"\n'
        "\n"
        '"stray continuation line"\n',
        encoding="utf-8",
    )

    report = lint_po_files(tmp_path)
    assert len(report.errors) == 1
    assert "PO parse error" in report.errors[0]


def test_lint_missing_language_header_is_error(tmp_path: Path) -> None:
    """A PO file without a Language header is rejected."""
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "no_lang.po"
    po_path.parent.mkdir(parents=True)
    body = _po_body([("https://example.org/T|rdfs:label", "x", "y", False)])
    body = body.replace('"Language: fr\\n"', '""')
    po_path.write_text(body, encoding="utf-8")

    report = lint_po_files(tmp_path)
    assert len(report.errors) == 1
    assert "missing Language header" in report.errors[0]


def test_lint_unmapped_language_is_error(tmp_path: Path) -> None:
    """A PO file whose Language value has no GMEOW internal tag is rejected."""
    po_path = tmp_path / "tests" / "fixtures" / "i18n" / "unknown_lang.po"
    po_path.parent.mkdir(parents=True)
    body = _po_body([("https://example.org/T|rdfs:label", "x", "y", False)])
    body = body.replace('"Language: fr\\n"', '"Language: xx-unknown\\n"')
    po_path.write_text(body, encoding="utf-8")

    report = lint_po_files(tmp_path)
    assert len(report.errors) == 1
    assert "no GMEOW internal tag mapping" in report.errors[0]


def test_lint_committed_fr_po_reports_orphaned_warnings() -> None:
    """The committed fr.po fixture carries expected orphaned/stale warnings."""
    report = lint_po_files(PROJECT_ROOT)
    assert "x-gmeow-french" in report.total_counts
    assert report.total_counts["x-gmeow-french"] >= 4
    assert report.fuzzy_counts.get("x-gmeow-french", 0) == 1
    assert any("NonExistentLintTerm" in w for w in report.warnings)
    assert any("stale" in w for w in report.warnings)


def test_lint_committed_zh_po_reports_fuzzy_entries() -> None:
    """The committed zh.po fixture carries expected fuzzy entries."""
    report = lint_po_files(PROJECT_ROOT)
    assert "x-gmeow-mandarin" in report.total_counts
    assert report.fuzzy_counts.get("x-gmeow-mandarin", 0) >= 2
    assert any("OrphanedMandarinTerm" in w for w in report.warnings)


def test_i18n_lint_report_dataclass_defaults() -> None:
    """The report dataclass exposes the documented fields."""
    report = I18nLintReport()
    assert report.errors == []
    assert report.warnings == []
    assert report.fuzzy_counts == {}
    assert report.total_counts == {}
