# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""OKF (Open Knowledge Format) export — the bidirectional agent surface (#780).

OKF is Google Cloud's vendor-neutral, agent-facing knowledge format: a directory
of Markdown documents with YAML frontmatter and ``[text](path)`` links, one
document per concept. AI agents consume it directly. This module projects the
folded GMEOW vocabulary (the ``Term`` model in :mod:`gmeow_tools.export`, read off
the narrow-waist GTS snapshot) into a conformant OKF bundle.

It is a **LOSSY down-projection** — the same doctrine slot as the SKOS / OBO
Graphs / ShEx views, and the deliberate opposite of the lossless YAML-LD-star
sibling (#699). Only the flat term surface is carried: label, definition, the
documentation advisories, and the IS-A / domain / range / sub-property links.
The OWL axioms, the RDF-star statement/reification layer, and the full alignment
graph are dropped — the GTS/OWL source stays canonical. The lossiness is declared
in-band (root ``index.md``), mirroring :func:`gmeow_tools.export.write_obographs`.

The bundle conforms to the ``okf:`` profile that the Rust ``gts from-okf`` /
``gts to-okf`` primitives speak (gmeow-gts, ``--features okf``): six recognized
frontmatter keys (``type``/``title``/``description``/``resource``/``tags``/
``timestamp``) plus arbitrary extension keys that fold to ``okf:<key>``. We never
re-implement that codec here — gmeow *produces* the bundle; ``gts`` consumes and
validates it (the seam doctrine).
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import yaml

from gmeow_tools.export import Term, collect_terms, fold_meta
from gmeow_tools.gts_views import load_fold

if TYPE_CHECKING:
    from gmeow_tools.gts_views import FoldView
    from gmeow_tools.language_tags import LangSelector

#: The bundle directory name under ``dist/`` and the gts blob (#780, Task 2).
OKF_DIR_NAME = "gmeow-okf"

#: Term category → OKF ``type:`` string and bundle subdirectory.
_CATEGORY_TYPE = {"class": "Class", "property": "Property", "individual": "Individual"}
_CATEGORY_DIR = {
    "class": "classes",
    "property": "properties",
    "individual": "individuals",
}


def _slug(term_curie: str) -> str:
    """The document stem for a term — its CURIE local part (stable, NCName-safe)."""
    return term_curie.split(":", 1)[-1]


def _doc_relpath(term: Term) -> str:
    """The bundle-relative POSIX path of a term's document (``classes/Foo.md``)."""
    return f"{_CATEGORY_DIR[term.category]}/{_slug(term.curie)}.md"


def _relative_link(from_path: str, to_path: str) -> str:
    """A POSIX relative link from one bundle document to another."""
    base = PurePosixPath(from_path).parent
    target = PurePosixPath(to_path)
    # PurePosixPath has no relpath; compute via the common-prefix walk.
    base_parts = base.parts
    target_parts = target.parts
    common = 0
    for a, b in zip(base_parts, target_parts[:-1], strict=False):
        if a != b:
            break
        common += 1
    ups = [".."] * (len(base_parts) - common)
    downs = list(target_parts[common:])
    return "/".join(ups + downs) if (ups or downs) else target_parts[-1]


def _frontmatter(term: Term, *, version: str) -> dict[str, object]:
    """Build the deterministic OKF frontmatter mapping for a term.

    The six recognized keys come first in a fixed order; every remaining
    ``Term`` field rides as an ``okf:<key>`` extension (sorted, non-empty only).
    """
    fm: dict[str, object] = {"type": _CATEGORY_TYPE[term.category]}
    if term.label:
        fm["title"] = term.label
    if term.definition:
        fm["description"] = term.definition
    fm["resource"] = term.iri
    # ``tags`` (a recognized key → multiple okf:tag literals): the box roles, which
    # are the categorical, agent-meaningful markers. Sorted for determinism.
    if term.box_roles:
        fm["tags"] = sorted(term.box_roles)
    # NOTE: ``timestamp`` is typed xsd:dateTime by the OKF profile, so the semver
    # version cannot ride there — it folds as the ``okf:version`` extension instead.
    fm["version"] = version
    fm["curie"] = term.curie

    # Category-specific structured fields, then the shared advisories. Lists fold
    # to okf:json; scalars to typed literals. Emit only non-empty values.
    extension: dict[str, object] = {}
    if term.category == "class" and term.parents:
        extension["parents"] = term.parents
    if term.category == "property":
        if term.prop_kind:
            extension["prop_kind"] = term.prop_kind
        if term.domain:
            extension["domain"] = term.domain
        if term.range:
            extension["range"] = term.range
        if term.functional:
            extension["functional"] = True
        if term.sub_property_of:
            extension["sub_property_of"] = term.sub_property_of
    if term.category == "individual" and term.types:
        extension["types"] = term.types
    for key, value in (
        ("alignments", term.alignments),
        ("scope_notes", term.scope_notes),
        ("examples", term.examples),
        ("use_when", term.use_when),
        ("avoid_when", term.avoid_when),
        ("how_to_use", term.how_to_use),
        ("use_for_consumer", term.use_for_consumer),
        ("avoid_for_consumer", term.avoid_for_consumer),
    ):
        if value:
            extension[key] = value
    for key in sorted(extension):
        fm[key] = extension[key]
    return fm


def _link_targets(term: Term, by_curie: dict[str, Term]) -> list[tuple[str, Term]]:
    """In-bundle GMEOW relation targets as ``(relation, target Term)`` pairs.

    Only ``gmeow:`` targets that exist as documents become body links (so
    ``from-okf`` resolves them to real subject IRIs). External / blank-node
    domains and alignment targets stay in frontmatter as ``okf:json``.
    """
    out: list[tuple[str, Term]] = []
    refs: list[tuple[str, str]] = []
    if term.category == "class":
        refs += [("subClassOf", p) for p in term.parents]
    if term.category == "property":
        if term.domain:
            refs.append(("domain", term.domain))
        if term.range:
            refs.append(("range", term.range))
        refs += [("subPropertyOf", p) for p in term.sub_property_of]
    if term.category == "individual":
        refs += [("type", t) for t in term.types]
    for relation, ref in refs:
        target = by_curie.get(ref)
        if target is not None:
            out.append((relation, target))
    return out


def _body(term: Term, by_curie: dict[str, Term]) -> str:
    """The Markdown body for a term — definition, advisories, and relation links."""
    lines: list[str] = []
    if term.definition:
        lines += [term.definition, ""]

    def section(heading: str, items: Sequence[str]) -> None:
        """Append a ``## heading`` block with a bullet per item (skip if empty)."""
        if not items:
            return
        lines.append(f"## {heading}")
        lines.append("")
        lines.extend(f"- {item}" for item in items)
        lines.append("")

    section("Scope notes", term.scope_notes)
    section("Use when", term.use_when)
    section("Avoid when", term.avoid_when)
    section("How to use", term.how_to_use)
    section("Examples", term.examples)

    links = _link_targets(term, by_curie)
    if links:
        lines.append("## Relations")
        lines.append("")
        from_path = _doc_relpath(term)
        for relation, target in links:
            rel_path = _relative_link(from_path, _doc_relpath(target))
            label = target.label or target.curie
            lines.append(f"- {relation}: [{label}]({rel_path})")
        lines.append("")
    return "\n".join(lines).rstrip("\n") + "\n"


def _render_doc(frontmatter: dict[str, object], body: str) -> str:
    """One OKF Markdown document — YAML frontmatter block plus the body."""
    fm = yaml.safe_dump(
        frontmatter,
        sort_keys=False,
        default_flow_style=False,
        allow_unicode=True,
        width=10**9,
    )
    return f"---\n{fm}---\n{body}"


def _index_doc(
    title: str, entries: list[tuple[str, str]], *, lossy_note: str = ""
) -> str:
    """A navigation ``index.md`` linking to ``entries`` (``(label, relpath)``)."""
    fm: dict[str, object] = {"type": "Index", "title": title}
    lines: list[str] = []
    if lossy_note:
        lines += [lossy_note, ""]
    for label, rel_path in entries:
        lines.append(f"- [{label}]({rel_path})")
    body = "\n".join(lines).rstrip("\n") + "\n"
    return _render_doc(fm, body)


#: In-band lossy declaration, mirroring the OBO Graphs ``basicPropertyValues`` note.
_LOSSY_NOTE = (
    "> LOSSY projection: the flat GMEOW term surface (label, definition, advisories, "
    "and IS-A / domain / range / sub-property links). The OWL axioms, the RDF-star "
    "statement/reification layer, and the full alignment graph are dropped — the "
    "GTS/OWL source is canonical."
)


def write_okf(
    terms: Sequence[Term],
    out_dir: Path,
    *,
    title: str,
    version: str,
) -> Path:
    """Write the OKF bundle (one document per term + indexes) into ``out_dir``.

    ``out_dir`` is the bundle root (``…/gmeow-okf``). Returns it. Emission is
    fully deterministic: terms arrive sorted by ``(category, curie)``, frontmatter
    keys are fixed-then-sorted, and bodies carry no wall-clock content — so the
    folded ``gts`` blob digest is stable across runs (#780, Task 2).
    """
    root = out_dir
    by_curie = {t.curie: t for t in terms}

    # Per-term documents, grouped by category subdirectory.
    by_category: dict[str, list[Term]] = {"class": [], "property": [], "individual": []}
    for term in terms:
        rel = _doc_relpath(term)
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        doc = _render_doc(_frontmatter(term, version=version), _body(term, by_curie))
        path.write_text(doc, encoding="utf-8")
        by_category[term.category].append(term)

    # Per-directory indexes (links relative to the index, i.e. siblings).
    for category, members in by_category.items():
        if not members:
            continue
        entries = [(m.label or m.curie, f"{_slug(m.curie)}.md") for m in members]
        # _CATEGORY_DIR already holds the correct English plural ("classes",
        # "properties", "individuals") — reuse it rather than re-pluralize.
        idx = _index_doc(f"GMEOW {_CATEGORY_DIR[category]}", entries)
        (root / _CATEGORY_DIR[category] / "index.md").write_text(idx, encoding="utf-8")

    # Root index — links to each category index, carrying the lossy declaration.
    root_entries = [
        (f"{title} — {_CATEGORY_DIR[category]}", f"{_CATEGORY_DIR[category]}/index.md")
        for category in ("class", "property", "individual")
        if by_category[category]
    ]
    (root / "index.md").write_text(
        _index_doc(f"{title} (OKF)", root_entries, lossy_note=_LOSSY_NOTE),
        encoding="utf-8",
    )
    return root


def okf_index_records(terms: Sequence[Term]) -> list[dict[str, str]]:
    """Manifest records for the OKF bundle — one per term, for agent navigation.

    Each record is ``{path, type, title, resource}``: the bundle-relative document
    path (``gmeow-okf/classes/Foo.md``), the OKF ``type`` string, the term label,
    and the canonical IRI. Drives the MCP OKF-index resource (#780) without
    materializing the bundle on disk.
    """
    return [
        {
            "path": f"{OKF_DIR_NAME}/{_doc_relpath(term)}",
            "type": _CATEGORY_TYPE[term.category],
            "title": term.label or term.curie,
            "resource": term.iri,
        }
        for term in terms
    ]


def export_okf_bundle(
    out_dir: Path,
    *,
    view: FoldView | None = None,
    selector: LangSelector | None = None,
) -> Path:
    """Collect terms from the snapshot and write the OKF bundle to ``out_dir``."""
    if view is None:
        view = load_fold()
    title, version = fold_meta(view)
    terms = collect_terms(view, selector=selector)
    return write_okf(terms, out_dir, title=title, version=version)
