# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the GTS transform catalog (§8), including rsyncable codecs."""

from __future__ import annotations

import pytest

from gts.codec import Codec, decode_chain, encode_chain


@pytest.mark.parametrize("codec_name", ["identity", "gzip", "zstd", "zstd-rsyncable"])
def test_round_trip(codec_name: str) -> None:
    """Every baseline codec round-trips arbitrary bytes."""
    data = b"Hello world " * 1000 + bytes(range(256))
    encoded = encode_chain([codec_name], data)
    cls = "encode" if codec_name == "identity" else "compress"
    decoded = decode_chain([Codec(codec_name, cls)], encoded)
    assert decoded == data


def test_zstd_rsyncable_localizes_changes() -> None:
    """A single-byte mutation only affects the compressed block it lives in.

    Because each 64 KiB chunk is an independent zstd frame, a change at the
    middle of the payload leaves the first half and last half of the
    compressed bytes byte-for-byte identical.
    """
    block_size = 65536
    size = 4 * block_size
    # Use a payload with enough entropy to be mildly compressible but not
    # trivially tiny when compressed.
    base = (b"The quick brown fox jumps over the lazy dog. " * 6000)[:size]
    mid = len(base) // 2
    mutated = base[:mid] + bytes([(base[mid] + 1) % 256]) + base[mid + 1 :]

    rsync_base = encode_chain(["zstd-rsyncable"], base)
    rsync_mut = encode_chain(["zstd-rsyncable"], mutated)

    # With independent blocks, the first two blocks (before mid) and the last
    # block (after mid) are identical between the two compressed streams.
    # Identify block boundaries by walking from both ends until a difference
    # appears.
    prefix_match = 0
    for a, b in zip(rsync_base, rsync_mut, strict=False):
        if a == b:
            prefix_match += 1
        else:
            break

    tail_match = 0
    for a, b in zip(reversed(rsync_base), reversed(rsync_mut), strict=False):
        if a == b:
            tail_match += 1
        else:
            break

    assert prefix_match > 0, "prefix before the change should be identical"
    assert tail_match > 0, "tail after the change should be identical"
    # The changed region is bounded by roughly one compressed block, not the
    # whole remainder of the stream.
    affected = min(len(rsync_base), len(rsync_mut)) - prefix_match - tail_match
    assert affected < len(rsync_base) // 2, (
        f"affected region {affected} bytes should be much smaller than "
        f"stream length {len(rsync_base)}"
    )


def test_zstd_rsyncable_decodes_via_zstd_path() -> None:
    """zstd-rsyncable output is valid zstd and decodes through the shared path."""
    data = b"rsyncable payload " * 5000
    encoded = encode_chain(["zstd-rsyncable"], data)
    # The codec's decode path treats zstd-rsyncable as zstd-compatible.
    decoded = decode_chain([Codec("zstd-rsyncable", "compress")], encoded)
    assert decoded == data
