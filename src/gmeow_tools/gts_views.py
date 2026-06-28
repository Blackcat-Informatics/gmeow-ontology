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

import gmeow_rdf as ox
from gts import read
from gts.model import Graph, Term

from gmeow_tools.config import GTS_SNAPSHOT_FILE, PREFIXES
from gmeow_tools.language_tags import LangSelector

if TYPE_CHECKING:
    from pathlib import Path

    from gts.model import Quad, Triple

#: Scope sentinel: every graph in the snapshot.
ALL: Final = "__all__"
#: Scope sentinel: the default graph (the ontology base). Named for clarity
#: at call sites; the value is what the fold uses for "no graph name".
DEFAULT: Final = None

#: PREFIXES sorted once (longest namespace first) — curie() runs per term.
_SORTED_PREFIXES = sorted(PREFIXES.items(), key=lambda x: -len(x[1]))


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
    data = target.read_bytes()
    graph = read(data)
    if any(d.code != "TornAppendError" for d in graph.diagnostics):
        codes = ", ".join(d.code for d in graph.diagnostics)
        msg = f"GTS snapshot has reader diagnostics ({codes}): {target}"
        raise ValueError(msg)
    view = FoldView(graph, data)
    _FOLD_CACHE.clear()  # one snapshot per process is the working set
    _FOLD_CACHE[key] = view
    return view


class FoldView:
    """Read-side idioms over a folded GTS graph backed by Rust indexes."""

    def __init__(self, graph: Graph, data: bytes | None = None) -> None:
        """Wrap a folded graph; all lookup indexes live in ``gmeow_rdf`` Rust."""
        self.graph = graph
        self._native = (
            ox.GtsFoldViewNative.from_bytes(data)
            if data is not None
            else ox.GtsFoldViewNative.from_parts(
                _term_rows(graph),
                list(graph.quads),
                _reifier_rows(graph),
                list(graph.annotations),
            )
        )

    @classmethod
    def from_bytes(cls, data: bytes) -> FoldView:
        """Read GTS bytes and build a Rust-backed fold view."""
        return cls(read(data), data)

    # -- terms ----------------------------------------------------------------

    def term(self, tid: int) -> Term:
        """The :class:`gts.model.Term` behind a term-id."""
        return self.graph.terms[tid]

    def is_iri(self, tid: int) -> bool:
        """Whether the term is an IRI."""
        return bool(self._native.is_iri(tid))

    def is_bnode(self, tid: int) -> bool:
        """Whether the term is a blank node."""
        return bool(self._native.is_bnode(tid))

    def is_literal(self, tid: int) -> bool:
        """Whether the term is a literal."""
        return bool(self._native.is_literal(tid))

    def iri(self, tid: int) -> str | None:
        """The IRI string, or ``None`` for non-IRI terms."""
        return self._native.iri(tid)

    def lex(self, tid: int) -> str:
        """The lexical form (IRI string, bnode label, or literal value)."""
        return self._native.lex(tid)

    def lang(self, tid: int) -> str | None:
        """The literal's language tag, if any."""
        return self._native.lang(tid)

    def datatype(self, tid: int) -> str:
        """The effective datatype IRI of a literal (§7.1 defaulting)."""
        return self._native.datatype(tid)

    def nq_token(self, tid: int) -> str:
        """The canonical N-Triples token — display and stable sort key."""
        return self._native.nq_token(tid)

    def python_value(self, tid: int) -> object:
        """A Python scalar for a term (the lpg conversion, fold-side)."""
        return self._native.python_value(tid)

    def tid_of_iri(self, iri: str) -> int | None:
        """The term-id of an IRI, or ``None`` when absent from the fold."""
        return self._native.tid_of_iri(iri)

    def curie(self, iri: str) -> str:
        """A CURIE under the longest matching known prefix, else the IRI."""
        for prefix, namespace in _SORTED_PREFIXES:
            if iri.startswith(namespace):
                return f"{prefix}:{iri[len(namespace) :]}"
        return iri

    # -- quads ----------------------------------------------------------------

    def quads(self, scope: str | None = DEFAULT) -> list[Quad]:
        """Quads in a scope: ``DEFAULT``, a named-graph IRI, or ``ALL``."""
        return self._native.quads(scope)

    def subjects_by_type(
        self, class_iri: str, scope: str | None = DEFAULT
    ) -> list[int]:
        """Subjects with ``rdf:type <class_iri>`` in scope, id-sorted."""
        return self._native.subjects_by_type(class_iri, scope)

    def objects(self, s_tid: int, p_iri: str, scope: str | None = DEFAULT) -> list[int]:
        """Objects of ``(s, p, ?)`` in scope, id-sorted (deterministic)."""
        return self._native.objects(s_tid, p_iri, scope)

    def value(self, s_tid: int, p_iri: str, scope: str | None = DEFAULT) -> int | None:
        """One object of ``(s, p, ?)`` — the token-smallest, never graph order."""
        return self._native.value(s_tid, p_iri, scope)

    def predicate_objects(
        self, s_tid: int, scope: str | None = DEFAULT
    ) -> list[tuple[int, int]]:
        """All ``(p, o)`` pairs for a subject in scope, id-sorted."""
        return self._native.predicate_objects(s_tid, scope)

    def has(
        self, s_tid: int, p_iri: str, o_tid: int, scope: str | None = DEFAULT
    ) -> bool:
        """Membership test for ``(s, p, o)`` in scope."""
        return bool(self._native.has(s_tid, p_iri, o_tid, scope))

    def rdf_list(self, head_tid: int, scope: str | None = DEFAULT) -> list[int]:
        """Walk an ``rdf:first``/``rdf:rest`` list from its head term."""
        return self._native.rdf_list(head_tid, scope)

    # -- statement layer -------------------------------------------------------

    def reifiers(self) -> dict[int, Triple]:
        """Reifier-id → quoted triple bindings (global, §7.3)."""
        return dict(self._native.reifiers())

    def annotations(self) -> list[Triple]:
        """``(reifier, predicate, value)`` annotation rows (global)."""
        return self._native.annotations()

    # -- language boundary -----------------------------------------------------

    def tag_map(self) -> dict[str, str]:
        """Internal language tag → BCP-47, from all bundled Language individuals."""
        return dict(self._native.tag_map())

    def available_languages(self) -> frozenset[str]:
        """Public BCP-47 tags that actually have literals in the snapshot."""
        return frozenset(self._native.available_languages())

    def public_text(self, s_tid: int, p_iri: str, scope: str | None = DEFAULT) -> str:
        """The public-facing text for ``(s, p)`` — the projection boundary."""
        return self._native.public_text(s_tid, p_iri, scope)

    def public_literal(
        self, s_tid: int, p_iri: str, scope: str | None = DEFAULT
    ) -> tuple[str, str | None]:
        """The public text for ``(s, p)`` plus its public BCP-47 tag."""
        return self._native.public_literal(s_tid, p_iri, scope)

    def public_literal_with_fallback(
        self,
        s_tid: int,
        p_iri: str,
        selector: LangSelector,
        scope: str | None = DEFAULT,
    ) -> tuple[str, str | None, bool]:
        """Selector-aware single literal for ``(s, p)`` plus fallback signal."""
        return self._native.public_literal_with_fallback(
            s_tid, p_iri, list(selector.requested), scope
        )

    def public_text_with_fallback(
        self,
        s_tid: int,
        p_iri: str,
        selector: LangSelector,
        scope: str | None = DEFAULT,
    ) -> tuple[str, bool]:
        """Selector-aware single text for ``(s, p)`` plus fallback signal."""
        return self._native.public_text_with_fallback(
            s_tid, p_iri, list(selector.requested), scope
        )

    def public_texts(
        self,
        s_tid: int,
        p_iri: str,
        selector: LangSelector,
        scope: str | None = DEFAULT,
    ) -> list[tuple[str, str | None, bool]]:
        """All requested-language literals for ``(s, p)`` plus fallback signal."""
        return self._native.public_texts(s_tid, p_iri, list(selector.requested), scope)


def _term_rows(
    graph: Graph,
) -> list[tuple[int, str | None, int | None, str | None, str | None, int | None]]:
    return [
        (
            int(t.kind),
            t.value,
            t.datatype,
            t.lang,
            getattr(t, "direction", None),
            t.reifier,
        )
        for t in graph.terms
    ]


def _reifier_rows(graph: Graph) -> list[tuple[int, tuple[int, int, int]]]:
    return list(graph.reifiers.items())


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
                target = (out_dir / rel).resolve()
                if not target.is_relative_to(out_dir.resolve()):
                    raise ValueError(
                        f"path traversal in docs blob member: {member.name!r}"
                    )
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
