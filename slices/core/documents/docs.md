<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Documents — works, releases, segments, and the depiction spine

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/documents` · **tier: core**
> The everyday creative-work vocabulary: what got written, released, segmented, titled, and pictured.

Creative works and document metadata — articles, patents, datasets, media, web pages (the schema.org / BIBO / BIBFRAME superset layer).

This slice furnishes the working subkinds of the four-tier WEMI spine:
`gmeow:Document`, `gmeow:Article`, `gmeow:Patent`, `gmeow:Dataset`,
`gmeow:LiteraryWork`, `gmeow:SerialWork`, `gmeow:Collection`, and `gmeow:Service`
specialize `gmeow:Work` (the abstract creation); `gmeow:MediaObject`, `gmeow:WebPage`,
`gmeow:BookRelease`, and `gmeow:SerialInstallment` specialize `gmeow:Manifestation`
(the concrete artifact). The umbrella `gmeow:CreativeWork` is a Category, not a Kind —
identity criteria live on the tiers, never on the umbrella.

Aboutness is kept honest by the tags-slice trichotomy (Principle 9): `rdf:type` says
what a thing *is*, `gmeow:hasTag` says what someone *labelled* it, and `gmeow:isAbout`
says what it *concerns* — three axes, property-disjoint, never collapsed.
`gmeow:depicts` is the image-shaped arm of `isAbout`, and `gmeow:ContentSegment` gives
every work structural decomposition without inventing per-genre part classes. Reading
order, finally, is a *standpoint* (`gmeow:ReadingOrder`) — publication order and
internal chronology coexist as subjective orderings, never canonical truth.

## Works and their structure

### gmeow:CreativeWork

The abstract umbrella Category for intellectual and artistic creations across the WEMI
spine. Formerly a flat Kind (pre WEMI spine); now a Category so each of the four tiers
beneath it can be a gufo:Kind with its own identity criteria. Carries the flat
metadata properties (`gmeow:title`, `gmeow:identifier`, `gmeow:datePublished`, the DC
date and description refinements).

### gmeow:ContentSegment

A structural part of a work, release, or installment — chapter, section, scene,
episode — a genuine Kind whose identity is position-and-type within a containing
whole. One class for all segments: the *kind* of segment is a value, never a subclass.

### gmeow:hasSegment · gmeow:segmentOf

The segment mereology, specializing the universal `gmeow:hasPart`/`gmeow:partOf`
spine. `segmentOf` is transitive — a scene of a chapter of a book is a segment of the
book; `hasSegment` is its non-functional inverse.

### gmeow:segmentType · gmeow:segmentIndex

Per-segment classification and position: `segmentType` points (functionally) into the
open `gmeow:ContentSegmentType` value vocabulary (chapter, section, scene, paragraph,
front matter, back matter — seeds, not a closed enum, Principle 9); `segmentIndex` is a
1-based-by-convention integer.

### gmeow:CreativeWorkTitle · gmeow:hasTitle

The structured title: an Appellation subkind carrying its own `gmeow:nameLanguage` and
`gmeow:nameScript`, so co-equal multilingual titles ("The Matrix" / "黑客帝国") are
separate first-class objects — never alternateName subordinates, none primary
(Principle 9). Superseded titles set `gmeow:displayable` false instead of being deleted
(Principle 10). The flat `gmeow:title` literal remains the 80 %-case shortcut.

### gmeow:ReadingOrder

A `gmeow:Standpoint` subkind that supplies a subjective consumption ordering for the
segments of a work — publication order, internal chronology, author-recommended,
fandom. Claims are annotated `gmeow:accordingTo` a ReadingOrder; no order is ever
ontology truth.

## Syndicated content

### gmeow:FeedPosting · gmeow:feedPostingKind · gmeow:quotesContent · gmeow:sharesContent · gmeow:DataFeed

The syndication surface (MoveAction projection). A `gmeow:FeedPosting` is a short-form work published
to a feed or platform — its kind (`gmeow:feedPostingKind`: social / blog / microblog)
is an **open value**, never a subclass (P9), and drives the schema split
(`schema:SocialMediaPosting` vs `schema:BlogPosting`). `gmeow:quotesContent` (with the
inline `gmeow:quotedText`) becomes `schema:Quotation` + `schema:citation`;
`gmeow:sharesContent` is the repost/boost edge → `schema:sharedContent` /
`sioc:links_to`. A `gmeow:DataFeed` is a published ordered item stream (RSS/Atom/JSON
Feed, a timeline) whose members ride the `hasPart` spine → `schema:DataFeed` /
`dataFeedElement` / `DataFeedItem`. FeedPostings **resolve the email slice's declared
SIOC partiality**: a posting IS a `sioc:Post`, so email is no longer SIOC's only
consumer. (`gmeow:Posting` in the finance slice is an unrelated ledger term — the feed
posting is deliberately `FeedPosting`.)

## Web presence

### gmeow:WebSite · gmeow:pageOfSite · gmeow:pagePrincipalSubject · gmeow:ProfilePage

The web-presence model. A `gmeow:WebSite` is the published collection; a
`gmeow:WebPage` belongs to it via `gmeow:pageOfSite` (a subproperty of `partOf`, so
site membership rides the universal mereology — the discrete hierarchy a breadcrumb
trail walks). `gmeow:pagePrincipalSubject` (functional, a subproperty of `isAbout`) names
the one entity a page principally represents. `gmeow:ProfilePage` is a **defined**
class — any WebPage whose `pagePrincipalSubject` is a `gmeow:Agent` — so
`schema:ProfilePage` / `schema:WebSite` / `schema:mainEntityOfPage` and the
`BreadcrumbList`/`ListItem` walk all derive structurally, never from stored
navigation config.

### gmeow:contentLanguage

The natural language of a **work's body** — distinct from the names slice's
per-appellation language tag, which is the language of a *name*. Non-functional (a
multilingual work has several); projects to `schema:inLanguage` / `schema:iso6391Code`
by reconstructing the BCP-47 / ISO 639-1 code from the `gmeow:Language`.

## Manifestations and the image-depiction spine

### gmeow:MediaObject

An image, audio, or video file — a concrete Manifestation carrying the technical
metadata of the design: `gmeow:pixelWidth`/`gmeow:pixelHeight` (functional),
`gmeow:imageOrientation` (EXIF degrees — the transform math is solver work,
Principle 12), `gmeow:captureTime` (non-functional: EXIF and catalogue claims coexist
with confidence), and `gmeow:captureDevice`. Declares `gmeow:requiresFrame
gmeow:colourspace` at warning severity (Principle 11).

### gmeow:depicts · gmeow:depictedIn

The flat depiction foundation (gathered here by the dependency refactor): any
MediaObject can depict any Entity with zero image machinery, as a subproperty of
`gmeow:isAbout`. The pair `gmeow:pairsWith` the `gmeow:DepictionUsage` relator in the
images extension — promote when context, audience, period, confidence, or evidence of
the depiction matters (regions and scene graphs live there too).

### gmeow:colourspace

The colourspace *reference frame* of a media object's pixel values — sRGB, Adobe RGB,
CMYK — a subproperty of `gmeow:hasReferenceFrame` (Principle 11: a pixel value is
meaningless without its frame). Deliberately not OWL-functional so merged standpoints
never trigger `owl:sameAs` collapse; single-valuedness is SHACL's job.

### gmeow:BookRelease · gmeow:SerialInstallment

Concrete publication artifacts: an edition or release of a book, and a single issue,
episode, or chapter of a serial. Both are out-of-universe, rights-bearing
Manifestations, held strictly apart from any in-universe narrative frame they source
(linked via `gmeow:sourceFor`).

## Media identity

`gmeow:mediaType` is an object property from Entity to the open MediaType value
vocabulary. Registered MIME types use their IANA resource IRIs, such as
`<https://www.iana.org/assignments/media-types/text/html>`. An unregistered
format uses a GMEOW identity under
`https://blackcatinformatics.ca/gmeow/media-types/<type>/<subtype>`, such as
`<https://blackcatinformatics.ca/gmeow/media-types/text/x-gmeow-patch>`.
These are format values, grounded as `logic:QualityValue` individuals rather
than carrier subclasses (Principles 4 and 17).

`gmeow:mediaTypeName` carries the value's exact parameter-free MIME
`type/subtype` spelling as xsd:string or explicitly supplied xsd:token.
Preserve the supplied literal's lexical form and datatype without whitespace
normalization or inferred token typing. The spelling belongs to the value;
there is no parallel string media-type property on the carrier. A registered
IRI can be used without repeating its external metadata on each carrier.
The name is optional; supplied names retain their exact machine spelling and
are checked for parameter-free syntax. Neither a name nor a matching IRI shape
proves registration, byte content or agreement between a name and an IRI.
Authors determine registration and select the corresponding resource.

The existing functional characteristic permits at most one media identity on
an entity. It does not require a media type on every Entity. Distinct media
formats are modeled as distinct carriers. Charset and other MIME parameters
are separate carrier or source-header metadata, outside both the identity IRI
and `mediaTypeName`; no default charset or parameter parser is introduced.
Source header evidence retains casing, parameters and other syntax beyond the
parameter-free format identity.

The two authored schema.org bindings use this same IRI as the URL-valued
`schema:encodingFormat` object. They do not replace it with `mediaTypeName`
or invent a formatter. The media class, name metadata and authored identity
policy are outside those projected triples. The
[media identity design](design/MEDIA-TYPES.md) records the exact source contract
and its verification limits.

## Character encoding

`gmeow:charset` records the character encoding of an information object's textual
content, including a web page, a document carrier or a MIME body part. Keep the
supplied charset label as a literal, such as `"utf-8"` or `"iso-8859-1"`; the
property does not enumerate a closed registry or normalize that label.

Keep the encoding separate from `gmeow:mediaType` and from the content's natural
language (`gmeow:contentLanguage` or `gmeow:contentLanguageCode`). Compression and
MIME content-transfer encoding are different metadata. The
[web presence example](examples/web-presence.ttl) records the IANA media IRI
`<https://www.iana.org/assignments/media-types/text/html>` and the separate
charset literal `"utf-8"` on a web page. Email uses this same charset property
for a MIME part's `Content-Type` charset parameter.

## Identifiers, extent and decoded content

`gmeow:identifier` applies to a `gmeow:MediaObject`: the current canonical chain
is `MediaObject` → `Manifestation` → `CreativeWork`. Media manifestations are
already creative works in the WEMI model; the identifier property is not what
creates that classification. Keep the abstract work, its expression, the
manifestation and any physical carrier distinct.

For an entity outside that hierarchy, use the existing generic
[`gmeow:hasIdentifier`](../organization/docs.md) with a `gmeow:Identifier` record
carrying `gmeow:identifierScheme` and `gmeow:identifierValue`. Do not use the
work-scoped literal property merely to store an application record ID.

`gmeow:extent` is a human-readable size or duration, such as "635 pages",
"47 minutes" or "2.3 MB". It is not a typed byte-count property, and its lexical
form is not a numeric contract. Record an exact encoded length as an
[observation or measurement](../observations/docs.md) of the specific artifact:
the result is a `math:Quantity` with `math:quantityValue`, `math:hasDimension`
and an explicit applicable `gmeow:unit`. Keep the measured serialization/version
and provenance so that re-encoding is not mistaken for a change of abstract work.

[`gmeow:BodyValue`](../../extensions/email/docs.md#decoded-content-in-chat-and-other-transports)
is a decoded information object, not a literal string and not a physical or
encoded carrier. A media attachment and its decoded content can therefore be
different nodes without duplicating the abstract work.

## Solver layer & alignment

Pixel-space computation — orientation transforms, colourspace conversion, derived
thumbnails — belongs to the solver layer (Principle 12); the slice records the claims
and their frames. The slice supersets schema.org, BIBO, and BIBFRAME by reference
(Principle 5): aligned to, never imported.

## Dependencies

Depends on `kernel`, `creative-works` (the WEMI spine), `names` (Appellation),
`observations`, `places`, `standpoint` (ReadingOrder), and `tags` (the isAbout
trichotomy). Consumed by creative-works manifestations, the mail-corpus attachments,
and depictions of any entity.

## Source structural blocks and volumes

`segmentTypeVolume` classifies one structural volume using ContentSegment,
segmentOf and segmentIndex. It does not identify an abstract work with a
published Manifestation or physical copy.

`ContentBlock` specializes ContentSegment for the atomic unit inside a source
division under a declared segmentation. Use its immediate `blockOf` division,
one source structural index and one open segmentType value. blockOf specializes
segmentOf without being transitive; segmentOf still reaches structural
ancestors. A nonempty containment cycle or duplicate block index within one
immediate division violates the canonical obligations.

Block indivisibility is a retrieval boundary, not a prohibition on internal
text, media or cell structure. Later chunking consumers must preserve that
boundary. This vocabulary does not run a chunker.

The twelve block kinds are paragraph, heading, verse, dialogue, stage direction,
code listing, equation display, table, figure, formal block, epigraph and
quotation. They are ContentSegmentType values, never one subclass per kind.
Paragraph reuses the existing value; the vocabulary remains open.

`caption` admits MediaObject or ContentSegment through a single union domain.
A caption on a table or figure block therefore does not make it a media file.
The schema profile retains actual media captions as schema:caption; structural
captions become an explicitly lossy schema:description that loses the caption
role and attachment distinction.

[The ordinary source inventory](examples/source-blocks.ttl) shows every block
kind. The downstream [reference apparatus extension](../../extensions/reference-apparatus/docs.md)
adds source speaker/language/notation fields, documentary cells, printed formal
roles and actual lang/math rendering links without a reverse core dependency.

## Acquisition and original filenames

acquiredAt retains a source-supplied xsd:dateTime for acquisition of an actual
MediaObject. It differs from capture, dispatch, receipt and recording time.
Do not parse a provider's raw time string into it or invent a timezone.

originalFilename preserves the actual media object's filename exactly, including
spaces and Unicode. It does not reuse filename, whose owner is an email
attachment, and does not turn a refused-media description into a manifestation.

## Source channel layout

channelLayout preserves a MediaObject image's exact source channel-layout
string, such as RGBA. It is independent of colour space, image decoding and
pixel verification. Keep original case and spaces; the property provides no
parser, closed code list or inferred channel count.

## Source content modes

`contentMode` records how a ContentBlock's content is expressed, independently
of its structural `segmentType`. `ContentMode` is an open value category;
`contentModeProse`, `contentModeVerse`, `contentModeDialogue` and
`contentModeCode` are initial individuals rather than block subclasses or
an exhaustive inventory.

The relation is optional and nonfunctional. Mixed content and coexisting
attributed classifications may retain several modes without selecting a winner.
When a source supplies paragraph structure and dialogue expressed in verse,
the independent axes can be retained together:

```turtle
ex:block a gmeow:ContentBlock ;
    gmeow:segmentType gmeow:segmentTypeParagraph ;
    gmeow:contentMode gmeow:contentModeDialogue, gmeow:contentModeVerse .
```

Keep each mode assertion's actual source and viewpoint on that exact RDF 1.2
statement. The example shows the two axes; the block's existing identity,
container and source index obligations remain in force. A mode on this block
does not classify every internal fragment or propagate to its containers,
descendants or retrieval chunks. Missing mode metadata remains absent.

Structural verse, dialogue and code-listing values keep their existing roles.
Content mode supplies no line boundary, meter, actual speaker, conversation
event, programming-language identity, code execution, validity or safety result.
No mode follows from layout, markup or another classification alone.

Media format, language and genre remain separate axes. In particular,
`lang:UnanalyzedProse` describes surface analysis status, while
`lang:renderingProse` identifies a rendering kind; neither is this block's
prose mode. Retain independently supplied speaker and programming-language
wording through their existing apparatus properties (Principles 2, 4, 9, 17
and 19).

## Source heading levels

Use `headingLevel` for the integer a source or attributed analysis actually
reports as a heading block's level code. The property is optional and
nonfunctional: distinct attributed readings can coexist. Preserve the supplied
integer on the actual `ContentBlock`, retaining its existing `segmentType` and
the exact assertion's source, viewpoint and supplied convention.

```turtle
ex:heading gmeow:segmentType gmeow:segmentTypeHeading ;
    gmeow:segmentIndex 7 ;
    gmeow:headingLevel 2 .
```

Here the supplied heading-level code is independent of the block's structural
position. Neither value computes containment depth. This field supplies no
numbering origin, positivity requirement, bound, universal ordering or default;
it does not infer a heading kind or propagate a level to other blocks.

An unknown convention stays unknown. An actual explicit reference frame is
required before interpreting these codes as ranks, depths, conversions or
comparable values. Recording a supplied integer alone provides no such frame.
Retain an actually supplied convention and attribution without inventing missing
context or choosing a preferred reading (Principles 2, 4, 9, 11, 12, 17 and 19).

## Reported original media-format text

`originalMediaTypeText` preserves the exact source string reporting a format
before reencoding, on the actual stored MediaObject. It belongs with source
artifact metadata such as originalFilename and channelLayout. Keep case,
spaces and blank strings exactly as supplied. Do not collapse whitespace into
a token, trim the string or add a language tag.

The stored carrier’s actual `mediaType` remains its existing format identity.
`mediaTypeName` belongs on that MediaType value and requires its own exact
parameter-free MIME spelling. Neither property is an alias for arbitrary
pre-reencoding source text. This raw evidence field performs no MIME parsing,
normalization, registration lookup or conversion and establishes no original
asset, original bytes, successful transformation or independently identified
original-format resource.

The field is optional and nonfunctional. Identical held bytes can have different
source histories, so preserve each supplied original-format claim with its own
exact-statement source and any actual supplied viewpoint. Do not choose one
winning history or impose a universal predecessor format on a content-addressed
carrier. Missing text stays absent; a supplied empty string remains a value.
The existing carrier, colour-space and format obligations remain unchanged.

Raw evidence retention is the selected operation. It is not a fallback from
an attempted format-identity mapping. Any separately selected identity mapping
must validate its actual input and fail invalid input; it must never substitute
this text silently. No original-format IRI role, MIME parser or registry is
introduced here. [Reported image analyses](../ai/docs.md#reported-image-analyses)
show multiple supplied histories on one actual stored carrier (Principles 4,
5, 9, 17 and 19).

## Source-supplied EPUB spine accounts

EpubSpineEntry is an information-record account supplied by a source. It retains
one reported order descriptor and one exact href string, plus an optional TOC
path. It neither certifies that an EPUB was parsed nor claims a complete package
inventory or a resolved content target. Different coherent accounts remain
separate even when their order descriptors and hrefs match.

When an actual source InformationObject is identified, epubSpineEntry associates
it with the account. That optional, nonfunctional link requires no invented
package, base URI, immutable revision or digest. An account remains representable
when source identity is unavailable. Keep the actual source and viewpoint on
exact statements, including the association; a source on one record does not
automatically attribute all its fields.

Retain epubSpineHref as the exact supplied xsd:string. Relative references,
fragments, case, spaces and supplied blanks survive. Do not resolve, fetch,
decode or normalize it, add a language tag or construct an IRI from matching
words. A separately supplied resolved target is different evidence and is not
computed by this vocabulary.

epubSpineOrder and each step's epubTocDepth are reported integer descriptors
under the source's actual convention. They are not computed counts, canonical
tree depth or universal sequence positions. All thirteen XSD integer-family
datatypes are admitted without retyping: integer, nonPositiveInteger,
negativeInteger, long, int, short, byte, nonNegativeInteger, unsignedLong,
unsignedInt, unsignedShort, unsignedByte and positiveInteger. Preserve each
supplied literal's lexical form and datatype. Do not infer a base or bound,
coerce strings or fractional values, or treat an arbitrary code as interpreted
order or depth. Invalid selected input fails rather than becoming a default.

A supplied epubTocPath is one StructuredSequence of actual EpubTocPathStep
occurrences. Each step specializes the existing StructuredValue: its single
scalar structuredTerm is the complete title, and epubTocDepth describes that
same occurrence. There is no separate title property or wrapper identity.
Both title and depth are required on every typed step, including one with no
supplied path membership. This lets the title/depth pair remain intact without
minting a distinct target entity. Every role-selected path item is a step;
a generic item cannot silently evade those requirements.

Use entryOrdinal only for the actual zero-based occurrence position in the
supplied path. Keep that position separate from the reported spine order and
depth. Distinct occurrence nodes retain repeated equal titles. Generic sequence
laws still require admitted nonnegative positions, one numeric position per
member and distinct positions for distinct members in a sequence; they add no
contiguous-index rule, inferred prefix, parent-child tree or pairing across
accounts. Do not copy a reported order or depth into entryOrdinal.

Titles admit the full xsd:string, rdf:langString and RDF 1.2 rdf:dirLangString
word union through structuredTerm. Keep complete whitespace and blank strings,
actual language and direction. Untagged input stays xsd:string. The existing
scalar branch and whole-term laws reject a conflicting payload branch or an
additional different scalar; the new title law checks every supplied term, so
a good string cannot hide a malformed extra value. No analyzed Form, translation,
preferred title or referent is inferred.

A supplied current zero-item sequence and an absent path are different. Preserve
a known empty current path when supplied; do not manufacture one from a missing
field or use a code default to claim historical source-field presence. Nothing
reconstructs wording or whitespace lost before the current input was supplied.
This source-record model adds no EPUB parser, URI resolver or execution engine
(Principles 2, 4, 9, 17 and 19).

### A fictional source scene

The scenario explicitly supplies the source records, one actually identified
source information object and a standpoint. A and B are independent accounts
for that actual source; C has no supplied package/source association. A retains
five ordered title occurrences: a repeated spaced string, a blank, a French
literal and an Arabic directional literal. Its reported order is 12 and its
first reported depth is 4, while the first occurrence position is 0. These
values belong to different fields and no ancestry or indexing convention is
inferred. B supplies an empty path; C supplies no path and explicitly supplies
a blank href. A standalone typed step has its own title and depth, with no
invented path, position or package. This is ordinary illustrative usage, not
a fixture, parser receipt or executed validation result.

```turtle
@prefix ex: <https://example.org/epub-spine-accounts/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Actual supplied background and account assertions.
ex:reportA rdf:type gmeow:InformationObject .
ex:reportB rdf:type gmeow:InformationObject .
ex:reportC rdf:type gmeow:InformationObject .
ex:actualSource rdf:type gmeow:InformationObject .
ex:view rdf:type gmeow:Standpoint .
ex:view rdf:type gmeow:Entity .
ex:view gmeow:sharpens gmeow:universalStandpoint .
ex:accountA rdf:type gmeow:EpubSpineEntry .
ex:accountA gmeow:epubSpineOrder "12"^^xsd:int .
ex:accountA gmeow:epubSpineHref "Text/Part.xhtml#Opening" .
ex:accountB rdf:type gmeow:EpubSpineEntry .
ex:accountB gmeow:epubSpineOrder "12"^^xsd:positiveInteger .
ex:accountB gmeow:epubSpineHref "Text/Part.xhtml#Opening" .
ex:accountC rdf:type gmeow:EpubSpineEntry .
ex:accountC gmeow:epubSpineOrder "1"^^xsd:integer .
ex:accountC gmeow:epubSpineHref "" .
ex:actualSource gmeow:epubSpineEntry ex:accountA .
ex:accountA gmeow:epubTocPath ex:pathA .
ex:pathA rdf:type gmeow:StructuredSequence .
ex:actualSource gmeow:epubSpineEntry ex:accountB .
ex:accountB gmeow:epubTocPath ex:pathB .
ex:pathB rdf:type gmeow:StructuredSequence .
ex:pathA gmeow:structuredItem ex:step0 .
ex:step0 rdf:type gmeow:EpubTocPathStep .
ex:step0 gmeow:structuredTerm " Opening " .
ex:step0 gmeow:epubTocDepth "4"^^xsd:int .
ex:step0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger .
ex:pathA gmeow:structuredItem ex:step1 .
ex:step1 rdf:type gmeow:EpubTocPathStep .
ex:step1 gmeow:structuredTerm " Opening " .
ex:step1 gmeow:epubTocDepth "4"^^xsd:int .
ex:step1 gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger .
ex:pathA gmeow:structuredItem ex:step2 .
ex:step2 rdf:type gmeow:EpubTocPathStep .
ex:step2 gmeow:structuredTerm "" .
ex:step2 gmeow:epubTocDepth "6"^^xsd:integer .
ex:step2 gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger .
ex:pathA gmeow:structuredItem ex:step3 .
ex:step3 rdf:type gmeow:EpubTocPathStep .
ex:step3 gmeow:structuredTerm "Retrouvailles"@fr .
ex:step3 gmeow:epubTocDepth "9"^^xsd:short .
ex:step3 gmeow:entryOrdinal "3"^^xsd:nonNegativeInteger .
ex:pathA gmeow:structuredItem ex:step4 .
ex:step4 rdf:type gmeow:EpubTocPathStep .
ex:step4 gmeow:structuredTerm "عودة"@ar--rtl .
ex:step4 gmeow:epubTocDepth "9"^^xsd:short .
ex:step4 gmeow:entryOrdinal "4"^^xsd:nonNegativeInteger .
ex:standalone rdf:type gmeow:EpubTocPathStep .
ex:standalone gmeow:structuredTerm "Unattached"@en .
ex:standalone gmeow:epubTocDepth "2"^^xsd:integer .

# Local exact-statement source and viewpoint; no propagation is implied.
ex:claim1 rdf:reifies <<( ex:reportA rdf:type gmeow:InformationObject )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim2 rdf:reifies <<( ex:reportB rdf:type gmeow:InformationObject )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim3 rdf:reifies <<( ex:reportC rdf:type gmeow:InformationObject )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim4 rdf:reifies <<( ex:actualSource rdf:type gmeow:InformationObject )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim5 rdf:reifies <<( ex:view rdf:type gmeow:Standpoint )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim6 rdf:reifies <<( ex:view rdf:type gmeow:Entity )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim7 rdf:reifies <<( ex:view gmeow:sharpens gmeow:universalStandpoint )>> ;
    gmeow:wasDerivedFrom ex:scenarioSource ;
    gmeow:accordingTo ex:view .
ex:claim8 rdf:reifies <<( ex:accountA rdf:type gmeow:EpubSpineEntry )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim9 rdf:reifies <<( ex:accountA gmeow:epubSpineOrder "12"^^xsd:int )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim10 rdf:reifies <<( ex:accountA gmeow:epubSpineHref "Text/Part.xhtml#Opening" )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim11 rdf:reifies <<( ex:accountB rdf:type gmeow:EpubSpineEntry )>> ;
    gmeow:wasDerivedFrom ex:reportB ;
    gmeow:accordingTo ex:view .
ex:claim12 rdf:reifies <<( ex:accountB gmeow:epubSpineOrder "12"^^xsd:positiveInteger )>> ;
    gmeow:wasDerivedFrom ex:reportB ;
    gmeow:accordingTo ex:view .
ex:claim13 rdf:reifies <<( ex:accountB gmeow:epubSpineHref "Text/Part.xhtml#Opening" )>> ;
    gmeow:wasDerivedFrom ex:reportB ;
    gmeow:accordingTo ex:view .
ex:claim14 rdf:reifies <<( ex:accountC rdf:type gmeow:EpubSpineEntry )>> ;
    gmeow:wasDerivedFrom ex:reportC ;
    gmeow:accordingTo ex:view .
ex:claim15 rdf:reifies <<( ex:accountC gmeow:epubSpineOrder "1"^^xsd:integer )>> ;
    gmeow:wasDerivedFrom ex:reportC ;
    gmeow:accordingTo ex:view .
ex:claim16 rdf:reifies <<( ex:accountC gmeow:epubSpineHref "" )>> ;
    gmeow:wasDerivedFrom ex:reportC ;
    gmeow:accordingTo ex:view .
ex:claim17 rdf:reifies <<( ex:actualSource gmeow:epubSpineEntry ex:accountA )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim18 rdf:reifies <<( ex:accountA gmeow:epubTocPath ex:pathA )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim19 rdf:reifies <<( ex:pathA rdf:type gmeow:StructuredSequence )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim20 rdf:reifies <<( ex:actualSource gmeow:epubSpineEntry ex:accountB )>> ;
    gmeow:wasDerivedFrom ex:reportB ;
    gmeow:accordingTo ex:view .
ex:claim21 rdf:reifies <<( ex:accountB gmeow:epubTocPath ex:pathB )>> ;
    gmeow:wasDerivedFrom ex:reportB ;
    gmeow:accordingTo ex:view .
ex:claim22 rdf:reifies <<( ex:pathB rdf:type gmeow:StructuredSequence )>> ;
    gmeow:wasDerivedFrom ex:reportB ;
    gmeow:accordingTo ex:view .
ex:claim23 rdf:reifies <<( ex:pathA gmeow:structuredItem ex:step0 )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim24 rdf:reifies <<( ex:step0 rdf:type gmeow:EpubTocPathStep )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim25 rdf:reifies <<( ex:step0 gmeow:structuredTerm " Opening " )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim26 rdf:reifies <<( ex:step0 gmeow:epubTocDepth "4"^^xsd:int )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim27 rdf:reifies <<( ex:step0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim28 rdf:reifies <<( ex:pathA gmeow:structuredItem ex:step1 )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim29 rdf:reifies <<( ex:step1 rdf:type gmeow:EpubTocPathStep )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim30 rdf:reifies <<( ex:step1 gmeow:structuredTerm " Opening " )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim31 rdf:reifies <<( ex:step1 gmeow:epubTocDepth "4"^^xsd:int )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim32 rdf:reifies <<( ex:step1 gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim33 rdf:reifies <<( ex:pathA gmeow:structuredItem ex:step2 )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim34 rdf:reifies <<( ex:step2 rdf:type gmeow:EpubTocPathStep )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim35 rdf:reifies <<( ex:step2 gmeow:structuredTerm "" )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim36 rdf:reifies <<( ex:step2 gmeow:epubTocDepth "6"^^xsd:integer )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim37 rdf:reifies <<( ex:step2 gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim38 rdf:reifies <<( ex:pathA gmeow:structuredItem ex:step3 )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim39 rdf:reifies <<( ex:step3 rdf:type gmeow:EpubTocPathStep )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim40 rdf:reifies <<( ex:step3 gmeow:structuredTerm "Retrouvailles"@fr )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim41 rdf:reifies <<( ex:step3 gmeow:epubTocDepth "9"^^xsd:short )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim42 rdf:reifies <<( ex:step3 gmeow:entryOrdinal "3"^^xsd:nonNegativeInteger )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim43 rdf:reifies <<( ex:pathA gmeow:structuredItem ex:step4 )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim44 rdf:reifies <<( ex:step4 rdf:type gmeow:EpubTocPathStep )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim45 rdf:reifies <<( ex:step4 gmeow:structuredTerm "عودة"@ar--rtl )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim46 rdf:reifies <<( ex:step4 gmeow:epubTocDepth "9"^^xsd:short )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim47 rdf:reifies <<( ex:step4 gmeow:entryOrdinal "4"^^xsd:nonNegativeInteger )>> ;
    gmeow:wasDerivedFrom ex:reportA ;
    gmeow:accordingTo ex:view .
ex:claim48 rdf:reifies <<( ex:standalone rdf:type gmeow:EpubTocPathStep )>> ;
    gmeow:wasDerivedFrom ex:reportC ;
    gmeow:accordingTo ex:view .
ex:claim49 rdf:reifies <<( ex:standalone gmeow:structuredTerm "Unattached"@en )>> ;
    gmeow:wasDerivedFrom ex:reportC ;
    gmeow:accordingTo ex:view .
ex:claim50 rdf:reifies <<( ex:standalone gmeow:epubTocDepth "2"^^xsd:integer )>> ;
    gmeow:wasDerivedFrom ex:reportC ;
    gmeow:accordingTo ex:view .
```

All 50 asserted scene facts have matching exact-statement reifiers: 7 supplied
background facts, 31 from A, 6 from B and 6 from C. The scene asserts no model,
run, Agent, base URI, resolved content resource, immutable revision or digest.
It supplies no evidence of EPUB XML conformance, parser execution or semantic
validation. Standalone steps remain subject to the title and depth requirements;
leaving either missing would violate the selected step contract.
