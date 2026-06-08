<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Attestation, verification, and transparency — modelling & interoperability guide

GMEOW models **attestation** as `gmeow:Attestation`, a `gufo:Relator` that reifies a
signed claim envelope.  The module is intentionally **cross-cutting**: it is not tied to
any single domain (identity, messaging, supply chain, or publishing) and is designed to
be used as a common substrate for any scenario that needs provenance-bearing assertions,
verification activities, and append-only transparency logs.

The design follows GMEOW Principle 12 (solver boundary): the model gives you *what to
assert* and *how to record verification*, not a decision procedure for whether an
assertion is true.

## Core classes

| GMEOW class | gUFO kind | What it represents |
|---|---|---|
| `gmeow:Attestation` | `gufo:Relator` | The reified assertion envelope (who said what, when, with what evidence). |
| `gmeow:AttestationArtifact` | `gufo:InformationObject` | A document or payload that carries the attestation (credential, certificate, signed statement, VC, etc.). |
| `gmeow:AttestationPolicy` | `gufo:InformationObject` | Rules under which an attestation was issued (acceptable issuers, validity windows, revocation policy, etc.). |
| `gmeow:VerificationActivity` | `gmeow:Activity` | The act of checking an attestation against its policy. |
| `gmeow:VerificationResult` | `gufo:InformationObject` | The outcome of a verification activity, including the final status. |
| `gmeow:TransparencyLogEntry` | `gufo:InformationObject` | A single append-only record in a transparency log (CT-like inclusion proof, signed timestamp, etc.). |

## Value vocabularies

| Vocabulary | Purpose |
|---|---|
| `gmeow:AttestationType` | What kind of attestation this is (`credential`, `certificate`, `endorsement`, `delegation`, `relinquishment`). |
| `gmeow:AttestationMethod` | How the attestation was produced (`digitalSignature`, `biometric`, `notarisedWitness`, `timestampedArchive`). |
| `gmeow:VerificationStatus` | Outcome of verification (`verified`, `failed`, `revoked`, `expired`, `unknown`). |
| `gmeow:ArtifactFormat` | Serialization of the artifact (`jsonLd`, `cbor`, `x509`, `pem`, `turtle`, `binary`). |
| `gmeow:LedgerKind` | What kind of transparency log (`certificateTransparency`, `sigstoreRekor`, `verifiableDataRegistry`, `supplyChain`, `identityAnchor`). |

## Key properties

### Attestation → world

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:attestationType` | `Attestation` | `AttestationType` | What kind of assertion envelope this is. |
| `gmeow:attestationMethod` | `Attestation` | `AttestationMethod` | How it was produced. |
| `gmeow:attestedSubject` | `Attestation` | `Entity` | What the attestation is *about*. |
| `gmeow:attestedClaim` | `Attestation` | `Statement` | The specific claim being asserted. |
| `gmeow:attestedAt` | `Attestation` | `xsd:dateTime` | When the attestation was issued. |
| `gmeow:attestationExpires` | `Attestation` | `xsd:dateTime` | Expiry, if any. |
| `gmeow:attestationPolicy` | `Attestation` | `AttestationPolicy` | Rules governing this attestation. |
| `gmeow:hasAttestationArtifact` | `Attestation` | `AttestationArtifact` | The carrying document. |
| `gmeow:hasSignature` | *(universal)* | `CryptographicSignature` | Cross-cutting signature pointer (also used by messages, documents, etc.). |

### Verification

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:verifiedBy` | `VerificationResult` | `VerificationActivity` | Which activity produced this result. |
| `gmeow:verificationOf` | `VerificationActivity` | `Attestation` | What was being verified. |
| `gmeow:verificationMethod` | `VerificationActivity` | `VerificationMethod` | How verification was performed (value vocabulary). |
| `gmeow:verificationStarted` | `VerificationActivity` | `xsd:dateTime` | Start timestamp. |
| `gmeow:verificationCompleted` | `VerificationActivity` | `xsd:dateTime` | End timestamp. |
| `gmeow:hasVerificationStatus` | `VerificationResult` | `VerificationStatus` | Final outcome. |

### Transparency

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:logEntryFor` | `TransparencyLogEntry` | `Attestation` | Which attestation this entry records. |
| `gmeow:logEntryTimestamp` | `TransparencyLogEntry` | `xsd:dateTime` | Monotonic timestamp. |
| `gmeow:logEntryIndex` | `TransparencyLogEntry` | `xsd:integer` | Position in the ledger sequence. |
| `gmeow:previousLogEntry` | `TransparencyLogEntry` | `TransparencyLogEntry` | Chain predecessor. |
| `gmeow:logEntrySignature` | `TransparencyLogEntry` | `CryptographicSignature` | Log operator's signature. |

## Signature model

`gmeow:hasSignature` is intentionally **domain-free** (no `rdfs:domain`).  Anything that
can carry a cryptographic signature — a message, a document, an attestation artifact, a
transparency-log entry — uses the same property.  The *kind* of signature is expressed
through the range class:

- `gmeow:CryptographicSignature` — abstract superclass.
- `gmeow:DKIMSignature` — email domain-key signature (lives in `messaging-trust`).
- `gmeow:SMIMESignature` — S/MIME email signature.
- `gmeow:PGPSignature` — OpenPGP signature.

Signature metadata (signer key, algorithm, literal verification status) is on the
signature object itself via `gmeow:signedBy`, `gmeow:signingKey`, `gmeow:signatureAlgorithm`,
and the datatype property `gmeow:verificationStatus`.

## What Attestation is **not**

- **Not a truth guarantee.** `Attestation` records *who asserted what*; truth is a
  standpoint-indexed claim (`gmeow:StandpointClaim`) handled by the coreference module.
- **Not a replacement for Certification.** `gmeow:Certification` (trust module) binds a
  public key to an identity via a `gmeow:Certifier`; its property shape is different from
  `Attestation` (which uses `attester`/`attestedSubject`/`attestedClaim`).  Certification is
  documented as a *specialisation* of attestation via `skos:scopeNote`, not a subclass.
- **Not a trust score.** Verification yields a discrete status (`verified`/`failed`/
  `revoked`/`expired`/`unknown`), not a numeric confidence.  Confidence values, when
  needed, are attached to the underlying `Statement` or `StandpointClaim`.

## Example: a self-issued verifiable credential

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/id/> .

ex:vcOver18 a gmeow:Attestation ;
    gmeow:attestationType    gmeow:attestationTypeCredential ;
    gmeow:attestationMethod  gmeow:attestationMethodDigitalSignature ;
    gmeow:attestedSubject    ex:alice ;
    gmeow:attestedClaim      ex:claimAliceOver18 ;
    gmeow:attestedAt         "2025-06-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:attestationExpires "2026-06-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:hasAttestationArtifact ex:vcArtifact ;
    gmeow:hasSignature       ex:sigAlice .

ex:vcArtifact a gmeow:AttestationArtifact ;
    gmeow:artifactFormat gmeow:artifactFormatJsonLd ;
    gmeow:artifactUrl    "https://example.org/vc/over18.jsonld"^^xsd:anyURI .

ex:sigAlice a gmeow:CryptographicSignature ;
    gmeow:signedBy           ex:alice ;
    gmeow:signingKey         "did:example:alice#keys-1" ;
    gmeow:signatureAlgorithm "Ed25519" ;
    gmeow:verificationStatus "unverified" .

ex:verifyVc a gmeow:VerificationActivity ;
    gmeow:verificationOf     ex:vcOver18 ;
    gmeow:verificationMethod gmeow:verificationMethodDigitalSignature ;
    gmeow:verificationStarted  "2025-06-02T12:00:00Z"^^xsd:dateTime ;
    gmeow:verificationCompleted "2025-06-02T12:00:01Z"^^xsd:dateTime .

ex:verifyResult a gmeow:VerificationResult ;
    gmeow:verifiedBy         ex:verifyVc ;
    gmeow:hasVerificationStatus gmeow:verificationStatusVerified .
```

## Example: certificate-transparency inclusion

```turtle
ex:ctEntry a gmeow:TransparencyLogEntry ;
    gmeow:logEntryFor        ex:certAttestation ;
    gmeow:ledgerKind         gmeow:ledgerKindCertificateTransparency ;
    gmeow:logEntryTimestamp  "2025-06-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:logEntryIndex      42 ;
    gmeow:previousLogEntry   ex:ctEntry41 ;
    gmeow:logEntrySignature  ex:ctOperatorSig .
```

## Interoperability layers

1. **Term alignment** — `mappings/gmeow-attestation.sssom.tsv` maps to PROV-O, W3C
   Verifiable Credentials, W3C DID, W3C Web of Things, and W3C DQV.
2. **SSSOM + EDOAL** — generated from `mapping-dsl/equivalences/attestation.ttl`.
   PROV-O `Entity` ↔ `AttestationArtifact`, `Activity` ↔ `VerificationActivity`,
   `wasGeneratedBy` ↔ `hasSignature` / `signedBy`, etc.
3. **Refused mappings** — in-toto, SLSA, DSSE, Sigstore, SCITT, C2PA, RATS/EAT, and
   nanopublications are acknowledged as related but **not** aligned because their
   granularity or property shapes differ from GMEOW's relator-based model.  Users who
   need those vocabularies should bridge at the application layer or via custom
   projection mappings.
4. **Round-trip** — VC `credentialSubject` maps to `attestedSubject`;
   `issuanceDate`/`expirationDate` map to `attestedAt`/`attestationExpires`;
   `proof` maps to `hasSignature` → `CryptographicSignature`.
5. **Contested / revoked** — a revoked attestation does not disappear.  It gains a
   `VerificationResult` with `verificationStatusRevoked`, and the original
   `Attestation` remains in the graph with its history intact.  For suppression of
   superseded labels (e.g. a corrected issuer name), use `gmeow:displayable false`.

## See also

- `ontology/modules/attestation.ttl` — canonical module source.
- `mapping-dsl/equivalences/attestation.ttl` — mapping DSL alignments.
- `tests/test_attestation.py` — structural and negative tests.
- `docs/email-mapping.md` — how `hasSignature` is used for DKIM / S/MIME / PGP in email.
- `ontology/modules/trust.ttl` — `gmeow:Certification`, the key-to-identity specialisation.
