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
objects. Datatype obligations preserve supplied string/token identity for machinery,
standard decimals and booleans. Digest roles reuse the existing algorithm-labelled literal convention;
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

injectionCategory is an exact source code carried as the supplied xsd:string
or xsd:token literal, without normalization or inferred language. The source
supplies its category semantics;
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

## Creature interactions reuse mental modes and events

DemandKind, DemandResponse and the eight creature event kinds belong here as
source vocabulary for situated agency. Mood and MoodType remain affect-owned;
the direct affect dependency supports the ordinary scene without copying
affective semantics. Its Message artifact also consumes messaging directly.
Desire, intentBearer and intentionGoal remain teleology’s
actual wanting contract. There is no new demand class or lifecycle engine.

DemandKind and DemandResponse are separate AbstractIndividualType classes
under QualityValue. Their selectors are nonfunctional. demandKind has Desire
domain; demandResponse has Event domain. The sole new binding answersDemand
names the exact Desire addressed by an answering Event. It is nonfunctional,
so explicitly shared answers are possible without a latest-demand heuristic.
Source reports may omit the response category while retaining the actual
answered Desire. A reported answer does not entail satisfaction.

The existing motivates relation is a motive claim from an IntentionalMoment
to an Event undertaken because of it. It can document a source-supported
motive for demanding, but it does not identify which demand another agent
answered. answersRequestRevision targets a WorldObjectRequest information
record; borrowing it would confuse the request document with Desire.
RequestResponse and RequestFulfillment are likewise object-request records,
not general answering occurrences or cuddlings. No wider alias or fallback
is added to those existing surfaces.

DemandKindEvidenceConstraint requires a supplied kind to belong to an actual
Desire with an actual Agent bearer and Goal. DemandResponseEvidenceConstraint
requires an actual answering Event with an answersDemand target.
AnsweredDemandBindingConstraint checks that target’s actual Desire, bearer,
Goal and typed kind. Each law joins the exact native field claim’s reader
and documentary source. CreatureDemandAnsweredConstraint requires the binding
for the answered-event kind even without a response category. All four laws
have direct binary field guards and use the existing canonical target path;
property-global closure validates supplied selector types without requiring
the fields on unrelated Desires or Events.

An actual acting software creature can be a SoftwareAgent. Its provider
program remains a separate SoftwareAgent, and its descriptive source remains
an InformationObject. A provider record’s recordedWorldObject, providerProgram
and providerWorld bindings preserve those distinctions. A passive model,
sprite or world record is not thereby an Agent; neither a creature label nor
a matching name identifies a Character, Person or actual-world organism.
Fictional identity and context interpretation retain their existing owners.

EventType individuals classify reported occurrences rather than mode or
artifact subclasses. Genesis is not automatically biological birth.
Renaming is not identity replacement. Transition is movement, not arbitrary
state change. Utterance is distinct from message content; remembering is
distinct from MemoryItem or documentary prose. A mood-shift report does not
derive a new Mood or erase an older claim. Source attribution remains on exact
native reifiers, without asserting the source document generated its subjects.

The ordinary scenes introduce no ActionSchema, plan, executor, new fixtures
or acceptance machinery. All semantic obligations live in canonical logic:
declarations, explicit closure and realized Formula trees. Their authoring
does not claim a completed contextual evaluation, provider execution or
runtime qualification (Principles 4, 9, 11, 17 and 19).

## Machinery literal identity

The six machinery ranges are anonymous canonical string/token unions.
The ArtifactOperationReceipt tool-name universal and ConditionVerifier key
universal use those same unions. Domains, required paths, functionality and
cardinalities remain unchanged. Native lexical checking is a separate capability; these
source declarations add no checker or token-preserving API implementation.

The existing condition/verifier/evaluation formulas join the same complete
literal through their shared key variable. Datatype identity remains part of
that binding; lexical equality alone does not supply a missing matching claim.
All integrity formula trees and closure entries remain unchanged. Tool-agent
identity, provider/world object resolution, execution evidence and screening
verdicts retain their existing owners (Principles 2, 4, 7, 9 and 17).

## Loop records preserve a pursuit separately from its reports

`OpenLoop` specializes the existing `InformationObject` as a durable pursuit
record. The record can span episodes and remain after reported closure. It
creates no second Intention, Plan, episode or attempt occurrence, and its class
does not entail `loopOpen`. The source wording reuses `description`; a supplied
task specification is an information artifact rather than the Task event or an
automatically inferred abstract Document Work.

Agency records already consume the intention and information-record owners.
`loopServesIntention` preserves an actual supplied link; the linked Intention
retains its own bearer and goal. A selected writer frame or recording agent
does not identify the pursuing agent. No additional dependency or notebook
bridge is introduced by this unit.

The open LoopState axis records source assessments. Its nonfunctional relation
preserves exact RDF 1.2 state assertions with their own declaring frame,
recording attribution and source. Conflicting and historical assessments do not
select a current winner, imply a latest-timestamp policy or compute transitions.
Done has no goal-satisfaction or execution bridge; letting go has no deletion,
cancellation-permission or intention-abandonment bridge.

LoopClosure is an information report, not an event or decision. Its reported
actual closer is separate from the recording and declaring roles; those roles
can share an actual identity only when supplied by the source. No missing actor
is synthesized from a consumer role code, evidence basis or recorder identity.
The closure's cited basis and its documentary source are separate relations.

Evidence targets remain information artifacts or native reifiers. An underlying
occurrence or Observation retains its own type; it is not made an information
object to fit the evidence range. Quoted content alone asserts no fact and
certifies no support, authenticity or permission. Existing domain-free
statedRationale retains the source's words without inferring an Activity or a
verified cause.

All new declarations use canonical `logic:` domain, range and subclass axioms.
Class-local supplied-value restrictions add no mandatory paths, functional
properties, closed enumerations, exclusivity or state/closure/intention rules.
No existing integrity formula is changed, and no hand-authored projection or
runtime state protocol is introduced. This is an authored source contract;
generated behavior and runtime acceptance remain unverified.

## Reference identity and attention-slot reuse

LoopReference specializes InformationObject, separately from the OpenLoop
pursuit record and task-specification artifact it may name. Its two target
relations are independent, nonfunctional and supplied-only. Class-local
restrictions type supplied targets without requiring either relation. No law
copies loopTaskDocument, equates two targets or chooses a latest revision.

slotCurrentLoop is a named InformationObject role descriptor. The recall-context
owner already defines slotRole with that range and an open set of role
resources. The descriptor introduces no mutable property or new slot class.
An actual AttentionSlot retains its bearer, role and history Path; immutable
SlotPositions retain their owning slot and existing IRI pointer. The pointer
targets the LoopReference record, not the OpenLoop or its task artifact.

The existing slotState reification obligation supplies typed SlotAssignmentClaim
records for actual assertions. Explicit source supersession relates those
assignment records without deleting them or computing a global winner.
Same-slot and bearer-history verification and target resolution remain the
existing consumer contract. No stronger generic StateAssignmentClaim laws,
replacement protocol, resolver, global ordering or permission check is added.

The actual scene and protocol references justify the single recall-context
dependency. Current manifest inspection found no return path from recall-context
to agency-records; this change therefore adds no cycle through that edge. This
is a scoped dependency observation, not certification of the whole existing
graph. The state-records dependency is not needed and is not added.

The existing agency-to-notes dependency supports notebookLoop as a domain-owned
association from NotebookEntry to OpenLoop. This direction avoids a reverse
notes-to-agency edge. The association adds no shared-writer requirement,
content/state copy or inferred Intention. Slot bearer, declaring frame,
recording attribution and actual pursuer retain separate source identities;
no inequality requirement prevents a supplied Agent from filling several roles.

All new property axioms and supplied-value restrictions use canonical logic:.
No existing slot obligation, integrity formula or required field is changed.
The ordinary scene illustrates the source contract; it does not establish
runtime history validation, resolution or generated-surface acceptance.

## Offered guidance remains a note

Guidance specializes the notes owner's Note as retained offer content. Existing
noteContent and statedRationale preserve wording and supplied reasons without
new text or reason predicates. The latter is domain-free and establishes
neither verified causation nor an executed decision. Documentary derivation,
recording attribution and the source's declaring frame retain separate roles.

The offer classification creates no command, Norm, permission, Intention,
adoption, action execution or computed preference winner. It neither replaces
the existing GuidanceSet of governing rubrics/checklists nor claims the
closed-roster comparison semantics of the preference model. The source record
does not gain a proposer or pursuer from note authorship, a recording Agent,
its declaring frame or the bearer of a slot pointing to it.

guidanceLoop has canonical domain Guidance and range OpenLoop.
The relation is nonfunctional and supplied-only, with a class-local all-values
restriction and no required target. It copies no loop state or content and
introduces no lifecycle or intention rule.

slotCurrentGuidance is an InformationObject role descriptor for the existing
slotRole relation. An actual AttentionSlot still needs its bearer, role and
history Path. Each immutable SlotPosition retains its owning slot and IRI
pointer to the actual Guidance note; each actual slotState assertion retains
its typed SlotAssignmentClaim native reifier. Supersession records the source's
relation between actual assignment claims without deleting history or deriving
a global current winner. The existing consumer obligations for same-slot and
bearer history and target resolution remain unchanged.

The role adds no new mutable field, resolver, permission protocol or stronger
generic StateAssignmentClaim profile. Source-identified identity overlap among
roles remains possible without inferring a missing role occupant. No activity
descriptor, ahead-anchor target, ordinal convention or time representation is
invented by this partial vocabulary foundation.

All new axioms use canonical logic: declarations and supplied-value restrictions.
No existing field requirement or integrity formula is changed. The ordinary
scene illustrates retained source metadata, not executed history validation,
offer adoption or generated-surface qualification.
