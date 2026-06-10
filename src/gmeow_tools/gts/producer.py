# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The ``RDF → GTS`` producer (issue #271).

Interns an rdflib ``Graph``/``Dataset`` into the GTS term dictionary and emits a
single ``dist``-profile ``snapshot`` frame (§10, §13). This is the encoder side of
the narrow waist: one producer, many ``GTS → *`` shims.

Scope: RDF 1.1 terms — IRIs, blank nodes, and literals (datatype + language),
plus named graphs (quads). RDF 1.2 triple-terms / statement metadata (reifier +
annot) require the RDF-star source and are a follow-up under #267.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from rdflib import BNode, Dataset, Graph, Literal, URIRef

from gmeow_tools.gts.model import Quad, Term, TermKind
from gmeow_tools.gts.writer import Writer, term_to_wire

if TYPE_CHECKING:
    from rdflib.term import Node


class _Interner:
    """Assigns stable, append-order term-ids and de-duplicates terms (§7.2)."""

    def __init__(self) -> None:
        self.terms: list[Term] = []
        self._index: dict[tuple[object, ...], int] = {}

    def _intern(self, key: tuple[object, ...], make: object) -> int:
        existing = self._index.get(key)
        if existing is not None:
            return existing
        tid = len(self.terms)
        assert callable(make)
        self.terms.append(make())
        self._index[key] = tid
        return tid

    def iri(self, iri: str) -> int:
        return self._intern(("iri", iri), lambda: Term(TermKind.IRI, iri))

    def bnode(self, label: str) -> int:
        return self._intern(("bnode", label), lambda: Term(TermKind.BNODE, label))

    def literal(self, lex: str, datatype: str | None, lang: str | None) -> int:
        # Datatype IRI must be interned first so its id precedes the literal (§7.5).
        dt_id = self.iri(datatype) if datatype is not None else None
        key = ("lit", lex, datatype or "", lang or "")
        return self._intern(
            key, lambda: Term(TermKind.LITERAL, lex, datatype=dt_id, lang=lang)
        )


def _intern_node(interner: _Interner, node: Node) -> int | None:
    """Intern a single rdflib node; ``None`` for an unsupported term kind."""
    if isinstance(node, URIRef):
        return interner.iri(str(node))
    if isinstance(node, BNode):
        return interner.bnode(str(node))
    if isinstance(node, Literal):
        dt = str(node.datatype) if node.datatype is not None else None
        return interner.literal(str(node), dt, node.language)
    return None  # quoted triples (RDF 1.2) are out of scope here


def _iter_quads(graph: Graph) -> list[tuple[Node, Node, Node, URIRef | None]]:
    """Yield (s, p, o, graph-name) rows; default graph has ``None`` name."""
    if isinstance(graph, Dataset):
        rows: list[tuple[Node, Node, Node, URIRef | None]] = []
        for s, p, o, ctx in graph.quads((None, None, None, None)):
            name = ctx.identifier if isinstance(ctx, Graph) else ctx
            rows.append((s, p, o, name if isinstance(name, URIRef) else None))
        return rows
    return [(s, p, o, None) for s, p, o in graph]


def gts_from_graph(
    graph: Graph,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS ``dist`` snapshot from an rdflib ``Graph``/``Dataset``.

    Args:
        graph: the source graph (or dataset for quads).
        profile: the GTS profile (``dist`` by default).
        transform: codec chain for the snapshot payload (default ``["zstd"]``).
    """
    chain = ["zstd"] if transform is None else transform
    interner = _Interner()
    quads: list[Quad] = []
    for s, p, o, name in _iter_quads(graph):
        sid, pid, oid = (
            _intern_node(interner, s),
            _intern_node(interner, p),
            _intern_node(interner, o),
        )
        gid = _intern_node(interner, name) if name is not None else None
        if sid is None or pid is None or oid is None:
            continue  # skip rows with an unsupported (e.g. quoted-triple) term
        quads.append((sid, pid, oid, gid))

    writer = Writer(profile=profile)
    snapshot: dict[str, object] = {
        "terms": [term_to_wire(t) for t in interner.terms],
        "quads": [
            [q[0], q[1], q[2], *([q[3]] if q[3] is not None else [])] for q in quads
        ],
    }
    writer.add_frame("snapshot", payload=snapshot, transform=chain)
    return writer.to_bytes()
