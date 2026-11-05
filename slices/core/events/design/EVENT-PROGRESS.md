<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Event descriptions and attributed progress

A described event and an actual occurrence have different identities.
`EventDescription` specializes kernel `InformationObject` and is explicitly
disjoint from `Event`. An anticipated occurrence can therefore be described
without asserting an Event, its participants or a time. This is a descriptive
seam, not a generic system of anticipated entities. Preserve the description's
actual identity and source evidence; an actual realization is separate
(Principles 4, 9, 11, 17 and 19).

## One reading, one explicit progress value

`EventProgressReading` specializes the existing `Observation`. Its
`observedFeature` is exactly one actual Event or EventDescription. Its
`observationResult` is exactly one member of the closed EventProgress set:
`eventAwaited`, `eventInProgress` or `eventCompleted`. The set is authored once
with `logic:oneOf`; an extension value cannot evade the conditional obligations.
These are quality values, not Event kinds, temporal coordinates or confidence.

A reading carries at least one actual `vantage` with no upper bound. Actual
joint holders can share a reading; coexisting frames and historically different
reports can retain separate readings. Neither status nor vantage becomes a
global functional property of an Event, description or context. A source may
leave progress unspecified; missing fields do not select any value.

No extra status/feature/vantage predicates are needed: the Observation roles
already carry those meanings, and observationResult admits abstract quality
values. The local required paths and maxima specialize these reading records
without changing generic Observation, Event or temporal contracts.

## Realization is only for a description reported realized

An awaited reading observes an EventDescription and has no progressRealization.
It reports anticipation in its actual frame, not universal nonexistence. No
Event, participant, occurrence time or execution record is synthesized.

A description reported in progress or completed has exactly one actual Event
through `progressRealization`. Retain the source evidence that identifies that
occurrence as the description's realization. Endpoint and presence admission
do not independently verify that correspondence or the report's truth.

For a reading directly observing an actual Event, progressRealization is
forbidden even when it repeats that same Event. The feature already identifies
the occurrence; an additional realization slot must not accept an unrelated
Event. Awaited status is invalid on that direct Event feature. Description/Event
disjointness also rejects attempts to satisfy both sides by dual typing.

A later realized reading can observe the same description and supply an actual
occurrence while an earlier or rival-frame awaited reading keeps none. Neither
the description nor the old reading is globally mutated, retired or erased.
No latest-status computation or global present is implied.

## Canonical structural and Formula obligations

EventProgressReading has seven direct restrictions: the feature union and
maximum, result class and maximum, uncapped Entity vantages, and actual Event
realization class and maximum. Three class-scoped closure entries make feature,
result and vantage required. The optional progressRealization role has its own
canonical domain/range and one property-global supplied-value closure, so direct
use still selects reading/Event endpoint checks. No generic role becomes
functional and unrelated Events need no progress record.

EventProgressBindingsConstraint has one outer reading-class guard and five
independently focused arms. In this schematic form, r is that reading:

```text
A1 result(r,eventAwaited) and feature(r,f) -> type(f,EventDescription)
A2 result(r,eventAwaited) -> not exists p. progressRealization(r,p)
A3 result(r,eventInProgress) and feature(r,d) and type(d,EventDescription)
     -> exists p. progressRealization(r,p)
A4 result(r,eventCompleted) and feature(r,d) and type(d,EventDescription)
     -> exists p. progressRealization(r,p)
A5 feature(r,e) and type(e,Event)
     -> not exists p. progressRealization(r,p)
```

The actual serialized Formula gives every arm fresh local binders. Each guard
positively binds the same reading. All four existential bodies are single
atoms: two required realization witnesses and two forbidden-presence probes.
No joined existential, dynamic selector, equality policy, component filter or
native failure marker is used. Every required or forbidden realization belongs
to that very reading, never another reading elsewhere in the graph.

The closed three-value result set and Event/EventDescription feature union
combine with the five arms to cover each branch. In particular, awaited status
cannot acquire an actual realization; a reported realized description cannot
omit one; and a direct actual Event cannot smuggle in another occurrence
through a redundant role. Vantage cardinality remains a minimum only.

These are canonical `logic:` source obligations with generated validation
projections, not hand-authored shapes. The inspected projection lowers each
independent guard as a scoped violation arm and retains correlated atomic
presence/absence probes. Native marker execution, generated acceptance and
consumer behavior require their own evidence; a source definition is not an
execution receipt or a reasoner verdict.

## Progress does not decide another axis

Explicit source reports supply progress. An absent end, an open commitment,
a timestamp or a missing field supplies no awaiting, underway or completion
claim. Anticipation of an event is not commitment to a future subject stage.
An in-progress or completed report proves neither a goal outcome nor subject
continuity. It supplies no participant, generated result or end time.

The existing `logic:occurrentBoundary` contract distinguishes Open/Closed
occurrences for its own purpose. Open alone does not distinguish anticipation
from an occurrence underway. Coexisting progress reports do not produce global
boundary values or override the boundary-mismatch invariant. No executor,
clock sorter, linear-time assumption, latest-reading selector or automatic
transition is introduced. See the [owning guide](../docs.md#attributed-event-progress)
for the complete source-facing contract.
