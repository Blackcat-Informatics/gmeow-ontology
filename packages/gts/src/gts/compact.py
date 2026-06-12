# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Streamable compaction (GTS-SPEC §10.1): re-author the ordering, only the ordering.

``compact_streamable`` rewrites an accretive GTS file (or multi-segment
composition) into ONE delivery-ordered segment in the streamable layout state
(§3.3): a leading streaming index in the ``stream`` vocabulary (§13.3), the
content graph, blobs most-significant-first, and a trailing offset ``index``
footer. Content signatures ride through untouched; frame signatures are
carried *detached* in compaction provenance; the ordering commitment is
re-issued — the compactor is the sole attester of the new ordering.

The rewrite is byte-deterministic for the same input and parameters (§14.1):
blob order is ascending decoded size with digest tie-break, the agent string
is a constant, and the timestamp is a parameter — never ambient time.
"""

from __future__ import annotations

import base64
from collections.abc import Mapping
from dataclasses import replace

from gts import stream
from gts.model import Graph, Quad, Suppression, Term, TermKind
from gts.reader import read, read_segments
from gts.wire import digest_str
from gts.writer import Writer

RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
XSD_INTEGER = "http://www.w3.org/2001/XMLSchema#integer"
XSD_DATETIME = "http://www.w3.org/2001/XMLSchema#dateTime"


class CompactRefusedError(ValueError):
    """The input is not safely compactable (§10.1/§14.1 refuse-don't-trust)."""


def _refusal_gate(data: bytes, *, seal_original: bool) -> tuple[Graph, str]:
    """Verify the input cleanly and return its union fold + single profile."""
    segments, torn, fatal = read_segments(data)
    if fatal is not None:
        msg = f"input is not a clean GTS file: {fatal.code}: {fatal.detail}"
        raise CompactRefusedError(msg)
    if torn is not None:
        msg = f"input has a torn append at byte {torn}"
        raise CompactRefusedError(msg)
    for idx, seg in enumerate(segments):
        if seg.diagnostics:
            first = seg.diagnostics[0]
            msg = f"segment {idx} does not verify cleanly: {first.code}: {first.detail}"
            raise CompactRefusedError(msg)
    profiles = {p for seg in segments for p in seg.segment_profiles}
    if len(profiles) > 1:
        msg = f"mixed segment profiles {sorted(profiles)} are not compactable (v1)"
        raise CompactRefusedError(msg)
    profile = next(iter(profiles), "generic")
    if profile == "evidence" and not seal_original:
        msg = (
            "an 'evidence' artifact's signed chain IS the artifact; refusing "
            "to re-order it without --seal-original (§10.1)"
        )
        raise CompactRefusedError(msg)
    g = read(data)
    for sup in g.suppressions:
        for target in sup.targets:
            if target.get("kind") == "frame":
                msg = (
                    "input carries a frame-addressed suppression; the rewrite "
                    "assigns new frame ids, so the target would silently "
                    "dangle (§10.1)"
                )
                raise CompactRefusedError(msg)
    return g, profile


class _GraphBuilder:
    """Accumulates the streaming-index terms and quads with stable ids."""

    def __init__(self) -> None:
        self.terms: list[Term] = []
        self.quads: list[Quad] = []

    def add(self, term: Term) -> int:
        self.terms.append(term)
        return len(self.terms) - 1

    def literal(self, value: str, *, datatype: int | None = None) -> int:
        return self.add(Term(TermKind.LITERAL, value, datatype=datatype))

    def quad(self, s: int, p: int, o: int) -> None:
        self.quads.append((s, p, o, None))


def _streaming_index(
    g: Graph,
    blob_order: list[str],
    *,
    timestamp: str,
    sealed_digest: str | None,
    sealed_size: int | None,
) -> _GraphBuilder:
    """Build the leading streaming index + compaction provenance (§3.3, §13.3)."""
    b = _GraphBuilder()
    # Fixed vocabulary block — constant ids across engines for determinism.
    t_type = b.add(Term(TermKind.IRI, RDF_TYPE))
    t_int = b.add(Term(TermKind.IRI, XSD_INTEGER))
    t_dt = b.add(Term(TermKind.IRI, XSD_DATETIME))
    t_manifestation = b.add(Term(TermKind.IRI, stream.MANIFESTATION))
    t_digest = b.add(Term(TermKind.IRI, stream.DIGEST))
    t_mt = b.add(Term(TermKind.IRI, stream.MEDIA_TYPE))
    t_size = b.add(Term(TermKind.IRI, stream.SIZE))
    t_role = b.add(Term(TermKind.IRI, stream.ROLE))
    t_order = b.add(Term(TermKind.IRI, stream.ORDER))
    t_compaction = b.add(Term(TermKind.IRI, stream.COMPACTION))
    t_agent = b.add(Term(TermKind.IRI, stream.AGENT))
    t_timestamp = b.add(Term(TermKind.IRI, stream.TIMESTAMP))
    t_source_head = b.add(Term(TermKind.IRI, stream.SOURCE_HEAD))
    t_sealed_source = b.add(Term(TermKind.IRI, stream.SEALED_SOURCE))
    t_detached_sig = b.add(Term(TermKind.IRI, stream.DETACHED_SIGNATURE))
    t_source_frame = b.add(Term(TermKind.IRI, stream.SOURCE_FRAME))
    t_cose = b.add(Term(TermKind.IRI, stream.COSE))

    # One Manifestation per promised blob, in delivery order.
    for order, digest in enumerate(blob_order):
        m = b.add(Term(TermKind.BNODE, f"m{order}"))
        sealed = digest == sealed_digest
        size = sealed_size if sealed else len(g.blobs[digest])
        mt = "application/gts" if sealed else g.blob_meta.get(digest, {}).get("mt")
        b.quad(m, t_type, t_manifestation)
        b.quad(m, t_digest, b.literal(digest))
        if isinstance(mt, str):
            b.quad(m, t_mt, b.literal(mt))
        if size is not None:
            b.quad(m, t_size, b.literal(str(size), datatype=t_int))
        b.quad(m, t_role, b.literal("source" if sealed else "primary"))
        b.quad(m, t_order, b.literal(str(order), datatype=t_int))

    # The Compaction provenance node (§10.1).
    c = b.add(Term(TermKind.BNODE, "c"))
    b.quad(c, t_type, t_compaction)
    b.quad(c, t_agent, b.literal(stream.COMPACT_AGENT))
    b.quad(c, t_timestamp, b.literal(timestamp, datatype=t_dt))
    for head in g.segment_heads:
        b.quad(c, t_source_head, b.literal("blake3:" + head.hex()))
    if sealed_digest is not None:
        b.quad(c, t_sealed_source, b.literal(sealed_digest))

    # Detached frame signatures (§10.1): checkable claims about the original log.
    for j, sig in enumerate(s for s in g.signatures if s.cose is not None):
        node = b.add(Term(TermKind.BNODE, f"s{j}"))
        cose_b64 = base64.urlsafe_b64encode(sig.cose or b"").rstrip(b"=").decode()
        b.quad(node, t_type, t_detached_sig)
        b.quad(node, t_source_frame, b.literal("blake3:" + sig.frame_id.hex()))
        b.quad(node, t_cose, b.literal(cose_b64))
    return b


def _shift_term(t: Term, base: int) -> Term:
    """Shift a term's id references into the output id space."""
    if t.datatype is None and t.reifier is None:
        return t
    return replace(
        t,
        datatype=t.datatype + base if t.datatype is not None else None,
        reifier=t.reifier + base if t.reifier is not None else None,
    )


def _shifted_suppressions(g: Graph, base: int) -> list[Suppression]:
    """Carry suppressions forward, one output suppression per input (§10.1).

    Re-authoring of the ordering only: each original suppression keeps its own
    frame with its ``reason``/``by`` metadata intact — blob targets verbatim
    (content-addressing is layout-independent), id-addressed targets and
    ``by`` shifted into the output id space.
    """
    out: list[Suppression] = []
    for sup in g.suppressions:
        targets: list[Mapping[str, object]] = []
        for target in sup.targets:
            kind = target.get("kind")
            t = dict(target)
            tid = t.get("id")
            q = t.get("q")
            if kind in ("term", "reifier") and isinstance(tid, int):
                t["id"] = tid + base
            elif kind == "quad" and isinstance(q, list):
                t["q"] = [x + base if isinstance(x, int) else x for x in q]
            targets.append(t)
        out.append(
            Suppression(
                targets=targets,
                reason=sup.reason,
                by=sup.by + base if sup.by is not None else None,
            )
        )
    return out


def compact_streamable(
    data: bytes,
    *,
    timestamp: str,
    seal_original: bool = False,
) -> bytes:
    """Rewrite a GTS file into one streamable segment (§10.1).

    Args:
        data: the source GTS bytes; must verify cleanly (refuse-don't-trust).
        timestamp: the rewrite time recorded as ``stream:timestamp`` — an
            explicit parameter so the output is byte-reproducible.
        seal_original: carry the verbatim source bytes as a nested GTS blob
            (§12.1), role ``"source"`` — REQUIRED for ``evidence`` input.

    Returns:
        The compacted single-segment streamable GTS bytes.

    Raises:
        CompactRefusedError: on any §10.1/§14.1 refusal condition.
    """
    g, profile = _refusal_gate(data, seal_original=seal_original)

    # Delivery plan: most-significant-first — ascending decoded size, digest
    # tie-break; the sealed original (least significant) always travels last.
    blob_order = sorted(g.blobs, key=lambda d: (len(g.blobs[d]), d))
    sealed_digest: str | None = None
    if seal_original:
        sealed_digest = digest_str(data)
        blob_order = [d for d in blob_order if d != sealed_digest]
        blob_order.append(sealed_digest)

    index = _streaming_index(
        g,
        blob_order,
        timestamp=timestamp,
        sealed_digest=sealed_digest,
        sealed_size=len(data) if sealed_digest is not None else None,
    )
    base = len(index.terms)

    w = Writer(profile=profile, layout="streamable")
    # Leading streaming index: the catalog presages everything below it.
    w.add_terms(index.terms)
    w.add_quads(index.quads)
    # Content graph, re-emitted from the folded union (ids shifted by `base`).
    if g.terms:
        w.add_terms([_shift_term(t, base) for t in g.terms])
    if g.quads:
        w.add_quads(
            [
                (s + base, p + base, o + base, gr + base if gr is not None else None)
                for s, p, o, gr in g.quads
            ]
        )
    if g.reifiers:
        w.add_reifies(
            {
                r + base: (s + base, p + base, o + base)
                for r, (s, p, o) in g.reifiers.items()
            }
        )
    if g.annotations:
        w.add_annot([(r + base, p + base, v + base) for r, p, v in g.annotations])
    for sup in _shifted_suppressions(g, base):
        w.add_suppress(sup.targets, reason=sup.reason, by=sup.by)
    # Blobs in delivery order; declared metadata rides along.
    for digest in blob_order:
        if digest == sealed_digest:
            w.add_blob(data, mt="application/gts", rep="source")
            continue
        meta = g.blob_meta.get(digest, {})
        mt = meta.get("mt")
        rep = meta.get("rep")
        w.add_blob(
            g.blobs[digest],
            mt=mt if isinstance(mt, str) else None,
            rep=rep if isinstance(rep, str) else None,
        )
    # The re-issued ordering commitment: the compactor is its sole attester.
    w.add_index()
    return w.to_bytes()
