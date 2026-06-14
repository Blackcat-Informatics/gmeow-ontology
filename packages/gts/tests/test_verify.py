# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for embedded-key GTS signature verification."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from gts import read
from gts.crypto import Signer
from gts.verify import extract_transport_key, verify_file
from gts.writer import Writer

if TYPE_CHECKING:
    pass


_FIXTURES = Path(__file__).parent / "fixtures"


def _load_fixture_keypair() -> tuple[str, str, str]:
    """Load the static Ed25519 test keypair shipped in ``fixtures/``."""
    pub = (_FIXTURES / "test_key.pub.asc").read_text(encoding="utf-8")
    sec = (_FIXTURES / "test_key.sec.asc").read_text(encoding="utf-8")
    fp = (_FIXTURES / "test_key.fingerprint").read_text(encoding="utf-8").strip()
    return pub, sec, fp


def _make_signed_bytes(pub_armor: str, sec_armor: str) -> bytes:
    """Return a tiny signed GTS file with the transport key embedded."""
    signer = Signer.from_gpg_secret_key(sec_armor)
    writer = Writer(profile="dist", signer=signer)
    writer.add_meta({"gts:transportKey": {"kid": signer.kid, "gpg": pub_armor}})
    payload = {
        "terms": [
            {"k": 0, "v": "http://example.org/s"},
            {"k": 0, "v": "http://example.org/p"},
            {"k": 0, "v": "http://example.org/o"},
        ],
        "quads": [[0, 1, 2]],
    }
    writer.add_frame("snapshot", payload=payload)
    return writer.to_bytes()


def test_verify_signed_file_with_embedded_key() -> None:
    """A signed file verifies against its embedded transport key."""
    pub, sec, fingerprint = _load_fixture_keypair()
    data = _make_signed_bytes(pub, sec)
    result = verify_file(data, require_signatures=True)
    assert result.ok, result.errors
    assert result.kid == fingerprint
    assert result.fingerprint == fingerprint
    assert result.signed == 2  # meta + snapshot
    assert result.valid == 2
    assert result.invalid == 0
    assert result.unverified == 0
    assert result.emojihash is not None
    assert result.randomart is not None


def test_verify_with_trusted_key() -> None:
    """A signed file verifies against an out-of-band trusted public key."""
    pub, sec, fingerprint = _load_fixture_keypair()
    data = _make_signed_bytes(pub, sec)
    result = verify_file(data, armored_key=pub, require_signatures=True)
    assert result.ok, result.errors
    assert result.kid == fingerprint


def test_verify_unsigned_file_is_ok_when_not_required() -> None:
    """An unsigned file passes when signatures are not required."""
    data = Writer(profile="dist").to_bytes()
    result = verify_file(data, require_signatures=False)
    assert result.ok
    assert result.signed == 0


def test_verify_unsigned_file_fails_when_required() -> None:
    """An unsigned file fails when signatures are required."""
    data = Writer(profile="dist").to_bytes()
    result = verify_file(data, require_signatures=True)
    assert not result.ok
    assert "no gts:transportKey found" in result.errors[0]


def test_verify_tampered_file_fails() -> None:
    """A single-bit flip causes verification to fail."""
    pub, sec, _fingerprint = _load_fixture_keypair()
    data = bytearray(_make_signed_bytes(pub, sec))
    # Flip a byte somewhere in the middle of the file.  This will corrupt at
    # least one frame's self-hash or signature and cause verification to fail.
    data[len(data) // 2] ^= 0xFF
    result = verify_file(bytes(data), require_signatures=True)
    assert not result.ok


def test_extract_transport_key_round_trip() -> None:
    """The embedded transport public key can be read back from the graph."""
    pub, sec, fingerprint = _load_fixture_keypair()
    data = _make_signed_bytes(pub, sec)
    graph = read(data)
    transport = extract_transport_key(graph)
    assert transport is not None
    assert transport["kid"] == fingerprint
    assert "BEGIN PGP PUBLIC KEY BLOCK" in transport["gpg"]
