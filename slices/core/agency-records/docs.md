<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agency records

An agent can pursue an intention through prose work without an executable plan.
This slice retains that work, its pauses and stops, the artifact operations it
performed and the judgments recorded about it. Existing intentions, events,
activities, observations and tool calls remain their own canonical foundations.

## Attempts and intention transitions

Represent an attempt as an `Event` with `eventTypeAttempt`, motivated by an
existing `Intention` through `motivates`. Its start, continuation and finish are
sub-events classified with the corresponding event-type values.

`attemptStop` records how a finish stopped: finished, failed, interrupted or yielded
to foreground work. This outcome does not say how close the agent came to its goal.
Keep that judgment in a separate `logic:GoalEvaluation`. `stopFinished` therefore
does not imply goal satisfaction.

`pauseCause` describes a continuation pause. `pauseBudgetCheckpoint` retains an
actual `attemptResumePoint` information object; `pauseInterrupted` records an
interruption. The point can preserve prose about unfinished work without claiming
an enactment checkpoint’s prescription, input-snapshot or journal identities.

Use `eventTypeAttemptResumption` with `resumesAttempt`, `resumesFromPoint` and
`subEventOf` when the same prose attempt resumes. Existing
`logic:ContinuationResume` specifically resumes an enactment from its authenticated
checkpoint. These are different assertions; planless work does not relax the
requirements of `logic:Enactment`.

The five intention transition values retain active, paused, satisfied, abandoned
and superseded episodes. `intentionTransitionOf` identifies the affected intention.
A known `intentionTransitionTenure` can retain the existing holding interval.
Pausing does not itself close tenure or abandon the intention. A later withdrawal
can close that same tenure without making its earlier pause record false.

A satisfied transition links its separate judgment with `agencyGoalEvaluation`.
Supersession keeps the old intention and names the replacement through existing
`supersededBy`. No status selects a privileged global interpretation or erases an
older claim (Principles 9 and 10).

`CommitmentStatus` describes an intention held as a commitment to oneself: active,
renewed or released. It is distinct from the existing social `Commitment`, whose
beneficiary is another agent. A released self-commitment does not prove its goal
was met.

## Decisions and insights

`eventTypeDecision` records the act concluding deliberation. The agent may choose
to act and form an intention, or decide to leave something unchanged without
making a new commitment. Link an intention or attempt only when the source
actually supplies one.

`learningInsight` extends the existing open `LearningEventType` vocabulary.
It records understanding a connection or pattern through reasoning or reflection,
without a teacher or external source reported for that insight. This does not
claim a universal absence of influences or require formation of a new category.
Other supported learning kinds can coexist. Keep the existing learner and mental
process context on the `LearningEvent`.

## Artifact operations and tools

`gmeow:used`, owned by the logic grounding slice, links an activity to an entity
it actually used. This slice's `readArtifact` specializes
it for information artifacts or versions actually read. Context presence alone
proves neither use nor reading, and reading neither implies modification nor
acceptance of the content as true.

An `ArtifactOperationReceipt` is the artifact-operation activity occurrence retained
as a receipt, subordinate to an actual attempt or `ToolCall`. It names its
`receiptArtifact` and its `receiptOperation`: created, read or updated. A read also
retains `readArtifact` for that same target. This model does not turn an information
document into an activity merely because it records one.

`receiptTool` preserves the source’s tool name. `resultDigest` preserves the
algorithm-labelled digest of what the operation produced or returned. A returned
JSON envelope and the artifact it describes have different byte identities;
`resultDigest` is not automatically the artifact’s `contentDigest`.

A `ToolCall` can retain its verbatim `toolName` and source-scoped `toolCallId`.
Both admit `xsd:string` and explicitly supplied `xsd:token`, retaining the source
datatype and lexical form without whitespace normalization or inferred typing.
It still requires `usedTool` pointing to the actual `SoftwareAgent`. A minimally
described, source-scoped tool agent is enough when only the tool’s name is known.
Do not invent a holder, provider, executable path or cross-call identity from that
name. Use `derivedBy` for assertions produced by the call, and `wasGeneratedBy` for
entities it produced.

The string fields deliberately use `xsd:string`, retaining source case,
whitespace and punctuation. `xsd:token` normalization would weaken that promise.
Call identifiers remain local to their source; they do not widen unrelated
identifier properties or become universal keys.

## Completion and invocation outcomes

`artifactComplete` is the source-supplied boolean about the identified artifact’s
content being complete. A missing value remains unknown. It is independent of
whether a model invocation succeeded, whether an operation ran or whether a
receipt account contains every detail.

When a local `receiptComplete` field meant artifact completion, migrate its value
to `artifactComplete` on the identified artifact or version. When it meant that
the producer reported its operation complete, use `receiptOperationComplete`
on the actual `ArtifactOperationReceipt` occurrence. These are different claims;
an ambiguous local name alone cannot select either meaning. Preserve the source
account until its meaning is known.

`receiptOperationComplete` is an optional, nonfunctional source boolean. The
receipt still needs its actual operation, artifact and containing attempt or
ToolCall; a read retains `readArtifact` for the same artifact. Do not invent
those identities merely to place a bit. A plain document quoting a receipt is
not automatically the operation Activity itself.

Missing stays unknown. An error or outcome supplies neither false nor true,
and differently attributed reports can coexist. True does not imply complete
artifact content, successful execution, goal satisfaction, a held read-back
verdict or a complete receipt account. A reported complete write can persist
an unfinished draft; false does not prove rollback or absence of partial effects.

`invocationOutcome` retains an explicit model-invocation result. The open vocabulary
starts with succeeded and failed. `invocationError` preserves any supplied error
text verbatim. Neither a missing error nor a missing outcome implies success, and
an invocation failure does not automatically make its artifacts incomplete.

## Intention salience with its source frame

`IntentionSalienceObservation` carries `salienceOfIntention`, a decimal
`salienceValue`, a `salienceFrame` and the observing `vantage`. The frame identifies
the producer’s actual scale or convention, even when its bounds or direction are
unknown. Different producer frames are not implicitly comparable.

A known numeric contract can reuse `ScoreScale`. Do not manufacture its bounds,
assume a 0–10 scale, clamp a number or interpret larger values as greater salience
without source evidence. Claim-import `importanceLevel` is a different measure
and is not reused for an intention’s salience.

## Read-after-write verification

`VerifierKind` is an open specialization of `ObservationMethod`.
`verifierArtifactReadAfterWrite` selects actual read-back verification. The
`verifierKind` relation joins the existing observation-method query path.

An `ArtifactVerificationObservation` identifies its `verifiesArtifact`, method
and `vantage`. A reported verdict uses the existing evaluation-verdict individuals
through `observationResult`. Merely selecting the method supplies no successful
verdict.

A held read-after-write verdict needs distinct write and read operation receipts
for the same identified artifact version. The write is created or updated; the
read actually names that version through `readArtifact`. Retain explicit source
order: `before` when there is a strict gap, or `meets` when the operations abut.
The artifact version carries its own `contentDigest`. Do not verify a mutable path
name or substitute a tool-result envelope digest for those bytes.

A successful read-back can confirm byte persistence while `artifactComplete`
remains false: a faithfully stored draft can still be unfinished. Goal satisfaction
and invocation outcome likewise remain separate.

`conflictsWithGoal` records symmetric practical conflict, such as drafting versus
resting. It is neither transitive nor a bridge to constitutive `counterGoal`, and
it establishes no global priority. Explicit goal dependencies can be authored in
the canonical logic vocabulary; means-end search remains solver work.

The [ordinary working scene](examples/attempt-and-readback.ttl) retains an open
intention tenure, a budget pause, a prose resumption and a read-back of an unfinished
draft. The [design rationale](design/AGENCY-RECORDS.md) describes its canonical
obligations and source ownership.

The [stated reasons and operation reports scene](examples/stated-reasons-and-operation-reports.ttl)
also keeps general rationales on actual events and records, source-attributed
true and false operation reports, an unfinished artifact and an absent bit.

## Retain a repair and an intention-directed witness

`defectFixedBy` connects a recorded system defect to the repair activity or fix
artifact reported to have resolved it. A record of an unresolved defect needs
no fix link. Retain the exact resolution statement's reifier with
`wasAttributedTo` naming the reporting agent. The canonical binding law reads
completed statement-metadata lowering; it does not authenticate the report or
prove the repair's efficacy. Competing attributed repairs remain possible.
An activity is an occurrent and an artifact is an information object, so the
range admits either without confusing them. An artifact can independently name
its real producing repair activity through `wasGeneratedBy`; defect resolution
is not automatically a derivation link or a successful-operation verdict.

`witnessedForIntention` names the existing `Intention` a witness `Observation`
was gathered for. Keep its actual observed feature, vantage, method and supplied
world context. Observation purpose neither changes its feature to the intention
nor proves the intention's goal was satisfied. A simulated observation retains
its simulation provenance rather than acquiring a real-world truth claim.
See the [complete story-building scene](../../extensions/narrative/examples/story-building.ttl)
for these records alongside story-stage and recollection metadata.

## Goal conditions, verifier configuration and self-commitments

Goals retain ordered source Condition occurrences through
`hasSatisfactionCondition`. A ConditionVerifier is a configuration descriptor
with an exact source key and `conditionVerifierKind`, distinct from an actual
ArtifactVerificationObservation or a rubric Criterion. The seven additional
method values preserve artifact existence, external observations, user
assertions, retained-memory evidence, reflective appraisals, simulated-world
existence and foreground yields. Configuration never becomes execution.

Actual `logic:GoalEvaluation` records retain `conditionKey` and fallback
`evidenceItem` wording. Conclusive positive evaluations must cover every source
condition and retain the kind-specific structured evidence sources; fallback
prose cannot substitute for a cited observation, memory, note, provider record
or actual foreground-yield event. Existing read-after-write contracts remain
separate and unchanged.

Attempt starts can retain a prose `followedPlan` with `planText`; finishes
carry `attemptSummary`, `bodyCause` and `nextAction`. An Intention can preserve
`adoptedFrom`, its self-`commitmentText` and `commitmentOrigin`, source
`deadlineAt` and `nextReviewAt`. A closed IntentionTenure carries
`releaseReason`. Source events retain `eventReason`, exact `engineContract`
and writer-scoped `intentionEventSequence`; ToolCalls retain unparsed
`auditText`. Original numbering is scoped by actual intention, writer and
source context, independent of zero-based list ordinals and clock order.

Prose retains supplied language and direction; opaque identifiers preserve all
source code points as strings. See
[`GOALS-AND-SATISFACTION.md`](design/GOALS-AND-SATISFACTION.md) for the canonical
laws, evidence paths and honest limits (Principles 2, 3, 4, 9 and 17).

## Runtime trace kinds and lifecycle readings

The trace vocabulary contains source-reported EventType individuals, selected
through the existing nonfunctional eventType relation. A trace label identifies
an occurrence's reported kind; it is not the payload artifact, an executor,
an admission receipt or a terminal outcome. Multiple independently supported
kinds and attributed claims may coexist (Principles 2, 4 and 9).

### Thinking and model exchange

- `gmeow:eventTypeTraceRound` — thinking trace round.
- `gmeow:eventTypeTraceTurnStart` — thinking trace turn start.
- `gmeow:eventTypeTraceTurnOutput` — thinking trace turn output.
- `gmeow:eventTypeTraceModelRequest` — model request trace.
- `gmeow:eventTypeTraceModelResponse` — model response trace.
- `gmeow:eventTypeTraceModelError` — model error trace.
- `gmeow:eventTypeTraceGeneration` — model generation trace.
- `gmeow:eventTypeTraceMultimodalGeneration` — multimodal generation trace.
- `gmeow:eventTypeTraceImageGeneration` — image generation trace.

### Tool exchange and checkpoint reports

- `gmeow:eventTypeTraceToolCall` — tool call trace.
- `gmeow:eventTypeTraceToolResult` — tool result trace.
- `gmeow:eventTypeTraceToolReceipt` — tool result receipt trace.
- `gmeow:eventTypeTraceCheckpointRequest` — checkpoint request trace.
- `gmeow:eventTypeTraceCheckpointResponse` — checkpoint creation response trace.
- `gmeow:eventTypeTraceCheckpointValidation` — checkpoint validation trace.

### Episode accounting, interruption and correction

- `gmeow:eventTypeTraceEpisodeContinuation` — episode continuation trace.
- `gmeow:eventTypeTraceEpisodeTokenAccounting` — episode token accounting trace.
- `gmeow:eventTypeTraceEngineAuthoritativeTokenCount` — engine-reported authoritative token count trace.
- `gmeow:eventTypeTraceForegroundPreemption` — foreground preemption trace.
- `gmeow:eventTypeTracePromptProvenance` — prompt provenance trace.
- `gmeow:eventTypeTraceCorrection` — thinking correction trace.

### Response rejection, replay, recovery and runtime limits

- `gmeow:eventTypeTraceResponseCompletionRejected` — response completion rejection trace.
- `gmeow:eventTypeTraceResponseContractRejected` — response contract rejection trace.
- `gmeow:eventTypeTraceDegenerateRepetitionRejected` — repetition rejection trace.
- `gmeow:eventTypeTraceToolNoProgressCycle` — tool no-progress cycle trace.
- `gmeow:eventTypeTraceImmutableToolReplay` — immutable tool replay trace.
- `gmeow:eventTypeTraceInterruptedToolBatchReissuedByBody` — interrupted tool batch reissue trace.
- `gmeow:eventTypeTraceToolResultRestoredFromSuspensionReceipt` — tool result restoration trace.
- `gmeow:eventTypeTraceToolResultsHeldByReceipt` — tool results held by receipt trace.
- `gmeow:eventTypeTraceCacheReceiptRejected` — cache receipt rejection trace.
- `gmeow:eventTypeTraceBackgroundTerminalRecovered` — background terminal recovery trace.
- `gmeow:eventTypeTraceReplyGateFired` — reply gate firing trace.
- `gmeow:eventTypeTraceDreamPhaseRoundCapReached` — dream phase round cap trace.

The runtime-specific values remain shared source vocabulary. Their definitions
retain the provider's contract boundaries rather than imposing one universal
runtime. In particular, an engine's reported authoritative count is not a
privileged truth, image generation does not imply multimodal generation, and
cache, replay, receipt and terminal-recovery reports execute no mechanism.
Checkpoint labels do not substitute for native checkpoint admission evidence.
Tool calls retain their existing actual-tool contracts; a trace marker cannot
manufacture the missing tool or a second execution.

ModelInvocationState is an open value vocabulary, separate from
ModelInvocationOutcome. invocationState belongs to an actual Observation,
specializes observationResult and names a state of the actual ModelInvocation
in observedFeature. invocationStateRunning records an explicit in-progress
reading. Keep that observation's vantage, source and supplied time context;
do not infer running from a missing outcome or choose a global latest state.
A later succeeded or failed invocationOutcome can coexist with the earlier
reading. Observation time, record creation time and invocation occurrence time
remain distinct; missing precision or timezone is never invented.

injectionCategory retains the screen's exact source code as its supplied
xsd:string or xsd:token on an Observation of the actual outside-answer artifact.
Keep source whitespace, case and datatype, the observing vantage and the
retained screening evidence. The code is not injected instruction text, a
universal taxonomy or proof that an attack succeeded. These declarations
implement neither token normalization nor a lexical validator.

The two canonical evidence laws have direct binary field guards and therefore
target subjects supplying the new fields, including malformed subjects. Each
requires the actual observed feature, vantage and retained information-object
source. Property-global closure entries validate supplied domain/range values
without requiring those fields on every observation. Neither selector is
functional and no state-transition engine or current-state chooser is added.

The [runtime trace scene](examples/runtime-trace-states.ttl) retains all trace
kinds, an earlier running reading and a later terminal outcome, and an
outside-answer screening claim. These are illustrative source records, not an
execution receipt or an authentication result.

## Creatures, wants and answering occurrences

The creature event kinds classify source-reported occurrences through the
existing nonfunctional eventType relation. They mint no Creature subclass and
do not identify a real animal, a fictional Character, a simulated actor and
the program supplying its records as one thing.

| EventType value | Reported occurrence |
| --- | --- |
| eventTypeCreatureGenesis | The creature came into being within its source environment. |
| eventTypeCreatureRenamed | Its name changed while the source retained its identity. |
| eventTypeCreatureTransition | It moved between places, independently of a mood or status change. |
| eventTypeCreatureUtterance | It uttered something; actual words remain on their message artifact. |
| eventTypeCreatureMemory | It remembered a moment; the memory record is distinct from remembering. |
| eventTypeCreatureMoodShift | Its background mood changed, without choosing a latest global mood. |
| eventTypeCreatureDemand | It expressed a demand, distinct from the underlying Desire. |
| eventTypeCreatureDemandResolved | A demand was answered; this is not a satisfaction or acceptance verdict. |

DemandKind is an open QualityValue vocabulary. demandKind belongs to the actual
Desire, whose actual intentBearer and intentionGoal remain required. The
initial choices are demandKindAnnouncement, demandKindCuddles, demandKindLap,
demandKindPets and demandKindPlay. Announcement leaves speaker, wording and
audience to the source. Pets means petting; lap keeps the source’s interaction
meaning without inventing a physical body or location. Several kinds can coexist.

DemandResponse is a separate open QualityValue vocabulary. demandResponse
belongs to the actual answering Event and selects demandResponseCuddle,
demandResponseLap or demandResponsePet, or another explicitly defined kind.
answersDemand identifies the exact actual Desire addressed by that Event.
The response and demanded kinds need not match; a response does not establish
enjoyment, consent, acceptance, an improved mood or satisfied goals.

Each supplied demandKind, demandResponse and answersDemand assertion retains
its exact native RDF 1.2 claim with accordingTo naming the reader and
wasDerivedFrom naming the documentary source. The writer may differ from both
reader and creature. Canonical evidence laws preserve the actual Desire,
bearer, Goal, Event and source identities. The answered-event kind also requires
answersDemand when no response-kind code was supplied. Unknown response codes,
times, goals and identities are never fabricated.

The [ordinary creature scene](examples/creature-demands.ttl) keeps the acting
software agent, provider program, source record and provider world separate.
Its moods use the affect owner’s complete Mood contract. All reported events
remain source-scoped; source structure supplies no simulation execution,
actual-world travel, automatic delivery, native mood ingestion or EmotionML
export (Principles 2, 4, 9, 11, 17 and 19).

## Machine identifiers retain source datatypes

`receiptTool`, `verifierKey`, `expectedWorldObject`, `conditionKey`,
`engineContract` and `injectionCategory` admit supplied `xsd:string` and
`xsd:token` literals through canonical datatype unions. Keep the complete
supplied literal, including lexical form and datatype. Do not normalize source
whitespace or infer token typing from a string's spelling.

The key joining a goal condition, configured verifier, evaluation and exact
evidence citation is the same complete source literal within that goal and
citation. Matching spelling from unrelated producers remains independent.
A tool name does not identify a tool agent, and an unresolved object identifier
needs its provider-and-world context. The datatype union neither resolves
references nor supplies evidence of execution, success or contract compliance.
Existing required paths, scopes and integrity formulas remain intact.

## Durable pursuit records and closure reports

An `OpenLoop` is a durable information record of a pursuit, retained across
episodes and after reported closure. It is neither the pursuing agent's
`Intention` nor a `Plan`, episode or attempt occurrence. Its type supplies no
default open state. Existing `description` retains the source's unstructured
wording.

`loopTaskDocument` identifies a supplied task-specification `InformationObject`.
It does not name the `Task` occurrence or force a concrete source artifact into
the abstract `Document` Work type. `loopServesIntention` retains an actual
source-supplied link to an existing `Intention`. That Intention keeps its own
supplied `intentBearer` and goal. Neither a declaring writer nor a recorder
identifies the pursuing agent by implication.

The open `LoopState` vocabulary has three seed assessments:

| Value | Declared source assessment |
| --- | --- |
| `loopOpen` | The pursuit remains open. |
| `loopDone` | The pursuit is recorded as done. |
| `loopLetGo` | The pursuit is recorded as let go. |

Record each `loopState` assertion with its exact RDF 1.2 context and provenance.
Missing state is unspecified. Historical and conflicting claims coexist; the
vocabulary selects no default, current winner or latest timestamp. Done proves
neither successful execution nor goal satisfaction. Letting go deletes nothing,
grants no cancellation permission and does not abandon a linked Intention.
No transition or reopening is computed (Principles 9 and 11).

A `LoopClosure` is a separately identified information report about closure,
not the closing Event or Activity. `closureLoop` names the reported loop.
Its other roles remain separate:

| Relation | Reported role |
| --- | --- |
| `closedBy` | The source-identified actual closing Agent. |
| `wasAttributedTo` | The recording Agent. |
| `accordingTo` | The selected declaring writer or standpoint. |
| `closureEvidence` | The cited information evidence artifact or exact native reifier. |
| `wasDerivedFrom` | The source artifact supplying the report. |

These distinctions separate roles; they do not require different identities
when a source actually assigns several roles to one agent. Unknown actual
actors remain unknown. Evidence and consumer-specific closer codes do not
create an Agent identity or make the recorder the closer. Such local role
values remain local until their actual referents are established.

Use existing domain-free `statedRationale` for the source's exact reason,
retaining language and RDF 1.2 base direction. It does not turn an information
report into an Activity or Decision, or establish that the explanation is true.
When citing a fact about an occurrence or Observation, link an information
artifact or exact `rdf:reifies` record through `closureEvidence`; do not retype
the underlying occurrence as `InformationObject`. Citation establishes no
verification, entailment, authenticity or permission.

The new properties are nonfunctional and type supplied values without requiring
them. No closure report rewrites state or produces an enactment receipt or goal
evaluation. These records add no execution or permission protocol (Principle 17).

The ordinary [loop-record scene](examples/loop-records-and-closure.ttl) retains
all three states, task-artifact and Intention links, historical and competing
claims, separate reporting roles and a closure report with an unknown actual
actor. It reports no executed scheduler, closure, test, state update or
successful goal.

## Loop references and current-loop attention

A `LoopReference` is an independently identified information record. Its
`referencedLoop` names a source-supplied `OpenLoop`; `referencedTaskDocument`
names a supplied task-specification `InformationObject`. The reference is
neither target. Both links are nonfunctional and constrain supplied values
only: this vocabulary does not enforce a complete-reference record.

Keep the original target identities. A task-artifact link is not copied from
`loopTaskDocument`, selected by a latest-revision policy or forced equal to
another target. Missing targets remain unspecified. Naming an IRI establishes
neither resolution nor availability.

`slotCurrentLoop` is an `InformationObject` role descriptor used through the
existing `slotRole` relation. It is not a mutable property, slot, history
position, pointer or loop record. Follow the existing protocol:

1. The actual `AttentionSlot` has its source-supplied Agent `slotBearer`,
   `slotRole slotCurrentLoop` and history `Path` through `slotHistory`.
2. `slotState` names a `SlotPosition` belonging to that slot through
   `positionInSlot`. The position's existing IRI `slotPointer` targets the
   actual `LoopReference` record, which retains the supplied target links.
3. Each actual `slotState` assertion has its typed `SlotAssignmentClaim`
   native RDF 1.2 reifier. Source-supplied `supersedes` relates actual assignment
   claims, retaining earlier records and competing views.

These are the existing attention-slot requirements, not a new state protocol.
The role descriptor creates no assignment or current winner. Same-slot and
bearer-history checks and target resolution remain consumer obligations; the
descriptor proves none of them. Supersession supplies no global total order,
and no timestamp selects a current loop. The stronger generic
`StateAssignmentClaim` profile is not selected here.

Slot bearer, declaring writer, recorder and actual pursuer remain separate
roles, with identity overlap allowed when supplied by the source. A slot bearer
or recording attribution supplies no missing pursuer identity, authentication
or permission. The loop's own source-supported Intention link retains its
separate bearer and goal (Principles 9 and 11).

`notebookLoop` associates a `NotebookEntry` with an existing `OpenLoop` through a
nonfunctional supplied-value link. It copies neither state nor content, infers
no Intention or pursuer and imposes no common writer. Preserve the entry's
existing content, kind, state claims and attribution. Matching words, a shared
author or a current-slot claim cannot supply an unstated association.

The ordinary [loop-reference scene](examples/loop-reference-and-attention.ttl)
uses actual reference records as slot-pointer targets, retains distinct source
task artifacts and notebook content, and supplies the existing slot and
position fields, typed assignment reifiers and explicit supersession. It
reports no executed history check, resolver, state update or scheduler
(Principle 17).

## Offered guidance as retained content

`Guidance` specializes `Note` to retain a source's offered guidance as content.
Reuse `noteContent` for its wording and domain-free `statedRationale` for a
source-supplied reason. Source derivation through `wasDerivedFrom`, recording
attribution through `wasAttributedTo` and the declaring frame through
`accordingTo` remain separate. A supplied reason establishes no verified cause.

The note is an offer record. Its classification creates no command, Norm,
permission, Intention, adoption, execution or computed preference winner. The
versioned rubrics and checklists of `GuidanceSet` retain their separate meaning.
No offer is silently converted into governing guidance or a required action.

`guidanceLoop` is a nonfunctional, supplied-value association from the offer to
an existing `OpenLoop`. It copies neither state nor content. Matching words,
note authorship, a recording Agent or an attention selection cannot supply an
unstated association, proposer or pursuer. A loop's own source-supported
Intention retains its separate bearer and goal.

The named `slotCurrentGuidance` role reuses the existing attention-slot protocol:

1. An actual `AttentionSlot` retains its source-supplied Agent `slotBearer`,
   `slotRole slotCurrentGuidance` and history `Path` through `slotHistory`.
2. Its selected `slotState` names a `SlotPosition` belonging to that slot through
   `positionInSlot`. The position's IRI `slotPointer` targets the actual
   `Guidance` offer note.
3. Every actual `slotState` assertion retains its typed `SlotAssignmentClaim`
   native RDF 1.2 reifier. Source-supplied `supersedes` relates assignment
   records while retaining earlier claims and competing views.

The role is an `InformationObject` descriptor, not a mutable property, slot,
position, pointer or offer. It creates no assignment, global current winner,
resolver, authentication or permission. Same-slot/bearer history checking and
target resolution remain existing consumer obligations. No stronger generic
state-assignment profile or new protocol is introduced.

Note author, declaring frame, recorder, slot bearer, proposer and actual pursuer
are separate roles. The same source-identified Agent can occupy several roles,
but none supplies a missing identity by implication (Principles 9 and 11).

This foundation defines no activity target type, ahead-task target type,
ordering index base or ahead-anchor time representation. It claims no
equivalence to unspecified consumer fields.

The ordinary [guidance-offer scene](examples/guidance-offer-and-attention.ttl)
retains two offer notes, their supplied loop associations, source reasons,
separate recording roles and the existing typed assignment history. It reports
no executed scheduler, history validation, resolver or adoption of an offer
(Principle 17).

## Episode-transition reports

`EpisodeTransition` is an information report of a source-reported departure
from an episode Activity and entry into an episode Activity. It is distinct
from the episode occurrences, a persona transition and a state update.

The existing runtime episode model treats episodes as Activity sub-aggregates.
`Activity` is the provenance specialization of Event that acts upon or with
entities; the broader Event class also admits generic happenings. Accordingly,
`episodeLeft` and `episodeEntered` name the actual source-identified Activity
occurrences, not their descriptions, source documents or slot positions.

`transitionLoop` identifies the source-supplied OpenLoop record named as the
loop entered. It changes no loop state, selects no current slot and infers no
Intention or pursuer. The loop's own identity and any supplied Intention
evidence remain independent.

All three relations are nonfunctional and constrain supplied values only. No
target is required or fabricated, and competing source reports remain
attributable. Departure and entry preserve the source's meaning; they compute
no timestamps, completion, chronology, causal order, scheduler action,
permission or current winner (Principles 9, 11 and 17).

Keep each report's source artifact through `wasDerivedFrom` and recording Agent
through `wasAttributedTo` independent. For each supplied endpoint or loop
assertion, use an ordinary native `rdf:reifies` record retaining that exact
statement's source through `wasDerivedFrom`, recorder through `wasAttributedTo`
and declaring frame through `accordingTo`. Report metadata does not propagate
to constituent assertions. These roles do not identify episode performers or a
loop's pursuer by implication. When a source supplies an actual Activity performer, retain that
existing provenance relation on the Activity. One source-identified Agent may
occupy several roles; no inequality is imposed.

The ordinary [episode-transition scene](examples/episode-transition-reports.ttl)
retains actual Activity endpoints and loop records in two coexisting reports,
with report-level source and recorder metadata plus six exact-statement
reifiers retaining source, recorder and declaring frame. It reports no
locally executed transition, state update or scheduler, and derives no order
between the endpoint activities.

## Daily budget counter readings

`BudgetReading` is an `InformationObject` retaining a ledger’s actual daily
runtime counters. It has exactly one `budgetDay`, one `budgetSpender` and one
`budgetLedgerSchema`, plus at least one actually supplied `budgetRequests` or
`budgetCalls` quantity. Each count role has at most one quantity. A historical
request-only report remains request-only; absence of a web-call field never
becomes zero. This class creates no observation, vantage, successful execution,
payment, corpus coverage or aggregate total (Principles 4, 9, 11 and 17).

`budgetDay` preserves the supplied `xsd:date` as a UTC civil day. It is neither
a reporting instant nor a local day; do not invent midnight. Retain timezone-free
dates and dates with Z, +00:00 or -00:00. Reject an explicit nonzero timezone
offset without converting or rewriting the literal; preserve allowed XML Schema
whitespace. The day’s explicit
UTC convention does not supply the numerical counting frame. The exact supplied
`xsd:token` on `budgetLedgerSchema` identifies the schema declared by the writer.
Do not guess that identity from fields, compute a replacement schema hash,
normalize the literal or silently convert an `xsd:string` to a token.

`BudgetSpender` is an open `logic:QualityValue` axis for deployment-local budget
buckets. A bucket is not its consumer agent, account or payer. Local deployments
may introduce their actual named values without extending a global enumeration.
`budgetSpenderUnbucketed` means the explicit historical top-level request bucket
from before per-consumer buckets existed. It is never a missing, unknown or
unmapped-spender default.

Both count roles point to the existing [Math Quantity](../../grounding/math/docs.md)
class. Keep the original supplied `xsd:nonNegativeInteger` literal, including its
lexical form and datatype, on `math:quantityValue`; use `math:dimensionless` on
`math:hasDimension`. Every supplied magnitude must be a valid numeric value, and
all retained magnitudes of that quantity must be numerically equal. Equivalent
source-supported lexical aliases may coexist without rewriting either literal.
No parallel raw numeric role or duplicate quantity subclass is introduced.

Each quantity requires exactly one actual `gmeow:hasReferenceFrame` value typed
as `gmeow:ReferenceFrame`, with the complete existing
[Places frame profile](../places/docs.md). The frame’s actual interpretation must
be justified by source evidence or the adapter’s explicit interpretation and
scoped to the ledger, UTC day, spender and declared schema. It must identify the
counting convention: `budgetRequests` counts external model requests issued;
`budgetCalls` counts web calls written down before running. Issued requests do
not imply success or payment, and preregistered web calls do not prove execution
or completion. Reusing a frame is sound only when its actual interpretation
covers the same complete context and convention; merely naming a generic frame
or substituting a temporal frame is insufficient.

The supplied-value domain and range policies, report-local requiredness and
qualified maxima, and `BudgetCountValueConstraint` are authored in canonical
`logic:`. The Formula’s separate positive guards enforce count presence,
magnitude presence and validity, numeric equality, dimension presence and value,
frame presence, type and uniqueness, and zero offset whenever budgetDay has an
explicit signed timezone suffix. Existing frame constraints govern the
linked frames. These checks do not establish that the source counters are true
or complete, or that an asserted frame interpretation is supported by evidence.
Keep actual ledger provenance and interpretation evidence; do not fabricate
values, frames, sources or outcomes to satisfy validation.
