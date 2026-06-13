<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Documents — works, releases, segments, and the depiction spine

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/documents` · **tier: core**
> The everyday creative-work vocabulary: what got written, released, segmented, titled, and pictured.

Creative works and document metadata — articles, patents, datasets, media, web pages (the schema.org / BIBO / BIBFRAME superset layer).

This slice furnishes the working subkinds of the four-tier WEMI spine (issue #208):
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
spine. Formerly a flat Kind (pre issue #208); now a Category so each of the four tiers
beneath it can be a gufo:Kind with its own identity criteria. Carries the flat
metadata properties (`gmeow:title`, `gmeow:identifier`, `gmeow:datePublished`, the DC
date and description refinements of issue #60).

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

The syndication surface (#412). A `gmeow:FeedPosting` is a short-form work published
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

The web-presence model (#410). A `gmeow:WebSite` is the published collection; a
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
metadata of issue #22: `gmeow:pixelWidth`/`gmeow:pixelHeight` (functional),
`gmeow:imageOrientation` (EXIF degrees — the transform math is solver work,
Principle 12), `gmeow:captureTime` (non-functional: EXIF and catalogue claims coexist
with confidence), and `gmeow:captureDevice`. Declares `gmeow:requiresFrame
gmeow:colourspace` at warning severity (Principle 11).

### gmeow:depicts · gmeow:depictedIn

The flat depiction foundation (gathered here by the issue #287 dependency surgery): any
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
