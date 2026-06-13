# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Minimal OpenPGP parser for Ed25519 armored keys.

This module is intentionally narrow: it parses only the unencrypted armored
public-key and secret-key certificates that GPG emits for Ed25519 (OpenPGP
algorithm 22) keys. It extracts the raw Ed25519 key material so the GTS COSE
signer/verifier can use it without shelling out to ``gpg``.

Other OpenPGP algorithms, encrypted secret keys, and v5/v6 packets are
rejected with a clear error rather than silently mishandled.
"""

from __future__ import annotations

import base64
import hashlib
from typing import TYPE_CHECKING

from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

_ED25519_ALGO = 22
_ED25519_OID = bytes.fromhex("2b06010401da470f01")


class OpenPGPError(ValueError):
    """Raised when an OpenPGP key cannot be parsed or is unsupported."""


def _strip_armor(text: str) -> bytes:
    """Return the decoded packet bytes from an ASCII-armored OpenPGP block."""
    lines = text.splitlines()
    try:
        start = next(
            i for i, line in enumerate(lines) if line.startswith("-----BEGIN PGP")
        )
        end = next(
            i
            for i, line in enumerate(lines)
            if i > start and line.startswith("-----END PGP")
        )
    except StopIteration as exc:  # pragma: no cover - defensive
        raise OpenPGPError("missing armor BEGIN/END lines") from exc

    idx = start + 1
    # Skip optional armor headers (Key-Id, Comment, etc.) up to the blank line.
    while idx < end and lines[idx].strip() != "":
        if ":" in lines[idx]:
            idx += 1
        else:
            break

    body: list[str] = []
    while idx < end:
        line = lines[idx]
        if line.startswith("="):
            break
        if line:
            body.append(line)
        idx += 1

    if not body:
        raise OpenPGPError("empty armor body")

    try:
        return base64.b64decode("".join(body))
    except Exception as exc:
        raise OpenPGPError("invalid base64 armor body") from exc


def _read_mpi(data: bytes, offset: int) -> tuple[bytes, int]:
    """Read an OpenPGP multi-precision integer.

    Returns ``(big_endian_bytes, next_offset)``.
    """
    if offset + 2 > len(data):
        raise OpenPGPError("truncated MPI length")
    bits = int.from_bytes(data[offset : offset + 2], "big")
    length = (bits + 7) // 8
    end = offset + 2 + length
    if end > len(data):
        raise OpenPGPError("truncated MPI payload")
    return data[offset + 2 : end], end


def _next_packet(data: bytes, offset: int) -> tuple[int, bytes, int]:
    """Parse one OpenPGP packet.

    Returns ``(tag, body, next_offset)``. Supports old and new format headers.
    """
    if offset >= len(data):
        raise OpenPGPError("truncated packet header")
    header = data[offset]
    if not (header & 0x80):
        raise OpenPGPError("invalid packet tag octet")

    if header & 0x40:
        # New format packet.
        tag = header & 0x3F
        offset += 1
        if offset >= len(data):
            raise OpenPGPError("truncated new-format length octet")
        length_octet = data[offset]
        if length_octet < 192:
            length = length_octet
            offset += 1
        elif length_octet < 224:
            if offset + 1 >= len(data):
                raise OpenPGPError("truncated new-format 2-octet length")
            length = ((length_octet - 192) << 8) + data[offset + 1] + 192
            offset += 2
        elif length_octet == 255:
            if offset + 4 >= len(data):
                raise OpenPGPError("truncated new-format 4-octet length")
            length = int.from_bytes(data[offset + 1 : offset + 5], "big")
            offset += 5
        else:
            raise OpenPGPError("partial body lengths are not supported")
    else:
        # Old format packet.
        tag = (header >> 2) & 0x0F
        length_type = header & 0x03
        offset += 1
        if length_type == 0:
            if offset >= len(data):
                raise OpenPGPError("truncated old-format length octet")
            length = data[offset]
            offset += 1
        elif length_type == 1:
            if offset + 1 >= len(data):
                raise OpenPGPError("truncated old-format 2-octet length")
            length = int.from_bytes(data[offset : offset + 2], "big")
            offset += 2
        elif length_type == 2:
            if offset + 3 >= len(data):
                raise OpenPGPError("truncated old-format 4-octet length")
            length = int.from_bytes(data[offset : offset + 4], "big")
            offset += 4
        else:
            raise OpenPGPError("indeterminate-length packets are not supported")

    end = offset + length
    if end > len(data):
        raise OpenPGPError("packet body exceeds input")
    return tag, data[offset:end], end


def _iter_packets(data: bytes) -> Sequence[tuple[int, bytes]]:
    """Yield ``(tag, body)`` for every packet in ``data``."""
    offset = 0
    packets: list[tuple[int, bytes]] = []
    while offset < len(data):
        tag, body, offset = _next_packet(data, offset)
        packets.append((tag, body))
    return packets


def _parse_ed25519_public_material(
    body: bytes,
) -> tuple[bytes, bytes, int]:
    """Parse the OID and public-key bytes from a v4 public-key packet body.

    Returns ``(oid, ed25519_public_key_32_bytes, end_offset_of_public_material)``.
    """
    if len(body) < 6 or body[0] != 4:
        raise OpenPGPError("only OpenPGP v4 public keys are supported")
    if body[5] != _ED25519_ALGO:
        raise OpenPGPError(f"unsupported public-key algorithm {body[5]}")

    offset = 6
    if offset >= len(body):
        raise OpenPGPError("truncated public-key packet")
    oid_len = body[offset]
    offset += 1
    if offset + oid_len > len(body):
        raise OpenPGPError("truncated OID")
    oid = body[offset : offset + oid_len]
    offset += oid_len
    if oid != _ED25519_OID:
        raise OpenPGPError(f"unsupported curve OID {oid.hex()}")

    pub_mpi, offset = _read_mpi(body, offset)
    # GPG encodes Ed25519 public keys as a 33-byte MPI: 0x40 || 32-byte key.
    # If the key happened to need a leading zero for MPI sign-bit rules, the
    # length can also be 33 bytes with a 0x00 prefix. A 32-byte MPI is also
    # valid for keys whose high bit is clear.
    if len(pub_mpi) == 33:
        pub_bytes = pub_mpi[-32:]
    elif len(pub_mpi) == 32:
        pub_bytes = pub_mpi
    else:
        raise OpenPGPError(f"unexpected Ed25519 public MPI length {len(pub_mpi)}")
    return oid, pub_bytes, offset


def _parse_ed25519_secret_material(
    body: bytes,
) -> tuple[bytes, bytes]:
    """Parse the public and secret key bytes from a v4 secret-key packet body.

    Returns ``(ed25519_public_key_32_bytes, ed25519_secret_key_32_bytes)``.
    """
    _, pub_bytes, offset = _parse_ed25519_public_material(body)

    if offset >= len(body):
        raise OpenPGPError("truncated secret-key packet")
    s2k_usage = body[offset]
    offset += 1

    if s2k_usage != 0:
        raise OpenPGPError(
            "encrypted secret keys are not supported; export the key "
            "unencrypted (e.g. `gpg --batch --pinentry-mode loopback "
            "--passphrase-fd 0 --export-secret-keys --armor KEYID`)"
        )

    sec_mpi, offset = _read_mpi(body, offset)
    # The secret scalar is a 255-bit value; MPI length is 255 or 256 bits.
    if len(sec_mpi) == 33 and sec_mpi[0] == 0:
        sec_bytes = sec_mpi[1:]
    elif len(sec_mpi) == 32:
        sec_bytes = sec_mpi
    else:
        raise OpenPGPError(f"unexpected Ed25519 secret MPI length {len(sec_mpi)}")

    # Optional 2-octet checksum follows; we ignore it because the bytes we
    # extracted are enough to reconstruct the key.
    return pub_bytes, sec_bytes


def load_public_key(armored: str) -> Ed25519PublicKey:
    """Load an Ed25519 public key from an armored OpenPGP certificate."""
    data = _strip_armor(armored)
    for tag, body in _iter_packets(data):
        if tag == 6:
            _, pub_bytes, _ = _parse_ed25519_public_material(body)
            return Ed25519PublicKey.from_public_bytes(pub_bytes)
    raise OpenPGPError("no public-key packet found")


def load_secret_key(armored: str) -> Ed25519PrivateKey:
    """Load an Ed25519 secret key from an armored OpenPGP secret key block."""
    data = _strip_armor(armored)
    for tag, body in _iter_packets(data):
        if tag == 5:
            _, sec_bytes = _parse_ed25519_secret_material(body)
            return Ed25519PrivateKey.from_private_bytes(sec_bytes)
    raise OpenPGPError("no secret-key packet found")


def public_key_fingerprint(armored: str) -> str:
    """Compute the OpenPGP v4 fingerprint of the first Ed25519 key.

    Accepts either an armored public-key certificate or a secret-key block.
    The fingerprint is returned as an uppercase 40-character hex string.
    """
    data = _strip_armor(armored)
    for tag, body in _iter_packets(data):
        if tag == 6:
            pub_key_body = body
        elif tag == 5:
            # The secret-key packet body starts with the same public-key
            # material; slice up to the end of the public MPI.
            _, _, end = _parse_ed25519_public_material(body)
            pub_key_body = body[:end]
        else:
            continue
        # v4 fingerprint = SHA-1(0x99 || 2-octet body length || body).
        # SHA-1 is required by RFC 4880 for OpenPGP v4 fingerprints; it is not
        # used here as a general-purpose integrity mechanism.
        digest = hashlib.sha1(
            b"\x99" + len(pub_key_body).to_bytes(2, "big") + pub_key_body
        ).hexdigest()
        return digest.upper()
    raise OpenPGPError("no public-key packet found")
