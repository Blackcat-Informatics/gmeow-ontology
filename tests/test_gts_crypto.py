# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""COSE signing tests for GTS (§9.2, issue #272) + the COSE_Encrypt0 crypto core."""

from __future__ import annotations

import os

import pytest

from gmeow_tools.gts import InMemoryKeys, Signer, Term, TermKind, Writer, read
from gmeow_tools.gts.codec import CodecUnavailableError
from gmeow_tools.gts.crypto import decrypt0, encrypt0, sign_id, verify_sig

EX = "https://example.org/"
_ID = b"\x01" * 32


def _terms() -> list[Term]:
    return [
        Term(TermKind.IRI, EX + "s"),
        Term(TermKind.IRI, EX + "p"),
        Term(TermKind.IRI, EX + "o"),
    ]


# -- signing through the writer/reader ---------------------------------------


def test_signed_frames_verify_valid() -> None:
    """Writer(signer=…) signs every frame; the reader records all-valid with the key."""
    signer = Signer.generate("did:gmeow:test")
    keys = InMemoryKeys()
    keys.trust(signer)

    w = Writer(profile="evidence", signer=signer)
    w.add_terms(_terms())
    w.add_quads([(0, 1, 2, None)])
    g = read(w.to_bytes(), keys=keys)

    assert [d.code for d in g.diagnostics] == []
    assert len(g.signatures) == 2  # both frames signed
    assert {s.status for s in g.signatures} == {"valid"}
    assert {s.kid for s in g.signatures} == {"did:gmeow:test"}


def test_signed_frames_unverified_without_keys() -> None:
    """A signed log read without a key provider records sigs as 'unverified'."""
    signer = Signer.generate("k")
    w = Writer(signer=signer)
    w.add_terms(_terms())
    g = read(w.to_bytes())  # no keys
    assert g.signatures and all(s.status == "unverified" for s in g.signatures)


# -- COSE_Sign1 unit-level ----------------------------------------------------


def test_verify_sig_valid_invalid_unverified() -> None:
    signer = Signer.generate("k")
    sig = sign_id(_ID, signer)

    trusted = InMemoryKeys()
    trusted.trust(signer)
    assert verify_sig(sig, _ID, trusted) == ("valid", "k")

    # wrong key registered under the same kid -> invalid
    wrong = InMemoryKeys(verifiers={"k": Signer.generate("k").key.public_key()})
    assert verify_sig(sig, _ID, wrong)[0] == "invalid"

    # signature over a different id -> invalid
    assert verify_sig(sig, b"\x02" * 32, trusted)[0] == "invalid"

    # no key resolved -> unverified
    assert verify_sig(sig, _ID, InMemoryKeys())[0] == "unverified"


# -- truncation detection (§9, §17) -------------------------------------------


def test_truncation_detected_against_head() -> None:
    """A short log fails the head commitment; the full log passes."""
    full = Writer(profile="evidence", signer=Signer.generate("k"))
    full.add_terms(_terms())
    head = full.add_quads([(0, 1, 2, None)])  # the true head id

    assert all(
        d.code != "TruncatedLog"
        for d in read(full.to_bytes(), expected_head=head).diagnostics
    )

    short = Writer(profile="evidence", signer=Signer.generate("k"))
    short.add_terms(_terms())  # missing the quads frame -> different head
    codes = [d.code for d in read(short.to_bytes(), expected_head=head).diagnostics]
    assert "TruncatedLog" in codes


# -- COSE_Encrypt0 crypto core (wired into the reader in a follow-up) ---------


def test_encrypt0_round_trip_and_missing_key() -> None:
    key = os.urandom(32)
    sealed = encrypt0(b"verified id record", "did:court", key)

    holder = InMemoryKeys(content={"did:court": key})
    assert decrypt0(sealed, holder) == b"verified id record"

    with pytest.raises(CodecUnavailableError) as exc:
        decrypt0(sealed, InMemoryKeys())  # no key -> missing-key
    assert exc.value.reason == "missing-key"
