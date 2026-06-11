# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The ``RDF → GTS`` producer (issue #271).

The encoder side of the narrow waist. Two ingest paths feed a single term
dictionary:

* **rdflib** ``Graph``/``Dataset`` — the RDF 1.1 base graph (IRIs, blank nodes,
  literals, named-graph quads).
* **pyoxigraph** over an RDF 1.2 artifact (``statements/gmeow.rdf12.ttl``) — the
  statement layer: ``reifier rdf:reifies <<( s p o )>>`` becomes a GTS ``reifies``
  binding and the reifier's other triples become ``annot`` rows (§7.3). rdflib 7.6
  has no triple-term API, so the RDF-star source must be read with pyoxigraph.

Both feed :class:`_Builder`, which emits one ``dist``-profile ``snapshot`` (§10).
"""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from typing import TYPE_CHECKING

from rdflib import BNode, Dataset, Graph, Literal, URIRef

from gts.model import XSD_STRING, Quad, Term, TermKind, Triple
from gts.writer import Writer, term_to_wire

if TYPE_CHECKING:
    from rdflib.term import Node

_RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"


class _Interner:
    """Assigns stable, append-order term-ids and de-duplicates terms (§7.2)."""

    def __init__(self) -> None:
        self.terms: list[Term] = []
        self._index: dict[tuple[object, ...], int] = {}

    def _intern(self, key: tuple[object, ...], make: Callable[[], Term]) -> int:
        existing = self._index.get(key)
        if existing is not None:
            return existing
        tid = len(self.terms)
        self.terms.append(make())
        self._index[key] = tid
        return tid

    def iri(self, iri: str) -> int:
        return self._intern(("iri", iri), lambda: Term(TermKind.IRI, iri))

    def bnode(self, label: str) -> int:
        return self._intern(("bnode", label), lambda: Term(TermKind.BNODE, label))

    def literal(self, lex: str, datatype: str | None, lang: str | None) -> int:
        dt_id = self.iri(datatype) if datatype is not None else None
        key = ("lit", lex, datatype or "", lang or "")
        return self._intern(
            key, lambda: Term(TermKind.LITERAL, lex, datatype=dt_id, lang=lang)
        )


class _Builder:
    """Accumulates terms/quads/reifies/annot from one or more RDF sources."""

    def __init__(self) -> None:
        self.terms = _Interner()
        self.quads: list[Quad] = []
        self.reifies: dict[int, Triple] = {}
        self.annot: list[Triple] = []

    # -- rdflib (RDF 1.1) -----------------------------------------------------

    def add_graph(self, graph: Graph, *, graph_name: str | None = None) -> None:
        """Ingest an rdflib ``Graph``/``Dataset`` base graph.

        ``graph_name`` assigns rows that carry no name of their own to a named
        graph — the snapshot's source-partitioning hook (§3.1 of the plan:
        statements and alignments ride in their own graphs so consumers can
        scope to exactly the layer they need).
        """
        default_gid = self.terms.iri(graph_name) if graph_name is not None else None
        for s, p, o, name in _iter_quads(graph):
            sid, pid, oid = self._rdflib(s), self._rdflib(p), self._rdflib(o)
            gid = self._rdflib(name) if name is not None else default_gid
            if sid is None or pid is None or oid is None:
                continue
            self.quads.append((sid, pid, oid, gid))

    def _rdflib(self, node: Node) -> int | None:
        if isinstance(node, URIRef):
            return self.terms.iri(str(node))
        if isinstance(node, BNode):
            return self.terms.bnode(str(node))
        if isinstance(node, Literal):
            dt = str(node.datatype) if node.datatype is not None else None
            return self.terms.literal(str(node), dt, node.language)
        return None  # quoted triples handled via the RDF 1.2 path

    # -- pyoxigraph (RDF 1.2 statement layer) ---------------------------------

    def add_rdf12(self, path: Path, *, graph_name: str | None = None) -> None:
        """Ingest an RDF 1.2 artifact: ``rdf:reifies`` triple-terms + annotations.

        Base (non-reifier) triples land in ``graph_name`` when given; the
        ``reifies``/``annot`` tables are global (§7.3).
        """
        import pyoxigraph as ox

        default_gid = self.terms.iri(graph_name) if graph_name is not None else None
        statements = list(ox.parse(path.read_bytes(), format=ox.RdfFormat.TURTLE))
        reifier_ids: set[int] = set()
        pending: list[tuple[object, object, object]] = []
        # Pass 1: reifies bindings establish which subjects are reifiers.
        for st in statements:
            s, p, o = st.subject, st.predicate, st.object
            if (
                isinstance(p, ox.NamedNode)
                and p.value == _RDF_REIFIES
                and isinstance(o, ox.Triple)
            ):
                rid = self._ox(s)
                qs, qp, qo = (
                    self._ox(o.subject),
                    self._ox(o.predicate),
                    self._ox(o.object),
                )
                if (
                    rid is not None
                    and qs is not None
                    and qp is not None
                    and qo is not None
                ):
                    reifier_ids.add(rid)
                    existing = self.reifies.get(rid)
                    if existing is not None and existing != (qs, qp, qo):
                        # The canonical artifact demands clean input: with
                        # content-sorted emission, "first wins" would be
                        # order-defined — so a conflicting rebind is an error
                        # here, never a silent pick (the READER's tolerance
                        # rule, §7.8, is for foreign files, not our producer).
                        msg = f"conflicting reifier rebind for term {rid}"
                        raise ValueError(msg)
                    self.reifies[rid] = (qs, qp, qo)
            else:
                pending.append((s, p, o))
        # Pass 2: a reifier's other triples are annotations; the rest are base quads.
        for s, p, o in pending:
            sid, pid, oid = self._ox(s), self._ox(p), self._ox(o)
            if sid is None or pid is None or oid is None:
                continue
            if sid in reifier_ids:
                self.annot.append((sid, pid, oid))
            else:
                self.quads.append((sid, pid, oid, default_gid))

    def _ox(self, node: object) -> int | None:
        import pyoxigraph as ox

        if isinstance(node, ox.NamedNode):
            return self.terms.iri(node.value)
        if isinstance(node, ox.BlankNode):
            return self.terms.bnode(node.value)
        if isinstance(node, ox.Literal):
            # pyoxigraph always sets a datatype (xsd:string / rdf:langString implied).
            if node.language is not None:
                return self.terms.literal(node.value, None, node.language)
            dt = node.datatype.value
            return self.terms.literal(
                node.value, None if dt == XSD_STRING else dt, None
            )
        return None

    # -- canonical finalize ----------------------------------------------------

    def _canonical_tables(
        self,
    ) -> tuple[list[Term], list[Quad], dict[int, Triple], list[Triple]]:
        """Re-id every term by content and sort every row (determinism).

        Interning order is ingestion order — which for rdflib sources is
        process-unstable. Term-ids in the emitted snapshot must be a pure
        function of CONTENT, so: sort terms by (kind, value, datatype-IRI,
        lang) — IRIs first, so every literal's datatype IRI precedes it
        (§7.5's already-introduced rule holds by construction) — remap all
        tables through the permutation, then sort and de-duplicate rows
        (the folded graph is a set, §7.8).
        """
        terms = self.terms.terms

        def key(tid: int) -> tuple[int, str, str, str]:
            t = terms[tid]
            dt = terms[t.datatype].value or "" if t.datatype is not None else ""
            return (int(t.kind), t.value or "", dt, t.lang or "")

        order = sorted(range(len(terms)), key=key)
        remap = {old_id: new_id for new_id, old_id in enumerate(order)}

        def remap_term(t: Term) -> Term:
            return Term(
                kind=t.kind,
                value=t.value,
                datatype=remap[t.datatype] if t.datatype is not None else None,
                lang=t.lang,
                reifier=remap[t.reifier] if t.reifier is not None else None,
            )

        new_terms = [remap_term(terms[old_id]) for old_id in order]
        new_quads = sorted(
            {
                (remap[s], remap[p], remap[o], remap[g] if g is not None else None)
                for s, p, o, g in self.quads
            },
            key=lambda q: (-1 if q[3] is None else q[3], q[0], q[1], q[2]),
        )
        new_reifies = {
            remap[rid]: (remap[s], remap[p], remap[o])
            for rid, (s, p, o) in sorted(self.reifies.items())
        }
        new_annot = sorted({(remap[r], remap[p], remap[v]) for r, p, v in self.annot})
        return new_terms, new_quads, new_reifies, new_annot

    # -- emit -----------------------------------------------------------------

    def to_gts(
        self, *, profile: str = "dist", transform: list[str] | None = None
    ) -> bytes:
        """Emit a single ``dist`` snapshot frame from the accumulated tables."""
        chain = ["zstd"] if transform is None else transform
        terms, quads, reifies, annot = self._canonical_tables()
        writer = Writer(profile=profile)
        snapshot: dict[str, object] = {
            "terms": [term_to_wire(t) for t in terms],
            "quads": [
                [q[0], q[1], q[2], *([q[3]] if q[3] is not None else [])] for q in quads
            ],
        }
        if reifies:
            snapshot["reifies"] = {rid: list(spo) for rid, spo in reifies.items()}
        if annot:
            snapshot["annot"] = [list(row) for row in annot]
        writer.add_frame("snapshot", payload=snapshot, transform=chain)
        return writer.to_bytes()


def _iter_quads(graph: Graph) -> list[tuple[Node, Node, Node, Node | None]]:
    """Yield (s, p, o, graph-name) rows; the default graph has a ``None`` name."""
    if isinstance(graph, Dataset):
        rows: list[tuple[Node, Node, Node, Node | None]] = []
        default_id = graph.default_context.identifier
        for s, p, o, ctx in graph.quads((None, None, None, None)):
            name = ctx.identifier if isinstance(ctx, Graph) else ctx
            if name == default_id:
                name = None
            elif not isinstance(name, URIRef | BNode):
                continue  # skip quads with an unsupported (non-IRI/bnode) graph name
            rows.append((s, p, o, name))
        return rows
    return [(s, p, o, None) for s, p, o in graph]


def gts_from_graph(
    graph: Graph,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS ``dist`` snapshot from an rdflib graph/dataset (RDF 1.1)."""
    builder = _Builder()
    builder.add_graph(graph)
    return builder.to_gts(profile=profile, transform=transform)


def gts_from_rdf12(
    path: Path,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS snapshot from an RDF 1.2 artifact (statement layer; pyoxigraph)."""
    builder = _Builder()
    builder.add_rdf12(path)
    return builder.to_gts(profile=profile, transform=transform)


def compile_gts(
    graph: Graph,
    rdf12_path: Path | None = None,
    *,
    alignment_graph: Graph | None = None,
    transform: list[str] | None = None,
) -> bytes:
    """Compile the statement-complete, byte-deterministic ``dist`` GTS snapshot.

    The narrow waist's producer: the RDF 1.1 base graph rides in the default
    graph, the RDF 1.2 statement layer in ``gmeow:graph/statements`` (its
    reifies/annot tables are global), and the SSSOM alignment axioms in
    ``gmeow:graph/alignments``. rdflib blank-node labels are per-process
    UUIDs, so both rdflib sources are canonicalized
    (:func:`rdflib.compare.to_canonical_graph`) — together with the
    content-sorted term table this makes the emitted bytes a pure function
    of the inputs (the drift-gate requirement).

    Raises:
        FileNotFoundError: if ``rdf12_path`` is given but does not exist (a missing
            statement layer is an error, not a silent RDF-1.1-only fallback).
    """
    from rdflib.compare import to_canonical_graph

    from gmeow_tools.config import GTS_GRAPH_ALIGNMENTS, GTS_GRAPH_STATEMENTS

    builder = _Builder()
    builder.add_graph(to_canonical_graph(graph))
    if rdf12_path is not None:
        if not rdf12_path.exists():
            msg = f"RDF 1.2 statement artifact not found: {rdf12_path}"
            raise FileNotFoundError(msg)
        builder.add_rdf12(rdf12_path, graph_name=GTS_GRAPH_STATEMENTS)
    if alignment_graph is not None:
        builder.add_graph(
            to_canonical_graph(alignment_graph), graph_name=GTS_GRAPH_ALIGNMENTS
        )
    return builder.to_gts(profile="dist", transform=transform)
