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

The slice exercises the standpoint doctrine that governs every contested-fact
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
follow, and with what weight. Depth remains an integer policy limit. Amount now
points to an `IntroducerWeight`, a `math:Quantity` with one exact integer
`math:quantityValue`, `math:dimensionless` and the explicit
`openPgpIntroducerAmountFrame`. The amount remains optional on an assertion. The frame uses the existing scalar
axis and frame kind, mathematical realm, crisp determinacy, one dimension and
no hosting requirement; the places owner validates the full frame profile.

[OpenPGP section 5.2.3.21](https://datatracker.ietf.org/doc/html/rfc9580#section-5.2.3.21)
defines an amount octet from 0 through 255: values below 120 represent partial
trust and values at least 120 complete trust, with 60 and 120 recommended for
emission. These are protocol weights, preserved exactly; dividing by 120 or 255
would invent a probability. `IntroducerWeight` is disjoint from
`math:ProbabilityValue` even for the numbers 0 and 1. Separate weight and
probability objects may legitimately carry the same number. Integer type, range, single magnitude, dimension
and frame are authored as canonical `logic:` axioms and a range constraint;
validation shapes are generated.

These are *inputs* to a Web-of-Trust validity computation
that happens in the projection layer — represent inputs and outputs, never compute the
metric in OWL (Principle 12).

### gmeow:endorses

The flat convenience shortcut for "vouches for" — deliberately directional (not
symmetric) and not transitive. Promote to a `TrustAssertion` when the trust needs a
level, a window, or its own identity: the flat↔reified pairing in its standard form.

## Signatures

`gmeow:boundToKey` records a claimed vantage-to-key binding without making
possession or an untrusted assertion into authorization. The
[identity and disclosure guide](../../../docs/identity-and-disclosure.md) shows
how a verified signer, a certified binding and an observation's vantage remain
separate, including when a refiner signs a derived claim.

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

Depends on `accounts` (the OnlineAccount seam), `kernel`, `places` (the reference-frame owner), and `math` (the canonical quantity and dimension owners). Consumed wherever identity
must be vouched for: contacts, accounts, email wire-authentication, and the GTS packages'
COSE attestation chain (Principle 14).

## Evidence and confidence boundary

Evidence classes, source tiers, coverage and independence remain categorical
citation annotations. Categorical owner-trust and authority are separate from
this numeric protocol input. Statement confidence stays on the existing
`gmeow:confidence` annotation and canonical `logic:confidence` surface; neither
an introducer weight nor a confidence annotation supplies a probability model.
Actual probabilistic claims use the canonical probability layer with their
model and event context. No evidence-specific numeric confidence property or
probability conversion is introduced here.
