# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""A GTS writer: build frames, maintain the id/prev chain, emit a CBOR Sequence.

This is the encoder counterpart to :mod:`gmeow_tools.gts.reader`. It drives the
conformance vectors and is the seed of the future ``RDF 1.2 → GTS`` producer.
"""

from __future__ import annotations

import cbor2

from gts.codec import DEFAULT_CATALOG, Codec, encode_chain
from gts.crypto import Signer, encrypt0, sign_id
from gts.model import Quad, Term, Triple
from gts.wire import (
    MAGIC,
    SELF_DESCRIBE_TAG,
    VERSION,
    canonical,
    content_id,
    header_id,
)


def term_to_wire(t: Term) -> dict[str, object]:
    """Serialise a :class:`Term` to its wire map (dropping absent fields)."""
    out: dict[str, object] = {"k": int(t.kind)}
    if t.value is not None:
        out["v"] = t.value
    if t.datatype is not None:
        out["dt"] = t.datatype
    if t.lang is not None:
        out["l"] = t.lang
    if t.reifier is not None:
        out["rf"] = t.reifier
    return out


class Writer:
    """Accumulates a GTS log as a CBOR Sequence.

    Args:
        profile: The header ``"prof"`` value (§13).
        catalog: The transform catalog (id → :class:`Codec`).
        meta: Optional header metadata.
        magic_tag: Prefix the Header with the CBOR self-describe tag (§3).
    """

    def __init__(
        self,
        profile: str = "generic",
        catalog: dict[int, Codec] | None = None,
        meta: dict[str, object] | None = None,
        *,
        magic_tag: bool = True,
        signer: Signer | None = None,
    ) -> None:
        """Initialise the writer and emit the Header (the chain genesis).

        If ``signer`` is given, every appended frame is COSE_Sign1-signed over its
        ``id`` (§9.2) — the basis of the ``evidence`` profile's chain of custody.
        """
        self._signer = signer
        self.catalog = catalog or dict(DEFAULT_CATALOG)
        self._name_to_id = {c.name: i for i, c in self.catalog.items()}
        header: dict[str, object] = {
            "gts": MAGIC,
            "v": VERSION,
            "prof": profile,
            "cat": {i: {"name": c.name, "cls": c.cls} for i, c in self.catalog.items()},
        }
        if meta is not None:
            header["meta"] = meta
        header["id"] = header_id(header)
        self._prev: bytes = header["id"]  # type: ignore[assignment]
        first = cbor2.CBORTag(SELF_DESCRIBE_TAG, header) if magic_tag else header
        self._buf = bytearray(canonical(first))

    @property
    def head(self) -> bytes:
        """The id the next appended frame must reference as ``"prev"``."""
        return self._prev

    def _chain_ids(self, chain: list[str] | None) -> list[int]:
        """Resolve codec names to file-local catalog ids."""
        return [self._name_to_id[name] for name in (chain or [])]

    def add_frame(
        self,
        frame_type: str,
        *,
        payload: object | None = None,
        raw: bytes | None = None,
        transform: list[str] | None = None,
        pub: object | None = None,
        to: list[dict[str, object]] | None = None,
        sig: bytes | None = None,
        encrypt: tuple[str, bytes] | None = None,
    ) -> bytes:
        """Append one frame and return its ``"id"``.

        ``payload`` (structured CBOR) and ``raw`` (blob bytes) are mutually exclusive
        payload sources. ``transform`` compresses/encodes the payload; ``encrypt``
        ``(kid, key)`` then seals it as a ``COSE_Encrypt0`` (the outermost transform)
        and records the recipient in ``"to"`` (§9.3). ``"d"`` becomes a byte string.

        Raises:
            ValueError: if both ``payload`` and ``raw`` are given, or if ``transform``/
                ``encrypt`` is requested with neither source.
        """
        if payload is not None and raw is not None:
            msg = "payload and raw are mutually exclusive"
            raise ValueError(msg)
        if (transform or encrypt) and payload is None and raw is None:
            msg = "transform/encrypt requires a payload or raw source"
            raise ValueError(msg)
        frame: dict[str, object] = {"t": frame_type}
        if transform or encrypt is not None:
            data = raw if raw is not None else canonical(payload)
            x_ids: list[int] = []
            if transform:
                data = encode_chain(transform, data)
                x_ids += self._chain_ids(transform)
            if encrypt is not None:
                encrypt_id = self._name_to_id.get("cose-encrypt0")
                if encrypt_id is None:
                    msg = "encrypt requires a catalog entry for 'cose-encrypt0'"
                    raise ValueError(msg)
                kid, key = encrypt
                data = encrypt0(data, kid, key)
                x_ids.append(encrypt_id)
            frame["x"] = x_ids
            frame["d"] = data
        elif raw is not None:
            frame["d"] = raw
        elif payload is not None:
            frame["d"] = payload
        if pub is not None:
            frame["pub"] = pub
        recipients = list(to) if to is not None else []
        if encrypt is not None:
            recipients.append({"kid": encrypt[0]})
        if recipients:
            frame["to"] = recipients
        frame["prev"] = self._prev
        fid = content_id(frame)
        frame["id"] = fid
        if sig is None and self._signer is not None:
            sig = sign_id(fid, self._signer)
        if sig is not None:
            frame["sig"] = sig
        self._buf += canonical(frame)
        self._prev = fid
        return self._prev

    # -- convenience builders -------------------------------------------------

    def add_terms(
        self, terms: list[Term], *, transform: list[str] | None = None
    ) -> bytes:
        """Append a ``terms`` frame."""
        return self.add_frame(
            "terms", payload=[term_to_wire(t) for t in terms], transform=transform
        )

    def add_quads(
        self, quads: list[Quad], *, transform: list[str] | None = None
    ) -> bytes:
        """Append a ``quads`` frame (drops a ``None`` graph slot)."""
        rows = [
            [q[0], q[1], q[2], *([q[3]] if q[3] is not None else [])] for q in quads
        ]
        return self.add_frame("quads", payload=rows, transform=transform)

    def add_reifies(self, bindings: dict[int, Triple]) -> bytes:
        """Append a ``reifies`` frame binding reifier-ids to triples."""
        payload = {rid: list(spo) for rid, spo in bindings.items()}
        return self.add_frame("reifies", payload=payload)

    def add_annot(self, rows: list[Triple]) -> bytes:
        """Append an ``annot`` frame (reifier, predicate, value rows)."""
        return self.add_frame("annot", payload=[list(r) for r in rows])

    def add_blob(
        self,
        data: bytes,
        *,
        mt: str | None = None,
        rep: str | None = None,
        transform: list[str] | None = None,
    ) -> bytes:
        """Append an inline ``blob`` frame; metadata goes in ``pub`` (§12)."""
        pub: dict[str, object] = {}
        if mt is not None:
            pub["mt"] = mt
        if rep is not None:
            pub["rep"] = rep
        return self.add_frame("blob", raw=data, transform=transform, pub=pub or None)

    def add_meta(self, meta: dict[str, object]) -> bytes:
        """Append a ``meta`` frame."""
        return self.add_frame("meta", payload=meta)

    def add_suppress(
        self, targets: list[dict[str, object]], *, reason: str | None = None
    ) -> bytes:
        """Append a ``suppress`` frame (§11)."""
        payload: dict[str, object] = {"targets": targets}
        if reason is not None:
            payload["reason"] = reason
        return self.add_frame("suppress", payload=payload)

    def to_bytes(self) -> bytes:
        """Return the complete GTS file."""
        return bytes(self._buf)
