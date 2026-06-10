# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The GTS reader: parse a CBOR Sequence, verify the id/prev chain, fold the log.

Implements the Baseline Reader contract (§2.1): chain verification (§9.1), the
four-table fold (§7.5), opaque/damaged degradation (§7.6), torn-append detection
(§3), and the canonical diagnostics (§2.3). When a ``keys`` provider is supplied,
``sig`` frames are verified (§9.2) and ``encrypt``-class frames are decrypted (§9.3);
without it, an ``encrypt`` codec degrades to a ``missing-key`` opaque node.
"""

from __future__ import annotations

from collections.abc import Mapping

import cbor2

from gmeow_tools.gts.codec import Codec, CodecUnavailableError, decode_chain
from gmeow_tools.gts.crypto import KeyProvider, decrypt0, verify_sig
from gmeow_tools.gts.model import (
    Diagnostic,
    Graph,
    OpaqueNode,
    Signature,
    Suppression,
    Term,
    TermKind,
    Triple,
)
from gmeow_tools.gts.wire import (
    content_id,
    digest_str,
    header_id,
    iter_items,
    unwrap_header,
)

_IRI = TermKind.IRI
_KINDS = {int(k) for k in TermKind}


def _as_int(x: object) -> int | None:
    """Coerce a value to a non-negative ``int`` (rejecting ``bool``), else ``None``."""
    return x if isinstance(x, int) and not isinstance(x, bool) and x >= 0 else None


def term_from_wire(d: Mapping[str, object]) -> Term:
    """Parse a wire term map into a :class:`Term` (unknown kind defaults to IRI)."""
    raw_kind = d.get("k")
    kind = (
        TermKind(raw_kind)
        if isinstance(raw_kind, int) and raw_kind in _KINDS
        else TermKind.IRI
    )
    value = d.get("v")
    datatype = d.get("dt")
    lang = d.get("l")
    reifier = d.get("rf")
    return Term(
        kind=kind,
        value=value if isinstance(value, str) else None,
        datatype=datatype if isinstance(datatype, int) else None,
        lang=lang if isinstance(lang, str) else None,
        reifier=reifier if isinstance(reifier, int) else None,
    )


def _catalog(header: Mapping[str, object]) -> dict[int, Codec]:
    """Build the id → :class:`Codec` map from the header ``"cat"``."""
    raw = header.get("cat", {})
    out: dict[int, Codec] = {}
    if isinstance(raw, Mapping):
        for cid, entry in raw.items():
            if isinstance(cid, int) and isinstance(entry, Mapping):
                name = str(entry.get("name", ""))
                cls = str(entry.get("cls", "encode"))
                out[cid] = Codec(name, cls)  # type: ignore[arg-type]
    return out


class _Folder:
    """Mutable fold state; one per :func:`read` call (and per nested snapshot)."""

    def __init__(
        self, graph: Graph, catalog: dict[int, Codec], keys: KeyProvider | None = None
    ) -> None:
        self.g = graph
        self.catalog = catalog
        self.keys = keys
        # A key-bound decryptor for encrypt-class codecs; None ⇒ encrypt → missing-key.
        self._decryptor = (lambda b: decrypt0(b, keys)) if keys is not None else None

    def _resolve_codecs(self, ids: list[object]) -> list[Codec]:
        chain: list[Codec] = []
        for cid in ids:
            codec = self.catalog.get(cid) if isinstance(cid, int) else None
            if codec is None:
                raise CodecUnavailableError(
                    "unknown-codec", f"codec id {cid!r} not in catalog"
                )
            chain.append(codec)
        return chain

    def payload(self, frame: Mapping[str, object], *, blob: bool) -> object:
        """Resolve a frame's logical payload (§6.1); raise on missing capability."""
        d = frame.get("d")
        x = frame.get("x")
        if isinstance(x, list) and x:
            if not isinstance(d, bytes):
                msg = "transformed frame 'd' must be a byte string"
                raise ValueError(msg)
            decoded = decode_chain(self._resolve_codecs(x), d, decrypt=self._decryptor)
            return decoded if blob else cbor2.loads(decoded)
        return d

    def fold_frame(self, frame: Mapping[str, object], index: int) -> None:
        """Fold one already-verified frame into the graph.

        Total: a missing capability degrades to an opaque node, and any other failure
        (corrupt compression/CBOR payload, or a handler error on malformed data) is
        caught and degraded to a ``damaged`` opaque node — the reader never raises.
        """
        ftype = str(frame.get("t", ""))
        try:
            payload = self.payload(frame, blob=ftype == "blob")
        except CodecUnavailableError as exc:
            self._opaque(frame, ftype, exc.reason)
            self.g.diagnostics.append(
                Diagnostic(_REASON_DIAG[exc.reason], str(exc), index)
            )
            return
        except Exception as exc:  # corrupt compression / CBOR payload
            self._opaque(frame, ftype, "damaged")
            self.g.diagnostics.append(
                Diagnostic("DamagedFrame", f"payload decode failed: {exc}", index)
            )
            return
        handler = _HANDLERS.get(ftype)
        if handler is None:
            return  # index / unknown structural frames are ignored by the baseline
        try:
            handler(self, payload, frame, index)
        except Exception as exc:  # malformed payload structure
            self._opaque(frame, ftype, "damaged")
            self.g.diagnostics.append(
                Diagnostic("DamagedFrame", f"fold failed: {exc}", index)
            )

    # -- per-type handlers ----------------------------------------------------

    def _h_terms(self, payload: object, _f: Mapping[str, object], index: int) -> None:
        if not isinstance(payload, list):
            return
        for raw in payload:
            if not isinstance(raw, Mapping):
                continue
            term = term_from_wire(raw)
            tid = len(self.g.terms)
            # Sanitise refs: dt/rf MUST name an already-introduced term (§7.5). A
            # forward/out-of-bounds ref is diagnosed and dropped, so resolution and
            # serialisation can never IndexError.
            dt = (
                term.datatype
                if term.datatype is not None and term.datatype < tid
                else None
            )
            rf = (
                term.reifier
                if term.reifier is not None and term.reifier < tid
                else None
            )
            if (term.datatype is not None and dt is None) or (
                term.reifier is not None and rf is None
            ):
                self.g.diagnostics.append(
                    Diagnostic(
                        "ForwardReference", f"term {tid} has an out-of-range ref", index
                    )
                )
            self.g.terms.append(
                Term(
                    kind=term.kind,
                    value=term.value,
                    datatype=dt,
                    lang=term.lang,
                    reifier=rf,
                )
            )

    def _h_quads(self, payload: object, _f: Mapping[str, object], index: int) -> None:
        if not isinstance(payload, list):
            return
        for row in payload:
            if not isinstance(row, list) or len(row) < 3:
                continue
            s, p, o = _as_int(row[0]), _as_int(row[1]), _as_int(row[2])
            g = _as_int(row[3]) if len(row) >= 4 else None
            if s is None or p is None or o is None or (len(row) >= 4 and g is None):
                self.g.diagnostics.append(
                    Diagnostic("DamagedFrame", "quad has non-integer term ids", index)
                )
                continue
            if not self._check_positions(s, p, o, g, index):
                continue
            self.g.quads.append((s, p, o, g))

    def _h_reifies(self, payload: object, _f: Mapping[str, object], index: int) -> None:
        if not isinstance(payload, Mapping):
            return
        for rid, spo in payload.items():
            if not isinstance(rid, int) or not isinstance(spo, list) or len(spo) != 3:
                continue
            s, p, o = _as_int(spo[0]), _as_int(spo[1]), _as_int(spo[2])
            if s is None or p is None or o is None or not self._in_bounds(rid, s, p, o):
                self.g.diagnostics.append(
                    Diagnostic(
                        "DamagedFrame", f"reifier {rid} has bad/out-of-range ids", index
                    )
                )
                continue
            triple: Triple = (s, p, o)
            existing = self.g.reifiers.get(rid)
            if existing is not None and existing != triple:
                self.g.diagnostics.append(
                    Diagnostic("ConflictingReifier", f"reifier {rid} rebound", index)
                )
                continue  # keep the first binding
            self.g.reifiers[rid] = triple

    def _h_annot(self, payload: object, _f: Mapping[str, object], index: int) -> None:
        if not isinstance(payload, list):
            return
        for row in payload:
            if not isinstance(row, list) or len(row) != 3:
                continue
            r, p, v = _as_int(row[0]), _as_int(row[1]), _as_int(row[2])
            if r is None or p is None or v is None or not self._in_bounds(r, p, v):
                self.g.diagnostics.append(
                    Diagnostic(
                        "DamagedFrame", "annot row has bad/out-of-range ids", index
                    )
                )
                continue
            if self._kind(p) is not _IRI:
                self.g.diagnostics.append(
                    Diagnostic(
                        "PositionConstraint", f"annot predicate {p} not an IRI", index
                    )
                )
                continue
            self.g.annotations.append((r, p, v))

    def _h_blob(
        self, payload: object, frame: Mapping[str, object], _index: int
    ) -> None:
        if isinstance(payload, bytes):
            self.g.blobs[digest_str(payload)] = payload
        # else: external blob — bytes live elsewhere, referenced by "pub".digest (§12).

    def _h_meta(self, payload: object, _f: Mapping[str, object], _index: int) -> None:
        if isinstance(payload, Mapping):
            for k, v in payload.items():
                self.g.meta[str(k)] = v

    def _h_suppress(
        self, payload: object, _f: Mapping[str, object], _index: int
    ) -> None:
        if not isinstance(payload, Mapping):
            return
        targets = payload.get("targets")
        if isinstance(targets, list):
            self.g.suppressions.append(
                Suppression(
                    targets=[t for t in targets if isinstance(t, Mapping)],
                    reason=payload.get("reason")
                    if isinstance(payload.get("reason"), str)
                    else None,
                    by=payload.get("by")
                    if isinstance(payload.get("by"), int)
                    else None,
                )
            )

    def _h_snapshot(
        self, payload: object, _f: Mapping[str, object], index: int
    ) -> None:
        """Fold a self-contained snapshot (§10).

        Shifts the snapshot's local term ids into the outer id space and re-dispatches
        through the normal handlers, so a snapshot gets the **same** semantic checks
        (forward-reference, position, conflicting-reifier, annotation-predicate IRI) as
        the equivalent streamed frames — no silent acceptance of invalid content.
        """
        if not isinstance(payload, Mapping):
            return
        base = len(self.g.terms)

        def sh(value: object) -> object:
            # Shift a valid local id into the outer space; pass non-ints through so the
            # downstream handler's own checks reject them with diagnostics.
            iv = _as_int(value)
            return iv + base if iv is not None else value

        snap_terms = payload.get("terms")
        if isinstance(snap_terms, list):
            shifted_terms: list[object] = []
            for raw in snap_terms:
                if isinstance(raw, Mapping):
                    term = dict(raw)
                    if "dt" in term:
                        term["dt"] = sh(term["dt"])
                    if "rf" in term:
                        term["rf"] = sh(term["rf"])
                    shifted_terms.append(term)
                else:
                    shifted_terms.append(raw)
            self._h_terms(shifted_terms, _f, index)

        quads = payload.get("quads")
        if isinstance(quads, list):
            self._h_quads(
                [[sh(x) for x in r] if isinstance(r, list) else r for r in quads],
                _f,
                index,
            )

        reifies = payload.get("reifies")
        if isinstance(reifies, Mapping):
            shifted_reif: dict[object, object] = {
                sh(rid): ([sh(x) for x in spo] if isinstance(spo, list) else spo)
                for rid, spo in reifies.items()
            }
            self._h_reifies(shifted_reif, _f, index)

        annot = payload.get("annot")
        if isinstance(annot, list):
            self._h_annot(
                [[sh(x) for x in r] if isinstance(r, list) else r for r in annot],
                _f,
                index,
            )

        blobs = payload.get("blobs")
        if isinstance(blobs, Mapping):
            for b in blobs.values():
                if isinstance(b, bytes):
                    self.g.blobs[digest_str(b)] = b
        meta = payload.get("meta")
        if isinstance(meta, Mapping):
            for k, v in meta.items():
                self.g.meta[str(k)] = v

    def _h_opaque(self, payload: object, _f: Mapping[str, object], _index: int) -> None:
        if isinstance(payload, Mapping):
            self.g.opaque.append(
                OpaqueNode(
                    id=payload.get("id", b"")
                    if isinstance(payload.get("id"), bytes)
                    else b"",
                    frame_type=str(payload.get("type", "opaque")),
                    reason=str(payload.get("reason", "unknown-codec")),
                    sigstat=str(payload.get("sigstat", "none")),
                    pub=payload.get("pub"),
                )
            )

    # -- helpers --------------------------------------------------------------

    def _kind(self, tid: int) -> TermKind | None:
        return self.g.terms[tid].kind if 0 <= tid < len(self.g.terms) else None

    def _in_bounds(self, *ids: int) -> bool:
        """True iff every id names an already-introduced term (prevents IndexError)."""
        n = len(self.g.terms)
        return all(0 <= i < n for i in ids)

    def _check_positions(
        self, s: int, p: int, o: int, g: int | None, index: int
    ) -> bool:
        """Bounds-check, then enforce §7.4 positions; diagnose + reject on violation."""
        refs = (s, p, o) if g is None else (s, p, o, g)
        if not self._in_bounds(*refs):
            self.g.diagnostics.append(
                Diagnostic(
                    "PositionConstraint",
                    f"quad ({s},{p},{o},{g}) has out-of-range term ids",
                    index,
                )
            )
            return False
        ok = True
        if self._kind(p) is not _IRI:
            ok = False
        if self._kind(s) in (TermKind.LITERAL,):
            ok = False
        if g is not None and self._kind(g) in (TermKind.LITERAL, TermKind.TRIPLE):
            ok = False
        if not ok:
            self.g.diagnostics.append(
                Diagnostic(
                    "PositionConstraint",
                    f"quad ({s},{p},{o},{g}) violates positions",
                    index,
                )
            )
        return ok

    def _opaque(self, frame: Mapping[str, object], ftype: str, reason: str) -> None:
        fid = frame.get("id")
        to = frame.get("to")
        self.g.opaque.append(
            OpaqueNode(
                id=fid if isinstance(fid, bytes) else b"",
                frame_type=ftype,
                reason=reason,
                sigstat="unverified" if "sig" in frame else "none",
                pub=frame.get("pub"),
                recipients=[t for t in to if isinstance(t, Mapping)]
                if isinstance(to, list)
                else None,
            )
        )


_HANDLERS = {
    "terms": _Folder._h_terms,
    "quads": _Folder._h_quads,
    "reifies": _Folder._h_reifies,
    "annot": _Folder._h_annot,
    "blob": _Folder._h_blob,
    "meta": _Folder._h_meta,
    "suppress": _Folder._h_suppress,
    "snapshot": _Folder._h_snapshot,
    "opaque": _Folder._h_opaque,
}

_REASON_DIAG = {"unknown-codec": "UnknownCodec", "missing-key": "MissingKey"}


def read(
    data: bytes,
    *,
    keys: KeyProvider | None = None,
    expected_head: bytes | None = None,
) -> Graph:
    """Read and fold a GTS file into a :class:`Graph`.

    Verifies the header genesis hash, every frame's self-``id``, and the ``prev``
    chain, recording diagnostics; damaged frames and undecodable frames fold to
    opaque nodes (§7.6) rather than aborting the read.

    Args:
        data: the GTS file bytes.
        keys: optional :class:`~gmeow_tools.gts.crypto.KeyProvider` — when given,
            ``sig`` frames are verified (§9.2) and recorded in ``Graph.signatures``.
        expected_head: optional head commitment — if the observed head id differs,
            a ``TruncatedLog`` diagnostic is raised (§9, §17).
    """
    g = Graph()
    items, torn = iter_items(data)
    if not items:
        g.diagnostics.append(Diagnostic("EmptyFile", "no CBOR items", None))
        return g

    _, raw_header = items[0]
    try:
        header = unwrap_header(raw_header)
    except ValueError as exc:
        g.diagnostics.append(Diagnostic("DamagedFrame", f"invalid header: {exc}", 0))
        return g
    stored_hid = header.get("id")
    if blake3_256_header(header) != stored_hid:
        g.diagnostics.append(Diagnostic("DamagedFrame", "header self-hash mismatch", 0))
    folder = _Folder(g, _catalog(header), keys)
    expected_prev = stored_hid if isinstance(stored_hid, bytes) else b""

    for index, (_, raw) in enumerate(items[1:], start=1):
        if not isinstance(raw, Mapping):
            g.diagnostics.append(
                Diagnostic("DamagedFrame", "frame is not a map", index)
            )
            continue
        frame: Mapping[str, object] = raw
        stored_id = frame.get("id")
        computed = content_id(frame)
        if computed != stored_id:
            g.diagnostics.append(
                Diagnostic("DamagedFrame", "frame self-hash mismatch", index)
            )
            folder._opaque(frame, str(frame.get("t", "")), "damaged")
            expected_prev = stored_id if isinstance(stored_id, bytes) else computed
            continue
        if frame.get("prev") != expected_prev:
            g.diagnostics.append(
                Diagnostic("BrokenChain", "prev does not match", index)
            )
        expected_prev = stored_id if isinstance(stored_id, bytes) else computed
        sig = frame.get("sig")
        if isinstance(sig, bytes):
            if keys is not None:
                status, kid = verify_sig(sig, computed, keys)
            else:
                status, kid = "unverified", None
            g.signatures.append(Signature(computed, kid, status))
        folder.fold_frame(frame, index)

    if expected_head is not None and expected_prev != expected_head:
        g.diagnostics.append(
            Diagnostic(
                "TruncatedLog", "observed head does not match expected head", None
            )
        )

    if torn is not None:
        g.diagnostics.append(
            Diagnostic("TornAppendError", f"torn at offset {torn}", None)
        )
    return g


def blake3_256_header(header: Mapping[str, object]) -> bytes:
    """Recompute the Header genesis id for verification (§5)."""
    return header_id(header)
