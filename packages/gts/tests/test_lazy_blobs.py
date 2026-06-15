# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for deferred blob decompression in the GTS Python reader (#529)."""

from __future__ import annotations

import pytest
import zstandard

from gts import read
from gts.codec import Codec, decode_chain
from gts.model import Term, TermKind, _LazyBlobEntry, _LazyBlobs
from gts.wire import digest_str
from gts.writer import Writer

BLOB_MT = "text/plain"


def _term_triple() -> tuple[list[Term], list[tuple[int, int, int, None]]]:
    """A tiny term/quad payload so read() has ontology work to do."""
    terms = [
        Term(TermKind.IRI, "https://example.org/s"),
        Term(TermKind.IRI, "https://example.org/p"),
        Term(TermKind.IRI, "https://example.org/o"),
    ]
    quads = [(0, 1, 2, None)]
    return terms, quads


def _counting_decode_chain(counter: list[int]):
    """Return a monkey-patched decode_chain that increments *counter* per call."""
    original = decode_chain

    def wrapper(chain: list[Codec], data: bytes, *, decrypt=None) -> bytes:
        counter[0] += 1
        return original(chain, data, decrypt=decrypt)

    return wrapper


def test_blob_deferred_until_access() -> None:
    """A term-only read does not decompress a zstd blob."""
    terms, quads = _term_triple()
    blob = b"hello world " * 100000

    w = Writer()
    w.add_terms(terms)
    w.add_quads(quads)
    w.add_blob(blob, mt=BLOB_MT, transform=["zstd"])
    data = bytes(w.to_bytes())

    counter: list[int] = [0]
    original = _LazyBlobs.__getitem__

    def counting_getitem(self, digest: str) -> bytes:
        # Count decompressions by intercepting the lazy mapping's decode path.
        entry = self._entries[digest]
        assert isinstance(entry, _LazyBlobEntry)
        return _counting_decode_chain(counter)(entry.chain, entry.raw)

    _LazyBlobs.__getitem__ = counting_getitem  # type: ignore[assignment]
    try:
        g = read(data)
        # Force term resolution / a minimal "describe"-style lookup.
        assert len(g.terms) == 3
        assert len(g.quads) == 1
        assert counter[0] == 0
    finally:
        _LazyBlobs.__getitem__ = original


def test_blob_access_decompresses() -> None:
    """Accessing g.blobs[digest] decodes and caches the bytes."""
    terms, quads = _term_triple()
    blob = b"hello world " * 100000

    w = Writer()
    w.add_terms(terms)
    w.add_quads(quads)
    w.add_blob(blob, mt=BLOB_MT, transform=["zstd"])
    data = bytes(w.to_bytes())

    g = read(data)
    key = next(iter(g.blobs.keys()))
    assert g.blobs[key] == blob
    # Second access is cached and does not re-decode.
    assert g.blobs[key] == blob


def test_lazy_blob_identity_codec() -> None:
    """Identity-coded blobs are stored directly, not wrapped lazily."""
    blob = b"plain bytes"

    w = Writer()
    w.add_blob(blob, mt=BLOB_MT)
    data = bytes(w.to_bytes())

    g = read(data)
    key = digest_str(blob)
    assert g.blobs[key] == blob
    # The internal entry is raw bytes, not a lazy wrapper.
    assert isinstance(g.blobs._entries[key], bytes)


def test_lazy_blob_no_pub_digest_fallback() -> None:
    """A blob frame without pub.digest falls back to eager decoding."""
    blob = b"legacy blob"

    w = Writer()
    # Bypass add_blob so we can omit pub.digest.
    w.add_frame("blob", raw=blob, transform=["zstd"], pub={"mt": BLOB_MT})
    data = bytes(w.to_bytes())

    g = read(data)
    key = digest_str(blob)
    assert g.blobs[key] == blob
    assert g.blob_meta[key]["mt"] == BLOB_MT


def test_lazy_blob_unknown_codec_degrades() -> None:
    """A blob frame with an unknown codec folds to an opaque node."""
    from gts.wire import canonical, content_id

    w = Writer()
    w.add_blob(b"first", mt=BLOB_MT)
    base = bytes(w.to_bytes())

    # Build a follow-on blob frame that references a non-existent codec id.
    frame: dict[str, object] = {
        "t": "blob",
        "d": b"x",
        "x": [99],
        "pub": {"mt": BLOB_MT},
        "prev": w.head,
    }
    frame["id"] = content_id(frame)
    data = base + canonical(frame)

    g = read(data)
    assert any(o.reason == "unknown-codec" for o in g.opaque)


def test_lazy_blob_meta_eager() -> None:
    """blob_meta is populated before any blob access."""
    blob = b"metadata test"

    w = Writer()
    w.add_blob(blob, mt=BLOB_MT, rep="docs", transform=["zstd"])
    data = bytes(w.to_bytes())

    g = read(data)
    key = digest_str(blob)
    assert key in g.blob_meta
    assert g.blob_meta[key]["mt"] == BLOB_MT
    assert g.blob_meta[key]["rep"] == "docs"


def test_lazy_blob_multi_segment_union() -> None:
    """Multi-segment files copy lazy entries without decompressing during union."""
    blob_a = b"segment a " * 10000
    blob_b = b"segment b " * 10000

    w1 = Writer()
    w1.add_blob(blob_a, mt=BLOB_MT, transform=["zstd"])
    seg1 = w1.to_bytes()

    w2 = Writer()
    w2.add_blob(blob_b, mt=BLOB_MT, transform=["zstd"])
    seg2 = w2.to_bytes()

    data = bytes(seg1) + bytes(seg2)

    counter: list[int] = [0]
    original = _LazyBlobs.__getitem__

    def counting_getitem(self, digest: str) -> bytes:
        counter[0] += 1
        entry = self._entries[digest]
        if isinstance(entry, bytes):
            return entry
        return decode_chain(entry.chain, entry.raw)

    _LazyBlobs.__getitem__ = counting_getitem  # type: ignore[assignment]
    try:
        g = read(data)
        assert len(g.blobs) == 2
        assert counter[0] == 0
    finally:
        _LazyBlobs.__getitem__ = original


def test_lazy_blob_external_records_layout_iou() -> None:
    """An external blob (no inline bytes) records the layout event by digest."""
    digest = "blake3:" + "00" * 32

    w = Writer()
    w.add_frame("blob", pub={"digest": digest})
    data = bytes(w.to_bytes())

    g = read(data)
    assert len(g.blobs) == 0
    # The external blob should not create diagnostics and the digest should be
    # visible in blob_meta if a pub map was provided.
    assert digest in g.blob_meta


def test_lazy_blobs_mapping_interface() -> None:
    """_LazyBlobs satisfies the MutableMapping[str, bytes] contract."""
    lb = _LazyBlobs()
    lb["a"] = b"A"
    assert len(lb) == 1
    assert "a" in lb
    assert lb["a"] == b"A"
    assert lb.get("a") == b"A"
    assert lb.get("missing") is None
    assert list(lb.keys()) == ["a"]
    assert list(lb.values()) == [b"A"]
    assert list(lb.items()) == [("a", b"A")]

    other = _LazyBlobs()
    other["b"] = _LazyBlobEntry(raw=b"x", chain=[Codec("identity", "encode")])
    lb.update(other)
    assert "b" in lb
    assert lb["b"] == b"x"

    del lb["a"]
    assert "a" not in lb


def test_lazy_blobs_decode_failure_raises() -> None:
    """A corrupt lazy entry raises on access (it was valid-looking at fold time)."""
    lb = _LazyBlobs()
    lb["a"] = _LazyBlobEntry(raw=b"not zstd", chain=[Codec("zstd", "compress")])
    with pytest.raises(zstandard.ZstdError):
        lb["a"]
