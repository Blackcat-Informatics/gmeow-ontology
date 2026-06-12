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
(machine-readable, issue #325): promote the moment role, period, confidence, or
evidence must be recorded — the hasMet/InterpersonalRelationship duality.

### gmeow:ParticipantRole

The open role vocabulary: principal/subject, organizer, attendee, performer,
officiant, witness, victim, agent, beneficiary, employee, employer. A role not among
the seeds is a fresh individual — never a new participation subproperty.

## Time, fuzziness, and place

### gmeow:eventTime · gmeow:eventInterval · gmeow:earliestStart · gmeow:latestEnd · gmeow:temporalPrecision

The temporal axis, three honest shapes: a point event uses `eventTime`
(`xsd:dateTime`); a crisp span uses `eventInterval` (the temporal slice's
TimeInterval); a fuzzy date uses the `earliestStart`/`latestEnd` bounds plus a
`gmeow:TemporalPrecision` value (day/month/year/decade/circa) — uncertainty modelled,
not smuggled into a string. All non-functional: competing standpoint-indexed dates
coexist, confidence-weighted, annotated by the four statement clocks (temporal slice).

### gmeow:eventLocation · gmeow:eventTrajectory · gmeow:eventSpacetime

The spatial axis: one or more `gmeow:Location` values (geographic or virtual — a
property chain lifts an event to every containing location); a `gmeow:Trajectory` for
moving events; and `gmeow:LocationState` spacetime slices carrying pose and velocity
relative to an explicit frame (Principle 11).

### gmeow:subEventOf · gmeow:hasSubEvent

Event mereology — conference → session → talk — transitive specializations of the
universal `partOf`/`hasPart` spine, kept out of all cardinality axioms to preserve
OWL 2 DL regularity. Mereological, not temporal: containment in time is
`gmeow:during`/`gmeow:contains`.

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

The seam to the observation stack (issue #128): an activity whose purpose is producing
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
