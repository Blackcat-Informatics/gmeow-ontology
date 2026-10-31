<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agency records design

The model adds source vocabulary for agency evidence. Existing `Intention`,
`Goal`, `Event`, `Activity`, `ToolCall`, `ModelInvocation` and `Observation`
retain their definitions. Prose attempts are event kinds; they acquire no compiled
prescription, snapshot or journal merely because work continued.

## Independent axes

An attempt stopping, an intention being released, an invocation failing and an
artifact being incomplete are separate claims. Source-attributed alternatives
can coexist with temporal and standpoint metadata. No global functional status
or automatic lifecycle state machine is introduced.

Intention lifecycle events identify the affected mode. They do not make the mode
an event or require a new intention for a decision to do nothing. Paused holding
can remain open until a later withdrawal. Goal evaluation remains a separate
canonical judgment, never a renamed stop code.

`IntentionSalienceObservation` is a particular observation with its own decimal
reading, target, vantage and explicit source frame. It avoids the assumptions of
claim import importance and avoids inventing numeric bounds merely to create a
`ScoreScale`. An opaque source convention is retained as an `InformationObject`;
its unknown arithmetic remains unknown. No scale-conversion engine is implied.

## Canonical obligations

The module authors `logic:` declarations, restrictions, explicit closure policies
and realized integrity formulas (Principles 4 and 17). It authors no separate
OWL/RDFS or validation-shape surface.

| Object or assertion | Authored obligation |
| --- | --- |
| Artifact operation receipt | Required operation, artifact and containing-event paths; an actual attempt or ToolCall supplies the parent. |
| Read receipt | Its `readArtifact` target matches its identified receipt artifact. |
| Budget checkpoint pause | An actual information-object resume point is retained. |
| Planless resumption | The original attempt, resume point and sub-event context are explicit. |
| Intention transition | The affected Intention is identified; supersession names its replacement. |
| Satisfied intention transition | A separate GoalEvaluation is retained. |
| Salience observation | Intention, decimal value, producer frame and vantage are required. |
| Artifact verification observation | Artifact, method and vantage are required; missing verdicts do not become successes. |
| Held read-after-write | Distinct write and later-read receipts identify the same digest-bearing artifact version with explicit before-or-meets evidence. |
| Practical goal conflict | A canonical symmetry assertion, without transitivity or constitutive opposition. |

Presence requirements use universal restrictions with class-scoped closed-world
entries rather than existential witnesses that would manufacture missing source
objects. Fixed datatype obligations preserve standard strings, decimals and
booleans. Digest roles reuse the existing algorithm-labelled literal convention;
no custom digest datatype or provider is introduced.

A receipt is an actual subordinate activity occurrence, not a free-floating claim
of success. `artifactComplete` expresses content completeness of the identified
artifact. `receiptOperationComplete` separately preserves the producer's boolean
report about the operation represented by ArtifactOperationReceipt. Its named
domain and xsd:boolean range have a property-global closed-world reading for
supplied values, without requiring the field on every receipt or making it
functional. Existing receipt operation, artifact, parent and read-target laws
continue to apply; no new receipt class or duplicate integrity law is needed.

Migrate a local `receiptComplete` only after distinguishing these meanings.
Completeness of a receipt document, proof discharge, invocation outcome, tool
outcome and goal satisfaction remain different assertions. No status, error,
absence or artifact bit computes the reported operation bit, and that bit
produces no artifact-completeness or verification inference.

## Verification and byte identity

The read-after-write method is an observation method, not a verifier agent or a
successful status. The observation’s result reuses existing evaluation verdicts.
It does not use the Attestation-domain `verificationResult` property, which would
incorrectly classify an observation or activity as an attestation.

For a held verdict, the actual read and write are different occurrences. Both
refer to the same exact artifact version, whose digest identifies its bytes. A
mutable filename alone cannot stand for that version. The result returned by a
tool can be a report envelope; its `resultDigest` remains distinct from artifact
identity. A matching report hash alone therefore cannot confirm a write.

Explicit source ordering permits both a strict `before` gap and abutting `meets`.
It is not inferred from absent dates, a call identifier or a receipt’s list order.
The model does not execute a reader, materialize a checkpoint or calculate a goal
verdict from recorded metadata.

## Ownership and correspondence

This core slice consumes other core foundations by reference. It does not relax
ToolCall’s existing SoftwareAgent requirement or borrow enactment-only continuation
kinds for planless work. Minimally described tool agents preserve source evidence
without a fabricated holder or universal name-to-agent identity rule.

The general `gmeow:used` relation and its PROV-O correspondence are owned by the
logic grounding slice. This slice consumes that same IRI and specializes it with
`readArtifact`; Activity-to-Entity typing and nonfunctional usage are unchanged.
The logic-owned native close match carries ordinary unqualified actual usage,
with a validation-only bridge judgment and explicit qualified-usage, role and
usage-time residue. Its mapset coat and paired translations live with the logic
owner; the single linkage lives in
`../../../grounding/logic/mappings/provenance-usage.ttl`. No second usage primitive
or downstream external alignment is introduced (Principles 17 and 19).

## Runtime trace reports are event classifications

The event-kind surface reuses EventType and eventType from the events owner.
All requested runtime trace kinds are individual values rather than new Event
subclasses. The runtime-specific kinds are still useful shared names for
source reports: their coats state what the source reported and which runtime
mechanisms are not established by the label. No value runs a model, schedules a
tool batch, restores a receipt, changes a reply gate or applies a dream-phase
cap. Source-reported authority remains source-relative (Principles 4, 9 and 17).

A thinking round, model call, request-send occurrence, response arrival, tool
return and result receipt keep distinct identities. Actual output artifacts
remain information objects with their own genuine provenance. A replay report
does not invent another execution; a provider's checkpoint report does not
admit an executable checkpoint. Existing ArtifactOperationReceipt obligations
apply only when the source actually supplies an artifact operation.

## State observations and screening categories

ModelInvocationState is an AbstractIndividualType grounded in QualityValue.
The state selector is an Observation result, rather than a mutable state slot
on the invocation Event or model agent. Its observedFeature identifies the
actual ModelInvocation; vantage and wasDerivedFrom retain the actual observing
standpoint and source. Supplied observedAt indexes a reading; supplied
recordedAt or recordedDuring locates the record's creation. These different
clocks are preserved and unknown times remain unknown. Separate observations
and their context carry time-relative evidence without a globally functional
state, latest-value rule or a negative constraint against later outcomes.
Only the running value is seeded; absent state or outcome fields stay absent.

injectionCategory is an exact source code, not a normalized token or a
language-bearing instruction body. The source supplies its category semantics;
the ontology does not create a security taxonomy or a screening implementation.
Its actual Observation points to the retained information-object answer,
rather than retyping that answer or a ModelInvocation as the observation.
The observing vantage and recording writer remain independently attributable.

Both evidence laws use direct binary field predicates in their universal
antecedents. Current native target derivation admits those as SubjectsOf
targets; no variable-predicate-only guard, raw target selector or compiler
extension is used. The realized consequents retain actual feature, vantage and
source identities. Property-global ClosedWorldClosure entries explicitly opt
the supplied domain/range readings into validation; they impose no universal
presence requirement. All laws and property semantics stay in logic:, with no
hand-authored shape or external equivalence assertion (Principles 17 and 19).

## RDF consumers and API limits

These fields add ontology vocabulary, not a new custom receipt or claim API.
The existing generic MCP query_local path parses the caller's declared RDF
format, and its query-result envelope delegates RDF term transport to PurRDF,
including typed booleans, language, base direction and native triple terms.
That source path has no per-field rationale or completion allowlist.

The fixed store_claim input and store_segment Claim/ToolCall DTO export do not
gain these fields from an ontology change. No support for those fields through
that API, automatic local-field migration or adoption by an unseen downstream
adapter is claimed. Existing proof and pipeline receipt serializers retain
their own contracts; their completeness or execution status is not repurposed
as this producer-reported operation bit. Consumer source inspection establishes
the available generic RDF path, not executed downstream qualification.
