# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The ``RDF → GTS`` producer (issue #271).

The encoder side of the narrow waist. Two ingest paths feed a single term
dictionary:

* **rdflib** ``Graph``/``Dataset`` — the RDF 1.1 base graph (IRIs, blank nodes,
  literals, named-graph quads).
* **gmeow_rdf** over an RDF 1.2 artifact (``statements/gmeow.rdf12.ttl``) — the
  statement layer: ``reifier rdf:reifies <<( s p o )>>`` becomes a GTS ``reifies``
  binding and the reifier's other triples become ``annot`` rows (§7.3). rdflib 7.6
  has no triple-term API, so the RDF-star source must be read with gmeow_rdf.

The GTS BYTES are produced in Rust (#819 Task 8): :class:`_Builder` is a thin
glue layer that lowers rdflib sources to ``gmeow_rdf.Quad`` lists and hands them
to the native ``gmeow_rdf`` producer (``compile_gts_native`` /
``gts_from_*_native``), which authors the single ``dist``-profile ``snapshot``
frame (§10). The snapshot payload is byte-identical to the historical Python
encoder; only the zstd codec differs (codec-skew), so the committed ``gmeow.gts``
folds identically while its compressed head-id may drift.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import gmeow_rdf as ox
from rdflib import Dataset, Graph, URIRef

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from gts import Signer
    from rdflib.term import Node

    from gmeow_tools.saturate import DerivedTriple

_RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"


def _nquads_bytes(graph: Graph) -> bytes:
    """Serialize an rdflib ``Graph``/``Dataset`` to N-Quads bytes for native ingest.

    Lowering to text (not ``gmeow_rdf.Quad`` objects) is deliberate: the strict
    ``gmeow_rdf.Literal`` constructor rejects the ontology's private-use language
    tags (``@x-gmeow-*``), whereas the native producer parses these bytes with the
    LENIENT oxigraph parser — preserving every tag verbatim, exactly as the prior
    rdflib→``gts.model.Term`` ingest did. Quoted-triple components in the base
    graph are not representable in N-Quads and never occur here (they ride the RDF
    1.2 statement path); a Dataset's named graphs survive as the quad graph slot.
    """
    fmt = "nquads" if isinstance(graph, Dataset) else "nt"
    data = graph.serialize(format=fmt, encoding="utf-8")
    return data if isinstance(data, bytes) else data.encode("utf-8")


def _signer_parts(
    signer: Signer | None, public_key_armor: str | None
) -> tuple[bytes | None, str | None, str | None]:
    """Extract the 32 raw Ed25519 secret bytes + kid from a ``gts.Signer``.

    The native producer signs with the raw key (matching ``gts.crypto.sign_id``),
    so the armored OpenPGP secret never crosses the FFI. ``signer`` xor
    ``public_key_armor`` is rejected by the native side (no-optionality).
    """
    if signer is None:
        return None, None, public_key_armor
    from cryptography.hazmat.primitives import serialization

    raw = signer.key.private_bytes(
        serialization.Encoding.Raw,
        serialization.PrivateFormat.Raw,
        serialization.NoEncryption(),
    )
    return raw, signer.kid, public_key_armor


def _nt_term(node: Node) -> str:
    """An rdflib node's N-Triples token (IRI / blank node / literal)."""
    return node.n3()


def _reifies_nt(s: Node, p: Node, o: Node, reifier: URIRef) -> str:
    """The RDF 1.2 ``reifier rdf:reifies <<( s p o )>>`` N-Triples line."""
    triple_term = f"<<( {_nt_term(s)} {_nt_term(p)} {_nt_term(o)} )>>"
    return f"{_nt_term(reifier)} <{_RDF_REIFIES}> {triple_term} ."


class _Builder:
    """Glue that accumulates rdflib sources and emits GTS bytes via Rust.

    Each rdflib source is serialized to N-Quads bytes and handed to the native
    ``gmeow_rdf`` producer, which parses leniently, interns, content-sorts, and
    authors the snapshot frame. This class holds NO encoding logic of its own.
    """

    def __init__(self) -> None:
        self._base_data: bytes = b""
        self._named_graphs: list[tuple[bytes, str | None, str | None]] = []
        self._rdf12: tuple[bytes, str | None, str | None] | None = None
        # Programmatic statement layer (the maximal/annotated path, #34), built as
        # RDF 1.2 N-Triples lines because rdflib 7.6 has no triple-term API.
        self._statement_lines: list[str] = []

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
        graph — the snapshot's source-partitioning hook, so consumers can scope
        to exactly the layer they need.
        """
        data = _nquads_bytes(graph)
        if graph_name is None and bnode_scope is None:
            self._base_data += data
        else:
            self._named_graphs.append((data, graph_name, bnode_scope))

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

        The base triple stays a plain quad (consumers ignorant of RDF 1.2 still
        parse it); the reifier binds it in ``reifies`` and carries the
        ``annotations`` rows (§7.3) — the transpiler's inline-provenance emission
        path (#34). ``graph_name``/``bnode_scope`` are accepted for signature
        compatibility; the maximal path uses neither.
        """
        self._base_data += f"{_nt_term(s)} {_nt_term(p)} {_nt_term(o)} .\n".encode()
        self._statement_lines.append(_reifies_nt(s, p, o, reifier))
        for ann_p, ann_v in annotations:
            self._statement_lines.append(
                f"{_nt_term(reifier)} {_nt_term(ann_p)} {_nt_term(ann_v)} ."
            )

    # -- gmeow_rdf (RDF 1.2 statement layer) ---------------------------------

    def add_rdf12(
        self,
        path: Path,
        *,
        graph_name: str | None = None,
        bnode_scope: str | None = None,
    ) -> None:
        """Ingest an RDF 1.2 artifact: ``rdf:reifies`` triple-terms + annotations.

        Base (non-reifier) triples land in ``graph_name`` when given; the
        ``reifies``/``annot`` tables are global (§7.3). The native producer does
        the parsing + two-pass reifier/annotation classification.
        """
        self._rdf12 = (path.read_bytes(), graph_name, bnode_scope)

    def _statement_bytes(self) -> bytes | None:
        """The accumulated programmatic statement layer as RDF 1.2 N-Triples."""
        if not self._statement_lines:
            return None
        return ("\n".join(self._statement_lines) + "\n").encode("utf-8")

    # -- emit -----------------------------------------------------------------

    def snapshot_content_id(self) -> str:
        """A ``blake3:<hex>`` content address of the snapshot payload (#654).

        Mirrors the native producer's content id over the SAME accumulated base
        graph. Used by the diagnostics self-attestation (only ever called on a
        base-graph-only builder, the feedback-bundle case).
        """
        return ox.snapshot_content_id_native(
            self._base_data, format=ox.RdfFormat.N_QUADS
        )

    def to_gts(
        self,
        *,
        profile: str = "dist",
        transform: list[str] | None = None,
        doc_blobs: list[tuple[bytes, str, str]] | None = None,
        report_blobs: list[tuple[bytes, str, str]] | None = None,
        signer: Signer | None = None,
        public_key_armor: str | None = None,
        rsyncable_threshold: int = 65536,
    ) -> bytes:
        """Emit a single ``dist`` snapshot frame from the accumulated tables.

        ``doc_blobs`` (#325) and ``report_blobs`` (#654) ride as content-addressed
        blob frames AHEAD of the snapshot in a deterministic (rep, digest) order,
        so the bytes stay a pure function of the inputs. They are purely additive
        and never alter the snapshot frame, so the bundle's graph identity is
        unaffected by an embedded report.

        If ``signer`` and ``public_key_armor`` are supplied, a ``meta`` frame
        carrying the transport key is emitted first and signed along with every
        subsequent frame (tamper-evident attestation). ``rsyncable_threshold``
        (#513) selects ``zstd-rsyncable`` for payloads larger than it.
        """
        if (signer is None) != (public_key_armor is None):
            raise ValueError("signer and public_key_armor must be supplied together")
        secret, kid, armor = _signer_parts(signer, public_key_armor)

        # The programmatic statement layer (maximal/annotated, #34) is the RDF 1.2
        # rdf12 source when no file-based one was supplied; both never coexist.
        statement = self._statement_bytes()
        rdf12_data = self._rdf12[0] if self._rdf12 is not None else statement
        rdf12_format = (
            ox.RdfFormat.TURTLE if self._rdf12 is not None else ox.RdfFormat.N_TRIPLES
        )
        rdf12_graph_name = self._rdf12[1] if self._rdf12 is not None else None
        rdf12_scope = self._rdf12[2] if self._rdf12 is not None else None
        return ox.compile_gts_native(
            self._base_data,
            ox.RdfFormat.N_QUADS,
            base_scope=None,
            rdf12_data=rdf12_data,
            rdf12_format=rdf12_format if rdf12_data is not None else None,
            rdf12_graph_name=rdf12_graph_name,
            rdf12_scope=rdf12_scope,
            named_graphs=[
                (data, ox.RdfFormat.N_QUADS, name, scope)
                for data, name, scope in self._named_graphs
            ],
            transform=transform,
            doc_blobs=doc_blobs,
            report_blobs=report_blobs,
            signer_secret=secret,
            signer_kid=kid,
            public_key_armor=armor,
            rsyncable_threshold=rsyncable_threshold,
        )


def gts_from_graph(
    graph: Graph,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS ``dist`` snapshot from an rdflib graph/dataset (RDF 1.1)."""
    fmt = ox.RdfFormat.N_QUADS if isinstance(graph, Dataset) else ox.RdfFormat.N_TRIPLES
    return ox.gts_from_quads(
        _nquads_bytes(graph), format=fmt, profile=profile, transform=transform
    )


def gts_from_rdf12(
    path: Path,
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce a GTS snapshot from an RDF 1.2 artifact (statement layer; gmeow_rdf)."""
    return ox.gts_from_rdf12_bytes(
        path.read_bytes(),
        format=ox.RdfFormat.TURTLE,
        profile=profile,
        transform=transform,
    )


def gts_from_maximal(
    base: Graph,
    derived: Sequence[DerivedTriple],
    *,
    profile: str = "dist",
    transform: list[str] | None = None,
) -> bytes:
    """Produce the transpiler's MAXIMAL(G) snapshot (#34).

    ``base`` carries the canonical A-Box (assumed bnode-free — the transform
    driver skolemizes); every :class:`~gmeow_tools.saturate.DerivedTriple` lands
    as an asserted base triple plus its provenance reifier/annotations.
    """
    builder = _Builder()
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
    report_blobs: list[tuple[bytes, str, str]] | None = None,
    signer: Signer | None = None,
    public_key_armor: str | None = None,
    rsyncable_threshold: int = 65536,
) -> bytes:
    """Compile the statement-complete, byte-deterministic ``dist`` GTS snapshot.

    The narrow waist's producer: the RDF 1.1 base graph rides in the default
    graph, the RDF 1.2 statement layer in ``gmeow:graph/statements`` (its
    reifies/annot tables are global), the SSSOM alignment axioms in
    ``gmeow:graph/alignments``, and any additional explicitly named graphs
    supplied by the caller. rdflib blank-node labels are per-process UUIDs, so
    rdflib sources are canonicalized (:func:`rdflib.compare.to_canonical_graph`)
    — together with the content-sorted term table this makes the emitted snapshot
    payload a pure function of the inputs (the drift-gate requirement).

    If ``signer`` and ``public_key_armor`` are supplied, the snapshot is signed
    and the armored transport public key is embedded in the first ``meta`` frame.

    ``rsyncable_threshold`` (#513): payloads larger than this many bytes are
    compressed with ``zstd-rsyncable`` instead of ``zstd``.

    Raises:
        FileNotFoundError: if ``rdf12_path`` is given but does not exist (a missing
            statement layer is an error, not a silent RDF-1.1-only fallback).
    """
    from rdflib.compare import to_canonical_graph

    from gmeow_tools.config import GTS_GRAPH_ALIGNMENTS, GTS_GRAPH_STATEMENTS

    builder = _Builder()
    builder.add_graph(to_canonical_graph(graph), bnode_scope="base")
    if rdf12_path is not None:
        if not rdf12_path.exists():
            msg = f"RDF 1.2 statement artifact not found: {rdf12_path}"
            raise FileNotFoundError(msg)
        builder.add_rdf12(
            rdf12_path, graph_name=GTS_GRAPH_STATEMENTS, bnode_scope="stmt"
        )
    if alignment_graph is not None:
        builder.add_graph(
            to_canonical_graph(alignment_graph),
            graph_name=GTS_GRAPH_ALIGNMENTS,
            bnode_scope="align",
        )
    for named_graph, graph_name, bnode_scope in extra_named_graphs or ():
        builder.add_graph(
            to_canonical_graph(named_graph),
            graph_name=graph_name,
            bnode_scope=bnode_scope,
        )
    return builder.to_gts(
        profile="dist",
        transform=transform,
        doc_blobs=doc_blobs,
        report_blobs=report_blobs,
        signer=signer,
        public_key_armor=public_key_armor,
        rsyncable_threshold=rsyncable_threshold,
    )
