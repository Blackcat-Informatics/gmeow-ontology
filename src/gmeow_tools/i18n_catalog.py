"""Shared i18n catalog for translatable ontology prose.

This module is the canonical source of truth for discovering localizable strings
in GMEOW ontology graphs, producing gettext POT catalogs, reading translated PO
catalogs, and merging translations back into RDF graphs and Markdown docs.

Principle 4 (one canonical source): the authored Turtle modules and PO files are
the canonical sources; this catalog is the shared library that operates on them.
"""

from __future__ import annotations

import contextlib
import csv
import hashlib
import re
import sys
from collections.abc import Callable, Iterable, Iterator
from dataclasses import dataclass
from pathlib import Path
from xml.sax.saxutils import escape

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS, RDFS, SKOS

from gmeow_tools.config import NAMESPACE, PREFIXES, PROJECT_ROOT
from gmeow_tools.i18n_sync import PoEntry, PoParseError, parse_po
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

#: Hard-coded English UI strings used by the ontology-docs generator.
#: These are extracted into ``ontology-docs-templates.pot`` and can be
#: translated so that generated documentation pages render in other languages.
_ONTOLOGY_DOCS_TEMPLATES: dict[str, str] = {
    # Category labels
    "category_class": "Classes",
    "category_property": "Properties",
    "category_individual": "Individuals",
    "category_datatype": "Datatypes",
    # Site navigation
    "nav_home": "Home",
    "nav_getting_started": "Getting Started",
    "nav_learning_paths": "Learning Paths",
    "nav_recipes": "Recipes",
    "nav_examples": "Examples",
    "nav_concerns": "Concerns",
    "nav_four_boxes": "Four Boxes",
    "nav_slices": "Slices",
    "nav_adoption": "Adoption",
    "nav_linkages": "Linkages",
    "nav_bibliography": "Bibliography",
    "nav_reference": "Reference",
    "nav_external": "External",
    "nav_rdf12": "RDF 1.2",
    "nav_integrity": "Integrity Constraints",
    # Generic page titles
    "page_index": "Index",
    "page_getting_started": "Getting Started",
    "page_recipes": "Recipes",
    "page_learning_paths": "Learning Paths",
    "page_examples": "Examples",
    "page_about": "About GMEOW",
    "page_changelog": "Changelog",
    "page_visualizations": "Visualizations",
    "page_quality_gates": "Quality Gates",
    "page_references": "References",
    "page_reference": "Reference",
    "page_slices": "Slices",
    "page_linkages": "Linkages",
    "page_adoption_targets": "Adoption Targets",
    "page_external_ontologies": "External Ontologies",
    "page_external_terms": "External Terms",
    "page_statements": "RDF 1.2 Statement Layer",
    "page_search": "Search",
    # Section headings
    "section_start_here": "Start Here",
    "section_profiles": "Profiles",
    "section_slices": "Slices",
    "section_reference": "Reference",
    "section_distribution": "Distribution",
    "section_static_indexes": "Static Indexes",
    "section_install": "Install",
    "section_export_docs": "Export the bundled docs",
    "section_pick_first_path": "Pick a first path",
    "section_inspect_terms": "Inspect terms while reading examples",
    "section_read_slices": "Read slices as doctrine, not just reference",
    "section_read_next": "Read Next",
    "section_external_vocabulary_coverage": "External Vocabulary Coverage",
    "section_recipes": "Recipes",
    "section_references": "References",
    # Footer
    "footer_generated": (
        "Generated from the GMEOW ontology. Canonical source is RDF/OWL; this "
        "site is a deterministic projection."
    ),
    "footer_cite_prefix": "Cite as",
    "footer_license": "Ontology licensed CC BY 4.0",
    # Accessibility / misc
    "skip_to_content": "Skip to content",
    "open_canonical_page": "Open the canonical reference page.",
    "generated_documentation": "Generated documentation",
    "module": "Module",
}

#: Active template catalog used by the ontology-docs renderer.  Override this
#: temporarily (e.g. via :func:`translated_ontology_docs_templates`) to render
#: docs in another language.
_active_templates: dict[str, str] = dict(_ONTOLOGY_DOCS_TEMPLATES)


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
    the same slice, an error is raised.

    When ``slice_resolver`` is supplied it is called as
    ``slice_resolver(term_iri, predicate_iri, lexical_value)`` and may return a
    slice IRI to override the default path-derived grouping. Returning ``None``
    falls back to the path-based heuristic.

    Results are yielded sorted deterministically by
    ``(slice_iri, term_iri, predicate)``.

    Args:
        graph: The RDF graph to scan for localizable literals.
        slice_resolver: Optional callable that returns a slice IRI for a given
            term, predicate, and lexical value, or ``None`` to use the default
            heuristic.

    Yields:
        :class:`TranslationKey` records in deterministic order.

    Raises:
        ValueError: If multiple distinct ``@x-gmeow-english`` values exist for
            the same term, predicate, and slice.
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

    Args:
        entries: Translation keys to include in the template.

    Returns:
        The rendered POT file content.
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

    Args:
        path: Path to the PO catalog file.

    Returns:
        Mapping from ``(term_iri, predicate, internal_lang_tag)`` to the
        translated ``msgstr`` value.

    Raises:
        ValueError: If the PO ``Language:`` header cannot be mapped to a GMEOW
            internal tag.
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
        msgstr = entry.msgstr
        if not msgstr:
            continue
        catalog[(term_iri, predicate, internal_tag)] = msgstr

    return catalog


def merge_terms(base_graph: Graph, po_paths: list[Path]) -> Graph:
    """Return a new graph with translations from *po_paths* merged into *base_graph*.

    The returned graph contains all triples from *base_graph* plus, for each PO
    file, triples of the form ``(term_iri, predicate, Literal(msgstr, lang=tag))``.
    *base_graph* is not mutated.

    Args:
        base_graph: The English ontology graph to merge translations into.
        po_paths: Paths to translated PO catalogs.

    Returns:
        A new graph containing the merged triples.

    Raises:
        ValueError: If a PO catalog entry references a term/predicate pair that
            does not exist in *base_graph*.
    """
    allowed_keys = {
        (str(subject), str(predicate))
        for subject, predicate, obj in base_graph
        if isinstance(subject, URIRef)
        and predicate in LOCALIZABLE_PREDICATES
        and isinstance(obj, Literal)
    }

    merged = Graph()
    for triple in base_graph:
        merged.add(triple)

    for path in sorted(po_paths):
        catalog = load_po_catalog(path)
        for (term_iri, predicate, internal_tag), msgstr in catalog.items():
            if (term_iri, predicate) not in allowed_keys:
                raise ValueError(
                    f"PO catalog entry references unknown term/predicate: "
                    f"{term_iri} {predicate}"
                )
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


def ontology_docs_template(key: str, fallback: str = "") -> str:
    """Return the current ontology-docs template string for *key*.

    Args:
        key: Template identifier (e.g. ``"nav_home"``).
        fallback: Value to return when *key* is not registered.

    Returns:
        The registered template string, or *fallback* if the key is missing.
    """
    return _active_templates.get(key, fallback)


@contextlib.contextmanager
def translated_ontology_docs_templates(
    catalog: dict[str, str],
) -> Iterator[None]:
    """Temporarily override ontology-docs template strings with *catalog*.

    Args:
        catalog: Mapping from template id to translated string. The mapping is
            merged with the English defaults, so missing keys fall back to
            English.

    Yields:
        None. The original template catalog is restored on context exit.
    """
    global _active_templates
    previous = _active_templates
    _active_templates = merge_ontology_docs_templates(catalog)
    try:
        yield
    finally:
        _active_templates = previous


def extract_ontology_docs_templates() -> list[PoEntry]:
    """Return :class:`PoEntry` records for every ontology-docs template string.

    Each entry carries ``msgctxt "ontology-docs-template|<id>"`` and an empty
    ``msgstr`` so it can be shipped as a POT template.

    Returns:
        Template entries in deterministic id order.
    """
    return [
        PoEntry(msgctxt=f"ontology-docs-template|{key}", msgid=value, msgstr="")
        for key, value in sorted(_ONTOLOGY_DOCS_TEMPLATES.items())
    ]


def merge_ontology_docs_templates(catalog: dict[str, str]) -> dict[str, str]:
    """Return a complete template dict with translations from *catalog* merged.

    *catalog* maps template ids (without the ``ontology-docs-template|`` prefix)
    to translated strings. Missing or empty values fall back to the English
    defaults.

    Args:
        catalog: Mapping from template id to translated string.

    Returns:
        A full template dictionary including English fallbacks.
    """
    merged = dict(_ONTOLOGY_DOCS_TEMPLATES)
    for key, value in catalog.items():
        if key in merged and value:
            merged[key] = value
    return merged


def load_ontology_docs_template_catalog(
    lang: str, root: Path = PROJECT_ROOT
) -> dict[str, str]:
    """Load translated ontology-docs template strings for *lang*.

    Reads ``dist/i18n/ontology-docs-templates.<lang>.po`` and returns a dict
    mapping template id to translated string.

    Args:
        lang: BCP-47 language tag (e.g. ``"fr"``).
        root: Repository root used to locate ``dist/i18n/``.

    Returns:
        Mapping from template id to translated string, or an empty dict when the
        PO file does not exist.
    """
    po_path = root / "dist" / "i18n" / f"ontology-docs-templates.{lang}.po"
    if not po_path.is_file():
        return {}
    catalog: dict[str, str] = {}
    for entry in parse_po(po_path.read_text(encoding="utf-8")):
        if not entry.msgctxt or not entry.msgctxt.startswith("ontology-docs-template|"):
            continue
        key = entry.msgctxt[len("ontology-docs-template|") :]
        msgstr = entry.msgstr
        if msgstr:
            catalog[key] = msgstr
    return catalog


@dataclass(frozen=True, slots=True)
class _MdSegment:
    """One paragraph-level Markdown segment used for extraction/merge."""

    text: str
    trailing_blank_lines: int
    is_code_block: bool = False


def _split_markdown(text: str) -> list[_MdSegment]:
    """Split *text* into paragraph-level segments.

    Blank lines separate non-code segments.  Fenced code blocks (`` ``` ``)
    are kept as single segments, including their fences and any internal
    blank lines.
    """
    if "\r\n" in text:
        newline = "\r\n"
    elif "\r" in text:
        newline = "\r"
    else:
        newline = "\n"
    lines = text.split(newline)
    segments: list[_MdSegment] = []
    current: list[str] = []
    in_code = False
    pending_blanks = 0

    def flush(trailing: int = 0) -> None:
        if current:
            segments.append(
                _MdSegment(
                    text="\n".join(current),
                    trailing_blank_lines=trailing,
                    is_code_block=in_code and bool(current),
                )
            )
            current.clear()

    for line in lines:
        stripped = line.lstrip()
        if stripped.startswith("```"):
            if in_code:
                # Closing fence ends the code block segment.
                current.append(line)
                in_code = False
                flush(pending_blanks)
                pending_blanks = 0
            else:
                # Opening fence starts a new code block segment.
                flush(pending_blanks)
                pending_blanks = 0
                in_code = True
                current.append(line)
            continue
        if in_code:
            current.append(line)
            continue
        if line.strip() == "":
            if current:
                flush(pending_blanks)
                pending_blanks = 1
            else:
                pending_blanks += 1
            continue
        # Non-blank line: attach any pending blanks to the previous segment.
        if pending_blanks and segments:
            last = segments[-1]
            segments[-1] = _MdSegment(
                text=last.text,
                trailing_blank_lines=last.trailing_blank_lines + pending_blanks,
                is_code_block=last.is_code_block,
            )
            pending_blanks = 0
        current.append(line)

    flush(pending_blanks)
    return segments


def _anchor_hash(text: str) -> str:
    """Return a stable 12-character anchor hash for *text*."""
    return hashlib.sha1(text.encode("utf-8")).hexdigest()[:12]


def extract_markdown(path: Path, *, rel_path: str | None = None) -> list[PoEntry]:
    """Extract translatable paragraph-level segments from a Markdown file.

    Code blocks are preserved as single segments.  Each entry uses
    ``msgctxt "<rel-path>|<anchor-hash>"`` so translations can be merged back
    by matching the same source segments.

    Args:
        path: Markdown file to extract segments from.
        rel_path: Optional relative path used in the ``msgctxt`` key. Defaults
            to ``path.name``.

    Returns:
        PO entries for every translatable segment in source order.
    """
    with path.open("r", encoding="utf-8", newline="") as fh:
        text = fh.read()
    key = rel_path if rel_path is not None else path.name
    segments = _split_markdown(text)
    return [
        PoEntry(
            msgctxt=f"{key}|{_anchor_hash(segment.text)}",
            msgid=segment.text,
            msgstr="",
        )
        for segment in segments
    ]


def merge_markdown(source: Path, po_path: Path, output: Path) -> None:
    """Merge Markdown translations from *po_path* back into *source*.

    The PO file is keyed by ``msgctxt`` (``<rel-path>|<anchor-hash>``).  Each
    source segment is replaced by its ``msgstr`` when one exists and is
    non-empty; otherwise the original English segment is kept.  The result is
    written to *output* preserving the original line ending and structure.

    Args:
        source: English Markdown source file.
        po_path: PO catalog containing translated segments.
        output: Path to write the merged Markdown file.
    """
    po_text = po_path.read_text(encoding="utf-8")
    catalog: dict[str, str] = {}
    for entry in parse_po(po_text):
        if "|" not in entry.msgctxt:
            continue
        catalog[entry.msgctxt.rsplit("|", 1)[1]] = entry.msgstr

    with source.open("r", encoding="utf-8", newline="") as fh:
        source_text = fh.read()
    if "\r\n" in source_text:
        newline = "\r\n"
    elif "\r" in source_text:
        newline = "\r"
    else:
        newline = "\n"

    segments = _split_markdown(source_text)
    out_lines: list[str] = []
    for segment in segments:
        translation = catalog.get(_anchor_hash(segment.text), "")
        if not translation.strip():
            out_lines.extend(segment.text.splitlines())
        else:
            out_lines.extend(translation.splitlines())
        out_lines.extend([""] * segment.trailing_blank_lines)

    with output.open("w", encoding="utf-8", newline="") as fh:
        fh.write(newline.join(out_lines))


def discover_doc_languages(root: Path = PROJECT_ROOT) -> list[str]:
    """Return sorted BCP-47 language tags with committed PO translations.

    Languages are discovered from ``slices/*/*/i18n/*.po`` files whose
    ``Language:`` header is not ``en``.  The English carrier is handled
    separately so that consumers can always fall back to it.

    Args:
        root: Repository root to search for PO catalogs.

    Returns:
        Sorted list of BCP-47 tags (excluding ``en``).
    """
    tags: set[str] = set()
    for po_path in sorted(root.glob("slices/*/*/i18n/*.po")):
        if po_path.stem == "en":
            continue
        try:
            tag = _language_from_po(po_path.read_text(encoding="utf-8"))
        except ValueError:
            continue
        if tag and tag.lower() != "en":
            tags.add(tag)
    return sorted(tags)


def _md_po_path(rel_path: Path, lang: str, root: Path = PROJECT_ROOT) -> Path:
    """Return the conventional PO translation path for a Markdown file.

    Translations live alongside the POT templates emitted by
    ``gmeow-dev i18n extract``:
    ``dist/i18n/docs/<rel-path>.<lang>.po``.
    """
    return root / "dist" / "i18n" / "docs" / f"{rel_path}.{lang}.po"


def _internal_tag_for_lang(lang: str) -> str:
    """Map a BCP-47 tag to the GMEOW internal language tag."""
    inverse_map = _default_inverse_tag_map()
    return inverse_map.get(lang.lower(), f"x-gmeow-{lang}")


def merge_all_markdown(
    root: Path,
    lang: str,
    output_root: Path,
    *,
    include_readme: bool = True,
) -> None:
    """Build a translated Markdown tree for *lang* under *output_root*.

    English source files are merged with translations found at
    ``dist/i18n/docs/<rel-path>.<lang>.po``.  Missing PO files produce the
    original English content.

    Args:
        root: Repository root containing English Markdown sources.
        lang: BCP-47 target language tag.
        output_root: Directory to write the translated Markdown tree.
        include_readme: Also translate ``README.md`` at the repository root.
    """
    sources: list[tuple[Path, Path]] = []

    for guide in sorted(root.glob("slices/*/*/docs.md")):
        rel = guide.relative_to(root)
        sources.append((guide, output_root / rel))

    for doc in sorted((root / "docs").glob("*.md")):
        rel = doc.relative_to(root)
        sources.append((doc, output_root / rel))

    if include_readme and (root / "README.md").is_file():
        sources.append((root / "README.md", output_root / "README.md"))

    for source, target in sources:
        rel = source.relative_to(root)
        po_path = _md_po_path(rel, lang, root=root)
        target.parent.mkdir(parents=True, exist_ok=True)
        if po_path.is_file():
            merge_markdown(source, po_path, target)
        else:
            target.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")


def write_po(path: Path, entries: list[PoEntry], lang: str) -> None:
    """Write a ``.po`` file with the given language header and entries.

    Args:
        path: Destination file path.
        entries: PO entries to append after the header.
        lang: BCP-47 language tag for the ``Language:`` header.
    """
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
    """Write a ``.pot`` template file (no ``Language:`` header).

    Args:
        path: Destination file path.
        entries: PO entries to append after the header.
    """
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


@dataclass(frozen=True, slots=True)
class PoCatalogEntry:
    """One PO catalog entry, including translator flags such as ``fuzzy``."""

    msgctxt: str
    msgid: str
    msgstr: str
    fuzzy: bool = False


@dataclass(frozen=True, slots=True)
class PoCatalog:
    """A discovered PO catalog with its owning slice and parsed entries."""

    path: Path
    language: str
    slice_name: str
    slice_path: str
    entries: list[PoCatalogEntry]


def _split_po_blocks(text: str) -> list[str]:
    """Split PO text into blank-line separated blocks."""
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


def _is_fuzzy_block(block: str) -> bool:
    """Return ``True`` if the PO block carries a ``fuzzy`` flag comment."""
    for line in block.splitlines():
        stripped = line.strip()
        if stripped.startswith("#,"):
            flags = [flag.strip() for flag in stripped[2:].split(",")]
            if "fuzzy" in flags:
                return True
    return False


def parse_po_catalog(text: str) -> list[PoCatalogEntry]:
    """Parse a PO file into entries, preserving per-entry fuzzy flags.

    The header entry (empty ``msgid``) and any entries without a ``msgctxt`` are
    retained so callers can filter them as needed.

    Args:
        text: Raw PO file content.

    Returns:
        Parsed catalog entries in source order.
    """
    entries: list[PoCatalogEntry] = []
    for block in _split_po_blocks(text):
        if not any(
            line.startswith(("msgctxt", "msgid")) for line in block.splitlines()
        ):
            continue
        fuzzy = _is_fuzzy_block(block)
        try:
            parsed = parse_po(block, require_msgctxt=False)
        except PoParseError:
            continue
        for entry in parsed:
            # The PO header has an empty msgid; skip it like a structural entry.
            if not entry.msgid:
                continue
            entries.append(
                PoCatalogEntry(
                    msgctxt=entry.msgctxt,
                    msgid=entry.msgid,
                    msgstr=entry.msgstr,
                    fuzzy=fuzzy,
                )
            )
    return entries


def iter_po_catalogs(root: Path) -> Iterator[PoCatalog]:
    """Yield every PO catalog discovered under ``<root>/slices/*/*/i18n/*.po``.

    Each catalog carries the BCP-47 language parsed from its ``Language:``
    header, the owning slice name (last directory segment), the relative slice
    path, and all parsed entries.

    Args:
        root: Repository root to search for PO catalogs.

    Yields:
        :class:`PoCatalog` records in deterministic path order.
    """
    for po_path in sorted(root.glob("slices/*/*/i18n/*.po")):
        text = po_path.read_text(encoding="utf-8")
        language = _language_from_po(text)
        slice_dir = po_path.parent.parent
        slice_name = slice_dir.name
        slice_path = str(
            po_path.parent.parent.relative_to(root)
            if po_path.is_relative_to(root)
            else slice_dir
        )
        yield PoCatalog(
            path=po_path,
            language=language,
            slice_name=slice_name,
            slice_path=slice_path,
            entries=parse_po_catalog(text),
        )


def write_csv_export(catalogs: Iterable[PoCatalog], out: Path | None) -> None:
    """Write the CSV export to *out*, or to stdout when *out* is ``None``.

    Args:
        catalogs: PO catalogs to include in the export.
        out: Destination CSV file, or ``None`` to write to stdout.
    """
    header = ["slice", "term_iri", "predicate", "language", "msgid", "msgstr", "fuzzy"]
    rows: list[list[str]] = []
    for catalog in catalogs:
        for entry in catalog.entries:
            if not entry.msgctxt or "|" not in entry.msgctxt:
                continue
            term_iri, predicate = entry.msgctxt.split("|", 1)
            rows.append(
                [
                    catalog.slice_name,
                    term_iri,
                    predicate,
                    catalog.language,
                    entry.msgid,
                    entry.msgstr,
                    "true" if entry.fuzzy else "false",
                ]
            )

    if out is None:
        writer = csv.writer(sys.stdout)
        writer.writerow(header)
        writer.writerows(rows)
    else:
        with out.open("w", encoding="utf-8", newline="") as fh:
            writer = csv.writer(fh)
            writer.writerow(header)
            writer.writerows(rows)


def _xml_escape(text: str) -> str:
    """Escape text for XML character data."""
    return escape(text, {'"': "&quot;"})


def write_xliff_export(catalogs: Iterable[PoCatalog], out: Path | None) -> None:
    """Write the XLIFF 1.2 export to *out*, or to stdout when *out* is ``None``.

    Args:
        catalogs: PO catalogs to include in the export.
        out: Destination XLIFF file, or ``None`` to write to stdout.
    """
    lines: list[str] = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">',
    ]
    for catalog in catalogs:
        lines.append(
            f'  <file original="{_xml_escape(catalog.slice_path)}" '
            f'source-language="en" target-language="{_xml_escape(catalog.language)}" '
            'datatype="plaintext">'
        )
        lines.append("    <body>")
        for entry in catalog.entries:
            if not entry.msgctxt or "|" not in entry.msgctxt:
                continue
            term_iri, predicate = entry.msgctxt.split("|", 1)
            lines.append(
                f'      <trans-unit id="{_xml_escape(entry.msgctxt)}" translate="yes">'
            )
            lines.append(f"        <source>{_xml_escape(entry.msgid)}</source>")
            lines.append(f"        <target>{_xml_escape(entry.msgstr)}</target>")
            lines.append(
                "        <note>"
                f"Term: {_xml_escape(term_iri)} Predicate: {_xml_escape(predicate)}"
                "</note>"
            )
            lines.append("      </trans-unit>")
        lines.append("    </body>")
        lines.append("  </file>")
    lines.append("</xliff>")

    text = "\n".join(lines) + "\n"
    if out is None:
        sys.stdout.write(text)
    else:
        out.write_text(text, encoding="utf-8")
