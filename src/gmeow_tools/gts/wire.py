# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Wire primitives: deterministic CBOR, BLAKE3 content-ids, and the id/prev rule.

A frame's ``"id"`` is ``BLAKE3-256`` of the deterministic CBOR (RFC 8949 §4.2) of its
content — every key except ``"id"`` and ``"sig"`` (§6, §9.1). The Header is hashed the
same way, excluding only ``"id"`` (§5). ``"prev"`` names the previous item's ``"id"``;
the first frame's ``"prev"`` is the Header's ``"id"``.
"""

from __future__ import annotations

from collections.abc import Mapping

import cbor2
from blake3 import blake3

# CBOR self-describe tag (RFC 8949 §3.4.6); MAY prefix the Header item (§3).
SELF_DESCRIBE_TAG = 55799

MAGIC = "GTS1"
VERSION = 1

# Frame map keys excluded from the self-hash preimage.
_ID_EXCLUDED = ("id", "sig")


def canonical(obj: object) -> bytes:
    """Encode an object as deterministic CBOR (RFC 8949 §4.2)."""
    return cbor2.dumps(obj, canonical=True)


def blake3_256(data: bytes) -> bytes:
    """Return the 32-byte BLAKE3-256 digest of ``data``."""
    return blake3(data).digest()


def digest_str(data: bytes) -> str:
    """Return a ``blake3:<hex>`` content digest for inline blob addressing (§12)."""
    return "blake3:" + blake3_256(data).hex()


def content_id(frame: Mapping[str, object]) -> bytes:
    """Compute a frame's ``"id"`` over its content (excluding ``"id"``/``"sig"``)."""
    content = {k: v for k, v in frame.items() if k not in _ID_EXCLUDED}
    return blake3_256(canonical(content))


def header_id(header: Mapping[str, object]) -> bytes:
    """Compute the Header's genesis ``"id"`` (excluding only ``"id"``) — §5."""
    content = {k: v for k, v in header.items() if k != "id"}
    return blake3_256(canonical(content))


def iter_items(data: bytes) -> tuple[list[tuple[int, object]], int | None]:
    """Decode a CBOR Sequence into ``(offset, item)`` pairs plus a torn marker.

    Detects a torn append (a partial trailing item) by position: at an item boundary
    the offset is either end-of-data (clean end) or the start of a complete item; a
    decode failure there is a torn append (§3). Survivors are returned regardless, so
    a reader can fold the intact prefix.

    Returns:
        ``(items, torn_offset)`` — ``torn_offset`` is ``None`` for a clean end, or the
        byte offset of the incomplete trailing item.
    """
    import io

    buf = io.BytesIO(data)
    dec = cbor2.CBORDecoder(buf)
    out: list[tuple[int, object]] = []
    length = len(data)
    torn: int | None = None
    while True:
        start = buf.tell()
        if start == length:
            break
        try:
            item = dec.decode()
        except cbor2.CBORDecodeError:  # partial (EOF) or corrupt trailing item
            torn = start
            break
        out.append((start, item))
    return out, torn


def unwrap_header(item: object) -> Mapping[str, object]:
    """Return the Header map, unwrapping the optional self-describe tag (§3).

    Most CBOR libraries auto-process tag 55799 (self-describe) and return the inner
    value directly; this also handles a library that surfaces the raw tag.
    """
    if isinstance(item, cbor2.CBORTag):
        if item.tag != SELF_DESCRIBE_TAG:
            msg = f"unexpected CBOR tag {item.tag} on the header item"
            raise ValueError(msg)
        item = item.value
    if not isinstance(item, Mapping):
        msg = "header item is not a CBOR map"
        raise ValueError(msg)
    return item
