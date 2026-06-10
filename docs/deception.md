<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Deception — contested divergence as coexisting standpoint claims

GMEOW models deception, falsehood, and misinformation as **standpoint-indexed divergences** between what a party holds true and what they project — never as a global truth verdict. This is the doctrine document for the deception facility (`ontology/modules/deception.ttl`); its companion is the mapping-dsl equivalences in `mapping-dsl/equivalences/deception.ttl` and the FnO projection functions in `projections/functions.fno.ttl`.

## The no-`isFalse` doctrine

There is **no** `isFalse` or `isDeceptive` axiom and no boolean truth datatype property in GMEOW. Truth and intent are **never entailed** by the reasoner (Principles 1, 12). Instead:

- A **falsehood** is a frame-relative `gmeow:StandpointClaim` whose `claimModality = gmeow:refuted` — settled-false *per a designated reference frame*.
- **Deceptiveness** is asserted as a standpoint-indexed, attributed, confidence-weighted claim that **coexists with its denial** (Principle 9).
- A **deception** is the divergence `held-standpoint ≠ projected-standpoint`, both modelled as ordinary `StandpointClaim`s; the gap is the act, not a truth verdict.

This design dissolves the "who decides what is fake news?" problem structurally: every assessment of deceptiveness is itself a standpoint-indexed claim, co-equal with its contradiction. There is nothing to overwrite, so nothing to revert.

## The four-family taxonomy

GMEOW distinguishes four related but non-overlapping phenomena:

| Phenomenon | Structure | Intent | Truth-orientation |
|---|---|---|---|
| **Lying** | `heldStandpoint` = refuted, `projectedStandpoint` = unequivocal | present (cares about truth, asserts opposite) | truth-directed |
| **Paltering** | `heldStandpoint` = unequivocal, `projectedStandpoint` = unequivocal, `implicates` = refuted | present (misleading by truthful implication) | truth-directed |
| **Bullshitting** | `heldStandpoint` = bullshit, `projectedStandpoint` = bullshit | absent (indifference to truth) | truth-indifferent |
| **Deception-by-omission** | no `projectedStandpoint`, withheld `heldStandpoint` | present | truth-directed |

Bullshit (Frankfurt) is modelled as a **fifth standpoint modality** (`gmeow:bullshit`) alongside unequivocal, probable, conceivable, and refuted. It is distinct from lying because the bullshitter does not care whether the claim is true or false — they assert it with indifference to its correspondence with reality.

## The divergence model

A deception event carries at minimum two standpoint claims:

- `gmeow:heldStandpoint` → what the deceiver actually believes.
- `gmeow:projectedStandpoint` → what the deceiver communicates.

The divergence between them is the deceptive act. Both claims are ordinary `StandpointClaim`s, indexed to their respective standpoints, carrying `gmeow:confidence` and `gmeow:wasAttributedTo`. Neither is privileged.

## Intent outside the logic

`gmeow:deceptiveIntentClaim` is a `StandpointClaim` about the event, not an axiom of the event. It records that *some assessor* attributes deceptive intent to the event, with a confidence and a vantage. Another assessor may attribute no intent, or attribute benevolent intent — these claims coexist (Principle 9). Intent is never entailed because it is a mental state inaccessible to the reasoner (Principle 12).

## Self-deception

Self-deception is a structural configuration, not a special class:

- `roleDeceiver` = `roleDeceived` (the same entity plays both roles).
- The avowed (conscious) standpoint and the tacit (unconscious) standpoint are modelled as separate `StandpointClaim`s, each indexed to a sub-vantage (the conscious self vs the unconscious self).
- The divergence is the same `heldStandpoint ≠ projectedStandpoint` pattern, but the projection is to oneself.

## The licensed-falsehood safety property

Fiction, satire, and sarcasm are **not deception** because the audience understands the non-truth-asserting frame. GMEOW makes this explicit via `gmeow:veridicalityLicensedFalsehood` — a `ClaimVeridicality` value that marks a claim as false-but-harmless. This separates:

- A novel's claim that "Harry Potter attended Hogwarts" (`veridicalityLicensedFalsehood`).
- A politician's false claim about election results (`veridicalityUntrue`, potentially deceptive).

The safety property is structural, not a matter of inference: the creator or publisher asserts the licensed-falsehood frame explicitly.

## Cues and evidence

`gmeow:deceptionCue` links a deception event to an `Observation` — a behavioural, linguistic, or evidential signal. Cue-scoring and weighting live in the solver layer (Principle 12), not the ontology. The ontology records only that a cue observation exists and is linked to the event.

## Carrier deceptions — artifact and identity binding (issue #216)

Not all deceptions are speech acts. Three additional `gmeow:eventType` values cover deceptions where the held↔projected divergence is borne by a **fabricated artifact** or a **false identity binding**:

| Type | Structure | Distinctive machinery |
|---|---|---|
| **Fabrication** | `heldStandpoint` = refuted provenance, `projectedStandpoint` = genuine provenance | A false `gmeow:CreativeWork` whose attestation returns `gmeow:verificationStatusFailed` |
| **Forgery** | As fabrication, plus `counterpartOf` link to genuine work | A **specific** imitation target + failed `gmeow:CryptographicSignature` verification |
| **Impersonation** | `heldStandpoint` = refuted identity claim, `projectedStandpoint` = unequivocal identity claim | Projected `gmeow:IdentityFacet` whose `facetSubject` ≠ the deceiver |

### Fabrication vs forgery

Both involve a false artifact, but **forgery** is distinguished by two additional properties:

1. **Specificity of imitation** — the forged work bears a `gmeow:counterpartOf` link to the specific genuine work it mimics. A fabrication invents a false artifact without mimicking a specific genuine one.
2. **Signature evidence** — a forgery typically carries a failed cryptographic signature verification (DKIM, S/MIME, PGP) that exposes the forgery. The verification result is an `Observation`, not a truth axiom (Principle 12).

### Impersonation — identity as the carrier

Impersonation reuses the `gmeow:IdentityFacet` machinery from the gender/orientation/identity modules. The projected identity facet's `gmeow:facetSubject` is the victim, not the deceiver. Email spoofing is modelled as **impersonation + a failed `gmeow:AuthenticationResult`** (DKIM/SPF/DMARC fail). A **sockpuppet** is an impersonation where the deceiver controls a `gmeow:counterpartOf` identity they publicly deny controlling.

**Phishing is an instance, never a taxonomy primitive.** Following the PAPO/ROSE lesson, phishing is `eventTypeImpersonation` + a fraudulent-solicitation event — the specific combination of mechanisms, not a new type.

### Alignment

| Target | Relationship | Rationale |
|---|---|---|
| `wd:Q18387855` (tampering with evidence) | `skos:closeMatch` | Fabrication |
| `wd:Q1332286` (forgery) | `skos:closeMatch` | Forgery; shared with fabrication at lower confidence |
| `wd:Q693988` (document forgery) | `skos:closeMatch` | Document-specific forgery |
| `wd:Q2146099` (impersonation) | `skos:closeMatch` | Impersonation |
| PAPO `PhishingAttack` | `skos:relatedMatch` | Phishing is an instance of impersonation |
| ROSE `SocialEngineeringThreat` | `skos:relatedMatch` | Risk-treatment view of impersonation |

## SOTA, and how GMEOW transcends it

| Prior work | What it offers | Where it falls short | GMEOW's response |
|---|---|---|---|
| **Frankfurt, *On Bullshit*** | The bullshit modality (indifference to truth) | Informal; no formal logic or ontology | **Aligns directly** — `bullshit` as a `StandpointModality` value, fully integrated with the standpoint logic |
| **Standpoint Logic** (Gómez Álvarez & Rudolph) | `□_S`/`◊_S` operators, standpoint poset | No deception-specific divergence model | **Extends** — `heldStandpoint`/`projectedStandpoint` divergence as the core deception construct |
| **CRMinf** (CIDOC-CRM Argumentation) | `I6_Belief_Value`, `I1_Argumentation` | Heavyweight; no veridicality or licensed-falsehood | **At least as expressive** — `StandpointModality` superset + `ClaimVeridicality` + licensed-falsehood safety property |
| **schema.org ClaimReview** | Fact-check verdicts | Single-verdict model re-creates the winner slot | **Refuses single verdict** — fact-check is an `Attestation` with `VerificationResult`, not a truth axiom |
| **AIF** (Argument Interchange Format) | I/RA/CA/PA nodes for argumentation | No ontology integration | **Referenced in solver layer** — `fnArgumentAcceptability` projects to AIF by reference |

## Doctrine

- **Principle 1 — SOTA by being SOTA.** Model what should have been written: a licensed-falsehood safety property, a bullshit modality, and divergence-as-claim rather than truth-as-axiom.
- **Principle 9 — no single slot to win.** A deception assessment is a standpoint-indexed claim that coexists with its denial; there is no `isDeceptive` boolean.
- **Principle 11 — frame-relativity.** Falsehood is `refuted` per a reference frame, not globally false.
- **Principle 12 — compute outside the logic.** Cue-scoring, implicature extraction, maxim-violation detection, and argument acceptability are solver-layer computations, never OWL axioms.
