# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The ``gts → nquads`` transform (§14).

Serialises the folded base quads, plus reifier/annotation triples in the RDF 1.2
reifying style (``<reifier> rdf:reifies <<( s p o )>>`` and ``<reifier> p v``).
Inline blobs are externalised by the caller; this module emits the graph text only.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from gts.model import Graph, Term, TermKind

if TYPE_CHECKING:
    from collections.abc import Mapping

_RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"


def _escape(lex: str) -> str:
    """Escape a literal lexical form for N-Triples (incl. all C0 control chars)."""
    out: list[str] = []
    for ch in lex:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif ord(ch) < 0x20:
            out.append(f"\\u{ord(ch):04X}")
        else:
            out.append(ch)
    return "".join(out)


def _render(g: Graph, tid: int, lang_map: Mapping[str, str] | None = None) -> str:
    """Render a term-id as an N-Triples token.

    ``lang_map`` remaps literal language tags on output (tags absent from the
    map pass through unchanged); the stored graph is never modified.
    """
    t: Term = g.terms[tid]
    if t.kind is TermKind.IRI:
        return f"<{t.value or ''}>"
    if t.kind is TermKind.BNODE:
        return f"_:{t.value or f'b{tid}'}"
    if t.kind is TermKind.LITERAL:
        lit = f'"{_escape(t.value or "")}"'
        if t.lang is not None:
            lang = lang_map.get(t.lang, t.lang) if lang_map else t.lang
            return f"{lit}@{lang}"
        if t.datatype is not None:
            return f"{lit}^^{_render(g, t.datatype, lang_map)}"
        return lit  # plain literal == xsd:string (§7.1)
    # quoted triple (RDF 1.2 triple term), resolved through its reifier
    if t.reifier is not None and t.reifier in g.reifiers:
        s, p, o = g.reifiers[t.reifier]
        return (
            f"<<( {_render(g, s, lang_map)} {_render(g, p, lang_map)} "
            f"{_render(g, o, lang_map)} )>>"
        )
    # degraded but syntactically valid: an unbound reifier becomes a blank node
    return f"_:unbound_triple_{tid}"


def term_token(g: Graph, tid: int, lang_map: Mapping[str, str] | None = None) -> str:
    """Render the canonical N-Triples token for a term-id (public API).

    IRIs in angle brackets, escaped literals with language or datatype,
    quoted triples through their reifier — the stable sort key and display
    form that tooling builds on. ``lang_map`` optionally remaps language tags
    on output (a rendering option; the graph is untouched).
    """
    return _render(g, tid, lang_map)


def to_nquads(g: Graph, lang_map: Mapping[str, str] | None = None) -> str:
    """Serialise a folded :class:`Graph` to N-Quads text.

    ``lang_map`` optionally remaps literal language tags on output.
    """
    lines: list[str] = []
    for s, p, o, gname in g.quads:
        triple = (
            f"{_render(g, s, lang_map)} {_render(g, p, lang_map)} "
            f"{_render(g, o, lang_map)}"
        )
        if gname is not None:
            lines.append(f"{triple} {_render(g, gname, lang_map)} .")
        else:
            lines.append(f"{triple} .")
    for rid, spo in g.reifiers.items():
        quoted = (
            f"<<( {_render(g, spo[0], lang_map)} {_render(g, spo[1], lang_map)} "
            f"{_render(g, spo[2], lang_map)} )>>"
        )
        lines.append(f"{_render(g, rid, lang_map)} <{_RDF_REIFIES}> {quoted} .")
    for r, p, v in g.annotations:
        lines.append(
            f"{_render(g, r, lang_map)} {_render(g, p, lang_map)} "
            f"{_render(g, v, lang_map)} ."
        )
    return "\n".join(lines) + ("\n" if lines else "")
