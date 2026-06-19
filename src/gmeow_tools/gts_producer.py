# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The ``RDF → GTS`` producer (issue #271).

The encoder side of the narrow waist. Two ingest paths feed a single term
dictionary:

* **rdflib** ``Graph``/``Dataset`` — the RDF 1.1 base graph (IRIs, blank nodes,
  literals, named-graph quads).
* **pyoxigraph** over an RDF 1.2 artifact (``statements/gmeow.rdf12.ttl``) — the
  statement layer: ``reifier rdf:reifies <<( s p o )>>`` becomes a GTS ``reifies``
  binding and the reifier's other triples become ``annot`` rows (§7.3). rdflib 7.6
  has no triple-term API, so the RDF-star source must be read with pyoxigraph.

Both feed :class:`_PyBuilder`, which emits one ``dist``-profile ``snapshot`` (§10).
"""

from __future__ import annotations

import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import TYPE_CHECKING, Any

from cryptography.hazmat.primitives import serialization
from gts import Signer
from gts.model import XSD_STRING, Quad, Term, TermKind, Triple
from gts.wire import canonical
from gts.writer import Writer, term_to_wire
from rdflib import BNode, Dataset, Graph, Literal, URIRef

from gmeow_tools.config import PROJECT_ROOT

try:
    import gmeow_gts_producer
except ImportError:  # pragma: no cover
    gmeow_gts_producer = None

if TYPE_CHECKING:
    from collections.abc import Sequence

    from rdflib.term import Node

    from gmeow_tools.saturate import DerivedTriple

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

    def bnode(self, label: str, scope: str | None = None) -> int:
        """Intern a blank node, optionally scoped to an ingest source.

        Sources are canonicalized INDEPENDENTLY, so two different
        existential nodes in different sources can carry the same canonical
        label — scoping prevents them collapsing into one term. ``None``
        (single-source builders) preserves the raw label.
        """
        value = label if scope is None else f"{scope}-{label}"
        return self._intern(
            ("bnode", scope, label), lambda: Term(TermKind.BNODE, value)
        )

    def literal(self, lex: str, datatype: str | None, lang: str | None) -> int:
        dt_id = self.iri(datatype) if datatype is not None else None
        key = ("lit", lex, datatype or "", lang or "")
        return self._intern(
            key, lambda: Term(TermKind.LITERAL, lex, datatype=dt_id, lang=lang)
        )


class _PyBuilder:
    """Accumulates terms/quads/reifies/annot from one or more RDF sources."""

    def __init__(self) -> None:
        self.terms = _Interner()
        self.quads: list[Quad] = []
        self.reifies: dict[int, Triple] = {}
        self.annot: list[Triple] = []

    # -- rdflib (RDF 1.1) -----------------------------------------------------

    def add_graph(
        self,
        graph: Graph,
        *,
        graph_name: str | None = None,
        bnode_scope: str | None = None,
    ) -> None:
        """Ingest an rdflib ``Graph``/``Dataset`` base graph.

        ``graph_name`` assigns rows that carry no name of their own to a named
        graph — the snapshot's source-partitioning hook, so consumers can
        scope to exactly the layer they need.
        """
        default_gid = self.terms.iri(graph_name) if graph_name is not None else None
        for s, p, o, name in _iter_quads(graph):
            sid, pid, oid = (
                self._rdflib(s, bnode_scope),
                self._rdflib(p, bnode_scope),
                self._rdflib(o, bnode_scope),
            )
            gid = self._rdflib(name, bnode_scope) if name is not None else default_gid
            if sid is None or pid is None or oid is None:
                continue
            self.quads.append((sid, pid, oid, gid))

    def _rdflib(self, node: Node, bnode_scope: str | None = None) -> int | None:
        if isinstance(node, URIRef):
            return self.terms.iri(str(node))
        if isinstance(node, BNode):
            return self.terms.bnode(str(node), bnode_scope)
        if isinstance(node, Literal):
            dt = str(node.datatype) if node.datatype is not None else None
            return self.terms.literal(str(node), dt, node.language)
        return None  # quoted triples handled via the RDF 1.2 path

    def add_annotated(
        self,
        s: Node,
        p: Node,
        o: Node,
        *,
        reifier: URIRef,
        annotations: Sequence[tuple[URIRef, Node]],
        graph_name: str | None = None,
        bnode_scope: str | None = None,
    ) -> None:
        """Add an asserted triple PLUS its RDF 1.2 statement-layer rows.

        The base triple stays a plain quad (consumers ignorant of RDF 1.2
        still parse it); the reifier binds it in ``reifies`` and carries the
        ``annotations`` rows (§7.3) — the transpiler's inline-provenance
        emission path (#34).
        """
        sid, pid, oid = (
            self._rdflib(s, bnode_scope),
            self._rdflib(p, bnode_scope),
            self._rdflib(o, bnode_scope),
        )
        if sid is None or pid is None or oid is None:
            msg = f"unsupported node in annotated triple: ({s!r}, {p!r}, {o!r})"
            raise ValueError(msg)
        gid = self.terms.iri(graph_name) if graph_name is not None else None
        self.quads.append((sid, pid, oid, gid))
        rid = self.terms.iri(str(reifier))
        existing = self.reifies.get(rid)
        if existing is not None and existing != (sid, pid, oid):
            msg = f"conflicting reifier rebind for {reifier!r}"
            raise ValueError(msg)
        self.reifies[rid] = (sid, pid, oid)
        for ann_p, ann_v in annotations:
            ap, av = self._rdflib(ann_p, bnode_scope), self._rdflib(ann_v, bnode_scope)
            if ap is None or av is None:
                msg = f"unsupported annotation node on {reifier!r}"
                raise ValueError(msg)
            self.annot.append((rid, ap, av))

    # -- pyoxigraph (RDF 1.2 statement layer) ---------------------------------

    def add_rdf12(
        self,
        path: Path,
        *,
        graph_name: str | None = None,
        bnode_scope: str | None = None,
    ) -> None:
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
                rid = self._ox(s, bnode_scope)
                qs, qp, qo = (
                    self._ox(o.subject, bnode_scope),
                    self._ox(o.predicate, bnode_scope),
                    self._ox(o.object, bnode_scope),
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
                        term = self.terms.terms[rid]
                        msg = (
                            "conflicting reifier rebind for "
                            f"{term.value!r} ({term.kind.name})"
                        )
                        raise ValueError(msg)
                    self.reifies[rid] = (qs, qp, qo)
            else:
                pending.append((s, p, o))
        # Pass 2: a reifier's other triples are annotations; the rest are base quads.
        for s, p, o in pending:
            sid, pid, oid = (
                self._ox(s, bnode_scope),
                self._ox(p, bnode_scope),
                self._ox(o, bnode_scope),
            )
            if sid is None or pid is None or oid is None:
                continue
            if sid in reifier_ids:
                self.annot.append((sid, pid, oid))
            else:
                self.quads.append((sid, pid, oid, default_gid))

    def _ox(self, node: object, bnode_scope: str | None = None) -> int | None:
        import pyoxigraph as ox

        if isinstance(node, ox.NamedNode):
            return self.terms.iri(node.value)
        if isinstance(node, ox.BlankNode):
            return self.terms.bnode(node.value, bnode_scope)
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
        # CBOR canonical encoding sorts map keys at emit, so dict order
        # never reaches the bytes — sorted by NEW id for inspection sanity.
        new_reifies = {
            remap[rid]: (remap[s], remap[p], remap[o])
            for rid, (s, p, o) in sorted(
                self.reifies.items(), key=lambda item: remap[item[0]]
            )
        }
        new_annot = sorted({(remap[r], remap[p], remap[v]) for r, p, v in self.annot})
        return new_terms, new_quads, new_reifies, new_annot

    # -- emit -----------------------------------------------------------------

    def to_gts(
        self,
        *,
        profile: str = "dist",
        transform: list[str] | None = None,
        doc_blobs: list[tuple[bytes, str, str]] | None = None,
        signer: Signer | None = None,
        public_key_armor: str | None = None,
        rsyncable_threshold: int = 65536,
    ) -> bytes:
        """Emit a single ``dist`` snapshot frame from the accumulated tables.

        ``doc_blobs`` (#325): content-addressed documentation payloads —
        ``(data, media_type, rep)`` rows, emitted as blob frames AHEAD of the
        snapshot frame in a deterministic (rep, digest) order so the bytes
        stay a pure function of the inputs. The graph links each slice to
        its guide via ``gmeow:guideBlob "blake3:<hex>"`` (the reader keys
        blobs by BLAKE3 of the decoded content).

        If ``signer`` and ``public_key_armor`` are supplied, a ``meta`` frame
        carrying the file's transport key is emitted first and signed along
        with every subsequent frame.

        ``rsyncable_threshold`` (#513): payloads larger than this many bytes
        are compressed with ``zstd-rsyncable`` instead of ``zstd``, improving
        rsync/Git delta compression at a small size cost.
        """
        if (signer is None) != (public_key_armor is None):
            raise ValueError("signer and public_key_armor must be supplied together")
        base_chain = ["zstd"] if transform is None else transform
        terms, quads, reifies, annot = self._canonical_tables()
        writer = Writer(profile=profile, signer=signer)
        if signer is not None:
            writer.add_meta(
                {
                    "gts:transportKey": {
                        "kid": signer.kid,
                        "gpg": public_key_armor,
                    }
                }
            )

        def choose_transform(payload_bytes: bytes) -> list[str]:
            """Use zstd-rsyncable for large payloads to improve delta compression."""
            if base_chain == ["zstd"] and len(payload_bytes) > rsyncable_threshold:
                return ["zstd-rsyncable"]
            return list(base_chain)

        for data, media_type, rep in sorted(
            doc_blobs or [], key=lambda row: (row[2], row[0])
        ):
            writer.add_blob(
                data, mt=media_type, rep=rep, transform=choose_transform(data)
            )
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
        snapshot_bytes = canonical(snapshot)
        writer.add_frame(
            "snapshot", payload=snapshot, transform=choose_transform(snapshot_bytes)
        )
        return writer.to_bytes()


def _iter_quads(graph: Graph) -> list[tuple[Node, Node, Node, Node | None]]:
    """Yield (s, p, o, graph-name) rows; the default graph has a ``None`` name."""
    if isinstance(graph, Dataset):
        rows: list[tuple[Node, Node, Node, Node | None]] = []
        default_id = graph.default_graph.identifier
        for s, p, o, ctx in graph.quads((None, None, None, None)):
            name = ctx.identifier if isinstance(ctx, Graph) else ctx
            if name == default_id:
                name = None
            elif not isinstance(name, URIRef | BNode):
                continue  # skip quads with an unsupported (non-IRI/bnode) graph name
            rows.append((s, p, o, name))
        return rows
    return [(s, p, o, None) for s, p, o in graph]


def _canonical_bytes(graph: Graph) -> bytes:
    """Return deterministic serialized bytes for an rdflib graph/dataset.

    Plain ``Graph`` instances are canonicalized and emitted as N-Triples;
    ``Dataset`` instances are emitted as N-Quads so named graphs are preserved.
    """
    from rdflib.compare import to_canonical_graph

    canonical = to_canonical_graph(graph)
    fmt = "nquads" if isinstance(graph, Dataset) else "ntriples"
    return canonical.serialize(format=fmt).encode("utf-8")


def _add_graph_file(
    builder: Any,
    graph: Graph,
    *,
    graph_name: str | None = None,
    bnode_scope: str | None = None,
) -> None:
    """Canonicalize ``graph`` and feed it to the Rust builder from a temp file."""
    data = _canonical_bytes(graph)
    suffix = ".nq" if isinstance(graph, Dataset) else ".nt"
    with tempfile.NamedTemporaryFile(
        dir=PROJECT_ROOT,
        prefix=".gmeow-gts-base-",
        suffix=suffix,
        delete=False,
    ) as tmp:
        tmp.write(data)
        tmp_path = tmp.name
    try:
        builder.add_graph(tmp_path, graph_name=graph_name, bnode_scope=bnode_scope)
    finally:
        Path(tmp_path).unlink(missing_ok=True)


def _require_rust() -> Any:
    """Return the Rust extension module or raise a build-time helper message."""
    if gmeow_gts_producer is None:
        raise RuntimeError(
            "The gmeow_gts_producer Rust extension is not installed. "
            "Run `make gts-producer-py` to build it."
        )
    return gmeow_gts_producer


def gts_from_graph(
    graph: Graph,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS ``dist`` snapshot from an rdflib graph/dataset (RDF 1.1)."""
    builder = _PyBuilder()
    builder.add_graph(graph)
    return builder.to_gts(profile=profile, transform=transform)


def gts_from_rdf12(
    path: Path,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS snapshot from an RDF 1.2 artifact (statement layer; pyoxigraph)."""
    builder = _PyBuilder()
    builder.add_rdf12(path)
    return builder.to_gts(profile=profile, transform=transform)


def gts_from_maximal(
    base: Graph,
    derived: Sequence[DerivedTriple],
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce the transpiler's MAXIMAL(G) snapshot (#34).

    ``base`` carries the canonical A-Box (assumed bnode-free — the transform
    driver skolemizes); every :class:`~gmeow_tools.saturate.DerivedTriple`
    lands as an asserted base triple plus its provenance reifier/annotations.
    """
    builder = _PyBuilder()
    builder.add_graph(base)
    for row in derived:
        s, p, o = row.triple
        builder.add_annotated(s, p, o, reifier=row.reifier, annotations=row.annotations)
    return builder.to_gts(profile=profile, transform=transform)


def compile_gts(
    graph: Graph,
    rdf12_path: Path | None = None,
    *,
    alignment_graph: Graph | None = None,
    extra_named_graphs: Sequence[tuple[Graph, str, str]] | None = None,
    transform: list[str] | None = None,
    doc_blobs: list[tuple[bytes, str, str]] | None = None,
    signer: Signer | None = None,
    public_key_armor: str | None = None,
    rsyncable_threshold: int = 65536,
) -> bytes:
    """Compile the statement-complete, byte-deterministic ``dist`` GTS snapshot.

    The narrow waist's producer: the RDF 1.1 base graph rides in the default
    graph, the RDF 1.2 statement layer in ``gmeow:graph/statements`` (its
    reifies/annot tables are global), the SSSOM alignment axioms in
    ``gmeow:graph/alignments``, and any additional explicitly named graphs
    supplied by the caller. rdflib blank-node labels are per-process UUIDs,
    so rdflib sources are canonicalized
    (:func:`rdflib.compare.to_canonical_graph`) — together with the
    content-sorted term table this makes the emitted bytes a pure function
    of the inputs (the drift-gate requirement).

    If ``signer`` and ``public_key_armor`` are supplied, the snapshot is signed
    and the armored transport public key is embedded in the first ``meta`` frame.

    ``rsyncable_threshold`` (#513): payloads larger than this many bytes are
    compressed with ``zstd-rsyncable`` instead of ``zstd``.

    Raises:
        FileNotFoundError: if ``rdf12_path`` is given but does not exist (a missing
            statement layer is an error, not a silent RDF-1.1-only fallback).
    """
    from gmeow_tools.config import GTS_GRAPH_ALIGNMENTS, GTS_GRAPH_STATEMENTS

    mod = _require_rust()
    builder = mod.Builder()

    _add_graph_file(builder, graph, graph_name=None, bnode_scope="base")

    if rdf12_path is not None:
        if not rdf12_path.exists():
            msg = f"RDF 1.2 statement artifact not found: {rdf12_path}"
            raise FileNotFoundError(msg)
        builder.add_rdf12(
            str(rdf12_path), graph_name=GTS_GRAPH_STATEMENTS, bnode_scope="stmt"
        )

    if alignment_graph is not None:
        _add_graph_file(
            builder,
            alignment_graph,
            graph_name=GTS_GRAPH_ALIGNMENTS,
            bnode_scope="align",
        )

    for named_graph, graph_name, bnode_scope in extra_named_graphs or ():
        _add_graph_file(
            builder,
            named_graph,
            graph_name=graph_name,
            bnode_scope=bnode_scope,
        )

    if (signer is None) != (public_key_armor is None):
        raise ValueError("signer and public_key_armor must be supplied together")

    signer_kid: str | None = None
    signer_secret: bytes | None = None
    if signer is not None:
        signer_kid = signer.kid
        signer_secret = signer.key.private_bytes(
            serialization.Encoding.Raw,
            serialization.PrivateFormat.Raw,
            serialization.NoEncryption(),
        )

    return bytes(
        builder.to_gts(
            profile="dist",
            transform=transform,
            doc_blobs=doc_blobs,
            signer_kid=signer_kid,
            signer_secret=signer_secret,
            public_key_armor=public_key_armor,
            rsyncable_threshold=rsyncable_threshold,
        )
    )
