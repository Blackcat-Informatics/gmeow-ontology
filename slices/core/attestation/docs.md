<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Attestation, verification, and transparency — modelling & interoperability guide

GMEOW models **attestation** as `gmeow:Attestation`, a `logic:Relator` that reifies a
signed claim envelope.  The module is intentionally **cross-cutting**: it is not tied to
any single domain (identity, messaging, supply chain, or publishing) and is designed to
be used as a common substrate for any scenario that needs provenance-bearing assertions,
verification activities, and append-only transparency logs.

The design follows GMEOW Principle 12 (solver boundary): the model gives you *what to
assert* and *how to record verification*, not a decision procedure for whether an
assertion is true.

## Core classes

| GMEOW class | Local grounding | What it represents |
|---|---|---|
| `gmeow:Attestation` | `logic:Relator` | The reified assertion envelope (who said what, when, with what evidence). |
| `gmeow:AttestationArtifact` | `gmeow:InformationObject` | A document or payload that carries the attestation (credential, certificate, signed statement, VC, etc.). |
| `gmeow:AttestationPolicy` | `logic:AbstractIndividualType` | Rules under which an attestation was issued (open value vocabulary). |
| `gmeow:VerificationActivity` | `gmeow:Activity` | The act of checking an attestation against its policy. |
| `gmeow:VerificationResult` | `gmeow:InformationObject` | The outcome of a verification activity, including the final status. |
| `gmeow:TransparencyLogEntry` | `gmeow:InformationObject` | A single append-only record in a transparency log (Rekor entry, SCITT receipt, CT inclusion proof, etc.). |

## Value vocabularies

| Vocabulary | Purpose |
|---|---|
| `gmeow:AttestationType` | What kind of attestation this is (`slsaProvenance`, `inToto`, `verifiableCredential`, `c2pa`, `eat`, `signedRdf`, `scitt`, `nanopublication`, `blockchainClaim`, `gitSignedTag`, `releaseManifest`, `qualityReport`, `aiOutput`, `conformanceVerdict`). |
| `gmeow:SignatureScheme` | The cryptographic algorithm (`rsaSha256`, `ed25519`, `ecdsaSecp256k1`, `ecdsaP256`, `bls12-381`). |
| `gmeow:VerificationStatus` | Outcome of verification (`verified`, `failed`, `unverified`, `expired`, `revoked`, `policyFailed`, `finalityPending`). |
| `gmeow:LedgerFinalityStatus` | Finality state of a ledger transaction/block (`pending`, `confirmed`, `finalized`, `orphaned`, `reorged`). |

## Key properties

### Attestation → world

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:attestationType` | `Attestation` | `AttestationType` | What kind of assertion envelope this is. |
| `gmeow:attestedSubject` | `Attestation` | `Entity` | What the attestation is *about*. |
| `gmeow:attestedClaim` | `Attestation` | `Observation` | The specific claim being asserted. |
| `gmeow:attester` | `Attestation` | `Agent` | Who issued the attestation (functional). |
| `gmeow:issuedAt` | `Attestation` | `xsd:dateTime` | When the attestation was issued (functional). |
| `gmeow:attestationArtifact` | `Attestation` | `AttestationArtifact` | The carrying document/serialization. |
| `gmeow:attestationPolicy` | `Attestation` | `AttestationPolicy` | Rules governing this attestation. |
| `gmeow:hasAttestation` | `Entity` | `Attestation` | Inverse of `attestedSubject`. |
| `gmeow:hasSignature` | *(universal)* | `CryptographicSignature` | Cross-cutting signature pointer (also used by messages, documents, etc.). |

### Artifact

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:artifactMediaType` | `AttestationArtifact` | `rdfs:Literal` | Media type of the serialization (e.g. `application/vc+ld+json`, `application/vnd.in-toto+json`). |

### Verification

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:verificationActivity` | `Attestation` | `VerificationActivity` | The verification activity performed on an attestation. |
| `gmeow:verificationResult` | `Attestation` | `VerificationResult` | The result of a verification activity. |
| `gmeow:verifiedBy` | `VerificationResult` | `Agent` | The agent that produced the verification result. |
| `gmeow:hasVerificationStatus` | `VerificationResult` | `VerificationStatus` | Final categorical outcome. |

### Transparency & ledger

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:transparencyLogEntry` | `Attestation` | `TransparencyLogEntry` | A transparency log entry associated with an attestation. |
| `gmeow:logEntryUrl` | `TransparencyLogEntry` | `rdfs:Literal` | URL at which the entry can be retrieved. |
| `gmeow:logEntryIndex` | `TransparencyLogEntry` | `xsd:integer` | Sequential index within its log (functional). |
| `gmeow:ledgerInclusionProof` | *(universal)* | `rdfs:Literal` | Cryptographic inclusion proof (domain-free). |
| `gmeow:confirmationDepth` | *(universal)* | `xsd:integer` | Number of confirming blocks (domain-free). |
| `gmeow:finalityStatus` | *(universal)* | `LedgerFinalityStatus` | Finality state (domain-free). |

### Signature (reused from the trust module)

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `gmeow:signedBy` | `CryptographicSignature` | `Agent` | The signing identity. |
| `gmeow:signingKey` | `CryptographicSignature` | `rdfs:Literal` | The key that produced the signature. |
| `gmeow:signatureAlgorithm` | `CryptographicSignature` | `rdfs:Literal` | Algorithm identifier. |

### Cross-cutting reuse (not redefined in this module)

The attestation module reuses existing cross-cutting properties rather than minting
new IRIs (Principle 4):

| What you need | Use |
|---|---|
| Expiry / validity window | `gmeow:validFrom` / `gmeow:validUntil` (temporal module) |
| Content identity | `gmeow:contentDigest` (sources module) |
| Version fingerprint | `gmeow:versionFingerprint` (versions module) |
| Confidence | `gmeow:confidence` (provenance module) |
| Standpoint | `gmeow:accordingTo` / `gmeow:standpointModality` (standpoint module) |
| Suppression | `gmeow:displayable false` (core module, Principle 10) |
| Activity provenance | `gmeow:wasGeneratedBy` / `gmeow:wasAttributedTo` (provenance module) |

## Signature model

`gmeow:hasSignature` is intentionally **domain-free** (no `rdfs:domain`).  Anything that
can carry a cryptographic signature — a message, a document, an attestation artifact, a
transparency-log entry — uses the same property.  The *kind* of signature is expressed
through the range class:

- `gmeow:CryptographicSignature` — abstract superclass.
- `gmeow:DKIMSignature` — email domain-key signature (lives in the email slice; specializes trust's `CryptographicSignature`).
- `gmeow:SMIMESignature` — S/MIME email signature.
- `gmeow:PGPSignature` — OpenPGP signature.

Signature metadata (signer key, algorithm) is on the signature object itself via
`gmeow:signedBy`, `gmeow:signingKey`, and `gmeow:signatureAlgorithm`.

## What Attestation is **not**

- **Not a truth guarantee.** `Attestation` records *who asserted what*; truth is a
  standpoint-indexed claim (`gmeow:StandpointClaim`) handled by the coreference module.
- **Not a replacement for Certification.** `gmeow:Certification` (trust module) binds a
  public key to an identity via a `gmeow:certifier`; its property shape is different from
  `Attestation` (which uses `attester`/`attestedSubject`/`attestedClaim`).  Certification is
  documented as a *specialisation* of attestation via `skos:scopeNote`, not a subclass.
- **Not a trust score.** Verification yields a discrete status (`verified`/`failed`/
  `revoked`/`expired`/`unverified`), not a numeric confidence.  Confidence values, when
  needed, are attached to the underlying `Observation` or `StandpointClaim`.

## Example: a self-issued verifiable credential

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/id/> .

ex:vcOver18 a gmeow:Attestation ;
    gmeow:attestationType    gmeow:attestationTypeVerifiableCredential ;
    gmeow:attestedSubject    ex:alice ;
    gmeow:attestedClaim      ex:claimAliceOver18 ;
    gmeow:attester           ex:alice ;
    gmeow:issuedAt           "2025-06-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:validUntil         "2026-06-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:attestationArtifact ex:vcArtifact ;
    gmeow:hasSignature       ex:sigAlice .

ex:vcArtifact a gmeow:AttestationArtifact ;
    gmeow:artifactMediaType "application/vc+ld+json" .

ex:sigAlice a gmeow:CryptographicSignature ;
    gmeow:signedBy           ex:alice ;
    gmeow:signingKey         "did:example:alice#keys-1" ;
    gmeow:signatureAlgorithm "ed25519" .

ex:slsaVerify a gmeow:VerificationActivity .

ex:slsaVerifyResult a gmeow:VerificationResult ;
    gmeow:verifiedBy         ex:verifierAgent ;
    gmeow:hasVerificationStatus gmeow:verificationStatusVerified .
```

## Example: certificate-transparency inclusion

```turtle
ex:ctEntry a gmeow:TransparencyLogEntry ;
    gmeow:logEntryUrl       "https://ct.example/entries/42" ;
    gmeow:logEntryIndex     42 ;
    gmeow:ledgerInclusionProof "base64:proofData…" .
```

## Release-as-evidence: GMEOW dogfoods its own attestations

GMEOW's own signed release is the flagship use of this slice (CONSTITUTION.md §18,
the *release-as-evidence* clause). `make full-release` runs the native authority gate
and the public conformance + perf suites, then folds **every** result into a single
signed `gmeow.gts` bundle: each
artifact rides as a BLAKE3-content-addressed blob, described by a `gmeow:Attestation`
envelope in a dedicated `graph/attestations` named graph that binds the envelope to its
blob by `gmeow:contentDigest`. One `AttestationType` individual exists for the evidence
kind the prior vocabulary could not name:

- `gmeow:attestationTypeConformanceVerdict` — the public conformance-suite verdicts,
  rolled up by `make conformance-report` (a Rust `gmeow-conformance` binary that runs
  every case through the native cores and emits one deterministic canonical-JSON
  artifact of per-case verdicts + certification).

The compliance report, perf results, and SHACL/diagnostics SARIF reuse
`gmeow:attestationTypeQualityReport`; the bundle itself reuses
`gmeow:attestationTypeSignedRDF`; the top-level envelope is a
`gmeow:attestationTypeReleaseManifest`. Each child attestation carries
`gmeow:contentDigest`, `gmeow:issuedAt`, `gmeow:hasSignature` (Ed25519), and a
`gmeow:verificationResult` — vouching that a given check *ran over given bytes*, never
that the ontology is "true" (Principle 9). The worked shape is
[`examples/release-evidence-bundle.ttl`](./examples/release-evidence-bundle.ttl), which
is exactly the RDF the Rust release stage emits into the bundle. `make verify-release`
is the consumer half, in Rust: native COSE signature + trust-policy verification, then a
walk of the attestation frames asserting every attested `gmeow:contentDigest` resolves to
a blob actually present in the bundle — so a consumer confirms exactly which checks ran
over which bytes, not merely that *something* was signed. A verified bundle is published
by the user-driven `make release-publish` (a content-addressed GitHub release plus the
Crossref concept-DOI deposit; signing and submission stay maintainer-credentialed). The
fold is reproducible: release timestamps are injected, blobs and frames are
content-hash-sorted, and perf timings ride as data, never as a gate.

## Interoperability layers

1. **Term alignment** — `mappings/gmeow-attestation.sssom.tsv` maps to PROV-O, W3C
   Verifiable Credentials, W3C DID, W3C Web of Things, and W3C DQV.
2. **SSSOM + EDOAL** — generated from `slices/core/attestation/mappings/equivalences.ttl`.
   PROV-O `Entity` ↔ `AttestationArtifact`, `Activity` ↔ `VerificationActivity`,
   `wasGeneratedBy` ↔ `hasSignature` / `signedBy`, etc.
3. **Refused mappings** — in-toto, SLSA, DSSE, Sigstore, SCITT, C2PA, RATS/EAT, and
   nanopublications are acknowledged as related but **not** aligned because their
   granularity or property shapes differ from GMEOW's relator-based model.  Users who
   need those vocabularies should bridge at the application layer or via custom
   projection mappings.
4. **Round-trip** — VC `credentialSubject` maps to `attestedSubject`;
   `issuanceDate`/`expirationDate` map to `issuedAt`/`validUntil`;
   `proof` maps to `hasSignature` → `CryptographicSignature`.
5. **Contested / revoked** — a revoked attestation does not disappear.  It gains a
   `VerificationResult` with `verificationStatusRevoked`, and the original
   `Attestation` remains in the graph with its history intact.  For suppression of
   superseded labels (e.g. a corrected issuer name), use `gmeow:displayable false`.

## Terms

The classes, properties, and value vocabularies this module declares, anchored to
the model above. Signature metadata (`signedBy`, `signingKey`,
`signatureAlgorithm`, `CryptographicSignature`) and the validity/identity
cross-cutters are reused from the trust and temporal modules, not redefined here.

### gmeow:Attestation · gmeow:attester · gmeow:attestedSubject · gmeow:attestedClaim · gmeow:hasAttestation

The core relator: a reified envelope recording that an `gmeow:attester` (one,
functional) vouches for a bare `gmeow:attestedSubject` entity and/or an
`gmeow:attestedClaim` observation, under a policy and with evidence. An entity
reaches its attestations through the inverse `gmeow:hasAttestation`; competing
attestations coexist (Principle 9) and an attestation proves who said what, never
that it is true.

### gmeow:attestationType · gmeow:AttestationType · gmeow:attestationPolicy · gmeow:AttestationPolicy · gmeow:issuedAt

The envelope's classification and provenance: `gmeow:attestationType` names one or
more `gmeow:AttestationType` kinds (SLSA provenance, in-toto, VC, DSSE, C2PA,
nanopublication…); `gmeow:attestationPolicy` names the `gmeow:AttestationPolicy`
rules the attester followed; `gmeow:issuedAt` stamps the single issue instant
(functional). Validity rides the temporal module's `validFrom`/`validUntil`.

### gmeow:AttestationArtifact · gmeow:attestationArtifact · gmeow:artifactMediaType · gmeow:hasSignature · gmeow:signatureOf

The concrete carrier: an `gmeow:AttestationArtifact` is the serialized document
(in-toto JSON, DSSE envelope, VC, C2PA manifest) linked from the logical
attestation by `gmeow:attestationArtifact` and typed by `gmeow:artifactMediaType`.
`gmeow:hasSignature` is the domain-free universal spine attaching a cryptographic
signature to anything signed, with `gmeow:signatureOf` its inverse — a valid
signature proves integrity and key control, never truth.

### gmeow:VerificationActivity · gmeow:VerificationResult · gmeow:verificationActivity · gmeow:verificationResult · gmeow:verifiedBy · gmeow:hasVerificationStatus · gmeow:VerificationStatus

Verification as act and outcome: a `gmeow:VerificationActivity` is the checking
event, producing a `gmeow:VerificationResult` (`gmeow:verifiedBy` an agent,
carrying `gmeow:hasVerificationStatus` from the `gmeow:VerificationStatus`
vocabulary — verified / failed / expired / revoked / policy-failed /
finality-pending). A result is one verifier's observation under one policy, not a
global verdict; competing results coexist (Principles 9, 12).

### gmeow:TransparencyLogEntry · gmeow:transparencyLogEntry · gmeow:logEntryIndex · gmeow:logEntryUrl

Transparency-log evidence: a `gmeow:TransparencyLogEntry` (Rekor entry, SCITT
receipt, CT inclusion proof) linked via `gmeow:transparencyLogEntry`, identified by
`gmeow:logEntryIndex` (functional, log-local) and retrievable at
`gmeow:logEntryUrl`. Inclusion proves it was logged, not that it is correct
(Principle 1).

### gmeow:LedgerTransaction · gmeow:LedgerEvent · gmeow:Block · gmeow:BlockchainNetwork · gmeow:SmartContract · gmeow:BlockchainAccount

Ledger/blockchain evidence: a `gmeow:LedgerTransaction` (information-object view of
a payload) and the on-chain `gmeow:LedgerEvent` it emits, located by `gmeow:Block`
on a `gmeow:BlockchainNetwork`, with `gmeow:SmartContract` and
`gmeow:BlockchainAccount` as the deployed program and the address-controlled
signing identity. Ledger inclusion proves inclusion under chain rules, not
real-world truth.

### gmeow:transactionHash · gmeow:blockHash · gmeow:blockNumber · gmeow:chainId · gmeow:contractAddress · gmeow:signatureRecoveryAddress · gmeow:logIndex · gmeow:confirmationDepth · gmeow:finalityStatus · gmeow:LedgerFinalityStatus · gmeow:ledgerInclusionProof · gmeow:SignatureScheme

The ledger identity and finality facets: hashes and identifiers
(`gmeow:transactionHash`, `gmeow:blockHash`, `gmeow:blockNumber`, `gmeow:chainId`,
`gmeow:contractAddress`, `gmeow:signatureRecoveryAddress`, `gmeow:logIndex`),
settlement state (`gmeow:confirmationDepth`, `gmeow:finalityStatus` over
`gmeow:LedgerFinalityStatus` — pending / confirmed / finalized / orphaned /
reorged), the domain-free `gmeow:ledgerInclusionProof`, and the
`gmeow:SignatureScheme` vocabulary of cryptographic algorithms. Finality checks run
in the solver layer (Principle 12).

## See also

- `slices/core/attestation/module.ttl` — canonical module source.
- `slices/core/attestation/mappings/equivalences.ttl` — mapping DSL alignments.
- `slices/core/attestation/tests/structural.ttl` — structural slice tests.
- `crates/validate/tests/conformance_cases/ontology_conformance.rs` — native negative fixture tests.
- `slices/extensions/email/docs.md` — how `hasSignature` is used for DKIM / S/MIME / PGP in email.
- `slices/core/trust/module.ttl` — `gmeow:Certification`, the key-to-identity specialisation.
