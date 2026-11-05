<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Refinement readings

These terms describe consumer-owned readings of records. `FirstOccurrence` and
`RelationOccasion` specialize the existing `Refinement` observation spine:
`observedFeature` names the source, `vantage` names the versioned reading process,
and `refinementOutcome` describes the interpretation. The result is the
process’s reading, never automatically the source subject’s assertion.

## Exact offered records and source chronology

A `ReadingScope` explicitly enumerates the source records offered to the selected
refiner through `scopeRecord`, including records it could not read. An empty
scope is valid. `scopeChronology` identifies the existing `TemporalFrame` in
which the original assertion-time values are interpreted. `scopeChronology`
specializes `hasReferenceFrame`, so the existing frame spine is inherited. The scope says which
records were offered; it asserts no completeness of the surrounding store.
`readingScope` associates this same scope with findings and limit aggregates
without making every aggregate an observation.

`FirstOccurrence` identifies an earliest source within that scope and declared
chronology. Ties can coexist. Missing or incomparable source times cannot be
repaired by assuming a clock value or claiming a global first. A process that
cannot support an earliest reading retains an unresolved refinement outcome
instead. The ontology records the attributed earliest claim; it does not execute
a search, certify that all offered records were interpretable or compare an
unrecorded source against it.

| Binding | Meaning |
| --- | --- |
| `relationPredicate` | Actual predicate IRI from the source relation. |
| `relationVault` | Store containing the source records; no access grant. |
| `relatedThing` | Actual referent IRI when the source supplies one. |
| `relatedName` | Original literal name when that is all the source supplies. |
| `relationAt` | Source assertion time at its supplied precision. |
| `relationIn` | Original occasion or context, such as a conversation. |
| `findingSummary` | A short account in its original language and direction. |

The two relational reading classes require a source, vantage, scope, predicate,
store, source time and either a related IRI or a source name. `relationAt` admits
a date-time, date, month or year without padding it into a more precise instant.
It is distinct from the refiner’s `observedAt` and from transaction-time
`recordedAt`. `relatedName` retains supplied language and base direction without
inventing a named referent. `findingSummary` requires a language-bearing literal,
including RDF 1.2 directional language strings.

## Milestones

A `Milestone` is an `InformationObject` annotation, attributed to its teller with
`wasAttributedTo`. Its `milestoneOf` points to the original source record and
specializes the existing `wasDerivedFrom` provenance relation. An open
`MilestoneKind` classifies the annotation. Consumers mint their own values.
The annotation is neither the life event itself nor the subject’s endorsement.

## Limits and actual losses

A `ReadingLimit` specializes the existing `QueryLossLedger`, grouping one
relation, store, input scope and reason for a selected refiner. `limitCount` reports the number of distinct excluded records.
`limitReason` draws from the open `ReadingLimitReason` specialization of
`QueryLossCause`:

- `limitNoAssertionTime`: no usable assertion time was supplied for ordering.
- `limitRecordUnnameable`: the selected operation cannot address a supplied
  blank-node record as a named record.
- `limitThingUnshareable`: its addressing contract cannot correlate a supplied
  blank-node or triple-term referent across records.
- `limitCueSuppressed`: a cue matched words the reading read, but a negating cue
  within the declared negation scope suppressed that match from counting.
- `limitCueUngrounded`: a cue matched, but evaluation of its declared grounding
  rules did not establish the match as grounded.
- `limitCueUnread`: supplied words fell outside a cue's declared speaker or
  field selection and were not read by that cue.

These describe the selected refiner’s limitations. RDF 1.2 can retain blank nodes
and triple terms. None of these values bans those terms, erases an input or
licenses a weaker implementation.

The aggregate inherits its information-object and empty-ledger semantics from
`QueryLossLedger`; it is separate from an actual `QueryLoss`. Zero is legal and supplies no `ledgerLoss`. For every actually
excluded source record, retain an identifiable `QueryLoss` through `ledgerLoss`;
its `lossConcerns` points to the original record and its `lossCause` retains the
same reason. Included losses must concern records in the declared input scope.
Recall can reference this specialized ledger directly through `queryLossLedger`.
Other ledgers can retain those same loss resources, preserving identity and
acknowledgement history.

The count is a reported distinct-record total. Keep every excluded record’s loss,
reuse an existing loss identity and avoid duplicate losses that inflate a count.
The intrinsic laws check zero-versus-member consistency, positive evidence and
scope/cause coherence. They do not certify the exact arithmetic of counts above
one or perform the consumer’s grouping computation.

Cue limits retain the original words and any detected match, together with the
selected cue, negating evidence and scope, or evaluated grounding rules and
supplied facts as applicable. Unread words remain offered inputs through
`scopeRecord`. These source-focused cue readings record their source through `observedFeature`
and their versioned process through `vantage`; a limit aggregate retains its reporting
agent through `wasAttributedTo`. Preserve source authorship separately. None of
these reasons turns a refiner's interpretation into the source author's claim
or turns an uncounted cue into a silent negative verdict.

Retain a source record actually excluded from the selected reading as a
`QueryLoss` with `lossConcerns` naming that original record and `lossCause`
naming the cue reason. A `ReadingLimit` groups those losses through `ledgerLoss`
under the same `limitReason`, input `readingScope`, relation and store. A
suppressed or ungrounded match alone does not show that its entire source
record was excluded: other contributions may remain usable. Keep such match
decisions as evidence without inventing an excluded-record loss. When records
are excluded, count distinct source records for the stated reason, not matches;
several excluded matches within one record do not inflate `limitCount`.

These values classify reported outcomes; they do not execute cue detection,
negation or grounding. Missing rules, an evaluation that never ran and an
execution failure do not establish an ungrounded outcome. The existing
scope/cause and count-evidence laws remain unchanged and do not certify the
consumer's cue decisions or exact record-count arithmetic (Principles 4, 9
and 11).

The ordinary [archive scene](examples/archive-readings.ttl) separates an earliest
reading over two ordered records from a wider input that also contains an
undated record, retains a source-only name and attributes a milestone to its
process. No refinement edits its original sources (Principles 3, 4, 9 and 11).

## Open items and changing readings

`OpenItemReading` specializes `Refinement`. Its `observedFeature` is the original
record, its `vantage` is the versioned reader, and `readingScope` retains exactly
the records offered to that reader. `openItemAgent` identifies the awaited Agent,
not the reader. `openItemAt` preserves the holder's original opening date-time;
the reader's `observedAt` must never replace it. Missing source precision cannot
be repaired by inventing a clock value. A source that supplies no usable opening
date-time cannot satisfy this reading's selected contract.

The open `OpenItemKind` vocabulary initially distinguishes `openItemRequest`
and `openItemQuestion`. These are interpretations of records, separate from
request acts, `AttentionRequest` lifecycles, or a question's own inquiry status.

The open `OpenItemState` vocabulary initially distinguishes:

| Value | Reading within the offered scope |
| --- | --- |
| `openItemOpen` | No reply or settling act is linked. |
| `openItemReplied` | A reply is linked, without a settling act. |
| `openItemSettled` | A settling act is linked and its original record is retained through `settledBy`. |
| `openItemLetGo` | The records support a reading that the agent let the item go, distinct from settlement. |

The settled value requires evidence. The other three seeded states exclude
`settledBy`; custom states remain open to their own semantics. Each cited settling
record belongs to the reading's offered scope. These intrinsic laws do not prove
that a reply or resolution happened outside the recorded evidence.

A later reading uses the existing `supersedes` link to its predecessor. Preserve
both scopes, outcomes and vantages, and suppress an older reading from display
when appropriate rather than retracting it. A changed reading does not edit the
original request or question. No state defaults from silence, elapsed time or
an absent field.

## Recurrences

`RecurrenceReading` also specializes `Refinement` and binds its original source,
versioned vantage, `readingScope`, outcome and actual IRI-valued `relatedThing`.
`recurrenceMember` retains each exact record counted in the current segment.
`recurrenceSettledBy` optionally retains the record of the settling act that began
that segment. Both relations specialize `wasDerivedFrom`, and both kinds of
evidence must belong to the offered scope. An omitted boundary says it was not
supplied; it does not claim that settlement never occurred.

Membership is by RDF identity, not by matching text. A member contributes once
to the retained set; chronological segmentation and arithmetic remain attributed
consumer computations. A recurrence says nothing about why something appeared,
the agent's preference, or a global absence of a settling act.

## Activity figures and day parts

`HabitFigure` is a `Refinement` with explicit source, scope, outcome and vantage.
`habitActivity` retains the source's activity designation or description without
mistaking a category for an individual occurrence. `habitMember` retains actual
`Activity` occurrences as evidence, through the existing derivation spine. Every
member is in the offered scope, including when the scope consists of act records.

`habitChosenCount` and `habitSituationCount` are nonnegative reported counts. Their
denominators and grouping belong to the declared consumer method; this vocabulary
introduces no hidden equality between membership and a count, or inequality
between the two counts. A zero figure need not fabricate a counted act. These
figures do not diagnose a habit or infer preference.

`habitHolder` names the Agent whose civil-day reckoning is used. This role is
separate from the process `vantage`. `habitTemporalFrame` names the actual
`TemporalFrame` and specializes `hasReferenceFrame`; `habitDayPart` selects the
temporal owner's open `DayPart` vocabulary. Morning, afternoon, evening and night
have no universal fixed hours. Their boundaries belong to that holder's reckoning
in the supplied frame. A new holder or frame produces a distinct reported figure,
not a new universal meaning for a shared day-part value.

The holder's civil-day frame and the scope's source assertion-time chronology
are separate bindings. They may differ; neither silently converts the other.

The ordinary [state-reading scene](examples/state-readings.ttl) retains a changed
open-item interpretation, recurrence boundary evidence and a holder-relative
activity figure without introducing an executor or a statistical certificate.

## Unrecorded sources and completed empty readings

`limitUnrecordedSource` specializes the existing `ReadingLimitReason`
vocabulary. Missing source attribution prevents the selected reader from
deciding whether words restate outside words the holder was told. It does not
prove originality or a negative restatement result. When an actual record is
excluded, retain its existing `QueryLoss`, source identity, input scope and
reason through the established `ReadingLimit` contract. Missing source
attribution is not permission to erase the offered record or invent its source.

`foundNothing` is an explicit `Refinement` report of a completed selected
computation with no selected reading. Its value is the number of distinct
declared input records, not repeated visits. The report's `readingScope` and
`observedFeature` name the same `ReadingScope`, and `vantage` retains the
declared reader. Keep `refinementOutcome refinementUnresolved`, which already
describes an explicit completed attempt with no interpretation, and no
`observationResult`. Nothing derives this report from missing results or a
computation that failed or never ran.

Retain the actual generating `Activity` through `wasGeneratedBy`, its reader
association through `wasAssociatedWith`, and its supplied `eventTime`.
These are reported completion and execution claims, not authenticated receipts.
The report has one count, scope, reader and generating activity. The same
activity, scope and reader identify one report; a later execution has a distinct
activity, preserving independent readings.

Every `scopeRecord` in the declared roster retains its exact RDF identity
as the report's `wasDerivedFrom` evidence, including supplied blank nodes
or triple terms. That activity `used` the same `ReadingScope` information
object, preserving the input roster without forcing its members into Entity.
Conversely every `wasDerivedFrom` source of the report belongs to that scope.
Rule and other execution provenance belongs on the activity, keeping the
report's evidence roster exact. An actually unreadable offered record is
retained as a limit rather than silently omitted from a complete empty report.

The existing COUNT DISTINCT aggregate compares the report's exact retained input
roster with `foundNothing`. The zero/positive evidence law handles the
aggregate's no-row case: zero has no scoped input, and a positive count has an
input. Two-way membership therefore also checks exact count agreement for
larger batches. Empty input is legal; it remains an explicitly reported
computation, not an inferred scalar or unexecuted operation.

The original `ReadingScope` chronology contract remains in force, including
for an empty scope. None of these statements proves completeness of the
surrounding store, executes the selected reader, authenticates completion or
proves that its declared inputs were actually read. They keep source evidence
and intrinsic consistency distinct from those execution claims.
See the [ordinary rule and reading scene](../refinement-cues/examples/reading-rules.ttl).

## Contact readings

ContactReading is a Refinement about an actual being the holder can address.
Its single observedFeature is that being. The named, versioned reading process
is its vantage; the holder of the source records is a different role. Retain
the actual source records separately through wasDerivedFrom and any supplied
ReadingScope/readingScope association. Do not put both a source record and the
being in observedFeature or substitute the holder for the observer. Existing
source-focused refinements retain their source-record feature convention.
This vocabulary records an observer's reading; it neither runs a refiner nor
turns that reading automatically into the being's or holder's claim.

contactName retains the selected current wording within the holder's supplied
records and their actual source scope and chronology. It is not a globally
preferred name. Retain earlier names as source-backed aliases, with their actual
name records and attribution, rather than deleting history or inferring identity
from equal strings. Missing or tied source chronology does not authorize an
invented winner. The literal retains xsd:string, rdf:langString or
rdf:dirLangString as actually supplied, including lexical form, language and
RDF 1.2 direction; no language is guessed. The reading is not an Appellation,
so contactName is not a fullName specialization.

contactKind uses the open ContactKind value vocabulary. contactKindInnerAgent
reports an agent within the holder's own environment. contactKindCorrespondent
records only message attribution in the holder's conversations and supplies no
Person/Agent type, authenticated authorship or independent identity. Preserve
any separately supplied classification under its own evidence; the vocabulary
does not choose one global kind or invent a holder/environment context.

contactRingOutcome uses the separate open ContactRingOutcome axis.
contactRingStated reports a ring decision under the holder's stated rules;
the value alone does not identify that ring. contactRingUndecided explicitly
reports that those rules decide none: this reading states no ring and guesses
none. It is distinct from missing outcome data and from the overall
refinementOutcome. A reading can preserve an actual name and contact kind while
its ring remains undecided. No rule executor, fallback ring, audience rank or
universal intimacy scale is supplied.

The three roles are optional and nonfunctional. Their provided-value closures
validate supplied subjects and values without imposing field presence, global
uniqueness or arbitrary maxima. Existing observedFeature/vantage obligations,
coexisting attributed readings and old record-focused examples remain in force.
No new holder role, identity rule or automatic subject assertion is introduced
(Principles 4, 9, 10, 17 and 19).

## Presence and offered situation counts

`habitPresenceCount` and `habitOfferedCount` add distinct reported counts to a
`HabitFigure`. Presence counts situations in the selected population where the
holder was in the designated activity, whoever chose it. Offered counts
situations where the guidance in force offered that activity to the holder,
including situations where it was neither chosen nor performed. Preserve the
source’s actual holder, activity designation, DayPart, TemporalFrame, input scope,
situation grouping and applicable guidance. The counts remain the named
process’s readings, never the holder’s own assertions or preference judgments.

Both roles are optional and each has at most one `math:Quantity` per figure.
A figure may omit either or both. Missing evidence is not zero; retain an actual
supplied zero without fabricating a counted act. Situations are not identified
with the `Activity` occurrences retained by `habitMember`. These roles establish
no equality with membership size and no inequality or denominator relationship
to `habitChosenCount`, `habitSituationCount` or each other.

Use the existing [Math Quantity](../../grounding/math/docs.md) for each supplied
count. Keep every original `xsd:nonNegativeInteger` on `math:quantityValue`,
including its lexical form and datatype. All magnitudes must be valid and have
the same numeric value; equal source-supported lexical aliases may coexist.
Retain `math:dimensionless` on `math:hasDimension` and exactly one actual
`hasReferenceFrame` value typed as `ReferenceFrame`, subject to the existing
complete [Places frame profile](../places/docs.md).

The counting frame’s evidenced interpretation must cover the distinct count
role, selected population, holder, activity designation, DayPart, temporal frame,
input scope and declared grouping. For offered counts, retain the actual
guidance in force. A nearby `TemporalFrame`, a generic named frame or a matching
number does not establish that counting interpretation. Do not invent a frame,
raw scalar alias or duplicate Quantity subclass to fill a missing input.

Canonical `logic:` endpoint closures and figure-local maxima validate supplied
roles. `HabitCountValueConstraint` checks magnitude presence and validity,
numeric-value equality, dimension presence and value, and frame presence, type
and uniqueness through independent positive guards. These structural checks
cannot prove source truth, population membership, grouping, guidance history or
an asserted frame interpretation. Retain the actual evidence and declared method
without claiming a counter, guidance selector, civil-time classifier or rule
executor ran (Principles 4, 9, 11, 17 and 19).

## High-friction correction readings

`HighFrictionCorrection` is a teller’s source-derived `Refinement` that an
earlier unsuccessful attempt was followed by a distinct retry of the same tool
and reported argument interpretation in one episode. The existing
`observedFeature` is the actual earlier `ToolCall`; `frictionCorrection` is the
actual later `ToolCall`. `frictionEpisode` names the actual episode `Activity`,
`frictionTool` the actual shared `SoftwareAgent`, and `frictionArguments` the
retained argument evidence used as the teller’s grouping basis. These are
required single reading bindings, not fabricated events or a successful retry.
Keep the actual versioned `vantage`, `refinementOutcome` and `readingScope`.

`frictionRule` selects exactly one actual immutable
[ReadingRule](../refinement-cues/docs.md) release as a genuine `wasDerivedFrom`
input. It retains the existing lineage, version label, content digest and
accountable attribution, with all four supplied token parameters on that same
release. Other actual derivation inputs are the receipt records in the
nonempty input scope. The episode, attempt events and selected rule are not
substitutes for those receipts. Retain exact source receipt and field identities,
including the positive unsuccessful-outcome evidence and the evidence for the
reported later retry/order. Missing success, source loss or unreadability is
never an unsuccessful outcome. Missing required evidence prevents a complete
selected reading; it is not filled with a default.

`frictionArguments` uses the existing `StructuredValue` carrier. Reuse an actual
`toolArgumentsStructured` occurrence, or keep an opaque original `toolArguments`
literal through `structuredTerm` on a source-identified scalar occurrence.
Preserve each call’s actual payloads, media metadata and declared interpretation.
Coexisting opaque and structured representations do not become equivalent,
and a reading does not make one globally primary. No JSON canonicalizer,
normalization, hashing scheme, duplicate-key policy or semantic comparator is
implied. The teller’s reported same-arguments interpretation remains an
attributed claim supported by its actual rule and source evidence.

`frictionAt` belongs to the actual `ToolCall`, specializing `eventTime`. Retain
the source’s `xsd:dateTime`, `xsd:date`, `xsd:gYearMonth` or `xsd:gYear` without
padding precision or changing timezone. It is separate from the reading and
recording clocks. The existing Events contract assumes UTC-Gregorian when
`eventTemporalFrame` is absent; this vocabulary adds no different default or
conversion. Preserve an actual non-default frame when applicable. Source order
evidence is required by the selected method, even when no precise attempt time
is supplied. `before` is an Allen relation with a gap and is not inferred merely
from a teller’s “later retry” reading.

`frictionWithin` also belongs to each actual attempt. It specializes
`subEventOf` and names an actual containing episode or an actual
`eventTypeTraceRound` activity belonging to that episode. It is optional on
generic ToolCalls and has no single-container maximum; both attempts selected
by this reading require source-supported episode context. A receipt location,
label or temporal co-location cannot supply event containment or a scheduler.

`frictionAttemptCount` and `frictionUnsucceededCount` are optional annotations,
each at most one existing `math:Quantity`, never confidence. Their declared
population is the distinct attempts this reading groups as the same act within
its episode and input scope under the selected rule, including the two witness
attempts. A supplied attempt count is at least two; a supplied unsuccessful
count is at least one. Keep original `xsd:nonNegativeInteger` magnitudes with
one numeric value, the dimensionless dimension and exactly one actual complete
counting frame. Equal source-supported lexical aliases may coexist. The frame
interpretation retains population/cutoff, witnesses, tool, argument grouping,
scope, rule and actual outcome meaning. An event’s temporal frame alone does
not supply that counting interpretation. No absent count, zero, witness minimum
value, roster or equality with receipt membership is synthesized.

The canonical laws check direct required fields, distinct calls, consistent
tool identities, retained receipt-scope references, source containment,
argument-evidence presence, selected parameters and numeric/frame consistency.
They do not execute receipt parsing, field selection, tool resolution,
semantic argument comparison, outcome interpretation or chronology. A valid
graph is not proof that the teller ran, its interpretation is true, or the
later attempt succeeded. Preserve positive source evidence and actual method
identity without replacing unsupported computation with a certificate or
default (Principles 4, 9, 11, 17 and 19).
