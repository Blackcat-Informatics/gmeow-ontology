<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Contract-bound chunkings and summary retrieval

`Chunking` is one recorded segmentation of an exact source revision. It names the
actual source artifact and frozen rules artifact through `chunkingSource` and
`chunkingRulesArtifact`, with selected digest pins for each. These artifacts carry
their own `contentDigest` values. A digest of the enclosing segmentation record
does not identify the contract, and a contract digest alone does not identify a
segmentation of every possible source (Principles 4, 5 and 11).

`chunkingRulesArtifact` specializes the existing `contextIncluded` relation: the rules
artifact was present in the result’s creation context. The stronger local relation
identifies which artifact supplies the rules. Each pin must be the same complete
RDF literal as a `contentDigest` of its named artifact: both the lowercase
hexadecimal spelling and the algorithm datatype IRI match. BLAKE3-256, SHA-256
and MD5 use `blake3_256Digest`, `sha256Digest` and `md5Digest`, respectively,
grounded in the mathematics slice. They have no inline algorithm prefix,
language or direction. Equal hexadecimal text under different algorithm
datatypes does not satisfy the pin. Other algorithm digests remain valid on
the artifact. A result may also carry its own digest, with its own byte
meaning; neither the global digest property nor `hasGranularity` becomes
functional. A Chunk admits at most one distinct digest per algorithm; several
algorithms coexist. A selected source pin and contract pin each have exactly one
value under the existing closed-world Chunking contract. Their datatypes admit
supplied values; a typed literal alone does not verify the bytes. An explicit
`hasGranularity` value is required on the selected chunking, using the existing open granularity vocabulary (Principles 9 and 19).

The rules artifact is an `InformationObject`. `logic:ReasoningContract` selects
reasoner semantics, and `logic:PrescriptionVersion` versions a plan; a rules file
does not by itself assert either of those meanings or an executed workflow.

## Artifact bytes and segmentation identity

A Chunk digest hashes the exact supplied bytes of that chunk's own content.
The source pin hashes the named source artifact, and the contract pin hashes
the named rules artifact. Unicode-code-point span coordinates do not select an
encoding or a byte normalization. If an application hashes a canonical document,
it must identify that canonical artifact and its bytes; the digest cannot be
reused as the identity of a different original serialization.

The source artifact and its pin, the contract artifact and its pin, and the
ordered selected members describe a segmentation together. Hashing that tuple
under a scheme produces a scheme identifier, not the content digest of the
source, rules or a member. A result's own content digest is valid only for its
own identified artifact bytes. Existing same-term witness laws continue to bind
source and contract pins separately (Principles 4, 7 and 19).

The research-object chunk record set preserves every admitted digest as an item
with `algorithm`, `datatype` and `value`. Its `digests` field is an ItemList;
no lexically smallest digest or algorithm is selected. Missing declarations
produce an empty list, since ordinary Chunk digests are not required. Malformed
terms and conflicting values for one algorithm fail admission. The export does
not recompute the source bytes or infer verification.

## Migrating the rules-artifact relation

For a `Chunking` record that formerly used an object-valued
`chunkingContract`, replace that predicate with `chunkingRulesArtifact` and
retain the same rules-artifact object. Keep `chunkingContractDigest` and its
selected value unchanged; the artifact relation and its digest pin remain
separate statements. The literal-valued `chunkingContract` on an
`EmbeddingFamily` is a separate configuration relation and stays unchanged.
This migration supplies no compatibility alias or inferred conversion.

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
    gmeow:contentDigest "7cf72eeddc0c6811824aa5087f4e560067d06a4dc33407822730b6d525248a14"^^gmeow:sha256Digest .
ex:rules a gmeow:InformationObject ;
    lang:literalValue "Use zero-based half-open Unicode code point offsets. Keep atomic blocks whole. Overlap is permitted. Coverage need not be exhaustive. Preserve the declared chunk order." ;
    gmeow:contentDigest "b5a3b69e5ea5e6ba74ac565d740c281dd2d852286e7bf770286f7a413334af62"^^gmeow:sha256Digest .
ex:paragraphLevel a gmeow:GranularityLevel .
ex:segmentation a gmeow:Chunking ;
    gmeow:chunkingSource ex:source ;
    gmeow:chunkingSourceDigest "7cf72eeddc0c6811824aa5087f4e560067d06a4dc33407822730b6d525248a14"^^gmeow:sha256Digest ;
    gmeow:chunkingRulesArtifact ex:rules ;
    gmeow:chunkingContractDigest "b5a3b69e5ea5e6ba74ac565d740c281dd2d852286e7bf770286f7a413334af62"^^gmeow:sha256Digest ;
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
    gmeow:contentDigest "e3b0b40be44bd13bc38213d1116b4ab4b61bb1a3ada1d2aaa22d74674926aaf1"^^gmeow:sha256Digest ;
    gmeow:coversBlockExtent ex:aExtent .
ex:firstTwo a gmeow:ChunkingMember ;
    gmeow:inChunking ex:segmentation ; gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:chunkOf ex:source ; gmeow:spanStart "0"^^xsd:nonNegativeInteger ; gmeow:spanEnd "12"^^xsd:nonNegativeInteger ;
    gmeow:contentDigest "2621964493b8330a31a84ca7418dcbaa508f633436ad22b8eca4aa962437afc4"^^gmeow:sha256Digest ;
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
    gmeow:contentDigest "cb3ad665f702398b477870fae600fe52503aa8f8df79f0195f6746f4ceed64df"^^gmeow:sha256Digest .
ex:retrievedSummary a gmeow:Chunk ;
    gmeow:chunkOf ex:summaryCarrier ;
    gmeow:spanStart "9"^^xsd:nonNegativeInteger ; gmeow:spanEnd "24"^^xsd:nonNegativeInteger ;
    gmeow:contentDigest "5d4e54583f969b7a942b5a7330714698f4d3499da46fae0fea23e9672116100e"^^gmeow:sha256Digest .
```

One node may also be both `Summary` and `Chunk` when its own text is a condensed
account and a contiguous cut of its actual carrier. In that case it retains
`wasDerivedFrom ex:division` separately from `chunkOf ex:summaryCarrier`. Dual
typing never licenses `chunkOf ex:division` merely because the text summarizes
that division. `CommunitySummary` continues to mean a summary of a GraphRAG
community, not an arbitrary reference-work division (Principles 2 and 9).

## Analysis input descriptors and chunk identities

Reported Analysis inputs and contract-bound retrieval chunkings have
different roles. An Analysis can carry its supplied first and last source
keys, chapter key, source-key frame description, construction description
and actual source context directly. Those optional fields do not make it a
Chunk, Chunking, text carrier or execution event.

The integer descriptors preserve source values. Do not reuse `entryOrdinal`
without its zero-based sequence-position meaning or reinterpret keys as
half-open code-point bounds. Different source-key and text-assembly
conventions may describe inputs that cannot be compared or reconstructed from
their endpoints alone. Construction descriptions and identified source
contexts retain what was supplied without requiring a profile, digest or
input artifact.

The three text-unit granularity individuals reuse the kernel's open
GranularityLevel category and nonfunctional `hasGranularity`. They imply no
fixed size, endpoint equality or universal resolution order. Existing Chunk
and ChunkingMember source, rules, carrier, ordinal and extent obligations
remain unchanged (Principles 4, 9 and 19).

Only the optional first, last and chapter integer fields receive local
qualified maxima on Analysis; no required-path closure is added for these
descriptors. Existing literal ranges and canonical logic axioms provide their
value contracts without a new input class, global functional carrier or
hand-authored shape (Principle 17).

## Whole-content division containment

`chunkInDivision` directly asserts that a source division contains the
whole particular occurrence of source content represented by a Chunk. Its
ContentSegment target is a genuine containing source division, not a convenient
topic, a merely intersected division or identical wording elsewhere. The
relation is optional and nonfunctional; supplied nested containers may coexist
without a unique or nearest choice.

Whole-content containment justifies the broad `partOf` superproperty. That
information-content parthood leaves the Chunk's retrieval identity intact;
it does not type the subject as a structural ContentSegment or ContentBlock.
The relation therefore does not specialize `segmentOf` or `blockOf`. Its
own declaration has no transitive characteristic or property chain, even
though generic `partOf` retains its existing transitivity.

`chunkOf` continues to name the actual text carrier and coordinate origin.
For a ChunkingMember it must still equal the selected `chunkingSource`.
Source and rules-artifact digest pins, member ordinals, span bounds,
BlockExtent origins and every existing boundary law remain unchanged.
If a division genuinely is that carrier, both relations may name it; a
division link alone never rebinds the carrier or converts coordinates.

When a chunk crosses divisions, a child containing only part of its occurrence
is not a whole-chunk container. A supplied common ancestor qualifies only
when it genuinely contains the entire occurrence. The vocabulary chooses no
ancestor and adds no default when that information is absent. Existing
covered-block and structural-parent joins can expose candidate divisions,
but neither those joins nor represented extents supply an exhaustive source
inventory or a universal nearest-container policy.

The assertion preserves a supplied containment claim with its actual source,
segmentation and exact-statement attribution. It requires no new division
extent record merely to state the claim and establishes no solver-verified
geometry, byte authentication or executed selection algorithm. A separately
requested containment computation would need its own complete inputs and
proof contract; the declarative link is not that operation.

A generated summary remains distinct from the source content it summarizes.
Its original subject division is not automatically a container of the new
summary-text occurrence. Even when Summary and Chunk genuinely share a node,
retain derivation or summarized-subject information separately from the
actual carrier and any actual containing division. No new summary relation,
identity merger or structural inference follows from this containment term
(Principles 2, 4, 7, 9, 17 and 19).
