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
from dataclasses import dataclass
from typing import Literal

import zstandard

CodecClass = Literal["encode", "compress", "encrypt"]


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
}

_ZSTD_C = zstandard.ZstdCompressor()
_ZSTD_D = zstandard.ZstdDecompressor()


def _encode_one(name: str, data: bytes) -> bytes:
    """Apply a single codec by canonical name (encode direction)."""
    if name == "identity":
        return data
    if name == "gzip":
        return gzip.compress(data)
    if name == "zstd":
        return _ZSTD_C.compress(data)
    msg = f"writer cannot encode with codec {name!r}"
    raise CodecUnavailableError("unknown-codec", msg)


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
    if codec.name == "zstd":
        return _ZSTD_D.decompress(data)
    raise CodecUnavailableError("unknown-codec", f"unknown codec {codec.name!r}")


def encode_chain(chain: list[str], data: bytes) -> bytes:
    """Encode ``data`` through codec names in array order (§8.2)."""
    for name in chain:
        data = _encode_one(name, data)
    return data


def decode_chain(chain: list[Codec], data: bytes) -> bytes:
    """Reverse a resolved codec chain, last to first (§6.1, §8.2)."""
    for codec in reversed(chain):
        data = _decode_one(codec, data)
    return data
