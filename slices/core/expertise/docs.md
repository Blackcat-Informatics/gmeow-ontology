<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Expertise mapping doctrine

This note records the design contract for the GMEOW expertise module
(`ontology/modules/expertise.ttl`) and its relationship to the language
proficiency machinery, employment, attestation, and surface vocabularies.

## Core model

- **Skill**, **Occupation**, and **Credential** are `gufo:Kind` individuals.
- **SkillProficiency** is a `gufo:Relator` binding `{agent} × {skill} × {level} × {scale} × {interval}`.
- `gmeow:hasSkill` is the flat shortcut for the common case; promote to `SkillProficiency` when level, scale, temporal scope, provenance, or standpoint matter.

## Reuse, not parallel mechanisms

- The rating vocabulary classes (`ProficiencyScale`, `ProficiencyLevel`, `ProficiencyModality`) live in the `kernel` slice — relocated there to break a latent `expertise ↔ cognition` dependency cycle (Principle 6/16). They are domain-neutral; CEFR/ILR/ACTFL remain language scales, Dreyfus/NIH/Assessed are skill scales, and `cognition`'s `KnowledgeProficiency` reuses the same classes. `SkillProficiency` and `LanguageProficiency` consume the same value individuals unchanged (same IRIs).
- Temporal scope reuses `validFrom`/`validUntil` (lightweight) and `TimeInterval` / `hasCreationEvent` (heavyweight) from `temporal.ttl` and `lifecycle.ttl`.
- Standpoint and confidence reuse the cross-cutting `accordingTo` / `confidence` annotation properties from `standpoint.ttl`.

## Endorsement and credential verification

- A third-party skill endorsement is an `Attestation` whose `attestedSubject` is the `SkillProficiency`.
- Self-asserted, assessed, and endorsed proficiency levels coexist as standpoint-indexed claims (CONSTITUTION Principle 9); none is privileged.
- Credential verification is routed through `attestation.ttl`, which is already aligned to `vc:VerifiableCredential`. A verifiable credential is a `Credential` whose authenticity is borne by an `Attestation`. The expertise module supplies content; attestation supplies the signed envelope.

## Occupation classification

- `occupationClassification` carries ESCO, SOC, O*NET, ISCO, or NOC codes as literal values.
- Classification resolution and scheme identification are solver-side concerns (CONSTITUTION Principle 12); the ontology only records the code.

## Surface-vocabulary bridging (by reference)

- **schema.org** — `Skill`/`SkillProficiency` flatten to `schema:knowsAbout` / `schema:skills`; `holdsCredential` maps to `schema:hasCredential`; `credentialFor` to `schema:competencyRequired`; `credentialIssuer` to `schema:recognizedBy` / `schema:sourceOrganization`.
- **ESCO** — `Skill` → `esco:Skill`; `Occupation` → `esco:Occupation`.
- **CTDL / Credential Engine** — `Credential` → `ceterms:Credential`; `credentialIssuer` → `ceterms:ownedBy`.
- **Open Badges 3.0 / W3C VC** — no new terms in the expertise module; verification semantics are inherited from the attestation layer.

## SHACL

- `shapes/expertise-shapes.ttl` enforces closed-world well-formedness:
  - A `SkillProficiency` must reference exactly one `Skill` and carry exactly one `ProficiencyLevel`.
  - The level's `levelScale` should match the relator's `skillProficiencyScale` (warning).
  - A `Credential` intended to be verifiable should reference an `Attestation` (warning).
  - A `Credential`'s issuer must be an `Organization`.

## Terms

### gmeow:Skill · gmeow:Credential

A `gufo:Kind` competency or ability an agent can apply to a task, aligned to
`esco:Skill` by reference; and a degree, certification, badge, or license that
qualifies an agent — its issuer is `gmeow:credentialIssuer`, its subject matter
`gmeow:credentialFor`, and its verification is borne by a `gmeow:Attestation`
(Principle 4).

### gmeow:SkillProficiency · gmeow:skillProficiencyAgent · gmeow:skillProficiencyOf · gmeow:skillProficiencyLevel · gmeow:skillProficiencyScale · gmeow:skillProficiencyInterval

The reified `gufo:Relator` binding {agent} × {skill} × {level on a scale} ×
{interval}, mirroring languages' `LanguageProficiency`. Its functional roles fix
the agent, the rated skill, the attained level, and the scale that level is read
against; `skillProficiencyInterval` bounds the span the level held. Contested
levels coexist as separate standpoint-indexed relators (Principle 9).

### gmeow:hasSkill · gmeow:hasOccupation · gmeow:holdsCredential

The flat 80 % shortcuts: an agent possesses a skill (`hasSkill`, a
sub-property of `knowsAbout` touching the can-do rung); a person holds an
occupation or job role over time (`hasOccupation`); an agent holds a credential,
many-to-many (`holdsCredential`). Promote to the reified forms when level,
scale, temporal scope, or standpoint must become first-class.

### gmeow:credentialIssuer · gmeow:credentialFor · gmeow:occupationClassification

A credential's single issuing `Organization` (functional — a different issuer is
a different credential); what it certifies (a `Skill`, an `Occupation`, or both,
the union range a SHACL shape); and the raw external classification codes (ESCO,
SOC, O*NET, ISCO, NOC) carried on an occupation, scheme resolution being
solver-side (Principle 12).

### gmeow:ProficiencyScale · gmeow:ProficiencyLevel · gmeow:ProficiencyModality

The domain-neutral rating machinery generalized from languages: a `ProficiencyScale`
is the rating framework (CEFR, ILR, ACTFL, Dreyfus, NIH, assessed, self-reported);
a `ProficiencyLevel` is an attained rung on a scale (tied to it by `levelScale`),
meaningless without naming its scale; a `ProficiencyModality` is the channel a
language proficiency rates (speaking, listening, reading, writing). All are value
individuals, never subclasses (Principle 9).
