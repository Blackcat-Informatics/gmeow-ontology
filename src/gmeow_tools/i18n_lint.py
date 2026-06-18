"""Fuzzy/staleness lint gate for committed GMEOW PO translation catalogs.

Checks that committed ``.po`` files are structurally valid, their ``msgctxt``
keys resolve to current ``@x-gmeow-english`` literals, and they are not
entirely fuzzy/stale.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from rdflib import Graph, Literal, URIRef

from gmeow_tools.config import PREFIXES, PROJECT_ROOT
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.i18n_catalog import _LANGUAGE_HEADER_RE
from gmeow_tools.i18n_sync import PoParseError, parse_po
from gmeow_tools.language_tags import _default_inverse_tag_map

__all__ = ["I18nLintReport", "lint_po_files"]


#: PO flag-comment prefix that introduces translator flags such as ``fuzzy``.
_FLAG_COMMENT_RE = re.compile(r"^#,\s*(.+)$")


@dataclass
class I18nLintReport:
    """Outcome of linting the repository's PO translation catalogs.

    Attributes:
        errors: Fatal/structural problems that fail validation.
        warnings: Non-fatal issues such as orphaned/stale entries.
        fuzzy_counts: Per-language fuzzy entry counts.
        total_counts: Per-language total entry counts.
    """

    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    fuzzy_counts: dict[str, int] = field(default_factory=dict)
    total_counts: dict[str, int] = field(default_factory=dict)


def _extract_language_header(text: str) -> str | None:
    """Return the ``Language:`` value from a PO file header, or ``None``."""
    for line in text.splitlines():
        m = _LANGUAGE_HEADER_RE.match(line)
        if m:
            return m.group(1).strip()
    return None


def _expand_predicate(predicate: str) -> str:
    """Expand a CURIE or prefixed predicate to a full IRI when possible.

    Recognises the prefixes registered in :data:`gmeow_tools.config.PREFIXES`.
    Full IRIs and unknown prefixed forms are returned unchanged.
    """
    if ":" not in predicate:
        return predicate
    prefix, local = predicate.split(":", 1)
    ns = PREFIXES.get(prefix)
    if ns is not None:
        return ns + local
    return predicate


def _is_fuzzy_entry(block: str) -> bool:
    """Return whether a PO block (one entry) carries a ``fuzzy`` flag comment."""
    for line in block.splitlines():
        stripped = line.strip()
        if stripped.startswith("#,"):
            flags = [flag.strip() for flag in stripped[2:].split(",")]
            if "fuzzy" in flags:
                return True
    return False


def _is_header_block(block: str) -> bool:
    """Return whether a PO block is the catalog header (empty msgid/msgstr)."""
    has_empty_msgid = False
    has_empty_msgstr = False
    for line in block.splitlines():
        stripped = line.strip()
        if stripped.startswith("msgid") and stripped[len("msgid") :].strip() == '""':
            has_empty_msgid = True
        elif (
            stripped.startswith("msgstr") and stripped[len("msgstr") :].strip() == '""'
        ):
            has_empty_msgstr = True
    return has_empty_msgid and has_empty_msgstr


def _split_po_blocks(text: str) -> list[str]:
    """Split PO text into blank-line separated blocks (entries)."""
    blocks: list[str] = []
    current: list[str] = []
    for line in text.splitlines():
        if line.strip() == "":
            if current:
                blocks.append("\n".join(current))
                current = []
        else:
            current.append(line)
    if current:
        blocks.append("\n".join(current))
    return blocks


def _current_english_value(
    graph: Graph,
    term_iri: str,
    predicate_iri: str,
) -> str | None:
    """Return the unique current ``@x-gmeow-english`` value for a term+predicate.

    Returns ``None`` if there is no such literal or if multiple distinct values
    exist (the latter is an ontology consistency issue already caught elsewhere,
    but we avoid returning a possibly-ambiguous match).
    """
    subject = URIRef(term_iri)
    predicate = URIRef(predicate_iri)
    values: set[str] = set()
    for obj in graph.objects(subject, predicate):
        if isinstance(obj, Literal) and obj.language == "x-gmeow-english":
            values.add(str(obj))
    if len(values) == 1:
        return next(iter(values))
    return None


def lint_po_files(
    root: Path,
    *,
    max_fuzzy_ratio: float = 100.0,
) -> I18nLintReport:
    """Lint every committed PO catalog under *root*.

    Walks ``slices/*/*/i18n/*.po`` and ``tests/fixtures/i18n/*.po``.  For each
    file the catalog is parsed, its ``Language:`` header is mapped to a GMEOW
    internal tag, and every ``msgctxt`` entry is resolved against the merged
    English ontology graph.  Entries that no longer match a current
    ``@x-gmeow-english`` literal are reported as warnings.  Fuzzy entries are
    counted per language and validated against the all-fuzzy and ratio gates.

    Args:
        root: Repository root to search for PO files.
        max_fuzzy_ratio: Maximum acceptable ratio of fuzzy entries to total
            entries for any language (0.0-100.0).  Exceeding it produces an
            error.  Defaults to 100.0 (only the all-fuzzy check is enforced).

    Returns:
        A populated :class:`I18nLintReport`.
    """
    report = I18nLintReport()

    po_paths = sorted(
        [*root.glob("slices/*/*/i18n/*.po"), *root.glob("tests/fixtures/i18n/*.po")]
    )
    if not po_paths:
        return report

    try:
        english_graph = load_merged_graph(root=root, include_imports=False)
    except Exception as exc:  # pragma: no cover - build failure is structural
        report.errors.append(f"failed to load merged English graph: {exc}")
        return report

    try:
        inverse_tag_map = _default_inverse_tag_map()
    except Exception as exc:  # pragma: no cover - build failure is structural
        report.errors.append(f"failed to load language tag map: {exc}")
        return report

    for po_path in po_paths:
        rel_path = po_path.relative_to(root)
        try:
            text = po_path.read_text(encoding="utf-8")
        except OSError as exc:
            report.errors.append(f"{rel_path}: cannot read PO file: {exc}")
            continue

        bcp_lang = _extract_language_header(text)
        if not bcp_lang:
            report.errors.append(f"{rel_path}: missing Language header")
            continue

        internal_tag = inverse_tag_map.get(bcp_lang.lower())
        if internal_tag is None:
            available = ", ".join(sorted(inverse_tag_map))
            report.errors.append(
                f"{rel_path}: Language '{bcp_lang}' has no GMEOW internal tag mapping "
                f"(available: {available})"
            )
            continue

        try:
            entries = parse_po(text)
        except PoParseError as exc:
            report.errors.append(f"{rel_path}: PO parse error: {exc}")
            continue

        # Pair each parsed entry with the fuzzy flag from its originating block.
        # Header blocks (the empty msgid/msgstr header) are skipped because
        # parse_po drops them from *entries*.
        blocks = [
            block
            for block in _split_po_blocks(text)
            if any(line.startswith(("msgctxt", "msgid")) for line in block.splitlines())
            and not _is_header_block(block)
        ]
        fuzzy_by_msgctxt: dict[str, bool] = {}
        for block in blocks:
            try:
                parsed = parse_po(block, require_msgctxt=False)
            except PoParseError:
                parsed = []
            for entry in parsed:
                if entry.msgid:
                    fuzzy_by_msgctxt[entry.msgctxt] = _is_fuzzy_entry(block)

        for entry in entries:
            if not entry.msgctxt or "|" not in entry.msgctxt:
                continue

            term_iri, predicate = entry.msgctxt.split("|", 1)
            predicate_iri = _expand_predicate(predicate)
            current_value = _current_english_value(
                english_graph, term_iri, predicate_iri
            )

            if current_value is None:
                report.warnings.append(
                    f"{rel_path}: orphaned entry {entry.msgctxt!r}: "
                    f"no current @x-gmeow-english literal for {term_iri} "
                    f"{predicate_iri}"
                )
            elif current_value != entry.msgid:
                report.warnings.append(
                    f"{rel_path}: stale entry {entry.msgctxt!r}: "
                    f"msgid does not match current @x-gmeow-english literal"
                )

            report.total_counts[internal_tag] = (
                report.total_counts.get(internal_tag, 0) + 1
            )
            if fuzzy_by_msgctxt.get(entry.msgctxt, False):
                report.fuzzy_counts[internal_tag] = (
                    report.fuzzy_counts.get(internal_tag, 0) + 1
                )

    for lang, total in report.total_counts.items():
        if total == 0:
            continue
        fuzzy = report.fuzzy_counts.get(lang, 0)
        if fuzzy == total:
            report.errors.append(f"Catalog for {lang} has only fuzzy entries")
        if max_fuzzy_ratio < 100.0:
            ratio = (fuzzy / total) * 100.0
            if ratio > max_fuzzy_ratio:
                report.errors.append(
                    f"Catalog for {lang} is {ratio:.1f}% fuzzy "
                    f"(exceeds {max_fuzzy_ratio:.1f}% limit)"
                )

    return report


def lint_po_files_default() -> I18nLintReport:
    """Lint PO files under the default project root.

    Convenience wrapper for callers that do not need to override the root.
    """
    return lint_po_files(PROJECT_ROOT)
