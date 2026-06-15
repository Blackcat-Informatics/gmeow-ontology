# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Embedded-key signature verification for GTS files (issue #434).

The default verification path trusts the transport key embedded in the first
``meta`` frame of the file itself.  That key is an armored OpenPGP Ed25519
certificate; the GTS reader verifies every ``COSE_Sign1`` signature against it
as the file is folded.  ``verify_file`` wraps this in a friendly result object
suitable for CLI reporting.

A caller with an out-of-band trusted key can pass ``armored_key`` to override
the embedded key.  In either case the reported ``kid`` is the OpenPGP
fingerprint, and the visual hashes are computed from the raw Ed25519 public
key bytes.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field

from cryptography.hazmat.primitives import serialization

from gts.crypto import InMemoryKeys, KeyProvider
from gts.emojihash import emojihash, emojihash_labels, randomart
from gts.model import Diagnostic, Graph
from gts.openpgp import load_public_key, public_key_fingerprint
from gts.reader import read

_HEX = frozenset("0123456789ABCDEF")


@dataclass
class VerificationResult:
    """Outcome of verifying a GTS file's signatures."""

    ok: bool
    kid: str | None = None
    fingerprint: str | None = None
    emojihash: str | None = None
    emojihash_labels: str | None = None
    randomart: str | None = None
    frames: int = 0
    signed: int = 0
    valid: int = 0
    invalid: int = 0
    unverified: int = 0
    errors: list[str] = field(default_factory=list)
    diagnostics: list[Diagnostic] = field(default_factory=list)


def format_fingerprint(fingerprint: str) -> str:
    """Return an OpenPGP fingerprint grouped for human comparison."""
    compact = fingerprint.replace(" ", "").upper()
    if not compact or any(ch not in _HEX for ch in compact):
        return fingerprint
    return " ".join(compact[idx : idx + 4] for idx in range(0, len(compact), 4))


def extract_transport_key(graph: Graph) -> dict[str, str] | None:
    """Return the embedded ``gts:transportKey`` meta value if well-formed."""
    value = graph.meta.get("gts:transportKey")
    if not isinstance(value, Mapping):
        return None
    kid = value.get("kid")
    gpg = value.get("gpg")
    if not isinstance(kid, str) or not isinstance(gpg, str):
        return None
    return {"kid": kid, "gpg": gpg}


def _provider_from_armor(
    armored: str, kid: str | None = None
) -> tuple[InMemoryKeys, bytes, str]:
    """Build a key provider from an armored OpenPGP public key.

    Returns ``(provider, public_raw_bytes, fingerprint)``.  When ``kid`` is not
    supplied the OpenPGP fingerprint is used, matching the producer's default.
    """
    public_key = load_public_key(armored)
    fingerprint = public_key_fingerprint(armored)
    raw = public_key.public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    resolved_kid = kid if kid is not None else fingerprint
    return InMemoryKeys(verifiers={resolved_kid: public_key}), raw, fingerprint


def verify_file(
    data: bytes,
    *,
    armored_key: str | None = None,
    require_signatures: bool = True,
) -> VerificationResult:
    """Verify a GTS file's embedded signatures.

    Args:
        data: the GTS file bytes.
        armored_key: optional out-of-band armored OpenPGP public key.  When
            omitted, the key embedded in the file's first ``meta`` frame is
            used; if there is none, verification fails.
        require_signatures: when ``True`` (the release default), a file that
            carries no signed frames is treated as a verification failure.

    Returns:
        A :class:`VerificationResult` with per-frame counts and a summary
        ``ok`` boolean.  The result is never raised; malformed files are
        surfaced through ``diagnostics`` and ``errors``.
    """
    provider: KeyProvider | None = None
    public_raw: bytes | None = None
    fingerprint: str | None = None
    kid: str | None = None
    errors: list[str] = []

    if armored_key is not None:
        try:
            provider, public_raw, fingerprint = _provider_from_armor(armored_key)
            kid = fingerprint
        except Exception as exc:
            errors.append(f"cannot load --trusted-key: {exc}")
            return VerificationResult(ok=False, errors=errors)
    else:
        # First pass: read without keys to discover the embedded transport key.
        first = read(data)
        transport = extract_transport_key(first)
        if transport is None:
            # Without an embedded key there is nothing to verify against.
            if not require_signatures and not first.signatures:
                return VerificationResult(
                    ok=True,
                    errors=errors,
                    diagnostics=list(first.diagnostics),
                )
            errors.append("no gts:transportKey found in file metadata")
            return VerificationResult(
                ok=False,
                errors=errors,
                diagnostics=list(first.diagnostics),
            )
        try:
            provider, public_raw, fingerprint = _provider_from_armor(
                transport["gpg"], kid=transport["kid"]
            )
            kid = transport["kid"]
        except Exception as exc:
            errors.append(f"cannot load embedded transport key: {exc}")
            return VerificationResult(
                ok=False,
                kid=transport.get("kid"),
                errors=errors,
                diagnostics=list(first.diagnostics),
            )

    # Second pass: verify signatures as the file is folded.
    graph = read(data, keys=provider)
    diagnostics = list(graph.diagnostics)

    signed = len(graph.signatures)
    valid = sum(1 for s in graph.signatures if s.status == "valid")
    invalid = sum(1 for s in graph.signatures if s.status == "invalid")
    unverified = sum(1 for s in graph.signatures if s.status == "unverified")

    if invalid:
        errors.append(f"{invalid} signature(s) invalid")
    if unverified:
        errors.append(f"{unverified} signature(s) unverified (no key resolved)")
    if require_signatures and signed == 0:
        errors.append("no signed frames found")

    ok = not errors and invalid == 0 and unverified == 0

    return VerificationResult(
        ok=ok,
        kid=kid,
        fingerprint=fingerprint,
        emojihash=emojihash(public_raw) if public_raw is not None else None,
        emojihash_labels=emojihash_labels(public_raw)
        if public_raw is not None
        else None,
        randomart=randomart(public_raw, label="GTS transport")
        if public_raw is not None
        else None,
        frames=len(graph.signatures),
        signed=signed,
        valid=valid,
        invalid=invalid,
        unverified=unverified,
        errors=errors,
        diagnostics=diagnostics,
    )
