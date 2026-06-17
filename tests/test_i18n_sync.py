"""Tests for the English i18n synchronization engine."""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.i18n_sync import (
    PoEntry,
    PoParseError,
    parse_po,
    sync_english_from_po,
)


def test_parse_po_single_line_entry() -> None:
    text = """
# Some comment
msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "new value"
"""
    entries = parse_po(text)
    assert entries == [
        PoEntry(
            msgctxt="http://example.org/s|http://example.org/p",
            msgid="old value",
            msgstr="new value",
        )
    ]


def test_parse_po_multi_line_strings() -> None:
    text = """
msgctxt "http://example.org/s|http://example.org/p"
msgid ""
"Line one\\n"
"Line two"
msgstr ""
"New line one\\n"
"New line two"
"""
    entries = parse_po(text)
    assert len(entries) == 1
    assert entries[0].msgid == "Line one\nLine two"
    assert entries[0].msgstr == "New line one\nNew line two"


def test_parse_po_escape_sequences() -> None:
    text = """
msgctxt "http://example.org/s|http://example.org/p"
msgid "say \\"hello\\""
msgstr "say \\"goodbye\\""
"""
    entries = parse_po(text)
    assert entries[0].msgid == 'say "hello"'
    assert entries[0].msgstr == 'say "goodbye"'


def test_parse_po_backslash_escape() -> None:
    text = """
msgctxt "http://example.org/s|http://example.org/p"
msgid "C:\\Program Files\\app"
msgstr "C:\\Program Files\\app"
"""
    entries = parse_po(text)
    assert entries[0].msgid == r"C:\Program Files\app"
    assert entries[0].msgstr == r"C:\Program Files\app"


def test_parse_po_triple_quoted() -> None:
    text = '''
msgctxt "http://example.org/s|http://example.org/p"
msgid """old value"""
msgstr """new value"""
'''
    entries = parse_po(text)
    assert entries[0].msgid == "old value"
    assert entries[0].msgstr == "new value"


def test_parse_po_skips_comments_and_unknown_fields() -> None:
    text = """
# Header comment
msgctxt "http://example.org/s|http://example.org/p"
msgid "old"
msgstr "new"

# Another comment
msgid "orphan"
msgstr "value"

msgid_plural "plural old"
msgstr[0] "plural new"
"""
    entries = parse_po(text)
    assert len(entries) == 1
    assert entries[0].msgid == "old"


def test_parse_po_multiple_entries() -> None:
    text = """
msgctxt "http://example.org/s|http://example.org/p1"
msgid "first old"
msgstr "first new"

msgctxt "http://example.org/s|http://example.org/p2"
msgid "second old"
msgstr "second new"
"""
    entries = parse_po(text)
    assert len(entries) == 2
    assert entries[0].msgctxt == "http://example.org/s|http://example.org/p1"
    assert entries[1].msgctxt == "http://example.org/s|http://example.org/p2"


def test_parse_po_continuation_without_field_raises() -> None:
    text = '"orphan string"'
    with pytest.raises(PoParseError, match="continuation line without a field"):
        parse_po(text)


def _write_ttl(tmp_path: Path, content: str) -> Path:
    path = tmp_path / "source.ttl"
    path.write_text(content, encoding="utf-8")
    return path


def _write_po(tmp_path: Path, content: str) -> Path:
    path = tmp_path / "translations.po"
    path.write_text(content, encoding="utf-8")
    return path


class TestThreeWayMerge:
    """Exercise the four branches of the 3-way merge."""

    TTL = """@prefix ex: <http://example.org/> .
@prefix x-gmeow: <http://example.org/> .

ex:s ex:p "old value"@x-gmeow-english .
"""

    def test_apply_update(self, tmp_path: Path) -> None:
        ttl = _write_ttl(tmp_path, self.TTL)
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "new value"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert report.changed_files == [ttl]
        assert not report.conflicts
        assert not report.skipped
        assert not report.unchanged
        updated = ttl.read_text(encoding="utf-8")
        assert '"new value"@x-gmeow-english' in updated
        assert '"old value"@x-gmeow-english' not in updated

    def test_no_change(self, tmp_path: Path) -> None:
        ttl = _write_ttl(tmp_path, self.TTL)
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "old value"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert not report.conflicts
        assert not report.skipped
        assert report.unchanged == ["http://example.org/s|http://example.org/p"]

    def test_skip_source_changed_po_unchanged(self, tmp_path: Path) -> None:
        ttl = _write_ttl(tmp_path, self.TTL)
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "previous value"
msgstr "previous value"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert not report.conflicts
        assert len(report.skipped) == 1
        assert "source changed, PO unchanged" in report.skipped[0]
        assert not report.unchanged

    def test_conflict_both_changed(self, tmp_path: Path) -> None:
        ttl = _write_ttl(tmp_path, self.TTL)
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "previous value"
msgstr "proposed value"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert len(report.conflicts) == 1
        assert "conflict" in report.conflicts[0]
        assert not report.skipped
        assert not report.unchanged

    def test_source_already_has_new_value(self, tmp_path: Path) -> None:
        ttl = _write_ttl(tmp_path, self.TTL)
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "previous value"
msgstr "old value"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert not report.conflicts
        assert not report.skipped
        assert report.unchanged == ["http://example.org/s|http://example.org/p"]


class TestTurtleLiteralSync:
    """Verify that formatting is preserved during text replacement."""

    def test_preserves_quote_style_and_tag(self, tmp_path: Path) -> None:
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .

ex:s ex:p "old"@x-gmeow-english .
""",
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old"
msgstr "new"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert report.changed_files == [ttl]
        updated = ttl.read_text(encoding="utf-8")
        assert '"new"@x-gmeow-english' in updated
        assert "@prefix ex:" in updated

    def test_preserves_triple_quoted_style(self, tmp_path: Path) -> None:
        ttl = _write_ttl(
            tmp_path,
            '''@prefix ex: <http://example.org/> .

ex:s ex:p """old value"""@x-gmeow-english .
''',
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "new value"
""",
        )
        sync_english_from_po(po, ttl)
        updated = ttl.read_text(encoding="utf-8")
        assert '"""new value"""@x-gmeow-english' in updated

    def test_datatyped_literals_without_english_tag_are_skipped(
        self, tmp_path: Path
    ) -> None:
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:s ex:p "old"^^xsd:string .
""",
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old"
msgstr "new"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert len(report.skipped) == 1
        assert "no @x-gmeow-english literal" in report.skipped[0]

    def test_preserves_comments_blank_lines_and_ordering(self, tmp_path: Path) -> None:
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

# This is a comment

ex:s a owl:Class ;
    ex:p "old"@x-gmeow-english ;
    ex:q "untouched"@x-gmeow-english .
""",
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old"
msgstr "new"
""",
        )
        sync_english_from_po(po, ttl)
        updated = ttl.read_text(encoding="utf-8")
        lines = updated.splitlines()
        assert "# This is a comment" in lines
        assert any('ex:p "new"@x-gmeow-english ;' in line for line in lines)
        assert any('ex:q "untouched"@x-gmeow-english .' in line for line in lines)

    def test_skips_when_identity_missing(self, tmp_path: Path) -> None:
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .

ex:s ex:p "old"@x-gmeow-english .
""",
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/missing"
msgid "old"
msgstr "new"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert len(report.skipped) == 1
        assert "no @x-gmeow-english literal" in report.skipped[0]

    def test_conflict_when_literal_ambiguous(self, tmp_path: Path) -> None:
        # Same subject, same value, two predicates: replacing only ex:p is
        # ambiguous from a text search perspective.
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .

ex:s ex:p "shared"@x-gmeow-english ;
    ex:q "shared"@x-gmeow-english .
""",
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "shared"
msgstr "changed"
""",
        )
        report = sync_english_from_po(po, ttl)
        assert not report.changed_files
        assert len(report.conflicts) == 1
        assert "ambiguous literal" in report.conflicts[0]

    def test_dry_run_does_not_write(self, tmp_path: Path) -> None:
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .

ex:s ex:p "old"@x-gmeow-english .
""",
        )
        po = _write_po(
            tmp_path,
            """
msgctxt "http://example.org/s|http://example.org/p"
msgid "old"
msgstr "new"
""",
        )
        report = sync_english_from_po(po, ttl, dry_run=True)
        assert report.changed_files == [ttl]
        original = ttl.read_text(encoding="utf-8")
        assert '"old"@x-gmeow-english' in original
        assert '"new"@x-gmeow-english' not in original
