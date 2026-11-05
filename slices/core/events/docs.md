<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Events — occurrences, participation, and the four orthogonal axes

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/events` · **tier: core**
> The universal occurrence: entities participate in roles, over possibly fuzzy time, at possibly several places, per possibly conflicting sources.

A GMEOW event is a temporal occurrence in which entities participate *in roles*, over
*possibly fuzzy* time, at *possibly several* locations, asserted by *possibly
conflicting* sources. The slice supersets schema.org Event, CIDOC-CRM E5/E7/E92,
OWL-Time, PROV-O Activity, iCalendar VEVENT, LODE, SEM, and Wikidata by reference
(Principle 5), and absorbed the former genealogy event hierarchy (Principle 6):
thirty-odd LifeEvent subclasses and role subproperties became value individuals.

The no-mess rules. **Type is a value, not a class** — one `gmeow:Event` class, kinds in
an open value vocabulary (Principle 9, no overtyping). **Role is a value on a relator**,
never a subproperty. **Four orthogonal axes** — eventType ⟂ participationRole ⟂
temporal ⟂ location — with no inferential bridge. **Flat-first, reify on demand** —
`gmeow:hasParticipant` for the 80 % case, `gmeow:Participation` when role, period,
confidence, or evidence matters. **Co-equal conflicting claims coexist** — no primary
date; corrections use `gmeow:displayable` false, never deletion (Principle 10).
**Source precision is retained** — point placements keep their supplied XSD date/time
precision without invented clock fields.

## The occurrence and its kind

### gmeow:Event

The universal occurrence — `gmeow:Activity` (provenance) and `gmeow:LifeEvent`
re-parent onto it, so it is the genuine top event of the model. Declares
`gmeow:requiresFrame gmeow:eventTemporalFrame` at warning severity (Principle 11).

### gmeow:eventType · gmeow:EventType

The kind of an occurrence as an open value vocabulary — birth, marriage, merger,
commit, image capture, expression creation — seeded from BIO, GEDCOM X, LRMoo, and
CIDOC-CRM. Non-functional by design: one occurrence may be both a marriage and a
religious rite, and a birth is *also* a `gmeow:eventTypeCreation` (the lifecycle hook).
An unforeseen kind is a fresh labelled individual, never a new class (Principle 9).

### gmeow:LifeEvent

A thin, person-scoped phase of Event — *not* a type taxonomy: the specific kind stays a
value (`gmeow:eventTypeBirth`, …). It is the seam joining the names module to the event
spine: `gmeow:conferredByEvent` ranges over it.

## Participation (the centerpiece relator)

### gmeow:Participation

The reified, time-scoped, evidence-bearing fact that an entity took part in an event
in a role — the NameUsage/IdentityFacet idiom, one per (event, participant, role)
tuple. A disputed role is several standpoint-indexed Participations, none privileged; a
withdrawn one keeps `gmeow:displayable` false. EL axioms require some event and
participant; closed-world cardinality is SHACL's.

### gmeow:participationEvent · gmeow:participationParticipant · gmeow:participationRole · gmeow:participationInterval

The relator's posts: exactly one event (functional); one or more participants (range
`gmeow:Entity`, not just Agent — a document signed, a place visited); zero or more
roles (non-functional, mirroring eventType — competing role claims coexist via
`gmeow:accordingTo`); and an optional interval over which the participation held.

### gmeow:hasParticipant

The flat 80 %-case shortcut, Event → Agent. `gmeow:pairsWith gmeow:Participation`
(machine-readable, documentation doctrine): promote the moment role, period, confidence, or
evidence must be recorded — the hasMet/InterpersonalRelationship duality.

### gmeow:ParticipantRole

The open role vocabulary: principal/subject, organizer, attendee, performer,
officiant, witness, victim, agent, beneficiary, employee, employer. A role not among
the seeds is a fresh individual — never a new participation subproperty.

## Time, fuzziness, and place

### gmeow:eventTime · gmeow:eventInterval · gmeow:earliestStart · gmeow:latestEnd · gmeow:temporalPrecision

A point-like occurrence uses `eventTime` at the source's stated precision:
`xsd:dateTime`, `xsd:date`, `xsd:gYearMonth` or `xsd:gYear`. Keep every literal's
lexical form, datatype and timezone. A day, month or year supplies neither an
unstated midnight nor an event duration. Coexisting standpoint-indexed placements
remain separate statements, with their own confidence and provenance.

A crisp span uses `eventInterval`; a fuzzy date retains source-supplied
`earliestStart`/`latestEnd` dateTime bounds when actually supplied.
`temporalPrecision` describes reported resolution, including
`precisionNamedPeriod` for named-period or season-level resolution; it neither
places the event nor requires invented bounds. Use `eventDuring` only for an
actually established interval or NamedPeriod with its existing contracts.
Raw wording such as "`fall2025`" alone supplies no period identity, calendar,
hemisphere, season boundaries, exact instant or valid EDTF expression. Precision
alone does not satisfy the existing event type-or-placement warning. The
[wedding example](examples/wedding.ttl) includes source day, month and year
placements on the same occurrence.

A supplied `eventTemporalFrame` is retained; absence keeps the existing
UTC-Gregorian default. A frame cannot reinterpret XSD's Gregorian datatypes.
OWL-Time uses the matching precision predicate. The schema.org profile projects
dates, vCard may coarsen to a year, and PROV/iCalendar instant branches omit
coarse placements. Their owned loss notes declare frame and metadata omissions.
[TQL](../../../docs/temporal-queries.md) displays full typed placements but
restricts numeric comparisons to compatible zoned timestamps.

### gmeow:eventDuring

An event can be located within an established `gmeow:TimeInterval` or
`gmeow:NamedPeriod` through `gmeow:eventDuring`, even when its exact instant or
complete span is unknown. The referenced period supplies temporal placement;
it does not become the event's duration. Multiple placements may coexist as
standpoint-indexed claims.

Use `eventInterval` for the event's full known span and `during` for strict
Allen containment between events. Coincidence with a period boundary is
permitted; `eventDuring` supplies no instant or boundary conversion. A relative
phrase such as "last fall" needs its reference context resolved to an established
period before this relation can name it. Existing instant-only consumers gain
no period expansion or inclusion rule from this property.

The [wedding example](examples/wedding.ttl) retains an earlier wedding's
placement in a reporting interval and in the Holocene without inventing a clock time.

### gmeow:eventLocation · gmeow:eventTrajectory · gmeow:eventSpacetime

The spatial axis: one or more `gmeow:Location` values (geographic or virtual — a
property chain lifts an event to every containing location); a `gmeow:Trajectory` for
moving events; and `gmeow:LocationState` spacetime slices carrying pose and velocity
relative to an explicit frame (Principle 11).

### gmeow:eventFromLocation · gmeow:eventToLocation

The endpoint pair for movement events — migrations, moves, journeys, relocations.
Both are specialisations of `gmeow:eventLocation` (an origin or destination IS a
location of the event, so plain location consumers keep working), and together they
are the discrete sibling of the continuous `gmeow:eventTrajectory`. A movement event
with one known endpoint carries just that endpoint. With `gmeow:eventTypeMigration`
(or immigration/emigration) this is the canonical home of relocation history; the
schema.org projection derives `schema:MoveAction` + `fromLocation`/`toLocation` from
exactly this shape.

### gmeow:subEventOf · gmeow:hasSubEvent

Event mereology — conference → session → talk — transitive specializations of the
universal `partOf`/`hasPart` spine, kept out of all cardinality axioms to preserve
OWL 2 DL regularity. Mereological, not temporal: containment in time is
`gmeow:during`/`gmeow:contains`.

### gmeow:causalPartOf · gmeow:hasCausalPart

The *causal* sub-mereology of events — a hand-off, callback, or dependency edge where one
sub-occurrence brings about another, distinct from the plain part-of nesting of
`gmeow:hasSubEvent`. Each is a transitive specialization of both `gmeow:subEventOf` (a causal part
*is* a sub-event) and the foundational `logic:causalPartOf`, which is itself a specialization of
`logic:temporalPartOf` (causal ⊆ temporal). This is the domain-level dogfood of the YAMATO
process refinement: a workflow's data-flow edges are causal parts, not merely co-located
sub-events.

### Action vs event boundary (the open/closed dogfood)

The *arrive* vs *arrival* distinction — an on-going action vs a completed unitary event — is
recorded on an occurrence as a **value, not a subclass** (Principle 9) via the foundational
`logic:occurrentBoundary` over `logic:Open` (action, on-going) / `logic:Closed` (event, completed
unitary). A prescriptive plan step is `logic:Open`; the descriptive record of its completion is
`logic:Closed`, and a `logic:Closed` occurrence is constrained to bear no time-varying fluent (the
process≠event change-asymmetry). This keeps the planned-vs-actual seam — already visible in
`gmeow:EventSeries` and the observation linkage — typed without proliferating event subclasses.

### gmeow:before · gmeow:after · gmeow:coincidesWith (the event-level Allen family)

Thirteen qualitative event-event temporal relations (`meets`/`metBy`,
`overlaps`/`overlappedBy`, `starts`/`startedBy`, `during`/`contains`,
`finishes`/`finishedBy` complete the set) — the TimeML TLINK / TEO idiom. Before/after
and during/contains are transitive; `coincidesWith` asserts temporal simultaneity
only, never identity. Kept strictly apart from the interval-level Allen family.

## Series, duration, observation

### gmeow:EventSeries · gmeow:hasRecurrenceRule · gmeow:Duration

Planned-vs-actual: a series issues concrete occurrences (`gmeow:seriesOccurrence`); its
`gmeow:RecurrenceRule` carries an RFC 5545 RRULE string projected straight to
iCalendar; a `Duration` is an unanchored length (`xsd:duration`, DL-clean), distinct
from the anchored interval. Tense and aspect (`gmeow:eventTense`, `gmeow:eventAspect`)
are ISO-TimeML annotation-layer only — about the mention in text, never the occurrence.

### gmeow:ObservationalActivity · gmeow:generatedObservation

The seam to the observation stack (observation-linkage design): an activity whose purpose is producing
observations — survey, census activity, excavation, audit, clinical trial (all type
values). A property chain associates the activity with the vantage of its
observations; the inverse rides `gmeow:wasGeneratedBy`, deliberately not declared
`owl:inverseOf` to keep OWL 2 DL typing honest.

## Solver layer & deferred alignment

The full Allen composition table is not expressible in OWL DL and is not asserted: a
reasoner derives the sound transitive closures; everything richer — composition,
cross-family JEPD between event-level and interval-level relations — is SHACL-gate
and solver work (Principle 12). Recurrence expansion is likewise computed; RRULE
structure stays a projection concern, and the LODE/SEM/CIDOC bridges stay references,
never imports (Principle 5).

## Dependencies

Depends on `kernel`, `documents`, `observations`, `places`, `provenance`, and
`temporal`. Consumed by participation and life events across slices, the calendar
slice, the lifecycle slice's creation/destruction hooks, and provenance activities.

## Declared execution requirements

`requiresLocation` records where a selected occurrence must be performed under
a stated requirement. It ranges over `Location`, including geographic `Place`
and `VirtualLocation`. It identifies neither the performer nor the actual
presence of every participant. Use `eventLocation` for an occurrence's actual
location; no axiom connects a required location to an actual one.

Select the claim context before interpreting a set of requirements. Every
required location in that context is a requirement to meet: repeated locations
are conjunctive. Keep alternative contexts and conflicting declarations
separate, with source and `accordingTo` metadata on their exact RDF 1.2
statements. Location containment alone does not establish satisfaction. Missing
location metadata remains unspecified.

`interruptibility` records declared handling during an occurrence's performance.
Its open `Interruptibility` vocabulary has four seed values:

| Value | Declared handling |
| --- | --- |
| `interruptibilityNone` | No interruption point during performance. |
| `interruptibilityAtSentenceBoundary` | Between completed sentences of the occurrence's text activity. |
| `interruptibilityAtStepBoundary` | Between identified plan steps. |
| `interruptibilityAtAnyPoint` | No restriction to particular interruption boundaries. |

The sentence value needs the relevant text activity; the step value needs the
plan identifying the steps. Neither detects a boundary or certifies that a
sentence or step finished. The values establish no permission, cancellation
authority, checkpoint, capability, safety judgment or successful resumption.
In particular, no interruption point during performance says nothing about who
may cancel an occurrence.

Both properties are nonfunctional. Supplied-value restrictions type their
objects without requiring a value, closing the value vocabulary or selecting a
winner. Missing interruptibility remains unspecified. Multiple declarations
retain their sources and contexts; the vocabulary neither chooses one nor
combines them into a policy.

These requirements apply to generic occurrences, including meetings, readings
and procedures. Calendar-owned `Task` inherits them through `Event`; an RDF
document describing a task remains distinct from that occurrence. These
declarations do not turn an event into an `ActionSchema`, change `taskStatus`
or compute `ActionableFrontier` eligibility. A downstream consumer must evaluate
its selected requirements against its explicit state and context; no location
matching, scheduler execution or permission is supplied by these declarations
(Principles 9, 11 and 17).

The ordinary [execution-requirements scene](examples/execution-requirements.ttl)
illustrates conjunctive geographic requirements, a separate virtual context,
all four interruption values and Task inheritance. Its scoped declarations
report no executed scheduler or event.

## Attributed event progress

`EventProgressReading` is an `Observation` reporting one explicit progress value
for one actual `Event` or an `EventDescription`. It uses the existing
`observedFeature`, `observationResult` and `vantage` roles: exactly one feature
and result, and at least one actual vantage with no maximum. Joint holders can
share a reading, while rival or historically different readings remain separate.
No frame is primary, and there is no globally functional event-status slot
(Principles 4, 9 and 11).

`EventDescription` is an actual kernel `InformationObject` describing an event,
including an anticipated occurrence. It is explicitly disjoint from `Event`:
a description is not an occurrence or a placeholder for one. Its existence
supplies neither a bearer, participant, occurrence time nor execution evidence.
A later reading can identify an actual realization without deleting, mutating
or promoting that description into an Event.

`EventProgress` is closed by `logic:oneOf` to exactly `eventAwaited`,
`eventInProgress` and `eventCompleted`. The result is a quality value, which the
existing `observationResult` range admits. Missing progress is unspecified;
a fourth extension value cannot bypass the conditional realization contract.

| Observed feature and explicit result | Realization contract |
| --- | --- |
| EventDescription, eventAwaited | No progressRealization is permitted. The reading reports anticipation without selecting an actual occurrence. |
| EventDescription, eventInProgress or eventCompleted | Exactly one actual Event is required through progressRealization. Retain the source evidence identifying it as the description's realization. |
| Actual Event, eventInProgress or eventCompleted | The actual occurrence is already the feature. progressRealization is forbidden, including a redundant link back to that same occurrence. |
| Actual Event, eventAwaited | Invalid: an awaited reading requires the descriptive feature and cannot dual-type it as an Event. |

`progressRealization` is an optional role generally, with at most one actual
Event per reading. Its canonical domain/range and property-global supplied-value
closure check direct uses as well as values on a typed reading. Local required
paths and maxima do not make the existing generic Observation roles functional
or require every Event to have a progress reading.

`EventProgressBindingsConstraint` supplies five independent intrinsic guards:
awaited progress requires a description and forbids realization; in-progress
and completed description readings each require their actual realization; a
direct Event feature forbids an extra realization. Every existence body is one
atom, correlated to the same reading. The closed result set, feature union,
provided endpoints and these arms leave no unchecked progress branch. They
check retained structure, not the truth of the source's progress or descriptive
correspondence (Principles 17 and 19).

Keep actual source, frame and occurrence evidence. Do not derive status from
missing `latestEnd`, a timestamp, an open commitment, a global now or a chosen
latest reading. Ended and awaited interpretations need not fit one linear
history. A completed occurrence does not establish successful restoration,
goal satisfaction, a continuity verdict or a supplied end time. An awaited
event report alone does not establish commitment to a future subject stage.

The existing `logic:occurrentBoundary` Open/Closed contract is separate. Open
alone does not distinguish anticipation from underway progress. Coexisting
progress readings do not generate conflicting global boundary values, and
completion does not generate a closed boundary. There is no clock evaluator,
workflow executor or automatic status transition. The focused
[progress design](design/EVENT-PROGRESS.md) specifies these source/evidence and
projection boundaries together.
