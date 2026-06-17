"""English i18n synchronization engine.

Merges translations from a gettext PO catalog back into canonical Turtle source
files using a 3-way merge.  The PO file records the previous English value in
``msgid`` and the proposed new value in ``msgstr``; ``msgctxt`` encodes the
subject and predicate identity.  Only literals carrying the internal
``@x-gmeow-english`` tag are updated, and the original Turtle formatting is
preserved by text-level replacement rather than re-serialization.

Principle 4 (one canonical source): this tool writes back to the authored
Turtle modules, never to generated artifacts.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from rdflib import Graph, Literal, URIRef


class PoParseError(ValueError):
    """Raised when a PO file cannot be parsed."""


@dataclass(frozen=True, slots=True)
class PoEntry:
    """One PO catalog entry.

    Attributes:
        msgctxt: Subject-predicate identity, formatted as ``"<IRI>|<predicate>"``.
        msgid: The English value at the time of extraction / last sync.
        msgstr: The proposed new English value.
    """

    msgctxt: str
    msgid: str
    msgstr: str


@dataclass
class SyncReport:
    """Result of synchronizing one PO file against one Turtle source.

    Attributes:
        changed_files: Paths of Turtle files that were modified.
        conflicts: Human-readable conflict descriptions.
        skipped: Human-readable descriptions of skipped entries.
        unchanged: Human-readable descriptions of unchanged entries.
    """

    changed_files: list[Path] = field(default_factory=list)
    conflicts: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)
    unchanged: list[str] = field(default_factory=list)


#: Matches a Turtle literal (single or triple quoted) and its optional suffix.
_LITERAL_RE = re.compile(
    r"(?P<literal>"
    r'(?P<triple>""")(?P<lexical_triple>[\s\S]*?)(?P=triple)'
    r'|"(?P<lexical_single>(?:[^"\\]|\\.)*?)"'
    r")"
    r"(?P<suffix>(?:@[^\s.,;[\]{}()]+|\^\^[^\s.,;[\]{}()]+))?"
    r"(?=[\s.,;[\]{}()]|$)",
    re.DOTALL,
)

#: Matches an @prefix declaration in a Turtle file.
_PREFIX_RE = re.compile(
    r"@prefix\s+([a-zA-Z_][a-zA-Z0-9_-]*)\s*:\s*<([^>]+)>\s*\.",
)

#: Matches a standalone PO continuation string.
_PO_STRING_RE = re.compile(r'^"(?:[^"\\]|\\.)*"$')

#: Structural Turtle tokens used to locate the subject+predicate of a literal.
_CONTEXT_TOKEN_RE = re.compile(
    r"(?P<iri><[^>]+>)"
    r"|(?P<prefixed>[a-zA-Z_][a-zA-Z0-9_-]*:[a-zA-Z_][a-zA-Z0-9_-]*)"
    r"|(?P<bnode>_:[a-zA-Z_][a-zA-Z0-9_-]*)"
    r"|(?P<bnstart>\[)"
    r"|(?P<bnend>\])"
    r"|(?P<sep>[;,])"
    r"|(?P<dot>\.)"
    r"|(?P<keyword_a>\ba\b)"
)

_SUBJECT_KINDS = frozenset({"iri", "prefixed", "bnode", "bnstart"})
_PREDICATE_INTRODUCER_KINDS = _SUBJECT_KINDS | {"bnend", "sep"}


def _unescape_po(value: str) -> str:
    r"""Reverse PO escape sequences in *value*.

    Handles the sequences required by the task: ``\"``, ``\\``, and ``\n``.
    Other C-style escapes are passed through so the parser stays minimal.
    """
    value = value.replace("\\\\", "\x00")
    value = value.replace('\\"', '"')
    value = value.replace("\\n", "\n")
    value = value.replace("\\t", "\t")
    value = value.replace("\\r", "\r")
    return value.replace("\x00", "\\")


def _unescape_turtle(value: str) -> str:
    r"""Decode a Turtle string literal into its lexical form.

    Handles ``\n``, ``\t``, ``\r``, ``\\``, ``\"``, ``\uXXXX`` and
    ``\UXXXXXXXX`` while leaving literal UTF-8 characters untouched.
    """
    result: list[str] = []
    i = 0
    length = len(value)
    while i < length:
        ch = value[i]
        if ch != "\\":
            result.append(ch)
            i += 1
            continue

        if i + 1 >= length:
            raise PoParseError("invalid Turtle escape sequence")

        next_ch = value[i + 1]
        if next_ch == "n":
            result.append("\n")
            i += 2
        elif next_ch == "t":
            result.append("\t")
            i += 2
        elif next_ch == "r":
            result.append("\r")
            i += 2
        elif next_ch == "\\":
            result.append("\\")
            i += 2
        elif next_ch == '"':
            result.append('"')
            i += 2
        elif next_ch == "u":
            if i + 6 > length:
                raise PoParseError("invalid Turtle escape sequence")
            hex_chars = value[i + 2 : i + 6]
            try:
                result.append(chr(int(hex_chars, 16)))
            except ValueError as exc:
                raise PoParseError(f"invalid Turtle escape sequence: {exc}") from exc
            i += 6
        elif next_ch == "U":
            if i + 10 > length:
                raise PoParseError("invalid Turtle escape sequence")
            hex_chars = value[i + 2 : i + 10]
            try:
                result.append(chr(int(hex_chars, 16)))
            except ValueError as exc:
                raise PoParseError(f"invalid Turtle escape sequence: {exc}") from exc
            i += 10
        else:
            raise PoParseError("invalid Turtle escape sequence")

    return "".join(result)


def _escape_turtle_single(value: str) -> str:
    """Escape *value* for a Turtle single-quoted string literal."""
    return (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def _escape_turtle_triple(value: str) -> str:
    """Escape *value* for a Turtle triple-quoted string literal."""
    return value.replace("\\", "\\\\").replace('"""', '\\"""')


def _concat_po_strings(tokens: list[str]) -> str:
    """Concatenate one or more PO quoted tokens and unescape the result."""
    parts: list[str] = []
    for token in tokens:
        if token.startswith('"""') and token.endswith('"""'):
            parts.append(token[3:-3])
        elif token.startswith('"') and token.endswith('"'):
            parts.append(token[1:-1])
        else:
            raise PoParseError(f"invalid PO string token: {token!r}")
    return _unescape_po("".join(parts))


def _make_entry(fields: dict[str, list[str]]) -> PoEntry:
    """Build a :class:`PoEntry` from parsed fields."""
    if "msgid" not in fields:
        raise PoParseError("PO entry missing msgid")
    return PoEntry(
        msgctxt=_concat_po_strings(fields.get("msgctxt", ['""'])),
        msgid=_concat_po_strings(fields["msgid"]),
        msgstr=_concat_po_strings(fields.get("msgstr", ['""'])),
    )


def parse_po(text: str, *, require_msgctxt: bool = True) -> list[PoEntry]:
    """Parse a PO file into a list of :class:`PoEntry` records.

    Supports single-line ``"..."`` and multi-line ``\"\"\"...\"\"\"`` strings,
    PO escape sequences, comments, and unknown fields are skipped.  Continuation
    lines are concatenated as specified by gettext.

    Args:
        text: Raw PO file content.
        require_msgctxt: When ``True`` (the default), entries without a
            ``msgctxt`` are dropped.  Markdown PO files set this to ``False``
            because their identity is the segment content itself.

    Returns:
        A list of parsed entries in source order.

    Raises:
        PoParseError: If a structural error is encountered.
    """
    entries: list[PoEntry] = []
    fields: dict[str, list[str]] = {}
    current_key: str | None = None

    def flush() -> None:
        nonlocal fields, current_key
        if fields:
            try:
                entry = _make_entry(fields)
            except PoParseError:
                entry = None
            if entry and (entry.msgctxt or not require_msgctxt):
                entries.append(entry)
        fields = {}
        current_key = None

    header_key = re.compile(r"^(msgctxt|msgid|msgstr)\s+")
    single_token = re.compile(
        r"^(msgctxt|msgid|msgstr)\s+"
        r'((?:"""[\s\S]*?""")|(?:"(?:[^"\\]|\\.)*"))$'
    )

    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            flush()
            continue

        m = single_token.match(line)
        if m:
            current_key = m.group(1)
            fields.setdefault(current_key, []).append(m.group(2))
            continue

        if _PO_STRING_RE.match(line):
            if current_key is None:
                raise PoParseError(f"PO continuation line without a field: {line!r}")
            fields[current_key].append(line)
            continue

        if header_key.match(line):
            # An unknown or empty gettext field (e.g. ``msgid_plural``) starts a
            # new logical key but we do not store its value.  Reset continuation
            # state so following string lines are not attached to the previous
            # known field.
            current_key = None
            continue

        # Any other unrecognised line ends the current entry.
        flush()

    flush()
    return entries


def _extract_prefixes(text: str) -> dict[str, str]:
    """Return the ``prefix -> namespace`` map declared in a Turtle file."""
    return {m.group(1): m.group(2) for m in _PREFIX_RE.finditer(text)}


def _iri_text_forms(iri: str, prefixes: dict[str, str]) -> list[str]:
    """Return text forms to look for when locating *iri* in a Turtle file.

    Includes the bracketed full IRI plus every declared prefixed name that
    expands to *iri*.
    """
    forms = [f"<{iri}>"]
    for prefix, namespace in prefixes.items():
        if iri.startswith(namespace):
            local = iri[len(namespace) :]
            if local and re.match(r"^[a-zA-Z_][a-zA-Z0-9_-]*$", local):
                forms.append(f"{prefix}:{local}")
    return forms


def _tokenize_turtle(text: str, end: int | None = None) -> list[tuple[int, str, str]]:
    """Return structural Turtle tokens as ``(position, kind, value)`` up to *end*.

    String literals (double quoted only, matching the rest of this module) and
    comments are skipped so that periods, semicolons, and IRIs inside literals
    cannot be mistaken for statement structure.
    """
    if end is None:
        end = len(text)
    tokens: list[tuple[int, str, str]] = []
    i = 0
    while i < end:
        ch = text[i]
        if ch.isspace():
            i += 1
            continue
        if ch == "#":
            while i < end and text[i] != "\n":
                i += 1
            continue
        if text.startswith('"""', i):
            j = text.find('"""', i + 3, end)
            if j < 0:
                break
            i = j + 3
            continue
        if ch == '"':
            j = i + 1
            while j < end:
                if text[j] == "\\" and j + 1 < end:
                    j += 2
                elif text[j] == '"':
                    j += 1
                    break
                else:
                    j += 1
            i = j
            continue
        m = _CONTEXT_TOKEN_RE.match(text, i)
        if m:
            kind = m.lastgroup
            assert kind is not None
            tokens.append((i, kind, m.group(0)))
            i = m.end()
            continue
        i += 1
    return tokens


def _extract_context(
    text: str,
    pos: int,
    subject_forms: list[str],
    predicate_forms: list[str],
) -> tuple[str | None, str | None]:
    """Return the nearest subject+predicate text forms for the literal at *pos*.

    Scans structural tokens before *pos* to find the predicate that actually
    introduces the literal, then the subject that owns that predicate.  Because
    literals and comments are skipped by the tokenizer, punctuation inside
    literals cannot be mistaken for a statement boundary.
    """
    tokens = _tokenize_turtle(text, pos)
    idx = len(tokens)
    for i, (tpos, _kind, _value) in enumerate(tokens):
        if tpos >= pos:
            idx = i
            break

    predicate: str | None = None
    predicate_idx = -1
    # Scan backward from the literal, skipping object tokens, until we find a
    # token that is in predicate position (after a subject or a ';' separator).
    for i in range(idx - 1, -1, -1):
        _tpos, kind, value = tokens[i]
        if kind == "dot":
            break
        if kind == "sep":
            # ';' introduces a predicate, ',' introduces another object.
            # Both tell us to keep scanning backward.
            continue
        if kind not in ("iri", "prefixed", "keyword_a"):
            continue
        if i == 0:
            continue
        prev_kind, prev_value = tokens[i - 1][1], tokens[i - 1][2]
        if prev_kind not in _PREDICATE_INTRODUCER_KINDS:
            continue
        if prev_kind == "sep" and prev_value != ";":
            continue
        predicate = value
        predicate_idx = i
        break

    if predicate is None or predicate not in predicate_forms:
        return None, predicate

    subject: str | None = None
    for i in range(predicate_idx - 1, -1, -1):
        _tpos, kind, value = tokens[i]
        if kind == "dot":
            break
        if kind not in _SUBJECT_KINDS:
            continue
        if kind == "bnstart":
            subject = value
            break
        if i == 0:
            subject = value
            break
        prev_kind = tokens[i - 1][1]
        if prev_kind == "dot":
            subject = value
            break

    return subject, predicate


def _filter_by_context(
    text: str,
    candidates: list[tuple[int, int, str, str]],
    subject_forms: list[str],
    predicate_forms: list[str],
) -> list[tuple[int, int, str, str]]:
    """Keep only literal occurrences governed by the expected subject+predicate."""
    scoped: list[tuple[int, int, str, str]] = []
    for start, end, quote_style, suffix in candidates:
        subject, predicate = _extract_context(
            text, start, subject_forms, predicate_forms
        )
        if subject in subject_forms and predicate in predicate_forms:
            scoped.append((start, end, quote_style, suffix))
    return scoped


def _replace_literal_in_text(
    text: str,
    subject: URIRef,
    predicate: URIRef,
    old_value: str,
    new_value: str,
) -> tuple[str, str | None]:
    """Replace one English literal in *text* while preserving formatting.

    Returns the updated text, or ``(text, error)`` if the literal cannot be
    found or is ambiguous.
    """
    prefixes = _extract_prefixes(text)
    subject_forms = _iri_text_forms(str(subject), prefixes)
    predicate_forms = _iri_text_forms(str(predicate), prefixes)

    candidates: list[tuple[int, int, str, str]] = []
    for m in _LITERAL_RE.finditer(text):
        if m.group("triple"):
            lexical = m.group("lexical_triple") or ""
            quote_style = "triple"
        else:
            lexical = m.group("lexical_single") or ""
            quote_style = "single"
        try:
            decoded = _unescape_turtle(lexical)
        except PoParseError:
            continue
        if decoded != old_value:
            continue
        suffix = m.group("suffix") or ""
        candidates.append((m.start(), m.end(), quote_style, suffix))

    if not candidates:
        return text, f"literal {old_value!r} not found in source text"

    if len(candidates) > 1:
        scoped = _filter_by_context(text, candidates, subject_forms, predicate_forms)
        if len(scoped) == 1:
            candidates = scoped
        else:
            return text, (
                f"conflict: ambiguous literal {old_value!r}: "
                f"{len(candidates)} occurrences in source text"
            )

    start, end, quote_style, suffix = candidates[0]
    if quote_style == "triple":
        if '"""' in new_value:
            return text, "new value contains the triple-quote sequence"
        replacement = f'"""{_escape_turtle_triple(new_value)}"""{suffix}'
    else:
        replacement = f'"{_escape_turtle_single(new_value)}"{suffix}'

    return text[:start] + replacement + text[end:], None


def _current_english_literal(
    graph: Graph, subject: URIRef, predicate: URIRef
) -> Literal | None:
    """Return the unique ``@x-gmeow-english`` literal for the triple pattern."""
    matches: list[Literal] = []
    for obj in graph.objects(subject, predicate):
        if isinstance(obj, Literal) and obj.language == "x-gmeow-english":
            matches.append(obj)
    if not matches:
        return None
    # Multiple identical lexical values are harmless; distinct values are a
    # structural problem that the caller should treat as ambiguous.
    distinct = {str(m) for m in matches}
    if len(distinct) > 1:
        raise PoParseError(
            f"multiple distinct @x-gmeow-english literals for {subject} {predicate}"
        )
    return matches[0]


def _apply_entry(entry: PoEntry, ttl_text: str, graph: Graph) -> tuple[str, str | None]:
    """Apply one PO entry to *ttl_text* using a 3-way merge.

    Returns ``(new_text, None)`` on success, ``(ttl_text, reason)`` on skip or
    conflict.  The reason string is suitable for a :class:`SyncReport`.
    """
    identity = entry.msgctxt
    if "|" not in identity:
        return ttl_text, f"malformed identity {identity!r}"

    subject_iri, predicate_iri = identity.split("|", 1)
    try:
        subject = URIRef(subject_iri)
        predicate = URIRef(predicate_iri)
    except ValueError as exc:
        return ttl_text, f"invalid IRI in identity {identity!r}: {exc}"

    old_value = entry.msgid
    new_value = entry.msgstr

    try:
        current_literal = _current_english_literal(graph, subject, predicate)
    except PoParseError as exc:
        return ttl_text, str(exc)
    if current_literal is None:
        return ttl_text, f"no @x-gmeow-english literal for {identity}"

    current_value = str(current_literal)

    if old_value == current_value and old_value == new_value:
        return ttl_text, None
    if old_value == current_value and old_value != new_value:
        return _replace_literal_in_text(
            ttl_text, subject, predicate, current_value, new_value
        )
    if old_value != current_value and old_value == new_value:
        return ttl_text, f"source changed, PO unchanged for {identity}"

    # old_value != current_value and old_value != new_value
    if current_value == new_value:
        # Source already contains the proposed value; no update needed.
        return ttl_text, None
    return (
        ttl_text,
        f"conflict: source and PO both changed for {identity}",
    )


def sync_english_from_po(
    po_path: Path,
    ttl_path: Path,
    *,
    dry_run: bool = False,
) -> SyncReport:
    """Synchronize English translations from *po_path* into *ttl_path*.

    The PO file supplies ``msgctxt`` (subject|predicate), ``msgid`` (old
    English), and ``msgstr`` (new English).  The function performs a 3-way
    merge against the current Turtle source and, unless *dry_run* is ``True``,
    writes the result back to *ttl_path*.

    Args:
        po_path: Path to the PO catalog.
        ttl_path: Path to the Turtle source file to update.
        dry_run: If ``True``, compute the report without writing to disk.

    Returns:
        A :class:`SyncReport` describing the outcome.
    """
    report = SyncReport()
    po_text = po_path.read_text(encoding="utf-8")
    entries = parse_po(po_text)
    ttl_text = ttl_path.read_text(encoding="utf-8")

    try:
        graph = Graph()
        graph.parse(data=ttl_text, format="turtle")
    except Exception as exc:
        report.conflicts.append(f"failed to parse {ttl_path}: {exc}")
        return report

    changed = False
    for entry in entries:
        if not entry.msgctxt:
            report.skipped.append(f"empty msgctxt for msgid {entry.msgid!r}")
            continue

        new_text, error = _apply_entry(entry, ttl_text, graph)
        if error:
            if error.startswith("conflict:"):
                report.conflicts.append(error)
            else:
                report.skipped.append(error)
            continue

        if new_text != ttl_text:
            changed = True
            ttl_text = new_text
            # Re-parse so subsequent entries see the updated current values.
            try:
                graph = Graph()
                graph.parse(data=ttl_text, format="turtle")
            except Exception as exc:
                report.conflicts.append(
                    f"failed to re-parse {ttl_path} after update: {exc}"
                )
                return report
        else:
            report.unchanged.append(entry.msgctxt)

    if changed:
        report.changed_files.append(ttl_path)
        if not dry_run:
            ttl_path.write_text(ttl_text, encoding="utf-8")

    return report


def _find_segment_positions(text: str, segment: str) -> list[int]:
    """Return the start index of every non-overlapping occurrence of *segment*."""
    if not segment:
        return []
    positions: list[int] = []
    start = 0
    segment_len = len(segment)
    while True:
        idx = text.find(segment, start)
        if idx < 0:
            break
        positions.append(idx)
        start = idx + segment_len
    return positions


def _apply_md_entry(md_text: str, entry: PoEntry) -> tuple[str, str | None]:
    """Apply one markdown PO entry to *md_text* using a 3-way merge.

    Returns ``(new_text, None)`` when the entry is applied or requires no
    change, and ``(md_text, reason)`` when it is skipped or conflicts.
    """
    old_value = entry.msgid
    new_value = entry.msgstr

    if not old_value:
        return md_text, "empty msgid"

    positions = _find_segment_positions(md_text, old_value)
    if len(positions) > 1:
        return md_text, f"ambiguous segment {old_value!r}: {len(positions)} occurrences"

    if not positions:
        if old_value == new_value:
            return md_text, f"source changed, PO unchanged for segment {old_value!r}"
        return md_text, (
            f"conflict: source and PO both changed for segment {old_value!r}"
        )

    start = positions[0]
    if old_value == new_value:
        return md_text, None

    return md_text[:start] + new_value + md_text[start + len(old_value) :], None


def apply_md_sync(
    po_path: Path,
    md_path: Path,
    *,
    dry_run: bool = False,
) -> SyncReport:
    """Synchronize English translations from *po_path* into *md_path*.

    The PO file supplies ``msgid`` (old English segment) and ``msgstr`` (new
    English segment).  ``msgctxt`` is ignored for markdown masters because
    identity is the segment content itself.  The function performs a 3-way
    merge against the current markdown source and, unless *dry_run* is
    ``True``, writes the result back to *md_path*.

    Args:
        po_path: Path to the PO catalog.
        md_path: Path to the markdown source file to update.
        dry_run: If ``True``, compute the report without writing to disk.

    Returns:
        A :class:`SyncReport` describing the outcome.
    """
    report = SyncReport()
    po_text = po_path.read_text(encoding="utf-8")
    entries = parse_po(po_text, require_msgctxt=False)
    md_text = md_path.read_text(encoding="utf-8")

    changed = False
    for entry in entries:
        new_text, error = _apply_md_entry(md_text, entry)
        if error:
            if error.startswith("conflict:"):
                report.conflicts.append(error)
            else:
                report.skipped.append(error)
            continue

        if new_text != md_text:
            changed = True
            md_text = new_text
        else:
            report.unchanged.append(entry.msgid)

    if changed:
        report.changed_files.append(md_path)
        if not dry_run:
            md_path.write_text(md_text, encoding="utf-8")

    return report


def apply_ttl_sync(
    po_path: Path,
    ttl_path: Path,
    *,
    dry_run: bool = False,
) -> SyncReport:
    """Synchronize English translations from *po_path* into *ttl_path*.

    Thin wrapper around :func:`sync_english_from_po` that exposes the
    symmetric ``apply_*_sync`` API used by :func:`sync_english_file`.
    """
    return sync_english_from_po(po_path, ttl_path, dry_run=dry_run)


def sync_english_file(
    po_path: Path,
    source_path: Path,
    *,
    dry_run: bool = False,
) -> SyncReport:
    """Synchronize a PO catalog with its canonical source file.

    Dispatches to :func:`apply_ttl_sync` for Turtle sources or
    :func:`apply_md_sync` for Markdown sources.  The decision is driven by
    *source_path*'s extension, with the PO filename convention
    (``*.ttl.po`` / ``*.md.po``) as a fallback.

    Args:
        po_path: Path to the PO catalog.
        source_path: Path to the canonical source file (``.ttl`` or ``.md``).
        dry_run: If ``True``, compute the report without writing to disk.

    Returns:
        A :class:`SyncReport` describing the outcome.

    Raises:
        ValueError: If *source_path* is neither a Turtle nor a Markdown file
            and the PO filename convention does not resolve the type.
    """
    po_name = po_path.name
    if (
        source_path.suffix == ".ttl"
        or po_name.endswith(".ttl.po")
        or po_name.endswith(".ttl.pot")
    ):
        return apply_ttl_sync(po_path, source_path, dry_run=dry_run)

    if (
        source_path.suffix == ".md"
        or po_name.endswith(".md.po")
        or po_name.endswith(".md.pot")
    ):
        return apply_md_sync(po_path, source_path, dry_run=dry_run)

    raise ValueError(f"unsupported source file type: {source_path}")
