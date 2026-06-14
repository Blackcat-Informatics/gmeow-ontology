# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the minimal OpenPGP Ed25519 parser."""

from __future__ import annotations

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


_FIXTURES = Path(__file__).parent / "fixtures"


def _load_fixture_keypair() -> tuple[str, str, str]:
    """Load the static Ed25519 test keypair shipped in ``fixtures/``."""
    pub = (_FIXTURES / "test_key.pub.asc").read_text(encoding="utf-8")
    sec = (_FIXTURES / "test_key.sec.asc").read_text(encoding="utf-8")
    fp = (_FIXTURES / "test_key.fingerprint").read_text(encoding="utf-8").strip()
    return pub, sec, fp


def test_load_public_and_secret_keys_round_trip() -> None:
    """Public and secret key material round-trip to the same raw bytes."""
    pub_armor, sec_armor, _fp = _load_fixture_keypair()
    public = load_public_key(pub_armor)
    secret = load_secret_key(sec_armor)
    assert public.public_bytes_raw() == secret.public_key().public_bytes_raw()


def test_public_key_fingerprint_is_40_hex_chars() -> None:
    """The computed fingerprint is a 40-character uppercase hex string."""
    pub_armor, _sec, expected_fp = _load_fixture_keypair()
    fp = public_key_fingerprint(pub_armor)
    assert len(fp) == 40
    assert fp == fp.upper()
    assert int(fp, 16)  # valid hex
    assert fp == expected_fp


def test_load_public_key_rejects_secret_armor() -> None:
    """Loading a secret-key block as a public key raises an error."""
    _pub, sec_armor, _fp = _load_fixture_keypair()
    with pytest.raises(OpenPGPError):
        load_public_key(sec_armor)


def test_load_secret_key_rejects_public_armor() -> None:
    """Loading a public-key block as a secret key raises an error."""
    pub_armor, _sec, _fp = _load_fixture_keypair()
    with pytest.raises(OpenPGPError):
        load_secret_key(pub_armor)


def test_load_public_key_rejects_malformed_armor() -> None:
    """Malformed armor is rejected cleanly."""
    with pytest.raises(OpenPGPError):
        load_public_key("not an armored key")


def test_signer_from_gpg_secret_key_uses_fingerprint() -> None:
    """Signer.from_gpg_secret_key defaults its kid to the OpenPGP fingerprint."""
    pub_armor, sec_armor, expected_fp = _load_fixture_keypair()
    signer = Signer.from_gpg_secret_key(sec_armor)
    assert signer.kid == expected_fp
    assert signer.public_raw == load_public_key(pub_armor).public_bytes_raw()
