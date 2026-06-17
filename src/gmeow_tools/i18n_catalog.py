"""Shared i18n catalog for translatable ontology prose.

This module is the canonical source of truth for discovering localizable strings
in GMEOW ontology graphs, producing gettext POT catalogs, reading translated PO
catalogs, and merging translations back into RDF graphs.

Principle 4 (one canonical source): the authored Turtle modules and PO files are
the canonical sources; this catalog is the shared library that operates on them.
"""

from __future__ import annotations

import re
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS, RDFS, SKOS

from gmeow_tools.config import NAMESPACE, PREFIXES
from gmeow_tools.i18n_sync import PoEntry, parse_po
from gmeow_tools.language_tags import _default_inverse_tag_map

GMEOW = Namespace(NAMESPACE)

#: Predicates whose object literals are human-readable, translatable ontology prose.
LOCALIZABLE_PREDICATES: frozenset[URIRef] = frozenset(
    {
        RDFS.label,
        RDFS.comment,
        SKOS.definition,
        SKOS.scopeNote,
        SKOS.example,
        SKOS.prefLabel,
        SKOS.altLabel,
        SKOS.note,
        DCTERMS.title,
        DCTERMS.description,
        GMEOW.name,
        GMEOW.title,
        GMEOW.description,
        GMEOW.fullName,
    }
)

#: Internal authoring language tag used for English source strings.
_ENGLISH_TAG = "x-gmeow-english"

#: Regular expression to extract the ``Language:`` header from a PO file.
_LANGUAGE_HEADER_RE = re.compile(r'^"Language:\s*([^"\\]+)\\n"', re.MULTILINE)


@dataclass(frozen=True, slots=True)
class TranslationKey:
    """Identity + source string of one translatable ontology literal.

    Attributes:
        slice_iri: The slice manifest IRI that owns the term, or a deterministic
            namespace fallback for terms that do not live under ``/slices/``.
        term_iri: The subject IRI whose predicate carries the literal.
        predicate: The full predicate IRI (e.g. ``http://www.w3.org/2000/01/rdf-schema#label``).
        english_value: The source lexical value (the POT ``msgid``).
    """

    slice_iri: str
    term_iri: str
    predicate: str
    english_value: str


def _term_namespace(iri: str) -> str:
    """Return a deterministic namespace/fallback for *iri*.

    For ``https://blackcatinformatics.ca/gmeow/Entity`` the namespace is
    ``https://blackcatinformatics.ca/gmeow/``. This is used as the slice-like
    grouping key for terms that are not under a ``/slices/`` path.
    """
    # Prefer the last '#' or '/' boundary, keeping the delimiter.
    for delimiter in ("#", "/"):
        idx = iri.rfind(delimiter)
        if idx >= 0:
            return iri[: idx + 1]
    return iri


def _slice_iri_for_term(term_iri: str) -> str:
    """Map a term IRI to its owning slice IRI or a deterministic fallback."""
    if "/slices/" in term_iri:
        # Slice manifest IRIs are ``{NAMESPACE}slices/<name>``.
        prefix = f"{NAMESPACE}slices/"
        rest = (
            term_iri[len(prefix) :]
            if term_iri.startswith(prefix)
            else term_iri.split("/slices/", 1)[1]
        )
        name = rest.split("/", 1)[0]
        return f"{NAMESPACE}slices/{name}"
    return _term_namespace(term_iri)


def extract_terms(
    graph: Graph,
    *,
    slice_resolver: Callable[[str, str, str], str | None] | None = None,
) -> Iterator[TranslationKey]:
    """Yield :class:`TranslationKey` records for localizable literals in *graph*.

    Walks every triple whose predicate is in :data:`LOCALIZABLE_PREDICATES` and
    whose object is a ``Literal``. Accepts literals tagged
    ``@x-gmeow-english`` first; untagged literals are used as a fallback. If a
    subject+predicate carries multiple distinct ``@x-gmeow-english`` values in
    the same slice, a :class:`ValueError` is raised.

    When ``slice_resolver`` is supplied it is called as
    ``slice_resolver(term_iri, predicate_iri, lexical_value)`` and may return a
    slice IRI to override the default path-derived grouping. Returning ``None``
    falls back to the path-based heuristic.

    Results are yielded sorted deterministically by
    ``(slice_iri, term_iri, predicate)``.
    """
    collected: dict[tuple[str, str, str], tuple[str, str]] = {}
    english_values: dict[tuple[str, str, str], set[str]] = {}

    for subject, predicate, obj in graph:
        if not isinstance(subject, URIRef) or predicate not in LOCALIZABLE_PREDICATES:
            continue
        if not isinstance(obj, Literal):
            continue

        term_iri = str(subject)
        pred_iri = str(predicate)
        lexical = str(obj)
        slice_iri: str | None = None
        if slice_resolver is not None:
            slice_iri = slice_resolver(term_iri, pred_iri, lexical)
        if slice_iri is None:
            slice_iri = _slice_iri_for_term(term_iri)
        key = (slice_iri, term_iri, pred_iri)

        if obj.language == _ENGLISH_TAG:
            english_values.setdefault(key, set()).add(lexical)
            if len(english_values[key]) > 1:
                raise ValueError(
                    f"multiple distinct @x-gmeow-english values for "
                    f"{term_iri} {pred_iri} in {slice_iri}"
                )
            collected[key] = (_ENGLISH_TAG, lexical)
        elif obj.language is None and key not in collected:
            # Untagged fallback only used when no tagged English value exists.
            collected[key] = ("", lexical)

    for key in sorted(collected):
        slice_iri, term_iri, pred_iri = key
        _tag, value = collected[key]
        yield TranslationKey(
            slice_iri=slice_iri,
            term_iri=term_iri,
            predicate=pred_iri,
            english_value=value,
        )


def _po_escape(value: str) -> str:
    """Escape *value* for a PO quoted string.

    Handles backslash, double-quote, and newline characters.
    """
    return value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def _po_unescape(value: str) -> str:
    """Reverse PO escape sequences in *value*.

    Handles backslash, escaped double-quote, and escaped newline sequences.
    """
    value = value.replace("\\\\", "\x00")
    value = value.replace('\\"', '"')
    value = value.replace("\\n", "\n")
    return value.replace("\x00", "\\")


def build_pot(entries: list[TranslationKey]) -> str:
    """Return a gettext POT file as a string.

    The header carries the required SPDX comments, ``Project-Id-Version``,
    ``Content-Type``, and ``Content-Transfer-Encoding`` fields. Each entry is
    emitted as ``#: slice_iri``, ``msgctxt "term_iri|predicate"``,
    ``msgid "english_value"``, ``msgstr ""``.
    """
    lines: list[str] = [
        "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc.",
        "# SPDX-License-Identifier: CC-BY-4.0",
        "#",
        "# GMEOW translation template. Generated file — do not edit directly.",
        'msgid ""',
        'msgstr ""',
        '"Project-Id-Version: gmeow\\n"',
        '"MIME-Version: 1.0\\n"',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Content-Transfer-Encoding: 8bit\\n"',
        "",
    ]

    for entry in entries:
        lines.append(f"#: {entry.slice_iri}")
        lines.append(
            f'msgctxt "{_po_escape(entry.term_iri)}|{_po_escape(entry.predicate)}"'
        )
        lines.append(f'msgid "{_po_escape(entry.english_value)}"')
        lines.append('msgstr ""')
        lines.append("")

    return "\n".join(lines)


def _language_from_po(text: str) -> str:
    """Extract the ``Language:`` value from a PO file header."""
    for line in text.splitlines():
        m = _LANGUAGE_HEADER_RE.match(line)
        if m:
            return m.group(1).strip()
    raise ValueError("PO file is missing a Language header")


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


def load_po_catalog(path: Path) -> dict[tuple[str, str, str], str]:
    """Parse a ``.po`` file and return its translation mapping.

    Returns ``{(term_iri, predicate, internal_lang_tag): msgstr}``. The internal
    language tag is resolved from the PO ``Language:`` header via the merged
    ontology's inverse tag map (e.g. ``fr`` -> ``x-gmeow-french``). Entries with
    an empty ``msgstr`` are skipped.
    """
    text = path.read_text(encoding="utf-8")
    bcp_lang = _language_from_po(text)
    inverse_map = _default_inverse_tag_map()
    internal_tag = inverse_map.get(bcp_lang.lower())
    if internal_tag is None:
        raise ValueError(f"PO language '{bcp_lang}' has no GMEOW internal tag mapping")

    catalog: dict[tuple[str, str, str], str] = {}
    for entry in parse_po(text):
        if not entry.msgctxt or "|" not in entry.msgctxt:
            continue
        term_iri, predicate = entry.msgctxt.split("|", 1)
        predicate = _expand_predicate(predicate)
        msgstr = _po_unescape(entry.msgstr)
        if not msgstr:
            continue
        catalog[(term_iri, predicate, internal_tag)] = msgstr

    return catalog


def merge_terms(base_graph: Graph, po_paths: list[Path]) -> Graph:
    """Return a new graph with translations from *po_paths* merged into *base_graph*.

    The returned graph contains all triples from *base_graph* plus, for each PO
    file, triples of the form ``(term_iri, predicate, Literal(msgstr, lang=tag))``.
    *base_graph* is not mutated.
    """
    merged = Graph()
    for triple in base_graph:
        merged.add(triple)

    for path in sorted(po_paths):
        catalog = load_po_catalog(path)
        for (term_iri, predicate, internal_tag), msgstr in catalog.items():
            merged.add(
                (
                    URIRef(term_iri),
                    URIRef(predicate),
                    Literal(msgstr, lang=internal_tag),
                )
            )

    return merged


def _po_entry_lines(entry: PoEntry) -> list[str]:
    """Render one :class:`PoEntry` as PO source lines."""
    return [
        f'msgctxt "{_po_escape(entry.msgctxt)}"',
        f'msgid "{_po_escape(entry.msgid)}"',
        f'msgstr "{_po_escape(entry.msgstr)}"',
        "",
    ]


def _write_po_header(path: Path, *, language: str | None = None) -> None:
    """Write a PO/POT header to *path*."""
    lines: list[str] = [
        "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc.",
        "# SPDX-License-Identifier: CC-BY-4.0",
        "#",
        "# GMEOW translation catalog. Generated file — do not edit directly.",
        'msgid ""',
        'msgstr ""',
        '"Project-Id-Version: gmeow\\n"',
    ]
    if language is not None:
        lines.append(f'"Language: {language}\\n"')
    lines.extend(
        [
            '"MIME-Version: 1.0\\n"',
            '"Content-Type: text/plain; charset=UTF-8\\n"',
            '"Content-Transfer-Encoding: 8bit\\n"',
            "",
            "",
        ]
    )
    path.write_text("\n".join(lines), encoding="utf-8")


def write_po(path: Path, entries: list[PoEntry], lang: str) -> None:
    """Write a ``.po`` file with the given language header and entries."""
    _write_po_header(path, language=lang)
    if not entries:
        return
    lines: list[str] = []
    for entry in sorted(entries, key=lambda e: e.msgctxt):
        lines.extend(_po_entry_lines(entry))
    with path.open("a", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
        if not lines[-1].endswith("\n"):
            fh.write("\n")


def write_pot(path: Path, entries: list[PoEntry]) -> None:
    """Write a ``.pot`` template file (no ``Language:`` header)."""
    _write_po_header(path, language=None)
    if not entries:
        return
    lines: list[str] = []
    for entry in sorted(entries, key=lambda e: e.msgctxt):
        lines.extend(_po_entry_lines(entry))
    with path.open("a", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
        if not lines[-1].endswith("\n"):
            fh.write("\n")
