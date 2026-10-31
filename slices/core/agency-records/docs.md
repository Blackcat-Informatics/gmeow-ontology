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
to `artifactComplete` on the identified artifact or version. There is no canonical
`receiptComplete` property here: the source requirement establishes an artifact
bit, not a new receipt-completeness or operation-success capability.

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

injectionCategory retains the screen's exact source code as xsd:string on an
Observation of the actual outside-answer artifact. Keep source whitespace and
case, the observing vantage and the retained screening evidence. The code is
not injected instruction text, a universal taxonomy or proof that an attack
succeeded. The declaration neither admits an unsupported token datatype nor
implements token normalization or a lexical validator.

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
