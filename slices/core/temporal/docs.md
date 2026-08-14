<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Temporal — intervals, instants, frames, and time-scoped facts

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/temporal` · **tier: core**
> The cross-cutting temporal facility: every slice that says *when* says it here.

Most vocabularies treat time as a literal: `dateOfBirth "1972-03-01"`. GMEOW treats a
temporal value the way it treats every value — **frame-relative** (Principle 11: a date is
meaningless without its calendar and timescale), **attributed** (a date is usually a *claim*
about when something happened), and **structural** (intervals relate to each other by
topology — the Allen algebra — independent of any frame). This slice provides the four
layers that follow from that stance:

1. **Structure** — intervals and instants, related by Allen relations.
2. **Frame** — the `TemporalFrame` (calendar + timescale + reference position) every
   temporal value is read in.
3. **Claim** — temporal *measurements* (a dated claim with method, uncertainty, and
   determinacy — radiocarbon dates and "circa" both live here, honestly).
4. **Scope** — the statement-layer clocks (`validFrom`/`validUntil`/`assertedAt`/
   `recordedNoLaterThan`) that any fact in any slice can carry.

Heavy computation — interval-algebra closure, calendar conversion — is the solver layer's
job (Principle 12): the slice models the relations; `gmeow temporal` (TQL, the parameterized
queries in `queries/tql/`) evaluates them. See
[`docs/temporal-queries.md`](../../../docs/temporal-queries.md) for the TQL reference.

## The structural layer

### gmeow:TimeInterval

A bounded stretch of time, optionally open-ended, delimited by `gmeow:hasStartInstant` /
`gmeow:hasEndInstant`. Intervals are *frame-independent structure*: two intervals can be
ordered (`gmeow:intervalBefore`) even when their instants are expressed in different
calendars.

### gmeow:Instant

A point on a timeline. Carries at least one of `gmeow:instantValue` (an `xsd:dateTime`) or
`gmeow:edtfValue` (an EDTF literal — for the approximate, uncertain, and partial dates real
data is full of), and links to its frame with `gmeow:inTemporalFrame` when the default frame
does not apply. *(Shape-enforced: an instant without any value is rejected — see
`shapes.ttl` and `queries/verify/no-instant-without-value.rq`.)*

### The Allen relations

`gmeow:intervalBefore` / `intervalAfter` / `intervalMeets` / `intervalMetBy` /
`intervalOverlaps` / `intervalOverlappedBy` / `intervalStarts` / `intervalStartedBy` /
`intervalDuring` / `intervalContains` / `intervalFinishes` / `intervalFinishedBy` /
`intervalCoincidesWith` — the thirteen jointly-exhaustive, pairwise-disjoint interval
relations. JEPD is gate-checked on the reasoned graph
(`queries/verify/no-jepd-violation.rq`); relation *composition* is computed by the solver,
never asserted (Principle 12).

### gmeow:TimeScopedRelation

The reified relator for facts that hold over a period — the *promoted* form of the
flat-first pattern. Use the `validFrom`/`validUntil` annotations for the 80 % case; promote
to a `TimeScopedRelation` (with `gmeow:duringInterval`) when the tenure itself needs
identity, provenance, or standpoint.

## The frame layer (Principle 11)

### gmeow:TemporalFrame

A reference frame for temporal values: exactly one `gmeow:frameTimeScale` (UTC, TAI, a
ship's log), an optional `gmeow:frameCalendarSystem` (Gregorian, Julian, a fictional
calendar), and an optional `gmeow:frameReferencePosition`. A non-default date *names its
frame*; conversion between frames is a computation, not an assertion. *(Shape-enforced:
exactly one timescale, realm, and kind.)*

### gmeow:TimeScale · gmeow:CalendarSystem · gmeow:ReferencePosition

Open value vocabularies (Principle 9 — never enums): a new calendar is data, not a schema
change. The calendar slice builds its scheduling machinery atop these.

### gmeow:DayOfWeek and the week cycle

The seven ISO-8601 weekday positions (`gmeow:dayMonday` … `gmeow:daySunday`) are a *closed*
value vocabulary — closed because ISO 8601 closes it, which is exactly the case Principle 9
exempts from the open-vocabulary rule. A weekday is a **pattern slot, not a located span**:
`gmeow:dayMonday` picks out every Monday until an interval, a recurrence, or a frame anchors
it to one. That is why the class sits beside `gmeow:CalendarSystem` and `gmeow:PeriodType`
rather than beside the intervals: it is calendar *structure*, not a stretch of timeline.

Each day is documented by what actually distinguishes it — its ISO-8601 ordinal, its role in
the business-week / weekend partition, and the boundary rule it does or does not carry.
`gmeow:dayMonday` opens the ISO week and is what an ISO week number counts from;
`gmeow:dayThursday` is the week's median day and the one that decides which *year* a week
belongs to (week 1 holds January's first Thursday); `gmeow:daySunday` closes the ISO week
even though the Gregorian liturgical and North American conventions place it first. A rule
that says "the start of the week" or "midweek" must therefore name its convention — the days
themselves stay bare positions.

Two consumers reach this vocabulary. Opening hours in the **organization** slice range
`gmeow:openingDay` over it; the **calendar** slice's `gmeow:EventSchedule` /
`gmeow:ScheduleException` recurrence machinery needs the same seven values. `temporal` is
the right home precisely because `calendar` already depends on it, so both consumers reach
the days without `calendar` having to depend on `organization`.

### gmeow:NamedPeriod

A first-class named span — "the Bronze Age", "Q3 FY2026", "the Edo period" — with
`gmeow:periodStart`/`periodEnd`/`periodPartOf`/`periodContainsPeriod` structure and an open
`gmeow:periodType`. Periods are *claims about spans* and are typically standpoint-indexed
(archaeological periodisations disagree; both coexist, Principle 9).

## The claim layer

### gmeow:TemporalMeasurement

A dated claim: `gmeow:measuredDate` (or `gmeow:measuredAge`), a `gmeow:measurementMethod`
(`gmeow:DatingMethod` — radiocarbon, dendrochronology, stratigraphy, archival), a
`gmeow:measurementUncertainty`, and a `gmeow:measurementDeterminacy` (ontic vagueness held
apart from epistemic confidence — "circa 1850" is *determinately vague*, not unconfident).
Attach with `gmeow:hasTemporalMeasurement`; this is the unified observation stance applied
to time itself.

## The scope layer (the statement clocks)

### gmeow:validFrom · gmeow:validUntil · gmeow:assertedAt · gmeow:recordedNoLaterThan

Annotation properties (so the OWL downcast stays OWL 2 DL — Principles 2–3) carried by
reified statements in any slice: fact-time (`validFrom`/`validUntil`), assertion-time
(`assertedAt`), and the archival bound (`recordedNoLaterThan`). Together with standpoint
tenure these are the clocks that let a fact be true *then*, asserted *later*, recorded
*later still*, and disputed *now* — without collapsing any of those into another.

Convenience datatype properties for simple event timing (`gmeow:atTime`,
`gmeow:startedAtTime`, `gmeow:endedAtTime`) follow the flat-first pattern.

### gmeow:observationCutoff and keep-on-unbound (Principle 9)

`gmeow:observationCutoff` names the *derived* observation cutoff of a claim —
`COALESCE(gmeow:assertedAt, gmeow:recordedNoLaterThan)`, prefer `assertedAt`, else the
`recordedNoLaterThan` terminus ante quem — canonically computed by the bitemporal as-of
query (`queries/tql/bitemporal.rq`). It is **never asserted directly**; cite the IRI
instead of re-deriving the COALESCE.

**Keep-on-unbound is normative.** A claim with no observation cutoff at all (neither
`assertedAt` nor `recordedNoLaterThan` bound) has an *unknown* observation time, and the
as-of query **retains** it rather than dropping it — an explicit open-world default, not
an oversight. `!BOUND(?observed) || ?observed <= ?asOf` in `bitemporal.rq` is the
normative expression of this: an unbound observation time never disqualifies a claim.

**The stricter-than-slice pattern.** Some downstream consumers legitimately want a
*narrower* reading — e.g. a compliance profile that must reject any claim it cannot date.
The slice sanctions this as an explicit, profile-owned opt-in rather than changing the
slice-wide default: a profile asserts `gmeow:observationWindowRequired true` on itself,
declaring that IT requires a bound `gmeow:observationCutoff`, and that profile's own
downstream projection classifies any claim whose cutoff is unbound into its own typed
exclusion set (nonconforming for that profile only). The temporal slice ships the marker
and names the pattern; it does not enforce the exclusion itself. Certifying that
enforcement as a slice-level closed-world `logic:Constraint` over the statement-layer
annotation clocks would exceed the range-restricted guarded fragment the native coherence
gate decides (an annotation-property absence check is not an object/datatype edge the
guarded existential/counting deciders reach), so the boundary is recorded honestly as
`gmeow:ObservationWindowClosedWorldBoundary` (a `logic:expressivenessBoundary
logic:FirstOrder` retained-withhold record, mirroring `logic:rdf12-nested-triple-term`)
rather than improvised as an unsupported constraint. See
[`docs/MIGRATING-SHAPES-TO-LOGIC.md`](../../../docs/MIGRATING-SHAPES-TO-LOGIC.md) for the
doctrine on when a gap is an honest boundary rather than a constraint to author.

## Alignment & projection

Aligned by reference to **OWL-Time** (`time:Instant`/`time:Interval`/`time:TRS`, the Allen
relations) and **EDTF**; projected to pure OWL-Time via the `owl-time` profile
(`mappings/owl-time.ttl` — compiled to EDOAL + the executable CONSTRUCT). The frame concept
maps to `time:TRS`; GMEOW's addition is making the frame *first-class and self-describing*
(a Profile, Principle 11) rather than an opaque IRI.

## Dependencies

Depends on `core` (gufo grounding, base properties) and `profiles` (the reference-frame
Profile pattern). Depended on by nearly everything — events, places (tenures), calendar,
employment, lifecycle, and the statement layer's clocks.
