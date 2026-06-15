# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""A GTS writer: build frames, maintain the id/prev chain, emit a CBOR Sequence.

This is the encoder counterpart to :mod:`gts.reader`. It drives the
conformance vectors and is the seed of the future ``RDF 1.2 → GTS`` producer.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence

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
    digest_str,
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


def _private_use_language_tag(tag: str) -> bool:
    """True for private-use language tags such as GMEOW's ``x-gmeow-*``."""
    tag_lower = tag.lower()
    return tag_lower.startswith("x-") or "-x-" in tag_lower


def _validate_term_language_tags(terms: list[Term], section: str) -> None:
    """Enforce §13.1 language-tag discipline at write time.

    Canonical sections (e.g. a ``dist`` or ``ai-package`` graph payload) MAY
    carry internal private-use tags. Projection/docs sections MUST carry public
    BCP 47 tags only; a private-use tag leaking into a projection section is a
    hard failure, not a warning (§13.1, vector 20).
    """
    if section == "canonical":
        return
    for t in terms:
        if t.lang is not None and _private_use_language_tag(t.lang):
            msg = (
                f"private-use language tag {t.lang!r} is not allowed in "
                f"a projection/docs section (§13.1)"
            )
            raise ValueError(msg)


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
        layout: str | None = None,
    ) -> None:
        """Initialise the writer and emit the Header (the chain genesis).

        If ``signer`` is given, every appended frame is COSE_Sign1-signed over its
        ``id`` (§9.2) — the basis of the ``evidence`` profile's chain of custody.
        ``layout`` writes the header layout-state claim (§3.3; ``"streamable"``
        is the only value this revision defines).
        """
        if layout is not None and layout != "streamable":
            # §5: "streamable" is the only layout this revision defines; a
            # typo'd claim would persist into the tamper-evident header.
            msg = f"unsupported layout claim {layout!r} (§3.3)"
            raise ValueError(msg)
        self._signer = signer
        self.catalog = catalog or dict(DEFAULT_CATALOG)
        self._name_to_id = {c.name: i for i, c in self.catalog.items()}
        header: dict[str, object] = {
            "gts": MAGIC,
            "v": VERSION,
            "prof": profile,
            "cat": {i: {"name": c.name, "cls": c.cls} for i, c in self.catalog.items()},
        }
        if layout is not None:
            header["layout"] = layout
        if meta is not None:
            header["meta"] = meta
        header["id"] = header_id(header)
        self._prev: bytes = header["id"]  # type: ignore[assignment]
        first = cbor2.CBORTag(SELF_DESCRIBE_TAG, header) if magic_tag else header
        self._buf = bytearray(canonical(first))
        # Per-frame byte offsets and types, in append order — the raw material
        # of an `index` footer (§6.2): offsets enable random access/parallel
        # verify, types the "ti" locator map.
        self._offsets: list[int] = []
        self._types: list[str] = []

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
        self._offsets.append(len(self._buf))
        self._types.append(frame_type)
        self._buf += canonical(frame)
        self._prev = fid
        return self._prev

    # -- convenience builders -------------------------------------------------

    def add_terms(
        self,
        terms: list[Term],
        *,
        transform: list[str] | None = None,
        section: str = "canonical",
    ) -> bytes:
        """Append a ``terms`` frame.

        Args:
            terms: Terms to serialize into the frame.
            transform: Optional transform chain applied to the frame.
            section: ``"canonical"`` for graph payloads that may carry internal
                private-use language tags; ``"projection"`` for docs/derived-view
                sections that MUST use public BCP 47 tags only (§13.1).
        """
        _validate_term_language_tags(terms, section)
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
        """Append an inline ``blob`` frame; metadata goes in ``pub`` (§12).

        The decoded content digest is included in ``pub.digest`` so the reader
        can address the blob lazily without decompressing the frame first.
        """
        pub: dict[str, object] = {"digest": digest_str(data)}
        if mt is not None:
            pub["mt"] = mt
        if rep is not None:
            pub["rep"] = rep
        return self.add_frame("blob", raw=data, transform=transform, pub=pub)

    def add_meta(self, meta: dict[str, object]) -> bytes:
        """Append a ``meta`` frame."""
        return self.add_frame("meta", payload=meta)

    def add_suppress(
        self,
        targets: Sequence[Mapping[str, object]],
        *,
        reason: str | None = None,
        by: int | None = None,
    ) -> bytes:
        """Append a ``suppress`` frame (§11)."""
        payload: dict[str, object] = {"targets": [dict(t) for t in targets]}
        if reason is not None:
            payload["reason"] = reason
        if by is not None:
            payload["by"] = by
        return self.add_frame("suppress", payload=payload)

    def add_index(self) -> bytes:
        """Append an ``index`` footer covering every frame appended so far (§6.2).

        ``count``/``head`` delimit the covered region (the streamable boundary,
        §3.3); ``off`` carries each covered frame's byte offset from the start
        of this writer's output; ``ti`` locates frames by type (0-based frame
        positions). A later ``add_index`` covers the earlier one too — the last
        index wins (§6.2).
        """
        ti: dict[str, list[int]] = {}
        for pos, ftype in enumerate(self._types):
            ti.setdefault(ftype, []).append(pos)
        payload: dict[str, object] = {
            "count": len(self._types),
            "head": self._prev,
        }
        if self._offsets:  # "off"/"ti" are [+ uint]-shaped — omit when empty
            payload["off"] = list(self._offsets)
            payload["ti"] = ti
        return self.add_frame("index", payload=payload)

    def to_bytes(self) -> bytes:
        """Return the complete GTS file."""
        return bytes(self._buf)
