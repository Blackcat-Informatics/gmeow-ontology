<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Expertise mapping doctrine

This note records the design contract for the GMEOW expertise module
(`ontology/modules/expertise.ttl`) and its relationship to the language
proficiency machinery, employment, attestation, and surface vocabularies.

## Core model

- **Skill**, **Occupation**, and **Credential** are `gufo:Kind` individuals.
- **SkillProficiency** is a `gufo:Relator` binding `{agent} × {skill} × {level} × {scale} × {interval}`.
- `gmeow:hasSkill` is the flat shortcut for the common case; promote to `SkillProficiency` when level, scale, temporal scope, provenance, or standpoint matter.

## Reuse, not parallel mechanisms

- The rating vocabulary (`ProficiencyScale`, `ProficiencyLevel`) lives in `languages.ttl` and is domain-neutral. CEFR/ILR/ACTFL remain language scales; Dreyfus/NIH/Assessed are skill scales. `LanguageProficiency` consumes the same value individuals unchanged.
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
