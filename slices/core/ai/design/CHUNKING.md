<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Contract-bound chunkings and summary retrieval

`Chunking` is one recorded segmentation of an exact source revision. It names the
actual source artifact and frozen rules artifact through `chunkingSource` and
`chunkingContract`, with selected digest pins for each. These artifacts carry
their own `contentDigest` values. A digest of the enclosing segmentation record
does not identify the contract, and a contract digest alone does not identify a
segmentation of every possible source (Principles 4, 5 and 11).

`chunkingContract` specializes the existing `contextIncluded` relation: the rules
artifact was present in the result’s creation context. The stronger local relation
identifies which artifact supplies the rules. Each pin must match an actual
`contentDigest` of its named artifact. Other algorithm-labelled digests remain
valid on the artifact. A result may also carry its own digest, with its own byte
meaning; neither the global digest property nor `hasGranularity` becomes
functional. An explicit `hasGranularity` value is required on the selected
chunking, using the existing open granularity vocabulary (Principles 9 and 19).

The rules artifact is an `InformationObject`. `logic:ReasoningContract` selects
reasoner semantics, and `logic:PrescriptionVersion` versions a plan; a rules file
does not by itself assert either of those meanings or an executed workflow.

## Selected chunks and order

`ChunkingMember` is a role of the existing `Chunk`, retaining its contiguous-cut
meaning. `inChunking` selects exactly one result for that member, while the
existing `entryOrdinal` gives its nonnegative, zero-based position within that
result. Two members at one ordinal must be the same member. Re-segmentation uses
fresh chunk identities, as the existing `chunkOf` contract already requires.
These scoped obligations leave ordinary chunks outside a declared chunking valid.

Each selected member has an actual `chunkOf` carrier and `spanStart`/`spanEnd`
bounds. Its carrier must equal `chunkingSource`. The existing span vocabulary
fixes zero-based, half-open Unicode-code-point coordinates and requires end to
exceed start. Ordinals do not manufacture source order, consecutive positions,
non-overlap or exhaustive coverage: those choices belong to the declared
contract. A result’s named members are not a proof that its source was fully
enumerated.

## Atomic blocks share an explicit origin

`ContentBlock` retains its structural identity, division, index and kind in the
documents slice. `BlockExtent` records a particular whole-source extent of that
block through `extentOfBlock`, `extentSource`, and the existing span bounds.
`extentSource` is the exact carrier whose code-point origin those bounds use.
The extent record keeps whole-source offsets separate from any block-local or
division-local coordinates (Principles 15, 16 and 17).

A selected chunk’s `coversBlockExtent` relations identify the covered block
occurrences. Each asserted extent must have the selected source origin and
actually intersect the chunk. The atomic-boundary law examines every represented
`BlockExtent` in that same exact source frame, whether or not the chunk already
claims to cover it. Intersection requires full containment and the corresponding
coverage relation. A producer cannot hide a represented split by omitting that
relation. Merely touching an exclusive boundary does not cover a block.

The laws use the existing numeric comparison primitives for intersection and
containment. Records stay information objects: `math:Interval` denotes an
ordered set with explicit endpoint inclusions, while `math:Sequence` denotes a
function. Neither is the sort of a segmentation record or block-coordinate
record. `lang:literalValue` supplies the domain-free literal carrier for the
selected digest pins without interpreting them as numeric values.

These are constraints over the represented artifacts, members and coordinates.
They do not parse source bytes, calculate offsets, execute an opaque contract,
verify a hash, or certify that every source block has been represented. The
importer supplies faithful coordinates and the required atomic block inventory.
The declared contract supplies overlap and exhaustive-coverage policies; the
ontology supplies no hidden defaults or general contract-compliance certificate.

## A partial, overlapping segmentation

This ordinary scene records three atomic paragraphs. Its explicit contract
permits overlap and does not require exhaustive coverage. One chunk covers the
first paragraph; another covers the first two. The third paragraph is left out.
Both chunks respect every represented atomic boundary.

The source and contract artifacts below are exactly their `lang:literalValue`
text encoded as UTF-8, without a byte-order mark or trailing newline. The shown
SHA-256 values were calculated from those exact text bytes, not from a Turtle
serialization, the chunking record, or a description of the rules. The source
contains two newline characters. Its block ranges are `[0,6)`, `[7,12)`, and
`[13,19)`. The newline between paragraphs may belong to a contiguous chunk
without becoming a new atomic paragraph.

```turtle
@prefix ex: <https://example.org/chunking-scene/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:source a gmeow:InformationObject ;
    lang:literalValue "Alpha.\nBeta.\nGamma." ;
    gmeow:contentDigest "sha256:7cf72eeddc0c6811824aa5087f4e560067d06a4dc33407822730b6d525248a14" .
ex:rules a gmeow:InformationObject ;
    lang:literalValue "Use zero-based half-open Unicode code point offsets. Keep atomic blocks whole. Overlap is permitted. Coverage need not be exhaustive. Preserve the declared chunk order." ;
    gmeow:contentDigest "sha256:b5a3b69e5ea5e6ba74ac565d740c281dd2d852286e7bf770286f7a413334af62" .
ex:paragraphLevel a gmeow:GranularityLevel .
ex:segmentation a gmeow:Chunking ;
    gmeow:chunkingSource ex:source ;
    gmeow:chunkingSourceDigest "sha256:7cf72eeddc0c6811824aa5087f4e560067d06a4dc33407822730b6d525248a14" ;
    gmeow:chunkingContract ex:rules ;
    gmeow:chunkingContractDigest "sha256:b5a3b69e5ea5e6ba74ac565d740c281dd2d852286e7bf770286f7a413334af62" ;
    gmeow:hasGranularity ex:paragraphLevel .
ex:division a gmeow:ContentSegment ;
    gmeow:segmentType gmeow:segmentTypeChapter ;
    gmeow:segmentIndex 1 ; gmeow:segmentOf ex:source .
ex:a a gmeow:ContentBlock ; gmeow:blockOf ex:division ;
    gmeow:segmentType gmeow:segmentTypeParagraph ; gmeow:segmentIndex 1 .
ex:b a gmeow:ContentBlock ; gmeow:blockOf ex:division ;
    gmeow:segmentType gmeow:segmentTypeParagraph ; gmeow:segmentIndex 2 .
ex:c a gmeow:ContentBlock ; gmeow:blockOf ex:division ;
    gmeow:segmentType gmeow:segmentTypeParagraph ; gmeow:segmentIndex 3 .
ex:aExtent a gmeow:BlockExtent ;
    gmeow:extentOfBlock ex:a ; gmeow:extentSource ex:source ;
    gmeow:spanStart "0"^^xsd:nonNegativeInteger ; gmeow:spanEnd "6"^^xsd:nonNegativeInteger .
ex:bExtent a gmeow:BlockExtent ;
    gmeow:extentOfBlock ex:b ; gmeow:extentSource ex:source ;
    gmeow:spanStart "7"^^xsd:nonNegativeInteger ; gmeow:spanEnd "12"^^xsd:nonNegativeInteger .
ex:cExtent a gmeow:BlockExtent ;
    gmeow:extentOfBlock ex:c ; gmeow:extentSource ex:source ;
    gmeow:spanStart "13"^^xsd:nonNegativeInteger ; gmeow:spanEnd "19"^^xsd:nonNegativeInteger .
ex:first a gmeow:ChunkingMember ;
    gmeow:inChunking ex:segmentation ; gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:chunkOf ex:source ; gmeow:spanStart "0"^^xsd:nonNegativeInteger ; gmeow:spanEnd "6"^^xsd:nonNegativeInteger ;
    gmeow:contentDigest "sha256:e3b0b40be44bd13bc38213d1116b4ab4b61bb1a3ada1d2aaa22d74674926aaf1" ;
    gmeow:coversBlockExtent ex:aExtent .
ex:firstTwo a gmeow:ChunkingMember ;
    gmeow:inChunking ex:segmentation ; gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:chunkOf ex:source ; gmeow:spanStart "0"^^xsd:nonNegativeInteger ; gmeow:spanEnd "12"^^xsd:nonNegativeInteger ;
    gmeow:contentDigest "sha256:2621964493b8330a31a84ca7418dcbaa508f633436ad22b8eca4aa962437afc4" ;
    gmeow:coversBlockExtent ex:aExtent, ex:bExtent .
```

## A summary’s carrier is distinct from what it summarizes

A generated summary is a condensed account derived from its division. It is not
a contiguous cut of that division. `wasDerivedFrom` records the summarized
division; `chunkOf` records the actual carrier containing retrieved summary text.
A separate `Summary` artifact and retrieval `Chunk` express both meanings without
inventing a new summary relation or moving the GraphRAG community boundary.

The next carrier contains `Summary: Alpha and beta.` followed by one newline.
Its exact UTF-8 content digest is shown. The retrieved account occupies
`[9,24)` in that carrier, rather than any interval of the original division.

```turtle
ex:summaryCarrier a gmeow:Summary ;
    gmeow:wasDerivedFrom ex:division ;
    lang:literalValue "Summary: Alpha and beta.\n" ;
    gmeow:contentDigest "sha256:cb3ad665f702398b477870fae600fe52503aa8f8df79f0195f6746f4ceed64df" .
ex:retrievedSummary a gmeow:Chunk ;
    gmeow:chunkOf ex:summaryCarrier ;
    gmeow:spanStart "9"^^xsd:nonNegativeInteger ; gmeow:spanEnd "24"^^xsd:nonNegativeInteger ;
    gmeow:contentDigest "sha256:5d4e54583f969b7a942b5a7330714698f4d3499da46fae0fea23e9672116100e" .
```

One node may also be both `Summary` and `Chunk` when its own text is a condensed
account and a contiguous cut of its actual carrier. In that case it retains
`wasDerivedFrom ex:division` separately from `chunkOf ex:summaryCarrier`. Dual
typing never licenses `chunkOf ex:division` merely because the text summarizes
that division. `CommunitySummary` continues to mean a summary of a GraphRAG
community, not an arbitrary reference-work division (Principles 2 and 9).
