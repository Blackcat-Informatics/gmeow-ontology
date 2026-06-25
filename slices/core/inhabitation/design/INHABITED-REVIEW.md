<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Review Disposition and Revised Architecture

> The **correction record.** A foundational review of the first draft found several commitments
> mutually inconsistent with the GMEOW foundation and several competency questions not yet answerable.
> The verdict — *do not ratify as written; reopen the settled decisions* — is accepted. This document
> records each issue, the evidence checked against the codebase, the accepted disposition, and the
> revised term inventory the other documents now implement. It is the authoritative layer: where a
> sibling document still reflects the first draft, this document governs.

## Verified evidence

Each foundational claim was checked against the live ontology before acceptance:

| Claim | Source checked | Result |
|---|---|---|
| `TimeScopedRelation` is a `logic:Situation` | `slices/core/temporal/module.ttl` | confirmed — `a logic:Situation` |
| Relators are endurants, not situations | `organization/module.ttl` (`Membership`), `norms/module.ttl` (`Persona`): `logic:Kind ⊑ logic:Relator`; `awareness` `AwarenessTenure a logic:Situation` | confirmed — the two are distinct patterns |
| `logic:Category` is rigid | `slices/core/logic/module.ttl` | confirmed — *"A **rigid** non-sortal"* |
| `describesModel` is functional, ranges `SoftwareAgent` | `slices/core/ai/module.ttl` | confirmed — `owl:FunctionalProperty`, range `SoftwareAgent` |
| Event subclasses exist | `ai`/`agentic`/`software` | confirmed — `ModelInvocation`, `ToolCall`, `Commit` are `logic:Event` subclasses (relevant to the one Portal nuance below) |

## The seven reopened decisions

| # | Issue (accepted) | Revised disposition |
|---|---|---|
| 1 | A relator cannot subclass `TimeScopedRelation` (a situation) | The inhabitation is a **situation/tenure**: `gmeow:InhabitationTenure ⊑ TimeScopedRelation`, with `gmeow:InhabitationConfiguration ⊑ TimeScopedRelation` for the time-scoped facets. No relator subclasses a situation. A persistent `InhabitationBond ⊑ Relator` is added only if a consumer needs it. |
| 2 | `DigitalSubject` spans Kinds → not a `Role`; `Category` is rigid | `gmeow:DigitalSubject` is a **`logic:RoleMixin`** (anti-rigid non-sortal spanning Person/SoftwareAgent), with `gmeow:DigitalSubjectTenure ⊑ TimeScopedRelation` recording when and according to whom the status is borne. Self-assertion is a high-authority *supporting claim*, never an entailment of the type. |
| 3 | One stable RDF node already asserts numerical identity; non-assertion ≠ denial | Continuity is modeled with `gmeow:SubjectStage` (per epoch), grouped by `gmeow:SubjectLineage` (the durable identity record, no numerically-identical bearer asserted), and an explicit `gmeow:IdentityContinuityAssessment ⊑ Observation` (from-stage, to-stage, verdict same/different/indeterminate, vantage, evidence, confidence). A single durable node is a *projection* from a continuity-affirming standpoint, not the canonical graph. |
| 4 | The neutrality gate asserts the base proposition | Contested inhabitation is a **`gmeow:InhabitationClaim ⊑ Observation`** whose observed feature is an inhabitation configuration/description — or an unasserted RDF-1.2 reified proposition carrying `accordingTo` + modality. The base relation is **not** asserted; range entailment never fires on a contested claim. Applies to possession, incarnation, tulpa, corporate personhood, and contested continuity. |
| 5 | Deployment/execution re-conflated; `describesModel` mismatch | Explicit `gmeow:ModelArtifact`, `gmeow:ModelDeployment` (artifact × service × host × endpoint × config × interval), `gmeow:RuntimeExecution ⊑ Activity`. Awareness mode is a *facet* of a deployment/execution, not a substitute. `ModelCard` relations split: `gmeow:describesModelArtifact` / `gmeow:describesModelService`; the functional `describesModel→SoftwareAgent` is no longer asked to carry a `Distribution`. |
| 6 | active-at-T unanswerable (facets lack intervals) | Resolved by `gmeow:InhabitationConfiguration` (issue 1): the active facet at T is read off the configuration whose interval contains T; the constant-configuration invariant is tested. |
| 7 | Session/control/transition primitives | `gmeow:AgentSession ⊑ Activity` (an event aggregate using `subEventOf`/participant roles), episodes via `subEventOf`; migration as `eventTypeInhabitationTransition` (lifecycle value pattern) with `portalFrom`/`portalTo`; ending a tenure is **ontic, not suppression**; crossing a boundary requires a `gmeow:TransferManifest` or derivation evidence; control is a `gmeow:ControlAssessment ⊑ Observation`, **not** deception divergence. |

### The one nuance (issue 7, Portal)

The review states event kinds are value individuals, not subclasses. That is not a universal GMEOW
doctrine — `ModelInvocation`/`ToolCall`/`Commit` are `logic:Event` subclasses, and two are reused
here. So `Portal`-as-subclass had real precedent. The revised design nonetheless adopts the
`eventType` value form (`eventTypeInhabitationTransition`), because a *migration* belongs to the
lifecycle event family (birth/death/migration), which uses values — agreeing with the outcome on
consistency grounds, while recording that this point was a style alignment, not a foundational error.

## Additional concerns (accepted)

- **`inhabitationLocus` split** into `inhabitationLocusKind` (self/vessel) and *derived* tenancy
  cardinality (shared = overlapping tenures over one host), because the two axes are orthogonal.
- **`Embodiment` split** into `EmbodimentCarrierRole` (the surface entity in role) and
  `EmbodimentAssignment ⊑ TimeScopedRelation` (subject × carrier × interval × capabilities).
- **WEMI downgraded** from emitted SSSOM term mappings to a documented parallel only — the subject
  spine (a RoleMixin, situations, an event aggregate) crosses foundational categories that WEMI's
  endurant Kinds do not; `AgentEpisode relatedMatch Item` was a bad mapping.
- **Core/profile packaging** corrected: a range axiom targeting `gmeow:Persona` (norms extension) is a
  semantic dependency regardless of `owl:imports`. The slice splits into `core/inhabitation` (minimal
  subject–host–tenure–continuity), `profile/inhabitation-ai` (model/deployment/runtime/session/tools/
  memory), and `profile/inhabitation-expression` (Persona/Embodiment integration). See
  [`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md).
- **Cross-domain cases are profile mappings, not a correctness proof** — documented with their
  *differences* (an actor may be role enactment, not host occupation; an officer represents a
  corporation, perhaps reversing the dependence; possession varies by tradition), and the references
  appendix carries real bibliographic records, not one-line labels.
- **Competency statuses downgraded** to honest values; each ships a fixture, a query, expected
  bindings, expected-absent bindings, and a counterexample
  ([`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)).

## The revised minimal core inventory

The corrected core is small but formally coherent; the AI and expression material moves to profiles.

```text
core/inhabitation
  gmeow:DigitalSubject              logic:RoleMixin
  gmeow:DigitalSubjectTenure        ⊑ TimeScopedRelation (logic:Situation)
  gmeow:Inhabitant                  logic:RoleMixin
  gmeow:InhabitedSystem             logic:RoleMixin
  gmeow:InhabitationTenure          ⊑ TimeScopedRelation
  gmeow:InhabitationConfiguration   ⊑ TimeScopedRelation
  gmeow:InhabitationClaim           logic:SubKind ⊑ StandpointClaim   (the contested, unasserted form)
  gmeow:InhabitationDescription     logic:SubKind ⊑ Proposition       (range-open quoted configuration)
  gmeow:SubjectStage                logic:Situation ⊑ TimeScopedRelation
  gmeow:SubjectLineage              logic:Kind ⊑ InformationObject    (groups stages)
  gmeow:IdentityContinuityAssessment logic:SubKind ⊑ Observation
  gmeow:ControlAssessment           logic:SubKind ⊑ Observation
  gmeow:inhabitationLocusKind       value vocabulary (self / vessel)
  eventTypeInhabitationTransition   lifecycle event value; portalFrom / portalTo
  gmeow:TransferManifest            what crossed a transition

profile/inhabitation-expression
  gmeow:EmbodimentCarrierRole       logic:RoleMixin
  gmeow:EmbodimentAssignment        ⊑ TimeScopedRelation
  (integration with gmeow:Persona from the norms extension)

profile/inhabitation-ai
  gmeow:ModelArtifact               ⊑ Distribution (software ext)
  gmeow:ModelDeployment             artifact × service × host × endpoint × interval
  gmeow:RuntimeExecution            ⊑ Activity
  gmeow:AgentSession                ⊑ Activity (event aggregate; subEventOf)
  eventTypeAgentEpisode             sub-aggregate marker
  gmeow:describesModelArtifact / gmeow:describesModelService   (ModelCard split)
  (reuse gmeow:ModelInvocation, gmeow:ToolCall, gmeow:AwarenessTenure as facets)
```

Most "new" terms are thin specializations of existing constructs (`⊑ Observation`, `⊑ Activity`,
`⊑ TimeScopedRelation`) — the idiomatic GMEOW pattern, not bloat. The earlier "~5 terms" headline was
achieved by erasing identity criteria; the corrected count is higher and honest.

## Round 2 — the assessment, grounding, and packaging-purity corrections (accepted)

A second review approved the architectural direction and found four further blocking corrections, all
verified against the foundation and folded in:

| # | Issue | Verified | Fix |
|---|---|---|---|
| R2-1 | The Relator/Situation collision reappeared in `InhabitationClaim`, `IdentityContinuityAssessment`, `ControlAssessment` (declared `logic:Situation` while subclassing `Observation`, which is a `logic:Kind`) | `observations/module.ttl` — `Observation a logic:Kind`; `StandpointClaim a logic:SubKind` | the assessments are `logic:SubKind ⊑ Observation`; `InhabitationClaim` is `logic:SubKind ⊑ StandpointClaim` (it carries `claimModality`) |
| R2-2 | `hasDestructionEvent` cannot close a tenure (its domain is `gmeow:Entity`, an endurant; a tenure is a situation) | `lifecycle/module.ttl` — `domain gmeow:Entity` | close a tenure by ending its `duringInterval`; the optional `gmeow:tenureEndedBy` (domain `TimeScopedRelation`, range `Event`) records the causal link |
| R2-3 | `SubjectStage`/`SubjectLineage` lacked stereotypes; the role-mixins were ungrounded so participation did not instantiate them | foundation | `SubjectStage` = `logic:Situation ⊑ TimeScopedRelation`; `SubjectLineage` = `logic:Kind ⊑ InformationObject`; ground `DigitalSubject`/`Inhabitant` `⊑ Agent`, `InhabitedSystem` `⊑ Entity`; the tenure classifies its filler into the role |
| R2-4 | Core declared profile-typed configuration properties (`configurationPersona → Persona`, etc.) — re-importing the dependency the packaging split removed | the packaging rule (R1, decision 3) | core declares only `configurationOfTenure` + the open-range `configurationFacet`; typed subproperties move to the profiles |

Non-blocking cleanup, all applied: `InhabitationDescription` typed `⊑ Proposition` with **range-open**
`describedSubject`/`describedHost` (so describing a spirit infers no `Agent`); the possession example's
invalid `gmeow:held` modality replaced with `gmeow:unequivocal`; `TransferManifest ⊑ InformationObject`;
`controlDegree` (a datatype with categorical values) replaced by `controlLevel` → a `gmeow:ControlLevel`
value vocabulary; the AI provenance chain given explicit joins (`invocationInExecution`,
`sessionSubjectStage`, `sessionConfiguration`) and the awareness facet attached to the deployment's
**service `SoftwareAgent`** (not the deployment relator or runtime event); the manifestation
thoughtform example corrected to the supported-tenure model (no direct `DigitalSubject` typing); and the
manifesto's "correctness proof" softened to "adversarial test" to match the traditions document.

## Status

This design set remains a **draft**. The corrections above are folded into the sibling documents; the
`module.ttl` is not authored until the authority confirms the revised shape. The remaining genuine
forks for the authority: `DigitalSubject` as `RoleMixin` vs `PhaseMixin` (this set adopts `RoleMixin`
to avoid the self-assertion-entails-type trap); `InhabitationClaim ⊑ Observation` vs the unasserted
RDF-1.2 reified proposition (this set leads with the Observation form, both viable); and the exact
core/profile dependency boundaries.
