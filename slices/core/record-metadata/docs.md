<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Record metadata

This slice gives record stores a shared vocabulary for creation time, last
modification, local sequence order, fields chosen at run time and the artifacts
present in a record’s creation context. A catalogue entry, a message or an
annotation can use the same metadata without acquiring a domain-specific type.

## Preserve the time that was supplied

`recordedAt` locates creation of the record. `updatedAt` locates the last change
to the subject resource. Both accept standard `xsd:dateTime`, `xsd:date`,
`xsd:gYearMonth` and `xsd:gYear` literals. Keep the source datatype and its
actual timezone information. A month remains a month; it does not become a
midnight timestamp on the first day. An absent timezone remains absent.
These ranges introduce no new literal datatype or parsing convention.

`recordedDuring` and `updatedDuring` instead reference an established
`TimeInterval` or `NamedPeriod`. They preserve a period as an entity, independently
of any coarse literal. A vague phrase is not automatically a bounded named
period. Resolve its meaning with provenance before making that assertion.

Record creation differs from the event described, a witness asserting a claim,
a message being sent and a creative work being composed. Those clocks remain
separate. The safe `noteModifiedAt` and `dateModified` bridges expose the last
modification of the same resource through `updatedAt`.

## Keep order local

`entryOrdinal` is a zero-based `xsd:nonNegativeInteger`. Put it on an entry’s
occurrence node within its containing sequence. If two sequences order the same
content differently, give each sequence its own occurrences. The ordinal neither
identifies the content globally nor states importance. The email slice specializes
this property with its existing zero-based `participantOrdinal`.

## Treat run-time keys as content

Attach a `LabelledFacet` using `hasFacet`. Each facet has exactly one
`facetLabel`, preserving the source key as its supplied `xsd:string` or
`xsd:token`. Retain case, whitespace, punctuation and datatype. Do not convert
a verbatim string to token or normalize its source whitespace.

`facetLabelAsWritten` separately retains label wording supplied by the source.
It accepts an untagged string, a language string or a directional language
string, preserving the exact lexical form and any supplied language and
direction. Unspecified language or direction remains unspecified. Several
attributed wordings may coexist; no wording is required or preferred.
Keep the required `facetLabel` key or path separately. This relation neither
creates a normalized token key nor counts as `facetText` or `facetValue`
payload. It specializes the domain-free `lang:literalValue` carrier, preserving
the complete supplied literal without changing the existing key or payload
constraints (Principles 2, 4, 9 and 17).

A facet has at most one payload in total: `facetText` for text, or `facetValue`
for a typed literal such as a number or boolean. Text retains supplied language
and direction; untagged source text remains an `xsd:string`. A label without a
payload preserves a present key with no content. This representation does not
claim to distinguish a source null from a source empty value. Retain an explicitly supplied empty
string as text so it remains distinguishable from a missing payload.

Flatten a producer’s run-time object into one facet per leaf and keep its relative
path label, such as `mood.tone` or `refs[]`. Such labels have no universal path
parser: delimiters and array notation retain the producer’s meaning. Repeated
labels remain separate occurrences, ordered by `entryOrdinal`. Name facet nodes
by position within their record; do not place run-time keys in predicates or IRI
fragments. Reviewed domain vocabulary remains preferable when its semantics fit.

A reviewer can add `facetConcept` links to existing GMEOW `Concept` nodes.
Several interpretations may coexist. Preserve the original label and use
statement metadata when the interpretation needs provenance or a standpoint
(Principles 9 and 10). The link interprets a label; it does not assert the truth
of the labelled content.

## Identify context inputs without copying them

`contextIncluded` links a record to an information artifact that was present in
its creation context. `contextIncludedRelease` and `contextIncludedProjection`
retain the distinct input roles. Use a shared node for each release or projection,
with its own `contentDigest`. The existing algorithm-labelled digest contract
allows several digests on one artifact. No source bytes need be copied into the
record, and no new digest datatype is introduced.

A compiled projection has its own bytes and digest. Its presence does not imply
that its source release also reached the context. A release artifact can be typed
as a `Manifestation` when it embodies an edition or format of an `Expression`;
context artifacts in general are `InformationObject` instances.

Inclusion records exposure. It does not assert derivation, evidential grounding,
endorsement or actual use. Record any such stronger relationship separately when
there is evidence for it. The artifact’s digest identifies the input, while a
record’s own `contentDigest` identifies the record’s bytes.

The [cataloguing scene](examples/cataloguing.ttl) illustrates source precision,
position-named facets, typed and label-only fields, reviewed concepts and shared
context references. The [design rationale](design/RECORD-METADATA.md) explains the
canonical obligations and the bridges deliberately excluded.

## Retain an outside producer's record

Reception, authorship and truth remain separate. `ReceptionChannel` is an open
value vocabulary with conversation, tool answer, sensor feed, document read,
web fetch and component notice values. Put `receivedVia` on the record of one
holder's particular reception, or its attributed assertion reifier. Keep
independent receptions as independent occurrences even when their content is
the same. The single-channel law compares only that occurrence's channels; it
does not select a universal channel for a proposition or infer the holder from
its author. Existing statement standpoint and provenance retain that attribution.

`originatingTrace` and `originatingPacket` retain unresolved producer identifiers.
`evidenceReference` does the same for an unresolved evidence reference.
All three use `xsd:string`: verbatim producer identifiers may contain whitespace,
and `xsd:token` would normalize it. Their producer namespace remains provenance.
Use actual source, trace, message or evidence links when those resources are
established; the literal reference neither invents a node nor proves resolution.

A `MalformedValue` is a rejected field occurrence, specializing `LabelledFacet`.
Attach it with `hasMalformedValue`, a specialization of `hasFacet`.
Its required `malformedField` specializes `facetLabel`; required `valueAsSent`
specializes `facetText`. The field admits `xsd:string` or explicitly supplied
`xsd:token`, retaining its datatype and spelling. The rejected representation remains a single
`xsd:string` preserving the chosen source characters. Apply the
existing type and subproperty closure when consuming the inherited facet
contracts. Repeated keys are distinct occurrences, named by source position.
The existing text/value exclusion prevents a simultaneous typed scalar payload.

The rejected representation is kept beside the record. It is never asserted on
the intended domain predicate as an invalid date, digest or code, and is never
silently repaired. Its refusal provenance records whose contract judgement is
being retained. The ontology does not execute an arbitrary producer contract or
authenticate that judgement. Exact characters do not certify source bytes,
transport escapes or a complete serialization; preserve a separate source
artifact when those are needed.

`evidenceAsWritten` keeps the writer's evidence wording, while
`durabilityReason` and `suppressionReason` specialize the existing nonfunctional
`description`. Text preserves supplied language and direction; unknown language
remains an untagged string. Several attributed reasons may coexist. The words
alone neither prove evidential support nor cause a durable write or suppression.

Put a suppression reason on the explicit RDF 1.2 reifier of the particular
`displayable false` assertion. Its binding law uses the existing lowered
statement subject, predicate and object. Validation therefore consumes completed
statement-metadata lowering; it does not invent source component facts, execute
display policy or authenticate a writer. Quoting a statement alone does not
assert it.

The false-object comparison uses boolean value equality, so a supplied
`"0"^^xsd:boolean` remains admissible without rewriting its lexical form.

Unresolved tag wording belongs to [tags](../tags/docs.md), a note's unresolved
purpose wording to [notes](../notes/docs.md), and encoded byte size to
[sources](../sources/docs.md). [Held records](../ai/docs.md#records-held-among-prose)
reuse the existing AI-owned code-point span properties without a reverse
dependency from record metadata to AI.
The [ordinary intake scene](../ai/examples/record-intake.ttl) brings all seven surfaces
together as source declarations, without claiming execution.

## Stated reasons on records and claims

statedRationale preserves a source's verbatim reason on an event, information
record or exact statement reifier. It has no domain and no description
superproperty, so an Event is not turned into an enduring Entity merely to
retain its reason. Untagged strings, supplied language and RDF 1.2 base
direction remain intact; no language or explanation is guessed.

Several attributed reasons can coexist. Keep the actual writer and standpoint
on their exact native statements when supplied. A reason is not a computed
cause or independently verified fact. resolutionRationale specializes this
relation for an AttentionResolution and keeps that narrower domain.
Its inferred general edge is not a new assertion by the source; no mandatory
reifier is demanded for an inferred edge.

The [ordinary operation account](../agency-records/examples/stated-reasons-and-operation-reports.ttl)
keeps a record's rationale, an event's rationale and the source's competing
operation reports separate.
