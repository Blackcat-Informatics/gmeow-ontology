# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the minimal OpenPGP Ed25519 parser."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from gts.crypto import Signer
from gts.openpgp import (
    OpenPGPError,
    load_public_key,
    load_secret_key,
    public_key_fingerprint,
)

if TYPE_CHECKING:
    pass


pytestmark = [
    pytest.mark.skipif(
        shutil.which("gpg") is None, reason="gpg binary is not available"
    ),
]


def _generate_keypair(tmp_path: Path) -> tuple[str, str]:
    """Generate a temporary Ed25519 keypair and return (public_asc, secret_asc)."""
    gnupghome = tmp_path / "gnupg"
    gnupghome.mkdir(mode=0o700)
    keygen = gnupghome / "keygen.txt"
    keygen.write_text(
        "%no-protection\n"
        "Key-Type: EDDSA\n"
        "Key-Curve: ed25519\n"
        "Key-Usage: sign\n"
        "Name-Real: GTS Test Key\n"
        "Name-Email: test@gts.example\n"
        "Expire-Date: 0\n"
        "%commit\n"
    )
    subprocess.run(
        [
            "gpg",
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--gen-key",
            str(keygen),
        ],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        capture_output=True,
        text=True,
    )
    result = subprocess.run(
        ["gpg", "--list-keys", "--keyid-format", "long"],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        capture_output=True,
        text=True,
    )
    keyid = None
    for line in result.stdout.splitlines():
        if line.startswith("pub"):
            # pub   ed25519/LONGID 2026-06-13 [SC]
            keyid = line.split()[1].split("/")[1]
            break
    assert keyid is not None

    pub_path = tmp_path / "pub.asc"
    sec_path = tmp_path / "sec.asc"
    subprocess.run(
        ["gpg", "--armor", "--export", keyid],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        stdout=pub_path.open("wb"),
    )
    subprocess.run(
        ["gpg", "--armor", "--export-secret-keys", keyid],
        env={**dict(subprocess.os.environ), "GNUPGHOME": str(gnupghome)},
        check=True,
        stdout=sec_path.open("wb"),
    )
    return pub_path.read_text(), sec_path.read_text()


def test_load_public_and_secret_keys_round_trip(tmp_path: Path) -> None:
    """Public and secret key material round-trip to the same raw bytes."""
    pub_armor, sec_armor = _generate_keypair(tmp_path)
    public = load_public_key(pub_armor)
    secret = load_secret_key(sec_armor)
    assert public.public_bytes_raw() == secret.public_key().public_bytes_raw()


def test_public_key_fingerprint_is_40_hex_chars(tmp_path: Path) -> None:
    """The computed fingerprint is a 40-character uppercase hex string."""
    pub_armor, _ = _generate_keypair(tmp_path)
    fp = public_key_fingerprint(pub_armor)
    assert len(fp) == 40
    assert fp == fp.upper()
    assert int(fp, 16)  # valid hex


def test_load_public_key_rejects_secret_armor(tmp_path: Path) -> None:
    """Loading a secret-key block as a public key raises an error."""
    _, sec_armor = _generate_keypair(tmp_path)
    with pytest.raises(OpenPGPError):
        load_public_key(sec_armor)


def test_load_secret_key_rejects_public_armor(tmp_path: Path) -> None:
    """Loading a public-key block as a secret key raises an error."""
    pub_armor, _ = _generate_keypair(tmp_path)
    with pytest.raises(OpenPGPError):
        load_secret_key(pub_armor)


def test_load_public_key_rejects_malformed_armor() -> None:
    """Malformed armor is rejected cleanly."""
    with pytest.raises(OpenPGPError):
        load_public_key("not an armored key")


def test_signer_from_gpg_secret_key_uses_fingerprint(tmp_path: Path) -> None:
    """Signer.from_gpg_secret_key defaults its kid to the OpenPGP fingerprint."""
    pub_armor, sec_armor = _generate_keypair(tmp_path)
    expected_fp = public_key_fingerprint(pub_armor)
    signer = Signer.from_gpg_secret_key(sec_armor)
    assert signer.kid == expected_fp
    assert signer.public_raw == load_public_key(pub_armor).public_bytes_raw()
