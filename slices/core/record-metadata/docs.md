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
`facetLabel`, preserving the source key as `xsd:string`. This retains case,
whitespace and punctuation. It deliberately avoids `xsd:token`, whose whitespace
normalization would weaken verbatim preservation.

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
