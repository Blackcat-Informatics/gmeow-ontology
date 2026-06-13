# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for embedded-key GTS signature verification."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from gts import read
from gts.crypto import Signer
from gts.verify import extract_transport_key, verify_file
from gts.writer import Writer

if TYPE_CHECKING:
    pass


pytestmark = [
    pytest.mark.skipif(
        shutil.which("gpg") is None, reason="gpg binary is not available"
    ),
]


def _generate_keypair(tmp_path: Path) -> tuple[str, str, str]:
    """Generate a temporary Ed25519 keypair.

    Returns ``(public_asc, secret_asc, fingerprint)``.
    """
    gnupghome = tmp_path / "gnupg"
    gnupghome.mkdir(mode=0o700)
    keygen = gnupghome / "keygen.txt"
    keygen.write_text(
        "%no-protection\n"
        "Key-Type: EDDSA\n"
        "Key-Curve: ed25519\n"
        "Key-Usage: sign\n"
        "Name-Real: GTS Verify Test Key\n"
        "Name-Email: test@gts.example\n"
        "Expire-Date: 0\n"
        "%commit\n"
    )
    subprocess.run(
        ["gpg", "--batch", "--pinentry-mode", "loopback", "--gen-key", str(keygen)],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        capture_output=True,
        text=True,
    )
    result = subprocess.run(
        ["gpg", "--list-keys", "--with-colons"],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        capture_output=True,
        text=True,
    )
    fingerprint = None
    for line in result.stdout.splitlines():
        if line.startswith("fpr"):
            fingerprint = line.split(":")[9]
            break
    assert fingerprint is not None

    pub_path = tmp_path / "pub.asc"
    sec_path = tmp_path / "sec.asc"
    subprocess.run(
        ["gpg", "--armor", "--export", fingerprint],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        stdout=pub_path.open("wb"),
    )
    subprocess.run(
        ["gpg", "--armor", "--export-secret-keys", fingerprint],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        stdout=sec_path.open("wb"),
    )
    return pub_path.read_text(), sec_path.read_text(), fingerprint


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


def test_verify_signed_file_with_embedded_key(tmp_path: Path) -> None:
    """A signed file verifies against its embedded transport key."""
    pub, sec, fingerprint = _generate_keypair(tmp_path)
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


def test_verify_with_trusted_key(tmp_path: Path) -> None:
    """A signed file verifies against an out-of-band trusted public key."""
    pub, sec, fingerprint = _generate_keypair(tmp_path)
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


def test_verify_tampered_file_fails(tmp_path: Path) -> None:
    """A single-bit flip causes verification to fail."""
    pub, sec, _fingerprint = _generate_keypair(tmp_path)
    data = bytearray(_make_signed_bytes(pub, sec))
    # Flip a byte somewhere in the middle of the file.  This will corrupt at
    # least one frame's self-hash or signature and cause verification to fail.
    data[len(data) // 2] ^= 0xFF
    result = verify_file(bytes(data), require_signatures=True)
    assert not result.ok


def test_extract_transport_key_round_trip(tmp_path: Path) -> None:
    """The embedded transport public key can be read back from the graph."""
    pub, sec, fingerprint = _generate_keypair(tmp_path)
    data = _make_signed_bytes(pub, sec)
    graph = read(data)
    transport = extract_transport_key(graph)
    assert transport is not None
    assert transport["kid"] == fingerprint
    assert "BEGIN PGP PUBLIC KEY BLOCK" in transport["gpg"]
