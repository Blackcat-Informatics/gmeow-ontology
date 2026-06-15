<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Trust — keys, certifications, and perspectival owner-trust

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/trust` · **tier: core**
> The Web-of-Trust superset layer: who holds which key, who vouches for the binding, and who trusts whom — never computed, only recorded.

This is the cross-cutting trust facility — cryptographic keys, certifications
(key↔identity attestations), and owner-trust — the superset of OpenPGP (RFC 4880/9580),
X.509, SSH, and Nostr, aligned to the WOT schema by reference (Principle 5). Its governing
refusal: trust here is **asserted and perspectival**; trust *metrics* (transitive validity
propagation) stay outside the logical core (Principle 12). There is no global `trusts`
property, `endorses` is neither symmetric nor transitive, and no property chain ever makes
A trust C because A trusts B and B trusts C — bounding exactly that is what trust-signature
depth is *for*.

The slice exercises the standpoint doctrine standpoint doctrine that governs every contested-fact
slice: `accordingTo` (whose frame holds it) ⟂ `wasAttributedTo` (which source recorded it)
⟂ `confidence` (how sure we are) — three axes that never bridge (Principle 9). A
`TrustAssertion` is already perspectival (its trustor is the frame holder), but the
underlying `Certification` can *also* be disputed across standpoints — one holds the
binding unequivocal, another refutes it — through the cross-cutting standpoint facility
alone: no trust-specific dispute mechanism, no `primaryCertification`, no
`preferredTrust`. For the claim spine (Principle 14), this slice is the attestation floor:
the keys and signatures that make a GTS memory package signed, append-only, and
model-attested are first-class individuals here.

## Keys

### gmeow:CryptographicKey

A public key, certificate, or key material bound to an agent's identity — the thing a
signature is made with and a certification vouches for. An `InformationObject`. Carries
source-variable descriptors (`fingerprint`, `keyId`, `keyAlgorithm`, `keyMaterial`,
`keyExpiresAt`) — none functional, because different sources legitimately report
differing formats and values, and those reports coexist (Principle 9).

### gmeow:KeyScheme

The scheme/format of a key — `keySchemePGP`, `keySchemeX509`, `keySchemeSSH`,
`keySchemeNostr` — a value vocabulary, never key subclasses: schemes are open-ended and
carry no distinct structure here, so a new scheme is a new individual (the standard
open-vocabulary move). `gmeow:keyScheme` is functional: a key of a different scheme is a
different key.

### gmeow:holdsKey · gmeow:accountKey

The two possession seams: an Agent holds a key (tenure carried flat with
`validFrom`/`validUntil` on the statement — the flat-first pattern); an OnlineAccount is
identified by a key (`accountKey` joins a decentralized-identity account, e.g. a Nostr
pubkey literal, to the key as a first-class entity).

## Certification — the WoT edge

### gmeow:Certification

A reified `gufo:Relator`: agent X attests that key K belongs to identity Y — the PGP
key-signature. EL-axiomatised to mediate a `certifier`, a `certifiedKey`, and a
`certifiedIdentity` (all functional; closed-world cardinality is SHACL's, Principle 7).
Certifications expire and are revoked, so the validity window rides on
`validFrom`/`validUntil` — revocation *sets* `validUntil`, it never deletes
(Principle 10).

### gmeow:certificationLevel

How carefully the binding was verified — the OpenPGP ladder: generic, persona, casual,
positive. Recorded verbatim as input to downstream validity computation, never
interpreted by the reasoner.

## Owner-trust — perspectival by construction

### gmeow:TrustAssertion

The OpenPGP owner-trust notion, reified with an explicit `trustor` so one agent's
subjective trust never becomes a global fact: trustor, `trustee`, `trustLevel`
(ultimate / full / marginal / none), and a `validFrom`/`validUntil` window. The relator
*is* the standpoint — there is nothing to dispute about "S trusts T marginally" except
whether S really asserted it.

### gmeow:introducerDepth · gmeow:introducerAmount

The trust-signature parameters: how many levels of indirect introducers the trustor will
follow, and with what weight. These are *inputs* to a Web-of-Trust validity computation
that happens in the projection layer — represent inputs and outputs, never compute the
metric in OWL (Principle 12).

### gmeow:endorses

The flat convenience shortcut for "vouches for" — deliberately directional (not
symmetric) and not transitive. Promote to a `TrustAssertion` when the trust needs a
level, a window, or its own identity: the flat↔reified pairing in its standard form.

## Signatures

### gmeow:CryptographicSignature

A signature over any artifact — not only mail — asserting origin and integrity, with
subkinds `PGPSignature` (RFC 4880/9580, PGP-MIME) and `SMIMESignature` (RFC 8551).
Re-homed beside the keys it references in the dependency refactor; the
email-wire half (DKIM, Authentication-Results, relay hops) lives in the email extension.

### gmeow:signedBy · gmeow:signingKey

The identity and the key — exactly the pair a `Certification` attests. `signedBy` gives
the agent, `signingKey` (functional) gives the `CryptographicKey`; `signatureAlgorithm`
and `signingDomain` (the DKIM d= tag) describe the mechanism.

### gmeow:verificationStatus

The recorded verification outcome — verified, failed, or unverified. A *report* of a
computation done outside the graph (Principle 12), never an entailment: the reasoner
neither verifies signatures nor propagates their validity.

## Dependencies

Depends on `accounts` (the OnlineAccount seam) and `kernel`. Consumed wherever identity
must be vouched for: contacts, accounts, email wire-authentication, and the GTS packages'
COSE attestation chain (Principle 14).
