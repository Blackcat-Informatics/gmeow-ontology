<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Deception — the lie as a structural gap, never a truth bit

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/deception` · **tier: core**
> Graph-native epistemics of the lie: held ≠ projected, standpoint-indexed, with intent kept out of the logic.

Most vocabularies flag falsehood with a boolean. GMEOW refuses (Principles 1 and 12): there
is **no `isFalse`, no `isDeceptive`, and no truth datatype property** anywhere in the graph.
A falsehood is a frame-relative `gmeow:StandpointClaim` whose `claimModality` is
`gmeow:refuted` — settled-false *per a designated reference frame* — and a deception is not
a verdict at all but a **structural divergence**: an event in which the standpoint a party
*holds* differs from the standpoint they *project*. The gap is the act.

Three corollaries shape everything below. **Intent stays out of the logic** — deceptive
intent is an attributed, defeasible claim by an assessor, never entailed by the reasoner,
coexisting with its denial (Principle 9). **Deception kinds are values, never subclasses** —
each mechanism is one open `gmeow:eventType` individual (events doctrine), and the
deception-specific constraints are SHACL scoped to `gmeow:eventTypeDeception`
(DeceptionEventShape), never global OWL restrictions on `gmeow:Event` (Principle 3).
**Licensed falsehood is a safety property** — fiction, satire, and sarcasm are structurally
distinct from deception, so no pipeline ever "debunks" a novel.

## The divergence core

### gmeow:eventTypeDeception

The umbrella event kind: a participant projects a standpoint diverging from the one they
hold, inducing a false belief. A value individual on an ordinary `gmeow:Event` — birth,
marriage, and lie share one Event class, distinguished by `gmeow:eventType` alone.

### gmeow:heldStandpoint · gmeow:projectedStandpoint

The two sides of the gap, both ordinary `gmeow:StandpointClaim`s attached to the deception
event. Non-functional by design: a complex deception may hold several private positions and
project different stories to different audiences. The *relationship* between the pair —
negation (lie), implicature (paltering), absence (omission), sharpening (spin) — is what the
mechanism inventory below names.

### gmeow:deceptiveIntentClaim

Intent as a contestable attribution: a StandpointClaim whose vantage is the *assessor* (not
the deceiver), carrying `gmeow:confidence` and `gmeow:wasAttributedTo`. The reasoner never
entails intent; an accusation and its denial coexist (Principle 9).

## Veridicality

### gmeow:ClaimVeridicality · gmeow:claimVeridicality

The veridicality status of a claim, as a value vocabulary linked by the non-functional
`gmeow:claimVeridicality` — multiple assessments from different standpoints coexist. This is
an assessment axis, never a global verdict (Principle 11).

### gmeow:veridicalityUntrue · gmeow:veridicalityLicensedFalsehood

`veridicalityUntrue` is frame-relative non-truth — refuted in one standpoint, possibly
unequivocal in another. `veridicalityLicensedFalsehood` is the safety property: fiction,
satire, and sarcasm assert nothing because the audience understands the non-truth-asserting
frame — licensed, not deceptive (Principle 1).

## The mechanism inventory

### gmeow:eventTypeLie · gmeow:eventTypePaltering · gmeow:eventTypeOmission · gmeow:eventTypeDistortion · gmeow:eventTypeBullshit · gmeow:eventTypeSelfDeception

Six speech-act mechanisms, each one open `gmeow:eventType` value distinguished by *where the
gap lives*: projection negates the held claim (lie); a literally true projection implicates
a false conclusion (paltering); a held truth is never projected (omission); the projection
sharpens or re-frames — a modality shift, probable→unequivocal (distortion); the deceiver
projects certainty while holding the bullshit modality, Frankfurt's indifference to truth
(bullshit); one agent fills both roles, avowed vs tacit sub-vantage (self-deception). Each
maps to the Gricean maxim it violates.

### gmeow:MaximViolationType · gmeow:maximViolationType

The Gricean classification axis — Quality (lie, bullshit), Quantity (omission, paltering),
Relation (paltering, red herrings), Manner (distortion, obfuscation) — as a closed value
vocabulary. Assigning a violation is solver-layer classification (Principle 12).

### gmeow:implicates

The paltering hook: relates a deceptive event to the proposition its literally-true
statement misleadingly implies. Deliberately without reasoner semantics — implicature is a
pragmatics computation, not an OWL entailment (Principle 12).

## Carrier deception

### gmeow:eventTypeFabrication · gmeow:eventTypeForgery · gmeow:eventTypeImpersonation

Divergence borne by an artifact or identity binding rather than a bare utterance. A
fabrication invents a false `gmeow:CreativeWork` whose projected provenance the deceiver's
held standpoint refutes — evidenced by a failed `gmeow:VerificationResult`, never a truth
axiom. A forgery is a fabrication that mimics a *specific* genuine work
(`gmeow:counterpartOf` the original). An impersonation projects an `gmeow:IdentityFacet`
whose subject differs from the deceiver — email spoofing is impersonation with a failed
AuthenticationResult; phishing is an instance, never a taxonomy primitive.

## Participant roles

### gmeow:roleDeceiver · gmeow:roleDeceived · gmeow:roleBeneficiaryOfDeception · gmeow:roleDupe · gmeow:roleSpinDoctor

Open `gmeow:ParticipantRole` values on ordinary Participations. Self-deception is the
configuration where deceiver = deceived; the dupe is the unwitting conduit; the spin doctor
is specific to distortion events. No role implies guilt — roles are structure, intent is an
attribution.

## Propagation and the per-node boundary

### gmeow:eventTypeDisinformation

A coordinated campaign aggregating constituent deception events along the
`prov:wasDerivedFrom` / `gmeow:propagatesFrom` lineage spine. The
misinformation↔disinformation boundary is **per-node, never a global label**: at the origin
held ≠ projected (deception, `roleDeceiver`); at sincere intermediary nodes held ≈ projected
and the claim is merely untrue (`roleDupe`).

### gmeow:deceptionCue · gmeow:attestationTypeFactCheck

Evidence enters as observations: a cue is a behavioural, linguistic, or evidential signal
attached non-functionally to the event — competing standpoint-indexed cue claims coexist. A
fact-check is an attestation kind (the schema.org ClaimReview act) whose result is a
VerificationResult, not a truth assertion.

## Solver layer & deferred alignment

Everything that *scores* lives below the ontology (Principle 12): cue weighting
(`gmeow:credibilityScore`), propagation-chain analysis
(`gmeow:propagationMutationDistance`), argument evaluation (`gmeow:argumentAcceptability`,
referencing AIF), implicature derivation, and maxim classification. The slice carries the
structure those solvers read; it asserts no verdicts. Alignment is by reference — schema.org
ClaimReview via the fact-check attestation kind, AIF for argument scores — with SSSOM rows
deferred to the alignment window. DL-cleanliness is by construction: simple object
properties only, no transitivity, chains, or inverses (Principle 3).

## Bridge: aboutness (kernel, )

`gmeow:veridicalityLicensedFalsehood` (fiction, satire, sarcasm) is the
special case where the kernel's aboutness axis meets veridicality: a fictional
carrier *enacts* its content (`gmeow:hasAboutness gmeow:aboutnessEnacts`)
while asserting nothing — enactment without assertion is licensed, not
deceptive. The bridge is documentation only, deliberately: no axiom couples
`hasAboutness` to veridicality or standpoint modality, so enactment never
entails assertion (and text *about* deception is never inferred to deceive).

## Dependencies

Depends on `kernel`, `events` (the Event/eventType machinery), `observations` (cues), and
`attestation` (fact-checks, verification results). Consumed by the claim layer's refutation
modality and by the narrative extension's unreliable-narration and myth boundaries — both
documented bridges, neither an axiom coupling.
