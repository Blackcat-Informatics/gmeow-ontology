<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Inhabitation — module & alignment reference

The reference companion to the ratified [inhabitation design set](design/). The
canonical spec is `design/INHABITED-*.md`; this page indexes the terms the
`module.ttl` mints and the by-reference alignments in `mappings/`. Everything in
`mappings/` is **authored once** and compiled by `gmeow compile-mappings`
(Principle 4); nothing is hand-edited downstream.

## What the slice is

The connecting topology that says **which transformations preserve the same
durable digital subject and which do not** — a subject inhabiting a host over a
tenure, in a time-scoped configuration, without asserting metaphysical
singularity (Principle 9). Its Principle 15 consumer is the GTS `ai-package` and
the MCP memory triad: grounded agent memory that survives across sessions,
models, and vendors (Principle 14).

## Terms

| Term | Stereotype | Role |
|---|---|---|
| `gmeow:DigitalSubject` | `logic:RoleMixin ⊑ Agent` | the durable "who", borne over a tenure, supported (not entailed) by self-assertion |
| `gmeow:DigitalSubjectTenure` | `logic:Situation ⊑ TimeScopedRelation` | the time-scoped, vantage-attributed bearing of subject status |
| `gmeow:InhabitationTenure` | `logic:Situation ⊑ TimeScopedRelation` | the subject·host relationship over an interval (a Situation, disjoint from a Relator) |
| `gmeow:InhabitationConfiguration` | `logic:Situation ⊑ TimeScopedRelation` | a maximal constant-configuration sub-interval of a tenure |
| `gmeow:Inhabitant` / `gmeow:InhabitedSystem` | `logic:RoleMixin` | the derived agent/host roles (native rule, never asserted) |
| `gmeow:EmbodimentCarrierRole` | `logic:RoleMixin ⊑ Entity` | the surface a subject acts through (device/avatar/account/terminal/voice) |
| `gmeow:EmbodimentAssignment` | `logic:Situation ⊑ TimeScopedRelation` | the time-scoped use of a carrier (subject × carrier × interval × capabilities) |
| `gmeow:InhabitationClaim` | `logic:SubKind ⊑ StandpointClaim` | the contested, *unasserted* inhabitation (neutrality) |
| `gmeow:InhabitationDescription` | `logic:SubKind ⊑ Proposition` | the quoted, range-open configuration a claim observes |
| `gmeow:SubjectStage` | `logic:Situation ⊑ TimeScopedRelation` | one epoch of a subject's identity (distinct nodes, never reused) |
| `gmeow:SubjectLineage` | `logic:Kind ⊑ InformationObject` | the durable identity record grouping stages |
| `gmeow:IdentityContinuityAssessment` | `logic:SubKind ⊑ Observation` | the contestable same/different/indeterminate verdict (descriptive layer) |
| `gmeow:ContinuityDetermination` | `logic:SubKind ⊑ IdentityContinuityAssessment` | an authority's binding-for-action verdict (decisional layer) |
| `gmeow:ControlAssessment` | `logic:SubKind ⊑ Observation` | who causally controls a host/embodiment (not deception) |
| `gmeow:TransferManifest` | `logic:Kind ⊑ InformationObject` | what crossed a migration boundary (coarse-grained) |
| `gmeow:eventTypeInhabitationTransition` | `gmeow:EventType` | migration as a lifecycle-value event (`portalFrom`/`portalTo`) |
| `gmeow:InhabitationLocusKind` / `ControlLevel` / `DeterminationForce` / `ContinuityVerdict` | `logic:AbstractIndividualType ⊑ QualityValue` | the closed value vocabularies |

## The load-bearing invariants (tested in `tests/structural.ttl`)

- **Situation, not Relator.** Every tenure/configuration/assignment/stage is a
  `logic:Situation ⊑ TimeScopedRelation`; a Situation is disjoint from a
  `logic:Relator`, so the earlier both-typing was unsatisfiable.
- **RoleMixin, not Role.** `DigitalSubject`/`Inhabitant`/`InhabitedSystem`/
  `EmbodimentCarrierRole` span Kinds, so they are anti-rigid non-sortals, never
  single-Kind `logic:Role`s or rigid Kinds.
- **Support, not entailment.** Digital-subjecthood is borne over a tenure and
  *supported* by self-assertion; the native rule derives the classification from
  the tenure, so a bare "I am durable" utterance never entails the status.
- **Neutrality by range-openness.** A contested inhabitation is an unasserted
  `InhabitationClaim` whose `describedSubject`/`describedHost` carry **no** range —
  describing a spirit as a subject infers nothing about its type (Principle 9).
- **Continuity is never `owl:sameAs`.** Sameness across stages is a contestable
  `IdentityContinuityAssessment`; the decisional `ContinuityDetermination` binds
  *within its frame only*, superseded never erased (Principle 10).
- **No `primaryInhabitant`, no `gufo:inheresIn`.** Precedence among co-tenants is
  resolved over recorded `ControlAssessment`s; role-fillers ride per-branch
  object properties.
- **`profile mints nothing`.** Composition rides the open `configurationFacet`;
  the one typed facet whose range is core is `configurationEmbodiment`.

## By-reference alignments (`mappings/equivalences.ttl`)

All by reference (Principle 5), all `skos:relatedMatch` (the constructs cross
foundational categories the targets do not draw). Scoped to the terms the slice
**owns**: the OpenTelemetry-GenAI / ML-metadata / model-serving rows belong to
`extensions/model-serving`, and the gUFO / OWL-Time / PROV core are generated
CONSTRUCT projections owned by their concept slices.

| GMEOW | Predicate | Target | Note |
|---|---|---|---|
| `gmeow:DigitalSubject` | `skos:relatedMatch` | `vc:credentialSubject` | the DID subject a signed credential is about |
| `gmeow:DigitalSubjectTenure` | `skos:relatedMatch` | `vc:VerifiableCredential` | the tenure as a signed, issuer-attributed claim |
| `gmeow:tenureVantage` | `skos:relatedMatch` | `vc:issuer` | who asserts the status |
| `gmeow:InhabitationClaim` | `skos:relatedMatch` | `crm:E13_Attribute_Assignment` | a reified, attributed claim — the heritage seam for spiritual/fictional/legal cases |
| `gmeow:IdentityContinuityAssessment` | `skos:relatedMatch` | `crm:E13_Attribute_Assignment` | a vantage-indexed continuity verdict, never a global `owl:sameAs` |
| `gmeow:ControlAssessment` | `skos:relatedMatch` | `crm:E13_Attribute_Assignment` | attributed agency, not deception |
| `gmeow:EmbodimentCarrierRole` | `skos:relatedMatch` | `as:Actor` | the actor/avatar surface (actor model only) |
| `gmeow:TransferManifest` | `skos:relatedMatch` | `prov:Entity` | the transition-content record |

## The two flagship seams (developed in `design/INHABITED-ALIGNMENT.md`)

- **OpenTelemetry-GenAI — the occurrent shadow.** Traces/spans capture the
  runtime/invocation/tool layer and *nothing* above it. Projection GMEOW → OTel
  is a `SoundUnderApproximation`; ingest OTel → GMEOW is subsume-and-extend (a
  trace acquires a subject, a tenure, and contestable provenance). Owned by
  `extensions/model-serving`.
- **W3C DID / Verifiable Credentials — the cryptographic-identity shadow.** A DID
  *is* a subject of its own digital existence; a self-controlled DID Document *is*
  self-assertion; a VC *is* a `DigitalSubjectTenure`/`IdentityContinuityAssessment`
  with a signed, vantage-indexed proof. Projection GMEOW → VC is a
  `SoundUnderApproximation`; the `same`-not-`owl:sameAs` discipline maps to "a VC
  from issuer X asserts these two DIDs are the same subject," which another issuer
  may decline — contestable by construction.

## Worked examples (`examples/`)

- `subject-status.ttl` — the breaking ABox: a `SoftwareAgent` *bears*
  `DigitalSubject` via a tenure, reasoning green against the disjointness/rigidity
  gates.
- `inhabitation-tenure.ttl` — a non-contested inhabitation with a configuration,
  an embodiment, an open-facet deployment, and a lifecycle-value migration.
- `contested-possession.ttl` — the neutrality form: coexisting Vodou/secular
  `InhabitationClaim`s over a range-open description; nothing in the base graph.
- `continuity-upgrade.ttl` — coexisting atman/anatta assessments plus a
  governance-board `ContinuityDetermination`, superseded on appeal.
