# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""COSE signing & encryption for GTS (§9.2/§9.3, issue #272).

A focused, dependency-light RFC 9052 subset built on ``cryptography`` + ``cbor2``:

* **COSE_Sign1 (detached), EdDSA/Ed25519** — signs a frame's ``"id"`` (the payload
  is detached, since the id is already in the frame). Verification recomputes the
  id and checks the signature against a key resolved by ``kid`` (§9.2).
* **COSE_Encrypt0, AES-256-GCM** — seals a payload to a recipient keyed by ``kid``;
  decryption needs that key, else the frame degrades to a ``missing-key`` opaque
  node (§9.3, §8.3).

Key discovery / trust anchoring is **deployment policy** (a :class:`KeyProvider`):
``sigstat == "valid"`` means cryptographically valid under a *resolved* key, not
that the key is trusted (§9.2).
"""

from __future__ import annotations

import os
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Protocol

import cbor2
from cryptography.exceptions import InvalidSignature, InvalidTag
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from gmeow_tools.gts.codec import CodecUnavailableError

# COSE header labels / algorithm ids (RFC 9052 §3.1, IANA COSE registries).
_ALG, _KID, _IV = 1, 4, 5
_ALG_EDDSA, _ALG_A256GCM = -8, 3
_TAG_SIGN1, _TAG_ENCRYPT0 = 18, 16

SigStatus = str  # "none" | "valid" | "invalid" | "unverified"


@dataclass(frozen=True)
class Signer:
    """A signing identity: a ``kid`` and its Ed25519 private key."""

    kid: str
    key: Ed25519PrivateKey  # gitleaks:allow

    @staticmethod
    def generate(kid: str) -> Signer:
        """Generate a fresh Ed25519 signer (test/dev helper)."""
        return Signer(kid, Ed25519PrivateKey.generate())

    @property
    def public_raw(self) -> bytes:
        """The 32-byte raw Ed25519 public key (for registering a verifier)."""
        return self.key.public_key().public_bytes(
            serialization.Encoding.Raw, serialization.PublicFormat.Raw
        )


class KeyProvider(Protocol):
    """Resolves verification / content keys by ``kid`` (deployment policy)."""

    def verification_key(self, kid: str) -> Ed25519PublicKey | None:
        """Return the Ed25519 public key for ``kid`` (``None`` if unknown)."""
        ...

    def content_key(self, kid: str) -> bytes | None:
        """Return the symmetric content key for ``kid`` (``None`` if unknown)."""
        ...


@dataclass
class InMemoryKeys:
    """A simple in-memory :class:`KeyProvider` (tests, single-process tools)."""

    verifiers: dict[str, Ed25519PublicKey]
    content: dict[str, bytes]

    def __init__(
        self,
        verifiers: dict[str, Ed25519PublicKey] | None = None,
        content: dict[str, bytes] | None = None,
    ) -> None:
        """Build a provider from optional verifier and content-key maps."""
        self.verifiers = verifiers or {}
        self.content = content or {}

    def verification_key(self, kid: str) -> Ed25519PublicKey | None:
        """Return the registered verification key for ``kid``, if any."""
        return self.verifiers.get(kid)

    def content_key(self, kid: str) -> bytes | None:
        """Return the registered content key for ``kid``, if any."""
        return self.content.get(kid)

    def trust(self, signer: Signer) -> None:
        """Register a signer's public key for verification."""
        self.verifiers[signer.kid] = signer.key.public_key()


# -- COSE_Sign1 (detached payload = the frame id) -----------------------------


def sign_id(frame_id: bytes, signer: Signer) -> bytes:
    """Return a detached ``COSE_Sign1`` over ``frame_id`` (§9.2)."""
    protected = cbor2.dumps({_ALG: _ALG_EDDSA}, canonical=True)
    sig_structure = cbor2.dumps(
        ["Signature1", protected, b"", frame_id], canonical=True
    )
    signature = signer.key.sign(sig_structure)
    cose = cbor2.CBORTag(
        _TAG_SIGN1, [protected, {_KID: signer.kid.encode()}, None, signature]
    )
    return cbor2.dumps(cose, canonical=True)


def verify_sig(
    sig: bytes, frame_id: bytes, provider: KeyProvider
) -> tuple[SigStatus, str | None]:
    """Verify a detached ``COSE_Sign1`` over ``frame_id``; return (status, kid)."""
    try:
        msg = cbor2.loads(sig)
        body = msg.value if isinstance(msg, cbor2.CBORTag) else msg
        protected, unprotected, _payload, signature = body
        kid_raw = unprotected.get(_KID) if isinstance(unprotected, Mapping) else None
        kid = kid_raw.decode() if isinstance(kid_raw, bytes) else None
    except (ValueError, TypeError, KeyError):
        return ("invalid", None)
    if kid is None:
        return ("invalid", None)
    public = provider.verification_key(kid)
    if public is None:
        return ("unverified", kid)  # no key resolved — present but not checked
    sig_structure = cbor2.dumps(
        ["Signature1", protected, b"", frame_id], canonical=True
    )
    try:
        public.verify(signature, sig_structure)
    except InvalidSignature:
        return ("invalid", kid)
    return ("valid", kid)


# -- COSE_Encrypt0 (AES-256-GCM, keyed by kid) --------------------------------


def encrypt0(plaintext: bytes, kid: str, key: bytes) -> bytes:
    """Seal ``plaintext`` as a ``COSE_Encrypt0`` to the recipient ``kid`` (§9.3)."""
    protected = cbor2.dumps({_ALG: _ALG_A256GCM}, canonical=True)
    iv = os.urandom(12)
    aad = cbor2.dumps(["Encrypt0", protected, b""], canonical=True)
    ciphertext = AESGCM(key).encrypt(iv, plaintext, aad)
    cose = cbor2.CBORTag(
        _TAG_ENCRYPT0, [protected, {_IV: iv, _KID: kid.encode()}, ciphertext]
    )
    return cbor2.dumps(cose, canonical=True)


def decrypt0(blob: bytes, provider: KeyProvider) -> bytes:
    """Open a ``COSE_Encrypt0`` using a key resolved by ``kid``.

    Raises:
        CodecUnavailableError: ``missing-key`` if no key is held for the recipient,
            or if the ciphertext fails authentication.
    """
    try:
        msg = cbor2.loads(blob)
        body = msg.value if isinstance(msg, cbor2.CBORTag) else msg
        protected, unprotected, ciphertext = body
        iv = unprotected[_IV]
        kid = unprotected[_KID].decode()
    except (ValueError, TypeError, KeyError, AttributeError) as exc:
        raise CodecUnavailableError("missing-key", "malformed COSE_Encrypt0") from exc
    key = provider.content_key(kid)
    if key is None:
        raise CodecUnavailableError("missing-key", f"no content key for {kid!r}")
    aad = cbor2.dumps(["Encrypt0", protected, b""], canonical=True)
    try:
        return AESGCM(key).decrypt(iv, ciphertext, aad)
    except InvalidTag as exc:
        detail = "authentication failed (AES-GCM tag mismatch)"
        raise CodecUnavailableError("missing-key", detail) from exc


def recipient_kid(blob: bytes) -> str | None:
    """Best-effort recipient ``kid`` from a ``COSE_Encrypt0`` (cleartext)."""
    try:
        msg = cbor2.loads(blob)
        body = msg.value if isinstance(msg, cbor2.CBORTag) else msg
        kid = body[1][_KID]
        return kid.decode() if isinstance(kid, bytes) else None
    except (ValueError, TypeError, KeyError, IndexError):
        return None
