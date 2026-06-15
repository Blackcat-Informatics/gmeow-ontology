# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The GTS transform catalog (§8).

Each catalog entry is a codec with a canonical ``name`` and a ``cls`` of ``encode``,
``compress`` or ``encrypt``. Decoding a chain requires a capability per codec: a
library (encode/compress) or a key (encrypt). The baseline implements the core
``identity``/``gzip``/``zstd`` codecs; an unknown codec or an ``encrypt`` codec
(no keys in the baseline) raises :class:`CodecUnavailableError`, which the reader folds
into an opaque node (§7.6, §8.3).
"""

from __future__ import annotations

import gzip
import io
from collections.abc import Callable
from dataclasses import dataclass
from typing import Literal

import zstandard

CodecClass = Literal["encode", "compress", "encrypt"]

#: A key-bound callback that reverses an ``encrypt`` codec (supplied by the reader).
Decryptor = Callable[[bytes], bytes]


class CodecUnavailableError(Exception):
    """Raised when a transform cannot be reversed.

    Attributes:
        reason: ``"unknown-codec"`` (no library) or ``"missing-key"`` (encrypt class).
    """

    def __init__(self, reason: str, detail: str) -> None:
        """Store the degradation ``reason`` alongside the human-readable detail."""
        super().__init__(detail)
        self.reason = reason


@dataclass(frozen=True)
class Codec:
    """A catalog entry (§5, §8.5)."""

    name: str
    cls: CodecClass


# The default catalog a writer emits; ids are file-local (§8.5 — match by name).
DEFAULT_CATALOG: dict[int, Codec] = {
    0: Codec("identity", "encode"),
    1: Codec("gzip", "compress"),
    2: Codec("zstd", "compress"),
    3: Codec("zstd-rsyncable", "compress"),
    7: Codec("cose-encrypt0", "encrypt"),
}

_ZSTD_C = zstandard.ZstdCompressor()
_ZSTD_D = zstandard.ZstdDecompressor()

#: Uncompressed block size for the zstd-rsyncable codec. Each block is an
#: independent zstd frame so that a local change only affects that block.
_RSYNCABLE_BLOCK_SIZE = 65536


def _encode_zstd_rsyncable(data: bytes) -> bytes:
    """Compress with periodic state resets for rsync/Git friendliness.

    Each block is compressed as an independent zstd frame and concatenated.
    A change inside one block therefore only affects that block's compressed
    bytes, keeping rsync/Git deltas small.
    """
    out = io.BytesIO()
    view = memoryview(data)
    for i in range(0, len(data), _RSYNCABLE_BLOCK_SIZE):
        out.write(_ZSTD_C.compress(view[i : i + _RSYNCABLE_BLOCK_SIZE]))
    return out.getvalue()


def _encode_one(name: str, data: bytes) -> bytes:
    """Apply a single codec by canonical name (encode direction)."""
    if name == "identity":
        return data
    if name == "gzip":
        return gzip.compress(data)
    if name == "zstd":
        return _ZSTD_C.compress(data)
    if name == "zstd-rsyncable":
        return _encode_zstd_rsyncable(data)
    msg = f"writer cannot encode with codec {name!r}"
    raise CodecUnavailableError("unknown-codec", msg)


def _decode_zstd(data: bytes) -> bytes:
    """Decompress zstd bytes, including rsyncable streams with flush blocks."""
    out = io.BytesIO()
    with _ZSTD_D.stream_reader(io.BytesIO(data)) as reader:
        while True:
            chunk = reader.read(131072)
            if not chunk:
                break
            out.write(chunk)
    return out.getvalue()


def _decode_one(codec: Codec, data: bytes) -> bytes:
    """Reverse a single codec, or raise :class:`CodecUnavailableError` (§8.3)."""
    if codec.cls == "encrypt":
        raise CodecUnavailableError(
            "missing-key", f"no key for encrypt codec {codec.name!r}"
        )
    if codec.name == "identity":
        return data
    if codec.name == "gzip":
        return gzip.decompress(data)
    if codec.name in ("zstd", "zstd-rsyncable"):
        return _decode_zstd(data)
    raise CodecUnavailableError("unknown-codec", f"unknown codec {codec.name!r}")


def encode_chain(chain: list[str], data: bytes) -> bytes:
    """Encode ``data`` through codec names in array order (§8.2)."""
    for name in chain:
        data = _encode_one(name, data)
    return data


def decode_chain(
    chain: list[Codec],
    data: bytes,
    *,
    decrypt: Decryptor | None = None,
) -> bytes:
    """Reverse a resolved codec chain, last to first (§6.1, §8.2).

    ``encrypt``-class codecs are handed to ``decrypt`` (a key-bound callback the
    reader supplies); if none is given, the frame degrades to ``missing-key`` —
    keeping this module free of any crypto dependency.
    """
    for codec in reversed(chain):
        if codec.cls == "encrypt":
            if decrypt is None:
                raise CodecUnavailableError("missing-key", f"no key for {codec.name!r}")
            data = decrypt(data)
        else:
            data = _decode_one(codec, data)
    return data
