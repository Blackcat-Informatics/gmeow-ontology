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


# -- COSE encryption wired through writer/reader (opaque profile, §9.3) -------


def test_encrypted_frame_decrypts_with_key() -> None:
    """A frame encrypted (zstd then COSE_Encrypt0) decrypts and folds when keyed."""
    key = os.urandom(32)
    w = Writer(profile="opaque")
    w.add_frame(
        "meta",
        payload={"sealed": "value"},
        transform=["zstd"],
        encrypt=("did:court", key),
    )
    holder = InMemoryKeys(content={"did:court": key})
    g = read(w.to_bytes(), keys=holder)
    assert [d.code for d in g.diagnostics] == []
    assert g.meta.get("sealed") == "value"
    assert not g.opaque  # decrypted in place, not opaque


def test_encrypted_frame_opaque_without_key() -> None:
    """Without the key the sealed frame is opaque: content hidden, recipient shown."""
    key = os.urandom(32)
    w = Writer(profile="opaque")
    w.add_frame("meta", payload={"sealed": "value"}, encrypt=("did:court", key))
    g = read(w.to_bytes())  # no keys
    assert "MissingKey" in [d.code for d in g.diagnostics]
    assert g.opaque
    assert g.opaque[0].reason == "missing-key"
    assert g.opaque[0].recipients is not None
    assert g.opaque[0].recipients[0]["kid"] == "did:court"  # opacity invariant
    assert "sealed" not in g.meta  # content not leaked


def test_selective_disclosure() -> None:
    """A reader without the key reads the public frame; the sealed one is opaque."""
    key = os.urandom(32)
    w = Writer(profile="opaque")
    w.add_terms(_terms())
    w.add_quads([(0, 1, 2, None)])  # public
    w.add_frame("meta", payload={"private": True}, encrypt=("did:court", key))  # sealed
    g = read(w.to_bytes())  # no key
    assert g.quads == [(0, 1, 2, None)]  # public part readable
    assert g.opaque  # sealed part opaque
    assert g.opaque[0].reason == "missing-key"
    assert "private" not in g.meta


def test_decrypt_with_wrong_key_is_opaque() -> None:
    """A held-but-wrong key fails AEAD auth and degrades to missing-key, not a crash."""
    w = Writer(profile="opaque")
    w.add_frame("meta", payload={"x": 1}, encrypt=("did:court", os.urandom(32)))
    wrong = InMemoryKeys(content={"did:court": os.urandom(32)})
    g = read(w.to_bytes(), keys=wrong)
    assert g.opaque
    assert g.opaque[0].reason == "missing-key"


# -- input hardening on untrusted CBOR (PR #289 review) -----------------------


def test_verify_sig_rejects_non_bytes_signature() -> None:
    """A COSE_Sign1 with a non-bstr signature is invalid, not a crash."""
    import cbor2

    from gmeow_tools.gts.crypto import _KID, _TAG_SIGN1

    malformed = cbor2.dumps(
        cbor2.CBORTag(_TAG_SIGN1, [b"prot", {_KID: b"k"}, None, 123]),  # sig is int
        canonical=True,
    )
    status, kid = verify_sig(malformed, _ID, InMemoryKeys())
    assert status == "invalid"
    assert kid == "k"  # recipient still surfaced (opacity invariant)


def test_decrypt0_rejects_malformed_fields() -> None:
    """A held key with a non-bytes IV degrades to missing-key, not a crash."""
    import cbor2

    from gmeow_tools.gts.crypto import _IV, _KID, _TAG_ENCRYPT0

    body = [b"prot", {_IV: "not-bytes", _KID: b"k"}, b"ct"]  # IV is str, not bstr
    bad = cbor2.dumps(cbor2.CBORTag(_TAG_ENCRYPT0, body), canonical=True)
    with pytest.raises(CodecUnavailableError) as exc:
        decrypt0(bad, InMemoryKeys(content={"k": os.urandom(32)}))
    assert exc.value.reason == "missing-key"


def test_encrypt_requires_catalog_entry() -> None:
    """A catalog without 'cose-encrypt0' raises a stable API error, not KeyError."""
    from gmeow_tools.gts.codec import Codec

    catalog = {0: Codec("identity", "encode")}  # no encrypt codec
    w = Writer(profile="opaque", catalog=catalog)
    with pytest.raises(ValueError, match="cose-encrypt0"):
        w.add_frame("meta", payload={"x": 1}, encrypt=("did:court", os.urandom(32)))
