<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Address identity, policy findings and reader views

These patterns separate address identity, observation attribution, signature
verification, admission findings and reader disclosure. They compose without
turning a policy judgment into an unattributed fact (Principles 4, 9, 10 and 12).

## Address identity belongs to an explicit normalization policy

`gmeow:addressValue` carries an EmailAddress's normalized addr-spec and is
functional. It is not a global identity key. A store may derive an address node's
identifier from the pair **normalization-policy identity and normalized value**.
The policy must specify the transformation and retain the original spelling as
source evidence. Equality under two different normalization policies is not
implied. Do not silently add transformations when looking up an existing address.

Two source address nodes are not automatically merged by the ontology. A store
that canonicalizes its own address identifiers records the source-to-store
correspondence. Equal address values do not identify people: shared channels,
delegation and successive holders remain separate agents and address tenures.
This selects store-local identity without adding an OWL key or global equality.

## A policy finding is another attributed observation

Use `gmeow:PolicyFinding`, with the existing observation roles:

| Role | Value |
| --- | --- |
| `gmeow:vantage` | The named, versioned policy process. |
| `gmeow:observedFeature` | The exact submitted claim or write attempt. |
| `gmeow:observationResult` | An explicit finding kind owned by that policy. |

A serialized record hidden in a literal, an untagged prose value and an
unauthorized speaker attribution can have distinct policy-owned result kinds.
Keep the responsible writing process and its source evidence reachable from the
submitted claim. A recording-only admission policy retains the write and reports
the finding to its operator; another policy may refuse admission. The vocabulary
does not force either operational decision or silently alter the submitted claim.

A realized `logic:Constraint` and its generated validation result can supply
evidence for this observation. They remain distinguishable: a policy finding is
the policy's judgment, while a native constraint result identifies its actual
constraint, focus, failure class and derivation. Retain that result as evidence;
do not manufacture a constraint violation from prose or claim that every policy
finding was proved by the reasoner.

## Bind the vantage to a key; keep the signer separate

The existing signature spine is sufficient for attaching signatures to claims:

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/identity/> .

ex:observer gmeow:boundToKey ex:observerKey .
ex:original gmeow:vantage ex:observer ;
    gmeow:hasSignature ex:originalSignature .
ex:originalSignature a gmeow:CryptographicSignature ;
    gmeow:signedBy ex:observer ; gmeow:signingKey ex:observerKey .

ex:refined gmeow:vantage ex:observer ;
    gmeow:wasDerivedFrom ex:original ; gmeow:hasSignature ex:refinerSignature .
ex:refinerSignature a gmeow:CryptographicSignature ;
    gmeow:signedBy ex:refiner ; gmeow:signingKey ex:refinerKey .
```

The original claim is supplied by its observer. Re-expression retains that source
and derivation; the refiner signs its own output and does not become the observer
or acquire authority to author the observer's first-person statements.
`signedBy` belongs on the signature, not directly on the claim. `hasSignature`
links them. `boundToKey` names a CryptographicKey entity rather than an ambiguous
short identifier; dated, standpoint-indexed bindings and key rotation can coexist.
Use the existing Certification relator for a certified agent-key binding.

An assertion of `boundToKey` grants no authority by itself. The relying policy
checks an authorized, currently applicable binding, the signature over the exact
claim or its explicitly identified envelope, and the actual verification result.
It must bind any named standpoint as well as the signing agent when the vantage
has its own frame identity. Missing evidence is an explicit policy finding or
admission failure, never an inferred self-assertion. Cryptographic verification
and authorization are consumer operations, not entailments of an RDF link.

## Record decisions per reader and protect the decisions too

A `gmeow:DisclosureDecision` uses `observedFeature` for the protected claim,
`disclosureReader` for the reader principal, `vantage` for the versioned policy,
and `observationResult` for an explicit `DisclosureOutcome`.

Each decision requires exactly one reader and one typed outcome. These are local
constraints on DisclosureDecision; ordinary observations retain their multivalued
results, and separate decision nodes retain different readers and policy versions.

The seeded outcomes are:

| Outcome | Reader-visible behavior |
| --- | --- |
| `disclosureReleased` | Emit the authorized projection, including any required coarsening. |
| `disclosureVisibleRedaction` | Withhold content but permit the policy-selected indication of withholding. |
| `disclosureNonAcknowledgement` | Reveal neither content nor the fact that anything was withheld. |

The same claim may have different decisions for different readers. A missing
decision or outcome does not imply release, visible redaction or
non-acknowledgement; the consumer must obtain a decision under its selected policy.
Sensitivity, disclosure posture, coarsening and these results remain separate
axes. No global `displayable` edit substitutes for a reader-relative view.

Decision records are policy-side evidence. In a non-acknowledging view, never emit
the decision itself, a hidden-claim identifier, a redaction marker, a provenance
edge, or a count/error/pagination difference that acknowledges the hidden claim.
Apply the policy to the complete outward surface, not just literal values.
Visible redaction also needs explicit permission to disclose existence.

The worked [policy and disclosure records](../slices/core/observations/examples/policy-and-disclosure.ttl)
are private decision inputs, not an example of a safe public export. The owning
slice's exact-row competencies check reader separation and explicit result kinds;
they do not claim that an external store has implemented its disclosure boundary.
