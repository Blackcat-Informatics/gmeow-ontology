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
**DL-clean time** (Principle 3) — base triples are `xsd:dateTime`, never `xsd:date`.

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

A crisp span uses `eventInterval`; a fuzzy date uses source-supplied
`earliestStart`/`latestEnd` bounds and `temporalPrecision`. These alternatives
do not authorize deriving bounds from a coarse point. The
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
