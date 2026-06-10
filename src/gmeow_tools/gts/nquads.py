# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The ``gts → nquads`` transform (§14).

Serialises the folded base quads, plus reifier/annotation triples in the RDF 1.2
reifying style (``<reifier> rdf:reifies <<( s p o )>>`` and ``<reifier> p v``).
Inline blobs are externalised by the caller; this module emits the graph text only.
"""

from __future__ import annotations

from gmeow_tools.gts.model import Graph, Term, TermKind

_RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"


def _escape(lex: str) -> str:
    """Escape a literal lexical form for N-Triples."""
    return (
        lex.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def _render(g: Graph, tid: int) -> str:
    """Render a term-id as an N-Triples token."""
    t: Term = g.terms[tid]
    if t.kind is TermKind.IRI:
        return f"<{t.value or ''}>"
    if t.kind is TermKind.BNODE:
        return f"_:{t.value or f'b{tid}'}"
    if t.kind is TermKind.LITERAL:
        lit = f'"{_escape(t.value or "")}"'
        if t.lang is not None:
            return f"{lit}@{t.lang}"
        if t.datatype is not None:
            return f"{lit}^^{_render(g, t.datatype)}"
        return lit  # plain literal == xsd:string (§7.1)
    # quoted triple (RDF 1.2 triple term), resolved through its reifier
    if t.reifier is not None and t.reifier in g.reifiers:
        s, p, o = g.reifiers[t.reifier]
        return f"<<( {_render(g, s)} {_render(g, p)} {_render(g, o)} )>>"
    return "<<( )>>"


def to_nquads(g: Graph) -> str:
    """Serialise a folded :class:`Graph` to N-Quads text."""
    lines: list[str] = []
    for s, p, o, gname in g.quads:
        triple = f"{_render(g, s)} {_render(g, p)} {_render(g, o)}"
        if gname is not None:
            lines.append(f"{triple} {_render(g, gname)} .")
        else:
            lines.append(f"{triple} .")
    for rid, spo in g.reifiers.items():
        quoted = (
            f"<<( {_render(g, spo[0])} {_render(g, spo[1])} {_render(g, spo[2])} )>>"
        )
        lines.append(f"{_render(g, rid)} <{_RDF_REIFIES}> {quoted} .")
    for r, p, v in g.annotations:
        lines.append(f"{_render(g, r)} {_render(g, p)} {_render(g, v)} .")
    return "\n".join(lines) + ("\n" if lines else "")
