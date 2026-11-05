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
| `gmeow:InhabitationLocusKind` / `gmeow:ControlLevel` / `gmeow:DeterminationForce` / `gmeow:ContinuityVerdict` | `logic:AbstractIndividualType ⊑ QualityValue` | the closed value vocabularies |

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

## Prospective stages and attributed restoration status

`SubjectStage` remains an actual realized `Situation` under
`TimeScopedRelation`. Its bearer and epoch meanings do not change.
`SubjectStageProspect` is instead a source-described future position in a
lineage: an `InformationObject` explicitly disjoint from SubjectStage. It has
actual `prospectOfLineage` memberships, with no maximum. Its existence alone
supplies no bearer, epoch, realization or committed anticipation. Keep distinct
stage and description identities (Principles 4, 9, 11 and 19).

`StageStatusReading` uses the existing Observation roles: exactly one
`observedFeature`, exactly one `observationResult` and at least one actual
`vantage`, without a vantage maximum. The result is in the closed `StageStatus`
set: `stageEnded`, `stageCurrent`, `stageInProgress`, `stageAwaited`. Missing
status is unspecified, and an extension value cannot bypass conditional rules.
Actual joint and competing readings coexist without a global now, preferred
frame, status hierarchy or latest-reading selector.

Each reading also selects exactly one actual `stageStatusLineage` witness.
An actual feature or selected realization must have that exact `stageOfLineage`
membership; a prospective feature must have that exact `prospectOfLineage`
membership. Other legitimate global memberships remain intact. Required fields
and positive membership links establish these intrinsic associations; comparing
only supplied values for equality cannot excuse an absent witness.

| Feature and explicit status | Required or forbidden bindings |
| --- | --- |
| Prospect, stageAwaited | No stageStatusRealization. At least one actual stagePursuit is required, with no cap. Every supplied pursuit has an actual Goal directed to that exact prospect. |
| Prospect, stageEnded/stageCurrent/stageInProgress | Exactly one actual SubjectStage through stageStatusRealization, with the selected positive lineage membership. |
| Actual SubjectStage, nonawaited status | The feature already identifies the actual stage. stageStatusRealization is forbidden even if it repeats that feature. |
| Actual SubjectStage, stageAwaited | Invalid: the awaited reading must observe the distinct prospect. |

A prospective feature carries neither `stageBearer` nor `duringInterval`.
Realization remains on the attributed reading, never on the description itself.
The same prospect can therefore be awaited under one actual frame and reported
realized under another. Its old awaited readings remain retained while later
readings identify a separate actual stage. No missing field creates an awaited
status or a phantom bearer.

### Committed anticipation and actual pursuits

`stagePursuit` is reserved for awaited readings. It retains every actual
[Intention or social Commitment](../teleology/docs.md) supplied as held toward
the prospect; at least one is required and there is no maximum. A mere Desire,
plan, historical adoption, open tenure or missing abandonment does not prove
current committed pursuit. Keep internal commitment as Intention and a real
social Commitment's distinct beneficiary and actual bearer obligations; invent
none of those inputs.

Every supplied pursuit must have its actual `intentionGoal`, explicitly a
`Goal`, whose `goalStageProspect` names the same observed prospect. That
nonfunctional goal relation asserts intended realization, not `satisfiedBy`,
execution or success. Other source-supported goal/prospect relations remain
valid. Source evidence that a pursuit is actually held and its goal really
has that meaning is mandatory, beyond structural path checks.

### One selected restoration account

`StageRestorationReading` is an attributed Observation of an actual Event,
not a RestorationTransformation subclass or an invented EventType. It has
one actual event feature, at least one actual vantage without a cap, and
exactly one value for each required role:

- `restorationSourceStage`: the actual source SubjectStage;
- `restorationIntermediateStage`: the distinct actual intermediate SubjectStage;
- `restorationIntendedProspect`: the intended output description, never an
  actual generated stage or future bearer;
- `restorationLineage`: the selected actual common membership witness;
- `restorationEventProgress`: the selected actual EventProgressReading.

All three stage roles must have the exact positive membership named by
restorationLineage. This scopes the same-lineage profile to the selected account,
without constraining every stage membership or arbitrary transformation. Distinct
source/intermediate stages may share a bearer without becoming one stage.
Their actual bearer evidence remains separate.

The selected [event-progress reading](../events/docs.md#attributed-event-progress)
must directly observe the account's exact actual Event. Its complete actual
vantage set and the account's set must match by membership in both directions.
Do not borrow progress from a different event, description realization, account
or frame. Completion alone proves neither goal satisfaction nor a successful
restoration or continuity verdict.

`stageRestorationReading` on a status reading selects that exact account. It
is required for stageInProgress, optional for stageAwaited and forbidden for
stageCurrent/stageEnded. The status reading and selected account must share
one selected lineage and the same uncapped actual vantage set in both directions.
An in-progress feature or realization must be that account's exact intermediate
stage, and that account's selected progress record must explicitly report
`eventInProgress` for its actual Event. Missing end data or an open commitment
cannot satisfy this link.

When an awaited reading selects an account, its observed prospect must be the
account's exact intended prospect. Every actual pursuit retained on that awaited
reading must motivate the same actual Event observed by that account through
`motivates`. Required
pursuit/Goal links and every optional supplied role remain checked. Accounts
retain their own endpoints rather than mixing competing interpretations into
global Event slots.

### Strict prospective continuity boundary

`assessmentToStage` keeps its actual SubjectStage meaning and has explicit
supplied-value endpoint closure. Its canonical domain/range axioms retain
IdentityContinuityAssessment and SubjectStage. The directly guarded
`ProspectiveContinuityTargetConstraint` forbids a SubjectStageProspect target,
including one also typed as SubjectStage. A range entailment alone is not the
same as supplied-target validation (Principle 17).

A later assessment targets a distinct actual realization while its description
and old awaited readings stay retained. No historical awaiting claim vetoes
that actual endpoint, and no provisional fourth continuity verdict is introduced.
`continuityIndeterminate` remains an actual verdict for the two actual stages,
not a missing verdict or an awaited-stage marker. This boundary does not select
which status a consumer accepts, authenticate realization or merge stages.

The canonical laws check these intrinsic bindings with focused positive guards,
atomic presence probes and exact stored terms. They do not execute source
interpretation, commitment recognition, temporal sorting or restoration. Keep
source/viewpoint evidence and the original three-position distinction: ended
actual original, actual intermediate, committed awaited prospect without a
future bearer. Co-temporal ended/awaited interpretations need not fit a linear
clock. Existing model-serving actual-stage and lineage consumers remain on
SubjectStage/stageOfLineage. The [identity charter](design/INHABITED-IDENTITY.md#prospective-stages-and-scoped-status)
and [competency contract](design/INHABITED-COMPETENCY.md#prospective-stage-and-restoration-obligations)
state these boundaries without implying native or consumer execution evidence.
