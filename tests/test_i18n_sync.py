"""Tests for the English i18n synchronization engine."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import Graph, Literal, URIRef
from rdflib.namespace import OWL, RDF, RDFS, SKOS
from typer.testing import CliRunner

from gmeow_tools.cli_dev import app as dev_app
from gmeow_tools.i18n_sync import (
    PoEntry,
    PoParseError,
    apply_md_sync,
    parse_po,
    sync_english_file,
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


def _write_md(tmp_path: Path, content: str) -> Path:
    path = tmp_path / "source.md"
    path.write_text(content, encoding="utf-8")
    return path


@pytest.fixture
def i18n_fixtures_dir() -> Path:
    return Path(__file__).resolve().parent / "fixtures" / "i18n"


def _copy_fixture(tmp_path: Path, fixtures_dir: Path, name: str) -> Path:
    src = fixtures_dir / name
    dst = tmp_path / name
    dst.write_text(src.read_text(encoding="utf-8"), encoding="utf-8")
    return dst


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


class TestMarkdownSync:
    """Exercise markdown PO synchronization by segment."""

    def test_in_place_segment_replacement(self, tmp_path: Path) -> None:
        md = _write_md(tmp_path, "# Title\n\nOld paragraph.\n\nAnother line.\n")
        po = _write_po(
            tmp_path,
            'msgid "Old paragraph."\nmsgstr "Improved paragraph."\n',
        )
        report = apply_md_sync(po, md)
        assert report.changed_files == [md]
        assert not report.conflicts
        assert not report.skipped
        updated = md.read_text(encoding="utf-8")
        assert "# Title" in updated
        assert "Improved paragraph." in updated
        assert "Another line." in updated
        assert "Old paragraph." not in updated

    def test_preserves_front_matter(self, tmp_path: Path) -> None:
        md = _write_md(
            tmp_path,
            "---\ntitle: Doc\nversion: 1\n---\n\nFirst paragraph.\n",
        )
        po = _write_po(
            tmp_path,
            'msgid "First paragraph."\nmsgstr "Updated first paragraph."\n',
        )
        report = apply_md_sync(po, md)
        assert report.changed_files == [md]
        updated = md.read_text(encoding="utf-8")
        assert updated.startswith("---\ntitle: Doc\nversion: 1\n---\n")
        assert "Updated first paragraph." in updated

    def test_preserves_fenced_code_blocks(self, tmp_path: Path) -> None:
        md = _write_md(
            tmp_path,
            "Intro.\n\n```python\nprint('hi')\n```\n\nOutro.\n",
        )
        po = _write_po(
            tmp_path,
            'msgid "Intro."\nmsgstr "Introduction."\n\n'
            'msgid "Outro."\nmsgstr "Conclusion."\n',
        )
        report = apply_md_sync(po, md)
        assert report.changed_files == [md]
        updated = md.read_text(encoding="utf-8")
        assert "Introduction." in updated
        assert "Conclusion." in updated
        assert "```python\nprint('hi')\n```" in updated

    def test_idempotency_with_updated_po(self, tmp_path: Path) -> None:
        md = _write_md(tmp_path, "Original.\n")
        po1 = _write_po(
            tmp_path,
            'msgid "Original."\nmsgstr "Improved."\n',
        )
        report1 = apply_md_sync(po1, md)
        assert report1.changed_files == [md]

        po2 = _write_po(
            tmp_path,
            'msgid "Improved."\nmsgstr "Improved."\n',
        )
        report2 = apply_md_sync(po2, md)
        assert not report2.changed_files
        assert not report2.conflicts
        assert not report2.skipped
        assert report2.unchanged == ["Improved."]

    def test_conflict_when_source_and_po_both_changed(self, tmp_path: Path) -> None:
        md = _write_md(tmp_path, "Modified segment.\n")
        po = _write_po(
            tmp_path,
            'msgid "Original segment."\nmsgstr "Proposed segment."\n',
        )
        report = apply_md_sync(po, md)
        assert not report.changed_files
        assert len(report.conflicts) == 1
        assert "conflict" in report.conflicts[0]
        assert not report.skipped

    def test_skip_when_source_changed_po_unchanged(self, tmp_path: Path) -> None:
        md = _write_md(tmp_path, "Modified.\n")
        po = _write_po(
            tmp_path,
            'msgid "Original."\nmsgstr "Original."\n',
        )
        report = apply_md_sync(po, md)
        assert not report.changed_files
        assert not report.conflicts
        assert len(report.skipped) == 1
        assert "source changed, PO unchanged" in report.skipped[0]

    def test_ambiguous_segment_is_skipped(self, tmp_path: Path) -> None:
        md = _write_md(tmp_path, "Repeat. Repeat.\n")
        po = _write_po(
            tmp_path,
            'msgid "Repeat."\nmsgstr "New."\n',
        )
        report = apply_md_sync(po, md)
        assert not report.changed_files
        assert not report.conflicts
        assert len(report.skipped) == 1
        assert "ambiguous" in report.skipped[0]


class TestSyncEnglishFile:
    """Exercise the extension-based dispatcher."""

    def test_dispatches_to_md_sync(self, tmp_path: Path) -> None:
        md = _write_md(tmp_path, "Old.\n")
        po = _write_po(
            tmp_path,
            'msgid "Old."\nmsgstr "New."\n',
        )
        report = sync_english_file(po, md)
        assert report.changed_files == [md]
        assert "New." in md.read_text(encoding="utf-8")

    def test_dispatches_to_ttl_sync(self, tmp_path: Path) -> None:
        ttl = _write_ttl(
            tmp_path,
            """@prefix ex: <http://example.org/> .

ex:s ex:p "old"@x-gmeow-english .
""",
        )
        po = _write_po(
            tmp_path,
            """msgctxt "http://example.org/s|http://example.org/p"
msgid "old"
msgstr "new"
""",
        )
        report = sync_english_file(po, ttl)
        assert report.changed_files == [ttl]
        assert '"new"@x-gmeow-english' in ttl.read_text(encoding="utf-8")


class TestFixtureBasedSync:
    """Fixture-based end-to-end tests for the i18n sync engine."""

    def test_ttl_in_place_value_replacement(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        ttl = _copy_fixture(tmp_path, i18n_fixtures_dir, "module.ttl")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "lifecycle_en.po")
        report = sync_english_from_po(po, ttl)
        assert report.changed_files == [ttl]
        assert not report.conflicts
        assert not report.skipped
        updated = ttl.read_text(encoding="utf-8")
        assert '"Lifecycle Process"@x-gmeow-english' in updated
        assert '"Active State"@x-gmeow-english' in updated
        assert '"has lifecycle state"@x-gmeow-english' in updated
        assert (
            '"A process that manages entity states over time."@x-gmeow-english'
            in updated
        )
        assert '"Original lifecycle description."@x-gmeow-english' not in updated
        assert '"has state"@x-gmeow-english' not in updated

    def test_ttl_no_structural_drift(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        ttl = _copy_fixture(tmp_path, i18n_fixtures_dir, "module.ttl")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "lifecycle_en.po")
        report = sync_english_from_po(po, ttl)
        assert report.changed_files == [ttl]
        assert not report.conflicts
        graph = Graph()
        graph.parse(ttl, format="turtle")
        lifecycle = URIRef("http://example.org/i18n-fixture/lifecycle#Lifecycle")
        active = URIRef("http://example.org/i18n-fixture/lifecycle#Active")
        has_state = URIRef("http://example.org/i18n-fixture/lifecycle#hasState")
        assert (lifecycle, RDF.type, OWL.Class) in graph
        assert (active, RDF.type, OWL.Class) in graph
        assert (has_state, RDF.type, OWL.ObjectProperty) in graph
        assert (
            lifecycle,
            RDFS.label,
            Literal("Lifecycle Process", lang="x-gmeow-english"),
        ) in graph
        assert (
            lifecycle,
            SKOS.definition,
            Literal(
                "A process that manages entity states over time.",
                lang="x-gmeow-english",
            ),
        ) in graph

    def test_ttl_idempotency(self, tmp_path: Path, i18n_fixtures_dir: Path) -> None:
        ttl = _copy_fixture(tmp_path, i18n_fixtures_dir, "module.ttl")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "lifecycle_en.po")
        report1 = sync_english_from_po(po, ttl)
        assert report1.changed_files == [ttl]
        report2 = sync_english_from_po(po, ttl)
        assert not report2.changed_files
        assert not report2.conflicts
        assert not report2.skipped
        assert report2.unchanged

    def test_ttl_conflict_detection(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        ttl = _copy_fixture(tmp_path, i18n_fixtures_dir, "module.ttl")
        non_conflicting_po = _copy_fixture(
            tmp_path, i18n_fixtures_dir, "lifecycle_en.po"
        )
        # First sync brings the master up to date with the non-conflicting PO.
        sync_english_from_po(non_conflicting_po, ttl)
        # Now apply a PO that proposes different values for the same originals.
        conflicting_po = _copy_fixture(
            tmp_path, i18n_fixtures_dir, "lifecycle_en_conflicting.po"
        )
        report = sync_english_from_po(conflicting_po, ttl)
        assert not report.changed_files
        assert len(report.conflicts) == 6
        assert all("conflict" in conflict for conflict in report.conflicts)
        assert not report.skipped
        assert not report.unchanged

    def test_md_in_place_segment_replacement(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        md = _copy_fixture(tmp_path, i18n_fixtures_dir, "docs.md")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "sample.md.po")
        report = apply_md_sync(po, md)
        assert report.changed_files == [md]
        assert not report.conflicts
        assert not report.skipped
        updated = md.read_text(encoding="utf-8")
        assert "This is the updated introduction paragraph." in updated
        assert "This is the updated conclusion." in updated
        assert "This is the original introduction paragraph." not in updated
        assert "This is the original conclusion." not in updated

    def test_md_preserves_fenced_code_and_front_matter(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        md = _copy_fixture(tmp_path, i18n_fixtures_dir, "docs.md")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "sample.md.po")
        apply_md_sync(po, md)
        updated = md.read_text(encoding="utf-8")
        assert updated.startswith("---\ntitle: Sample Document\nversion: 1.0\n---\n")
        assert '```python\nprint("hello")\n```' in updated

    def test_md_idempotency(self, tmp_path: Path, i18n_fixtures_dir: Path) -> None:
        md = _copy_fixture(tmp_path, i18n_fixtures_dir, "docs.md")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "sample.md.po")
        report1 = apply_md_sync(po, md)
        assert report1.changed_files == [md]

        # A Markdown PO identifies segments by their content, so a second run
        # with the original PO would see a changed source.  Idempotency is
        # exercised by re-syncing with a PO whose msgid values match the
        # already-updated master.
        updated_po = tmp_path / "updated.md.po"
        updated_po.write_text(
            'msgid "This is the updated introduction paragraph."\n'
            'msgstr "This is the updated introduction paragraph."\n\n'
            'msgid "This is the updated conclusion."\n'
            'msgstr "This is the updated conclusion."\n',
            encoding="utf-8",
        )
        report2 = apply_md_sync(updated_po, md)
        assert not report2.changed_files
        assert not report2.conflicts
        assert not report2.skipped
        assert report2.unchanged

    def test_md_conflict_detection(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        md = _copy_fixture(tmp_path, i18n_fixtures_dir, "docs.md")
        po = _copy_fixture(tmp_path, i18n_fixtures_dir, "sample.md.po")
        original = md.read_text(encoding="utf-8")
        modified = original.replace(
            "This is the original introduction paragraph.",
            "This is an independently edited introduction paragraph.",
        ).replace(
            "This is the original conclusion.",
            "This is an independently edited conclusion.",
        )
        md.write_text(modified, encoding="utf-8")
        report = apply_md_sync(po, md)
        assert not report.changed_files
        assert len(report.conflicts) == 2
        assert all("conflict" in conflict for conflict in report.conflicts)
        assert not report.skipped
        assert not report.unchanged
        assert md.read_text(encoding="utf-8") == modified

    def test_cli_dry_run_reports_changes(
        self, tmp_path: Path, i18n_fixtures_dir: Path
    ) -> None:
        slice_dir = tmp_path / "slices" / "lifecycle"
        i18n_dir = slice_dir / "i18n"
        i18n_dir.mkdir(parents=True)
        ttl = _copy_fixture(slice_dir, i18n_fixtures_dir, "module.ttl")
        md = _copy_fixture(slice_dir, i18n_fixtures_dir, "docs.md")
        en_po = _copy_fixture(i18n_dir, i18n_fixtures_dir, "lifecycle_en.po")
        (i18n_dir / "en.po").write_text(
            en_po.read_text(encoding="utf-8"), encoding="utf-8"
        )
        md_po = _copy_fixture(i18n_dir, i18n_fixtures_dir, "sample.md.po")
        (i18n_dir / "docs.md.po").write_text(
            md_po.read_text(encoding="utf-8"), encoding="utf-8"
        )

        runner = CliRunner()
        result = runner.invoke(
            dev_app,
            ["i18n", "sync-english", "--dry-run", "--root", str(tmp_path)],
        )
        assert result.exit_code == 0, result.output
        assert "would change" in result.output
        assert ttl.name in result.output
        assert md.name in result.output
        assert "Lifecycle Process" not in ttl.read_text(encoding="utf-8")
        assert "This is the updated introduction paragraph." not in md.read_text(
            encoding="utf-8"
        )
