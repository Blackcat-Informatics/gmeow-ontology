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

## Episode transitions name Activity endpoints

The existing [runtime stack charter](../../inhabitation/design/INHABITED-RUNTIME-STACK.md)
models sessions as Activity aggregates and episodes as Activity sub-aggregates.
Its explicit episode example and the requested activity-change meaning support
Activity endpoints. Event would widen the contract to generic happenings with
no agent, generation or attribution role. The older illustrative event-kind
spellings in that charter are not copied into this source vocabulary.

EpisodeTransition specializes InformationObject as a report, not an occurrence.
episodeLeft and episodeEntered retain actual episode Activity identities;
documents or proposed activity descriptors cannot replace those endpoints.
The source-reported departure and entry remain separate from any computation
of timestamps, chronology, causal order, completion or execution.

transitionLoop names the source-supplied OpenLoop recorded as entered. No rule
copies state, assigns a current slot, identifies an Intention or infers a
pursuer. Each report retains its documentary source and recording attribution
independently. Each exact endpoint or loop assertion has an ordinary native
rdf:reifies record carrying its own source, recorder and accordingTo declaring
frame. Report metadata does not propagate to constituent assertions; no new
protocol class or type requirement is introduced. A source-identified Activity
performer is retained through existing
provenance; neither recorder nor frame supplies that identity. Distinct roles
do not require distinct Agents.

Canonical logic: domain/range and class-local all-values restrictions type only
supplied links. They require no endpoint or loop target, impose no functionality
or exclusivity and introduce no temporal, causal, permission or state rule.
Existing integrity formulas and manifest dependencies remain unchanged.

This foundation defines no chain-claim attachment, duration calculation,
cause-rule or cause-fact representation, interlude structure or ordering index.
The ordinary scene illustrates retained source reports; it supplies no runtime
transition, scheduler, history-validation or generated-surface qualification.

## Estimated ledger cost keeps its declared basis

The daily report's optional budgetEstimatedMillicents role points to the
existing math:Quantity counting original declared millicent units. Its valid
finite xsd:nonNegativeInteger magnitude remains unchanged on math:quantityValue;
numeric lexical aliases can coexist with one numeric value. Math owns the
dimensionless unit-count dimension. The quantity's actual complete reporting
frame retains the ledger/day/spender/schema and pricing interpretation.
This neither declares a monetary amount's physical dimension nor introduces a
parallel scalar, monetary amount class or observation.

The unit definition fixes one millicent to 1/100000 of the declared currency's
major unit. RationalValue syntax in the guide explains this coefficient only;
no disconnected coefficient node, scale role, unit hierarchy or conversion
law is added. The role preserves a source table-based estimate, not a measured
charge, bill, payment or statistical Estimate with invented targets/procedures.

budgetCostCurrency retains the actual source-declared major-unit basis, with
its complete scalar, one-dimensional currency ReferenceFrame. The major-unit
meaning is an explicit declaration, not an inference from code spelling or
an official minor-unit exponent. Places owns the complete frame obligations;
an appropriate existing scalar axis can serve without a new currency-specific
axis or numerical component. Missing actual context cannot be repaired by
manufacturing a frame or a default currency.

budgetCostPriceTable retains all actual declared releases as InformationObjects,
including their supplied identity and interpretation. It has no maximum and
creates no composite table, current-version selector, rate rule or digest.
Neither table identity nor the declaration proves execution or correct pricing.
Historical counter-only reports acquire no cost requirement; selected cost requires its
actual context and does not silently degrade when that context is missing.

The grouped ordinary Formula has one outer BudgetReading type guard and fifteen
independent positive cost/context arms: eight for the selected quantity, four
for currency presence/type/profile/coordinate-dimension value, one for currency
uniqueness and two for declared table presence/type. Each arm binds its focus
and local universals through actual positive relations; all five existential
bodies are atomic and every binder is fresh. Exact datatype identity selects
shared component admission for the entire law, including context arms. The
datatype/finite conjunction and realm/kind conjunction distribute into seventeen
consequence leaves without introducing a new native mechanism.

Property-global closure validates supplied endpoints. Direct class restrictions
type all three roles and bound the cost quantity and coherent currency basis
locally, with no table cap or global functional property. Cost presence is not
required on every BudgetReading; only a supplied cost selects the context law.
Existing counter and UTC-day axioms, source literals, metadata and catalog
contexts remain untouched. Existing Math, Places, Kernel and provenance owners
are already direct dependencies; no Finance extension dependency is introduced.

These declarations cannot establish source truth, actual major-unit meaning,
table authenticity, supported frame interpretation or computed cost. Those
remain actual source/adapter evidence obligations, distinct from the intrinsic
structural contract. No runtime normalization, financial conversion, estimation
execution, corpus or generated-surface qualification follows from authoring
these terms (Principles 4, 7, 9, 11, 17 and 19).

## Episode duration retains a source report

`episodeLasted` is a canonical DatatypeProperty with EpisodeTransition domain
and xsd:duration range. It records the source-reported duration of the actual
episode named by episodeLeft in the report's supplied context. The persona
owner's transitionLasted cannot serve this role because its domain is
ModeTransition. No persona property or functional characteristic is copied.

One property-global ClosedWorldClosure selects supplied-subject and
supplied-object validation. No class-local restriction, required endpoint,
maximum, numeric facet, Formula or duration comparator is introduced. The
property remains optional and nonfunctional; competing exact claims retain
their own source and declaring frame. Existing endpoints and ordinary scenes
remain unchanged.

The exact literal, datatype, lexical form and units survive. Its native reifier
keeps documentary source, recorder and declaring frame separately; report
metadata does not propagate. A source-computed span retains only its actual
supplied inputs and the supplied calculation and input roles. An omitted
endpoint or derivation cannot be fabricated to establish source correspondence.
No timestamp subtraction, clock precision, epoch, timezone, execution,
completion, causal order or global current winner is inferred. This upstream
contract establishes no private-extension equivalence or consumer adoption
(Principles 4, 9, 11 and 17).

## Complete reported chains and offered anchors

The chain attachment retains an actual native claim, rather than selecting a
private chain schema. episodeTransition targets its EpisodeTransition report;
the subject's actual rdf:reifies witness remains mandatory. transitionCauseFact
requires its own actual quoted witness. transitionCauseRule instead retains
an actual identified InformationObject specification. Citation does not create
a productive logic:Rule, ReasoningProgram, asserted fact or causal proof.
The two claim-witness arms preserve these exact bindings without authenticating
the source or executing its scheduler.

EpisodeInterlude is a scalar StructuredValue occurrence, not a new Activity
kind. Its actual Activity is retained through structuredTerm, under the existing
scalar branch and whole-term laws. Six guarded arms require one transition
parent and the scalar payload, type that payload and enforce numeric ordinal
value and report-local position uniqueness. Parent and payload existence retain
explicit positive inner class guards and atomic witnesses. Repeated payload
identities use distinct occurrences. No ordinal belongs to the shared Activity.

AheadAnchor specializes LoopReference to one offer-local task reference. Four
arms require one offer parent and enforce numeric ordinal value and offer-local
position uniqueness. Its required task artifact remains an InformationObject,
not an actual Task event. guidanceActivity points to an actual EventDescription
of the offer, without synthesizing an occurrence, Intention or adoption.
An independently supplied loop reference is preserved without copying one.

Both new occurrence classes require zero-based entryOrdinal values. Numeric
lexical aliases of one value remain valid and unchanged; no literal maximum
or contiguous-position rule is introduced. Resource comparisons retain the
actual parent/occurrence identities. These are record positions, not temporal
or causal order. No RDF list, global ordinal or preference winner is added.

Optional aheadWindow is locally at most one TimeInterval. Eight window-focused
arms require actual interval-frame evidence and type it, type each supplied
Instant boundary, and require and type its actual frame evidence. All three
frame-existence bodies are atomic. Unknown boundaries stay unspecified;
actual frames need not be equal. Existing Instant value-or-EDTF, inline datatype
and TemporalFrame contracts remain with Temporal. No boundary, conversion,
clock field or start-before-end proof is fabricated.

The new AgencyRecords structure is four Constraints with twenty guarded arms,
five direct restrictions, three class/path closures and eight new-property
global supplied-endpoint closures. Required paths apply only to the new
occurrence classes. Existing generic roles are not narrowed globally. Actual
existence, source correspondence and provenance remain distinct from structural
checks; report metadata does not propagate to exact assertions.

The Notes owner adds notebookStatement and its directly guarded same-term
quotation law; AgencyRecords owns the separate optional NotebookEntry-to-Activity
notebookWorkingThrough association, as it already owns notebookLoop. No truth,
belief, ongoing-work state, shared writer/performer or chronology follows.
Inherited Note content-or-label remains required. Existing declared dependencies
already cover every owner; no return dependency or parallel state, rule or time
mechanism is introduced (Principles 4, 7, 9, 11 and 17).

## Model-call mechanics as an information report

ModelCallMechanics belongs to this source-record owner as a SubKind of
InformationObject. Its actual mechanicsOf invocation is mandatory; one of
the ten source-valued datatype roles must be present. Completely unstated
mechanics does not require fabricating a report. Multiple actually sourced
reports can describe the same invocation without being merged. The report
neither subclasses the occurrence nor becomes an Observation imposing
unrelated observation fields.

The original timing, queue, session, usage, unreported and failure fields are
retained directly. Queue and token metrics remain supplied
xsd:nonNegativeInteger literals, with numeric value uniqueness and original
lexical aliases intact. This differs from BudgetReading's actual Quantity
representation: its source ledger uses framed count quantities, but machine
counters do not acquire four manufactured Quantity identities solely to
hold their supplied literal values. No new conversion or arithmetic engine,
coefficient or parallel unit field is introduced.

The two typed frame companions have distinct binding duties. Actual start
or finish dateTime literals require mechanicsTemporalFrame. Token counts
require mechanicsCountingFrame. Both specialize the domain-open generic
hasReferenceFrame; neither reuses a TimeInterval-domain property or caps the
generic spine. Full actual TemporalFrame/ReferenceFrame owner obligations
remain in force. No frame identity, provider, tokenizer, clock or calendar
is guessed, and no equality between the two frame resources is inferred.

Queue delay is different: its fixed declared milliseconds denotes an
unanchored elapsed magnitude. A delay alone creates no requirement for
TimeScale, FrameRealm, FrameKind, calendar or clock frame. Optional
actually supplied timing context remains permissible. Requiring a frame
merely because the value is numeric would fabricate a dependency rather
than make a source measurement more faithful (Principle 11).

The independent axes remain independent. Finish does not imply successful
generation. An opaque provider slot-session token does not type an
AgentSession or RuntimeExecution. Provider failure stage and diagnostic are
independently supplied source evidence; neither forces a missing companion
or writes an invocation outcome/state. A missing counter or unreported
marker remains unspecified, never zero, false or successful.

### Exact canonical obligations

The term batch is fourteen core terms and two Constraints. Thirteen
all-values restrictions cover the three object and ten datatype roles.
Eight qualified maxima cover the three object roles and five singleton
nonnumeric literal roles. Object maxima use onClass logic:Thing; literal
maxima use the actual onDataRange datatype or literal union. Numeric roles
have value equality laws rather than a lexical-triple maximum;
mechanicsUnreported is uncapped. One report-local closure requires
mechanicsOf and thirteen supplied-value closures validate actual endpoints.

The Values law has fifteen independent positive-guarded arms: one actual
field-presence disjunction, four exact datatype/finite numeric admissions,
four numeric alias equalities, one cached-subset bound, two required endpoint
frame witnesses and three required token-count frame witnesses. Queue delay
has no conditional frame arm. The Unreported law has three independently
guarded forbidden-count arms selected only by the actual token usage.

Every arm positively binds its focus through mechanicsOf or an actual
supplied field. Atomic existentials provide required frame/field witnesses;
the field-presence body is a finite disjunction of atomic existentials.
The forbidden-count arms use guarded not-exists. No positive-position
Forall, dynamic property lookup or new generic marker interpreter is used.

The actual exact-datatype predicate uses bound termSameComponent with fixed
componentDatatype/componentWhole selectors, together with termIsFiniteNumeric
to validate the lexical value. Checking datatype identity alone would not
admit a malformed nonNegativeInteger. The cached bound is represented as
Not(termGreater(cached,prompt)) under those finite numeric laws, using the
actual canonically declared comparator. Equal numeric aliases are compared
by value with termEqual, without rewriting supplied RDF terms.

The represented laws are not executed evidence. Real source/catalog/native
and generated projection qualification, exact conditional frame evidence and
consumer adoption remain requirements. Actual source producers may have
different wire grouping or serialization conventions; no private alias,
equivalence, migration or compatibility is asserted by the names.

## Reply-handling accounts and heterogeneous result references

ReplyHandlingReport is an InformationObject source account. Its one actual
invocation scope and one reported shape belong to that account; several
independent accounts can coexist for the same invocation. Its purpose is
concrete: retain classification and result associations without introducing
a forbidden forward output property on ModelInvocation or automatically
turning a trace/Experience into an information report. It is not a raw-reply,
group/side or processing-event identity layer.

ReplyShape remains an open QualityValue axis. Decomposed and fell-back values
retain the source's actual classification without implementing or verifying
the handling operation. A fallback history is not permission to degrade a
selected capability silently. The shape does not imply result count,
order, roster completeness, generation success, delivery or lifecycle status.

replyBecame has Report-to-resource association meaning. It retains genuine
source references including opaque IRIs, blank nodes and actual Events.
Neither co-transaction grouping nor a report's invocation scope establishes
generated-output ownership or derivation of every associated result.
No subproperty, inverse or property chain turns this association into
wasGeneratedBy or wasDerivedFrom. No raw reply or missing lineage witness
is synthesized.

Real generated reply artifacts and enduring materialized products retain
their actual wasGeneratedBy backlinks when known. The materializing activity
may differ from the original model invocation. Genuine source-artifact
lineage retains domain-open wasDerivedFrom. An associated Event must not
receive a generated-output link simply to fit Entity's enduring-product
domain. These independent owner contracts are unchanged.

### Exact canonical structure and raw endpoint admission

The unit contains seven terms: two classes, two open value individuals and
three ObjectProperties. Three ReplyHandlingReport-local allValuesFrom
logic:Thing restrictions project to BlankNodeOrIRI, admitting opaque resource
references without a raw concrete type witness. Two maxQualifiedCardinality
one/onClass logic:Thing restrictions and two class/path ClosureEntry records
require exactly one scope and shape per actual report.

There are zero property-global closure opt-ins, Constraints, messages, new
dependency edges or result cardinality/order constraints. Domain/range keeps
its ordinary open-world inference reading. The class-scoped presence policy
does not close concrete endpoint types globally. Adding a closed Activity,
Entity or ModelInvocation endpoint check would reject legitimate opaque
references and is not part of this source contract.

No existing source trace, private identifier or broad source association is
asserted equivalent to this canonical account. Source, language, projection
and native qualification remain distinct from authoring a vocabulary. Actual
retained raw replies, per-call result ownership, real derivation and producer
adoption must be supported by their actual sources, not by the names alone.

## Perception occurrences and source-reported prompts

AgencyRecords owns hearing/seeing event-kind values and their source-relative
links, composing the existing Activity, Message, provenance and event owners.
The actual hearing/seeing node is an occurrence, retained as an activity
record; its source information artifact and exact statement reifier are
distinct. It is not made an InformationObject merely by being recorded.
Message typing alone proves no receipt, and recipient addressing or sending
is not hearing evidence. Observation retains its own vantage/observedFeature
claim contract, ObservationalActivity its observation-producing purpose, and
Experience its phenomenal meaning; none is inferred here.

The seeing occurrence has its actual observer through wasAssociatedWith;
sawAct targets a distinct actual Activity whose performer and supplied place
remain its own. heardMessage targets the actual communicative Message, whose
speaker attribution is distinct from the hearer, recorder and declaring frame.
Messaging's domain-free saidOnBehalfBy retains an explicitly supplied stand-in
producer. Each exact assertion carries its own documentary source, recording
attribution and accordingTo declaring frame. Metadata does not propagate across
neighboring assertions or invent roles; source-supplied identity overlap remains
permitted.

promptedBy retains the source assertion that an actual occurrence is this
acting agent's own prompting hearing/seeing record. A known common associated
Agent can support that identity without selecting one globally unique performer.
The source may nevertheless supply an opaque target IRI without target kind or
holder triples. Preserve that actual reference without fabricating a target,
typing evidence or holder. Its Activity domain and range remain open-world
canonical axioms, including their entailment effects. This source meaning is
separate from independently verified endpoint kind, holder validity and runtime
isolation. Contradictory known endpoint evidence is retained as a source refusal;
absence of endpoint evidence alone is not such a contradiction.

Only promptedBy omits the global ClosedWorldClosure opt-in. Such an entry
would derive an objects-of endpoint class check, while raw validation does not
first materialize rdf:type from domain/range. It would reject the very opaque
source IRI the relation must preserve. This omission neither removes the
Activity range nor asserts that an unknown target has been checked. No new
common-Agent, hearing-kind, uniqueness or target-fetching law is added.
heardMessage, sawAct, promptedByAlias and actUnprompted retain their ordinary
global supplied-value closures. RecordMetadata owns the fifth supplied-value
closure, for receivedViaCode. No field is required on unrelated Activities;
no new class, wrapper, Constraint, functionality or required path is introduced.

promptedByAlias preserves an unresolved string or explicitly supplied token,
including its exact lexical form and source namespace. There is no whitespace
normalization, inferred token typing, string-equality resolver or invented
target. Direct references and aliases remain independently source-supplied
even if their evidence disagrees. A particular producer's alias admission
rules are not promoted into an arbitrary upstream validator.

actUnprompted expresses an explicit source assessment about an address to the
acting agent, not universal causal independence. True, false and unknown stay
distinct when actually supplied; false does not identify a prompt. A source
representation that emitted only a positive tag loses the distinction between
false and omission, so neither value is inferred from its absent tag. Conflicting
attributed assessments coexist without global exclusion or winner selection.

The actual tag boundary matters: hasTag has Entity domain, while taggingTagged
has Tagging domain and Entity range. Entity excludes Event/Activity occurrents.
The unprompted boolean avoids changing that shared API or inventing a wrapper
whose tag would describe a different object. The domain-owned tagWordsNotKept
is instead applied to an actual holder-specific Message information record.
Missing text does not infer it; it neither denies hearing nor deletes wording
held elsewhere. AgencyRecords therefore adds one direct dependency on Tags;
Tags has no return dependency on AgencyRecords. No other dependency changes.

Reception channels follow RecordMetadata's existing holder-specific metadata
contract. receivedViaCode ordinarily belongs to the hearing Activity, so two
holders can hear shared Message content without sharing their reception code.
A Message identity is admissible only with independent evidence that it already
names one holder-specific reception. Neither Message typing nor authorship
supplies that evidence. The source/identity obligation is explicit; no range
declaration is advertised as a global shared-content detector.

The ordinary native RDF 1.2 scene declares its own hypothetical event kinds
for the observed and resulting acts, avoiding an unanchored Event without
inventing original source facts. Every role, channel, reference, boolean,
wording and tag assertion retains exact source/frame metadata. Its unknown
prompt endpoint has no invented type or holder. This is source illustration,
not executed validation, private alias equivalence, consumer adoption or
runtime isolation (Principles 2, 3, 4, 9, 10 and 17).

## Inner-life wording and reported generation input state

The retained entry is an existing InformationObject, distinct from the
source-recorded Activity and any actual producing ModelInvocation. Source
entry kinds classify that occurrence through eventType; affectsObject records
only the supplied affected-object inventory. Actual generation provenance is
separate. No entry wrapper, Experience subclass or new forward output role is
needed. An independently source-described mental episode retains its own type
and experiencer; an actual recollection uses the existing recollects contract.
Generated words alone supply neither phenomenal character nor inference validity.

thoughtText admits xsd:string, rdf:langString and rdf:dirLangString with exact
lexical, language and base-direction identity. Explicit empty wording remains
present; absent wording stays absent. JSON-looking text, quoted assertions and
layer markers remain words. These declarations neither parse facts nor install
an index or normalize a producer's language. Three optional, nonfunctional
scalar roles across AgencyRecords and Dreaming have global supplied-value
closures: thoughtText, generationInputStateIdentity and dreamLayer. Their range
readings add no required field or globally singular interpretation.

generationInputStateIdentity retains the producer's supplied xsd:hexBinary
identity of the context slot state given to the record's generation. It is not
an artifact-byte contentDigest, a model execution identity, a payload snapshot,
a prompt copy, a reconstructed pointer set or an immutable history position.
Returning to the same source pointer set need not identify a new history
position. Equality of bytes alone establishes no cross-scheme state equality
or global holder identity. The particular source's actual algorithm, domain
bytes and canonical input convention are retained rather than turned into a
new universal mode, width, case or preimage rule.

For the exact generationInputStateIdentity assertion, its rdf:reifies node
uses wasDerivedFrom for the real source record. Establishing a qualified
interpretation additionally requires actually consulted, version-identified
documentary evidence establishing algorithm, domain/preimage convention,
source/version and holder scope, cited through wasDerivedFrom. accordingTo retains the real
declaring frame and wasAttributedTo the recorder; neither alone supplies scheme
evidence. No profile, ReferenceFrame or algorithm resource is manufactured merely
to hold that evidence. If the real documents do not qualify the scheme, keep
opaque bytes and withhold verified/comparable identity claims. An operation
requiring qualified identity must refuse rather than guess. This is an explicit
source boundary, not a claim that a datatype closure proves documentary truth.

AI's modelExecutionIdentity is the existing opaque binary-identity pattern but
has a different subject and meaning. Sources' contentDigest requires the exact
artifact-byte preimage and algorithm-bearing datatype; Math's digest types
retain their actual hash-mode, lexical and input-identity contracts, including
genuine domain bytes when present. This unit neither weakens those owners nor
silently retypes a supplied state identifier into their narrower contracts.
No hashing implementation, cryptographic verification or new digest datatype is
added. Documentary evidence does not itself certify a computed hash.

The four source kinds preserve waking solitary monologue, dream entry, patrol
note and pre-reply model-emitted reasoning categories. They create no automatic
awareness mode, Experience, DreamReport, actual patrol, proof or runtime success.
The pre-reply reasoning-entry value is not an alias for the general model
generation occurrence marker. Existing ModelCallMechanics, ReplyHandlingReport,
perception/prompt relations and holder-relative scopes remain independently
owned and do not gain new defaults or inferred bindings.

AgencyRecords already consumes the actual core owners. Dreaming owns only the
dream-specific layer literal and already consumes AgencyRecords; no core slice
gains a Dreaming dependency and no dependency edge changes. The source scene
separates a known invocation and independently described experience from the
retained information records, shows documentary support on the exact state
claim and an unqualified opaque case, and preserves empty/JSON-looking/language/
direction wording plus zero/nonzero/absent layers. Every hypothetical assertion
has its own native source/recorder/frame metadata. No runtime or consumer
adoption is claimed (Principles 2, 3, 4, 9, 10 and 17).
