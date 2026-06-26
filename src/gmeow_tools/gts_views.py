# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The fold-view layer: exporters' window onto the GTS snapshot (#267, #12).

:class:`FoldView` wraps a folded :class:`gts.model.Graph` with the access
idioms the exporters actually use — subjects-by-type, object lookup, RDF
lists, language-boundary text selection, CURIEs — so the exporter ports of the
narrow-waist parcel port mechanically. This module speaks ONLY ``gts.model``:
no rdflib, no graph engine (the whole point of the waist).

Scopes: every quad-access method takes a ``scope`` — ``DEFAULT`` (the default
graph: the authored import-free ontology), a named-graph IRI string, or
``ALL``.
"""

from __future__ import annotations

import io
import tarfile
from typing import TYPE_CHECKING, Final

from gts import read
from gts.model import Graph, Term, TermKind
from gts.nquads import term_token

from gmeow_tools.config import GTS_SNAPSHOT_FILE, NAMESPACE, PREFIXES
from gmeow_tools.language_tags import (
    LangSelector,
    filter_literals,
    is_internal_tag,
    rank_language,
    select_literal,
)

if TYPE_CHECKING:
    from collections.abc import Iterable
    from pathlib import Path

    from gmeow_rdf.compat.rdflib import Literal
    from gts.model import Quad, Triple

#: Scope sentinel: every graph in the snapshot.
ALL: Final = "__all__"
#: Scope sentinel: the default graph (the ontology base). Named for clarity
#: at call sites; the value is what the fold uses for "no graph name".
DEFAULT: Final = None

_RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
_RDF_TYPE = _RDF + "type"
_RDF_FIRST = _RDF + "first"
_RDF_REST = _RDF + "rest"
_RDF_NIL = _RDF + "nil"

_XSD = "http://www.w3.org/2001/XMLSchema#"

#: PREFIXES sorted once (longest namespace first) — curie() runs per term.
_SORTED_PREFIXES = sorted(PREFIXES.items(), key=lambda x: -len(x[1]))

_LANGUAGE_CLASS = NAMESPACE + "Language"
_LANGUAGE_TAG = NAMESPACE + "languageTag"
_BCP47_TAG = NAMESPACE + "bcp47Tag"


#: (resolved path, mtime_ns, size) → FoldView. Keyed by the file's stat
#: fingerprint, NOT just the path: a naive lru_cache would hand back a STALE
#: fold after `gmeow regenerate` republishes the snapshot mid-run (the gts
#: generator runs before its consumers in topological order).
_FOLD_CACHE: dict[tuple[str, int, int], FoldView] = {}


def load_fold(path: Path | None = None) -> FoldView:
    """Read the committed snapshot (or ``path``) into a :class:`FoldView`.

    Cached per (path, mtime, size): repeated loads within one process reuse
    the parsed fold, while a republished snapshot is picked up immediately.
    """
    target = path if path is not None else GTS_SNAPSHOT_FILE
    if not target.exists():
        msg = f"GTS snapshot not found: {target} — run `gmeow regenerate gts`"
        raise FileNotFoundError(msg)
    stat = target.stat()
    key = (str(target.resolve()), stat.st_mtime_ns, stat.st_size)
    cached = _FOLD_CACHE.get(key)
    if cached is not None:
        return cached
    graph = read(target.read_bytes())
    if any(d.code != "TornAppendError" for d in graph.diagnostics):
        codes = ", ".join(d.code for d in graph.diagnostics)
        msg = f"GTS snapshot has reader diagnostics ({codes}): {target}"
        raise ValueError(msg)
    view = FoldView(graph)
    _FOLD_CACHE.clear()  # one snapshot per process is the working set
    _FOLD_CACHE[key] = view
    return view


class FoldView:
    """Read-side idioms over a folded GTS graph, with lazy indexes."""

    def __init__(self, graph: Graph) -> None:
        """Wrap a folded graph; indexes build lazily on first use."""
        self.graph = graph
        self._iri_index: dict[str, int] | None = None
        self._by_scope: dict[str | None, list[Quad]] | None = None
        # per-scope lazy indexes: subject → [(p, o)] and (p, o) → [s]
        self._spo: dict[str | None, dict[int, list[tuple[int, int]]]] = {}
        self._po: dict[str | None, dict[tuple[int, int], list[int]]] = {}
        self._tag_map: dict[str, str] | None = None
        self._available_languages: frozenset[str] | None = None

    # -- terms ----------------------------------------------------------------

    def term(self, tid: int) -> Term:
        """The :class:`gts.model.Term` behind a term-id."""
        return self.graph.terms[tid]

    def is_iri(self, tid: int) -> bool:
        """Whether the term is an IRI."""
        return self.graph.terms[tid].kind is TermKind.IRI

    def is_bnode(self, tid: int) -> bool:
        """Whether the term is a blank node."""
        return self.graph.terms[tid].kind is TermKind.BNODE

    def is_literal(self, tid: int) -> bool:
        """Whether the term is a literal."""
        return self.graph.terms[tid].kind is TermKind.LITERAL

    def iri(self, tid: int) -> str | None:
        """The IRI string, or ``None`` for non-IRI terms."""
        t = self.graph.terms[tid]
        return t.value if t.kind is TermKind.IRI else None

    def lex(self, tid: int) -> str:
        """The lexical form (IRI string, bnode label, or literal value)."""
        return self.graph.terms[tid].value or ""

    def lang(self, tid: int) -> str | None:
        """The literal's language tag, if any."""
        return self.graph.terms[tid].lang

    def datatype(self, tid: int) -> str:
        """The effective datatype IRI of a literal (§7.1 defaulting)."""
        return self.graph.datatype_iri(self.graph.terms[tid])

    def nq_token(self, tid: int) -> str:
        """The canonical N-Triples token — display and stable sort key."""
        return term_token(self.graph, tid)

    def python_value(self, tid: int) -> object:
        """A Python scalar for a term (the lpg conversion, fold-side).

        Integers, floats, and booleans parse; language-tagged literals
        become ``{"value", "lang"}`` dicts; IRIs become CURIEs; blank nodes
        keep their ``_:`` form; everything else is the lexical string.
        """
        t = self.graph.terms[tid]
        if t.kind is TermKind.LITERAL:
            dt = self.graph.datatype_iri(t)
            lex = t.value or ""
            if dt == _XSD + "integer":
                return int(lex)
            if dt in (_XSD + "decimal", _XSD + "double", _XSD + "float"):
                return float(lex)
            if dt == _XSD + "boolean":
                # XSD admits four lexical forms: true/false/1/0
                return lex.lower() in ("true", "1")
            if t.lang is not None:
                return {"value": lex, "lang": t.lang}
            return lex
        if t.kind is TermKind.IRI:
            return self.curie(t.value or "")
        if t.kind is TermKind.BNODE:
            return f"_:{t.value or ''}"
        return self.nq_token(tid)

    def tid_of_iri(self, iri: str) -> int | None:
        """The term-id of an IRI, or ``None`` when absent from the fold."""
        if self._iri_index is None:
            self._iri_index = {
                t.value: tid
                for tid, t in enumerate(self.graph.terms)
                if t.kind is TermKind.IRI and t.value is not None
            }
        return self._iri_index.get(iri)

    def curie(self, iri: str) -> str:
        """A CURIE under the longest matching known prefix, else the IRI."""
        for prefix, namespace in _SORTED_PREFIXES:
            if iri.startswith(namespace):
                return f"{prefix}:{iri[len(namespace) :]}"
        return iri

    # -- quads ----------------------------------------------------------------

    def quads(self, scope: str | None = DEFAULT) -> Iterable[Quad]:
        """Quads in a scope: ``DEFAULT``, a named-graph IRI, or ``ALL``."""
        self._ensure_quad_indexes()
        assert self._by_scope is not None
        if scope == ALL:
            for rows in self._by_scope.values():
                yield from rows
            return
        yield from self._by_scope.get(scope, [])

    def subjects_by_type(
        self, class_iri: str, scope: str | None = DEFAULT
    ) -> list[int]:
        """Subjects with ``rdf:type <class_iri>`` in scope, id-sorted."""
        type_tid = self.tid_of_iri(_RDF_TYPE)
        class_tid = self.tid_of_iri(class_iri)
        if type_tid is None or class_tid is None:
            return []
        return sorted(set(self._po_index(scope).get((type_tid, class_tid), [])))

    def objects(self, s_tid: int, p_iri: str, scope: str | None = DEFAULT) -> list[int]:
        """Objects of ``(s, p, ?)`` in scope, id-sorted (deterministic)."""
        p_tid = self.tid_of_iri(p_iri)
        if p_tid is None:
            return []
        rows = self._spo_index(scope).get(s_tid, [])
        return sorted({o for p, o in rows if p == p_tid})

    def value(self, s_tid: int, p_iri: str, scope: str | None = DEFAULT) -> int | None:
        """One object of ``(s, p, ?)`` — the token-smallest, never graph order."""
        candidates = self.objects(s_tid, p_iri, scope)
        if not candidates:
            return None
        return min(candidates, key=self.nq_token)

    def predicate_objects(
        self, s_tid: int, scope: str | None = DEFAULT
    ) -> list[tuple[int, int]]:
        """All ``(p, o)`` pairs for a subject in scope, id-sorted."""
        return sorted(set(self._spo_index(scope).get(s_tid, [])))

    def has(
        self, s_tid: int, p_iri: str, o_tid: int, scope: str | None = DEFAULT
    ) -> bool:
        """Membership test for ``(s, p, o)`` in scope."""
        p_tid = self.tid_of_iri(p_iri)
        if p_tid is None:
            return False
        return (p_tid, o_tid) in self._spo_index(scope).get(s_tid, [])

    def rdf_list(self, head_tid: int, scope: str | None = DEFAULT) -> list[int]:
        """Walk an ``rdf:first``/``rdf:rest`` list from its head term."""
        nil = self.tid_of_iri(_RDF_NIL)
        out: list[int] = []
        seen: set[int] = set()
        current: int | None = head_tid
        while current is not None and current != nil and current not in seen:
            seen.add(current)
            first = self.value(current, _RDF_FIRST, scope)
            if first is not None:
                out.append(first)
            current = self.value(current, _RDF_REST, scope)
        return out

    # -- statement layer -------------------------------------------------------

    def reifiers(self) -> dict[int, Triple]:
        """Reifier-id → quoted triple bindings (global, §7.3)."""
        return self.graph.reifiers

    def annotations(self) -> list[Triple]:
        """``(reifier, predicate, value)`` annotation rows (global)."""
        return self.graph.annotations

    # -- language boundary -----------------------------------------------------

    def tag_map(self) -> dict[str, str]:
        """Internal language tag → BCP-47, from all bundled Language individuals."""
        if self._tag_map is None:
            out: dict[str, str] = {}
            for lang_tid in self.subjects_by_type(_LANGUAGE_CLASS, scope=ALL):
                internal = self.value(lang_tid, _LANGUAGE_TAG, scope=ALL)
                bcp = self.value(lang_tid, _BCP47_TAG, scope=ALL)
                if internal is not None and bcp is not None:
                    out[self.lex(internal)] = self.lex(bcp)
            self._tag_map = out
        return self._tag_map

    def available_languages(self) -> frozenset[str]:
        """Public BCP-47 tags that actually have literals in the snapshot.

        Always includes the carrier language (``en``). Non-English tags are
        included only when at least one language-tagged literal uses them —
        either directly or via an internal ``x-gmeow-*`` mapping.
        """
        if self._available_languages is None:
            tags: set[str] = set()
            tag_map = self.tag_map()
            for t in self.graph.terms:
                if t.kind is not TermKind.LITERAL or not t.lang:
                    continue
                lang = t.lang
                public = tag_map.get(lang, lang) if is_internal_tag(lang) else lang
                if public and public.lower() != "en":
                    tags.add(public.lower())
            self._available_languages = frozenset({"en"} | tags)
        return self._available_languages

    def public_text(self, s_tid: int, p_iri: str, scope: str | None = DEFAULT) -> str:
        """The public-facing text for ``(s, p)`` — the projection boundary.

        Same selection as :func:`gmeow_tools.language_tags.public_literal`
        (the shared :func:`rank_language` key, so the rdflib and fold paths
        agree by construction): an internal-tagged literal with a BCP-47
        mapping wins; otherwise the deterministically-first literal.
        """
        return self.public_literal(s_tid, p_iri, scope)[0]

    def public_literal(
        self, s_tid: int, p_iri: str, scope: str | None = DEFAULT
    ) -> tuple[str, str | None]:
        """The public text for ``(s, p)`` plus its public BCP-47 tag.

        The tag is the mapped form of the winning literal's internal tag
        (``None`` for untagged literals or unmapped tags) — what a projection
        that re-emits the literal, tag included, must carry (#287).
        """
        candidates = [
            o for o in self.objects(s_tid, p_iri, scope) if self.is_literal(o)
        ]
        if not candidates:
            return "", None
        tag_map = self.tag_map()
        # Same pre-sort as language_tags.public_literal: ties within a
        # language resolve by lexical value in BOTH paths.
        candidates.sort(key=lambda o: (self.lang(o) or "", self.lex(o)))
        for o in sorted(candidates, key=lambda o: rank_language(self.lang(o))):
            lang = self.lang(o)
            if lang and is_internal_tag(lang) and lang in tag_map:
                return self.lex(o), tag_map[lang]
        first_lang = self.lang(candidates[0])
        return self.lex(candidates[0]), (
            tag_map.get(first_lang) if first_lang else None
        )

    def _literal_candidates(
        self, s_tid: int, p_iri: str, scope: str | None = DEFAULT
    ) -> list[Literal]:
        """Objects of ``(s, p)`` as rdflib Literals for language selection."""
        from gmeow_rdf.compat.rdflib import Literal

        return [
            Literal(self.lex(o), lang=self.lang(o))
            for o in self.objects(s_tid, p_iri, scope)
            if self.is_literal(o)
        ]

    def public_literal_with_fallback(
        self,
        s_tid: int,
        p_iri: str,
        selector: LangSelector,
        scope: str | None = DEFAULT,
    ) -> tuple[str, str | None, bool]:
        """Selector-aware single literal for ``(s, p)`` plus fallback signal."""
        candidates = self._literal_candidates(s_tid, p_iri, scope)
        if not candidates:
            return "", None, False
        lit, fallback = select_literal(candidates, selector, tag_map=self.tag_map())
        if lit is None:
            return "", None, False
        return str(lit), lit.language, fallback

    def public_text_with_fallback(
        self,
        s_tid: int,
        p_iri: str,
        selector: LangSelector,
        scope: str | None = DEFAULT,
    ) -> tuple[str, bool]:
        """Selector-aware single text for ``(s, p)`` plus fallback signal."""
        text, _lang, fallback = self.public_literal_with_fallback(
            s_tid, p_iri, selector, scope
        )
        return text, fallback

    def public_texts(
        self,
        s_tid: int,
        p_iri: str,
        selector: LangSelector,
        scope: str | None = DEFAULT,
    ) -> list[tuple[str, str | None, bool]]:
        """All requested-language literals for ``(s, p)`` plus fallback signal.

        Returns a list of ``(text, bcp47_tag, is_fallback)`` tuples in request
        order. If no requested language is present, the English fallback is
        returned as a single item.
        """
        candidates = self._literal_candidates(s_tid, p_iri, scope)
        if not candidates:
            return []
        results: list[tuple[Literal, bool]] = filter_literals(
            candidates, selector, tag_map=self.tag_map()
        )
        return [(str(lit), lit.language, fallback) for lit, fallback in results]

    # -- internals ---------------------------------------------------------------

    def _ensure_quad_indexes(self) -> None:
        if self._by_scope is not None:
            return
        by_scope: dict[str | None, list[Quad]] = {}
        for q in self.graph.quads:
            name = self.graph.terms[q[3]].value if q[3] is not None else None
            by_scope.setdefault(name, []).append(q)
        self._by_scope = by_scope

    def _spo_index(self, scope: str | None) -> dict[int, list[tuple[int, int]]]:
        if scope not in self._spo:
            index: dict[int, list[tuple[int, int]]] = {}
            for s, p, o, _ in self.quads(scope):
                index.setdefault(s, []).append((p, o))
            self._spo[scope] = index
        return self._spo[scope]

    def _po_index(self, scope: str | None) -> dict[tuple[int, int], list[int]]:
        if scope not in self._po:
            index: dict[tuple[int, int], list[int]] = {}
            for s, p, o, _ in self.quads(scope):
                index.setdefault((p, o), []).append(s)
            self._po[scope] = index
        return self._po[scope]


# --------------------------------------------------------------------------- #
# Docs-site extraction (#1019): unpack the Rust-rendered `ontology-docs` site
# blob (#897) from the bundle. The Markdown/HTML tree is rendered at
# `regenerate` time by `gmeow_docs::render_site_lang` and packed verbatim into
# the single `ontology-docs` blob, one INTERNAL-tag (`x-gmeow-<lang>/`) prefix
# per language. `gmeow extract-docs` is now nothing but this blob-unpack — the
# Rust site IS the docs tree (the Python projection retired with #1019).
# --------------------------------------------------------------------------- #

_REP_ONTOLOGY_DOCS: Final = "ontology-docs"


def _resolve_doc_language(
    selector: LangSelector | None, tag_map: dict[str, str]
) -> str:
    """Internal documentation language tag for *selector*.

    The first requested public BCP-47 tag that maps to an internal tag wins;
    otherwise the English carrier (``x-gmeow-english``) is returned.
    """
    if selector is not None:
        public_to_internal = {v: k for k, v in tag_map.items()}
        for public in selector.requested:
            if public in public_to_internal:
                return public_to_internal[public]
    return "x-gmeow-english"


def extract_docs_site(
    view: FoldView,
    out_dir: Path,
    *,
    selector: LangSelector | None = None,
    force: bool = False,
) -> None:
    """Unpack the bundled ``ontology-docs`` site for one language into *out_dir*.

    The site is rendered at ``regenerate`` time and embedded in the snapshot;
    this only unpacks it — nothing is re-projected here.

    Raises:
        FileExistsError: ``out_dir`` is non-empty and ``force`` is False.
        ValueError: the snapshot carries no ``ontology-docs`` site blob.
    """
    if out_dir.exists() and any(out_dir.iterdir()) and not force:
        raise FileExistsError(
            f"output directory is not empty: {out_dir}; use force=True to overwrite"
        )
    out_dir.mkdir(parents=True, exist_ok=True)

    if selector is None:
        from gmeow_tools.language_tags import resolve_lang_input

        selector = resolve_lang_input(None, view.tag_map())
    doc_lang = _resolve_doc_language(selector, view.tag_map())

    prefix = f"{doc_lang}/"
    extracted = False
    for digest, meta in view.graph.blob_meta.items():
        if meta.get("rep") != _REP_ONTOLOGY_DOCS:
            continue
        raw = view.graph.blobs.get(digest)
        if raw is None:
            continue
        with tarfile.open(fileobj=io.BytesIO(raw), mode="r") as tar:
            for member in tar.getmembers():
                if not member.isfile() or not member.name.startswith(prefix):
                    continue
                rel = member.name[len(prefix) :]
                target = out_dir / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                fileobj = tar.extractfile(member)
                if fileobj is not None:
                    target.write_bytes(fileobj.read())
                    extracted = True
        break

    if not extracted:
        raise ValueError(
            "snapshot carries no ontology-docs site blob "
            f"(rep {_REP_ONTOLOGY_DOCS!r}); regenerate the bundle to embed it"
        )
