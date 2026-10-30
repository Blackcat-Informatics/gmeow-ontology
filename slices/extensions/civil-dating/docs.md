<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Civil-day evidence without an invented instant

A `CivilDateExpression` records the civil-day designation a source asserted.
It is an `InformationObject`, distinct from an `Instant`. Its required
`civilDateLexical` retains one complete RDF literal: lexical form, datatype,
language and direction survive when supplied. This is evidence of the source’s
assertion, not a date parser or a certificate that the designation exists in its
calendar (Principles 2, 4, 9 and 11).

`civilDateFrame` specializes the existing `hasReferenceFrame` spine and names
exactly one `TemporalFrame` for this expression. A class-guarded Formula law
requires that frame to name a typed `CalendarSystem` through the existing
`frameCalendarSystem`. The frame retains its normal scale, realm and kind
obligations. No Gregorian, UTC, location or day-boundary convention is guessed.
A calendar system supplies the recorded interpretation context; it supplies no
missing clock reading or conversion algorithm.

`hasGranularity` is domain-free and nonfunctional. The expression’s local
restrictions require `granularityDay` and constrain its granularity values to
that day designation; other uses of the generic property remain unchanged.
The generic `hasReferenceFrame` property has an existing result-propagation
chain and stays free of cardinality restrictions. Exactly-one is local to its
simple `civilDateFrame` specialization. No global functional property or
identity key is introduced (Principles 17 and 19).

`CivilDateObservation` specializes the universal `Observation`. Direct canonical
restrictions and closure entries require an `observedFeature` that is an `Event`,
one or more `CivilDateExpression` results, and the inherited `vantage` role.
Several results and joint or competing vantages can coexist. Existing provenance
properties retain the source record; `vantage` is the relator counterpart of
statement-level `accordingTo`, without adding an object-property axiom to that
annotation property.

## An ordinary archival statement

Suppose an archive explicitly reports a Gregorian civil day using the existing
UTC Gregorian frame. The example preserves the source’s actual `xsd:date`.
This datatype retains its intrinsic Gregorian semantics. It is not an instant,
and no midnight is added.

```turtle
@prefix ex: <https://example.org/civil-dating/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:notice a gmeow:InformationObject .
ex:archivist a gmeow:Agent .
ex:opening a gmeow:Event .
ex:day a gmeow:CivilDateExpression ;
    gmeow:civilDateLexical "2026-10-30"^^xsd:date ;
    gmeow:civilDateFrame gmeow:temporalFrameUTCGregorian ;
    gmeow:hasGranularity gmeow:granularityDay ;
    gmeow:wasDerivedFrom ex:notice .
ex:dating a gmeow:CivilDateObservation ;
    gmeow:observedFeature ex:opening ;
    gmeow:observationResult ex:day ;
    gmeow:vantage ex:archivist ;
    gmeow:wasDerivedFrom ex:notice .
```

A source using the Hebrew civil frame can instead retain its own words,
including RDF 1.2 direction, without assigning Gregorian `xsd:date` semantics
to them. This separate illustrative record claims no equivalence to the day
above and performs no conversion.

```turtle
ex:hebrewDay a gmeow:CivilDateExpression ;
    gmeow:civilDateLexical "א׳ בתשרי תשפ״ז"@he--rtl ;
    gmeow:civilDateFrame gmeow:temporalFrameUTCHebrew ;
    gmeow:hasGranularity gmeow:granularityDay .
```

An untagged string explicitly records unspecified language. No language tag is
inferred from its characters. A supplied typed literal retains its own datatype
semantics even when a source has typed it incorrectly: preserving such a record
is evidence of the producer’s assertion, never endorsement of that typing.
Do not retype non-Gregorian wording as `xsd:date` to make it resemble a machine
calendar value. See [literal semantics](../../../docs/LITERALS.md).

## Discovery and limits

Generic observation queries can follow `observedFeature`, `observationResult`
and `vantage` to these records. Temporal frame queries can follow
`hasReferenceFrame` from the result. Existing instant-only temporal queries do
not acquire parsing, arithmetic or a civil-date inclusion rule from this slice.
No timestamp, interval duration, calendar conversion or external equivalence is
inferred from retaining a civil-day expression.

This is a faithful source-evidence representation. Direct typed day, month and
year values on the broader temporal properties remain a separate migration;
this slice makes no claim to implement that behavior. The temporal owner keeps
its current timestamp datatypes while documenting that a declared datatype and
`hasGranularity` never authorize padding an incomplete date into a point.
