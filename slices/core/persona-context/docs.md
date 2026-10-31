<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Persona context

A record can retain the persona worn when it was made. An activity can retain the
changes of persona that happened within it. This slice supplies both relations,
using the existing bearer-bound `Persona` rather than inventing another identity
or choosing one persona as the agent’s true self (Principles 4 and 9).

## Attribute the record

`wornModality` links any record to a `Persona`. Its domain stays open: a note, a
message, a remembered account or an act can carry the attribution. Several
attributed personas may coexist, with provenance and standpoint on their
statements. No persona is selected as primary.

This differs from `activatedIn`. That existing relation names the condition or
situation type that activates a persona. A record’s actual persona attribution
neither defines that activating rule nor follows from it automatically.
`wornModality` therefore has no inverse or subproperty bridge to `activatedIn`.

## Retain a change within its activity

`ModeTransition` is an `InformationObject`, attached from an existing `Activity`
through `modeTransition`. It records a change without requiring a separate event.
Every transition record must have an incoming activity link. Nested activities
may retain the same change without turning the record into an activity itself.

A complete transition identifies its `transitionSlot`, `transitionFrom`,
`transitionTo`, `transitionAt`, `transitionSequence` and at least one
`transitionCause`. The two personas and the slot refer to the same bearer.

The consumer mints a `PersonaSlot` for one bearer and one producer sequence
stream. `slotBearer` identifies that agent. A slot is a source stream identity,
not a general mutable-state schema or a fixed list of permitted modes. Reuse it
across activities when their transitions share one continuing counter. Mint a
new slot when the producer counter restarts or its source identity changes.

`transitionFrom` is the persona replaced; `transitionTo` is its replacement.
Those fields describe one source record. An independently attributed account
uses another record, preserving both accounts rather than combining their fields
into an accidental composite transition.

## Separate source time, duration and order

`transitionAt` retains the producer’s `xsd:dateTime` clock reading and its actual
timezone information. Clock readings may repeat or move backward. Sort source
changes by `transitionSequence` within the same `transitionSlot`, rather than by
time. Sequence numbers are nonnegative, may contain gaps, and need not start at
zero. Tied reports remain tied unless the source supplies more information.
There is no cross-slot sequence comparison or global uniqueness claim.

`transitionLasted` is the producer-reported `xsd:duration` for which the replaced
persona lasted. Preserve its supplied units. Do not calculate it by subtracting
clock readings, and do not supply zero when duration was unobserved. This field
may be absent when the source supplied no duration observation.

`recordedAt` can independently date creation of the retained record, at the
source’s supplied precision. A coarse recording date remains coarse.
`transitionAt` is not a recording-time subproperty. Likewise, `transitionSequence`
is not a subproperty of zero-based `entryOrdinal`: a producer counter is a
different assertion from an entry’s position in a containing sequence.

## Preserve why and where it changed

The open `TransitionCause` vocabulary supplies five initial values:

- `transitionCauseChosen`: the agent deliberately chose the change.
- `transitionCauseAutomaticBoundary`: the change happened automatically at a
  reported cycle boundary.
- `transitionCauseHoldReleased`: the persona relaxed after a hold was released.
- `transitionCauseAudienceForced`: the listening audience could not admit the
  persona being worn.
- `transitionCauseHoldResumed`: a held persona resumed after the audience
  restriction ended.

These are explanations, not an executable policy engine. Several supported
causes may coexist. The model does not infer a cause from the replacement or
select a winning explanation.

An automatic-boundary cause requires a `transitionBoundary`. Its open vocabulary
initially distinguishes conversation opening, periodic wake-up, waking and
mid-conversation. The boundary describes the producer’s cycle; it does not assert
a biological sleep mechanism or force every agent to follow that cycle.

An audience-forced cause requires a `transitionRing`. A resumed hold does not by
itself assert that the constraining audience is still present.

## Keep audience ranking local

An `AudienceRing` is a consumer-defined description of the listening audience.
It describes who could listen, rather than placing an individual person in a
relationship category. No ring members are prescribed by this slice.

Each member names its `AudienceRingScheme` with `audienceRingScheme` and retains
its local `audienceRingRank`. Lower nonnegative ranks are more inward within that
scheme. Ranks may tie and need not begin at zero. Members of independently
defined schemes have separate identities; equal numbers across those schemes
provide no shared ordering. The membership and rank fields are singular on each
scheme-scoped member, not universal facts about a person’s intimacy or importance.

The [ordinary conversation scene](examples/persona-changes.ttl) retains a
clock rollback, a forced switch, a resumed hold and a continuing producer stream
across activities. The [design rationale](design/PERSONA-CONTEXT.md) records the
canonical obligations and the boundaries of these assertions.

## Audience register

personaMode records the Persona register in which a record was made for its
audience. wornModality independently records the persona worn. A record may name
different personas through these two relations, with attribution on each claim.
Neither relation selects a primary persona or implies the other.
