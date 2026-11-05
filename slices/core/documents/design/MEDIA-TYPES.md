<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Media identity and MIME spelling

The documents owner defines an open `MediaType` vocabulary as
`logic:AbstractIndividualType` / `logic:Class`, grounded in
`logic:QualityValue`. Its values identify formats rather than subclassing
the entities that carry them. The canonical `mediaType` property is
`logic:ObjectProperty`, retaining its Entity domain and targeting MediaType.
The existing functional characteristic retains at-most-one semantics;
it does not require a value on every entity (Principles 4 and 17).

Registered types use their existing
`https://www.iana.org/assignments/media-types/<type>/<subtype>` resource IRIs.
Unregistered types use GMEOW-owned
`https://blackcatinformatics.ca/gmeow/media-types/<type>/<subtype>` identities.
The unregistered `text/x-gmeow-patch` value describes the source email-state
patch format without claiming IANA registration or a universal diff language.
The seeded values are an open inventory used by the existing authored corpus,
not a closed registry or an enumeration of every MIME type.

`mediaTypeName` is the exact parameter-free MIME machine spelling on the
value, with canonical domain MediaType and range xsd:string or xsd:token.
It is a machine code, not natural-language prose. Preserve the supplied literal's
lexical form and datatype without whitespace normalization, string retyping or
inferred token typing. It remains optional:
using an IANA IRI does not oblige a consumer to redeclare external metadata.
Charset belongs to the carrier's existing `charset`; other parameters remain
in their actual source header or parameter records. The media identity never
contains charset, boundary, profile or other MIME parameters. Original source
header evidence preserves spelling and syntax beyond that identity.

The supplied-value integrity laws are authored once in `logic:`.
`MediaTypeIriConstraint` checks IRI node kind and the two permitted
namespace/path shapes on present mediaType assertions.
`MediaTypeNameSyntaxConstraint` checks supplied names for a parameter-free
type/subtype spelling. The property range admits supplied xsd:string or
xsd:token without retyping or whitespace normalization.
Neither law introduces a presence requirement. Regex behavior remains a
projection-dialect limit. These checks do not consult IANA, parse a MIME header,
normalize strings, authenticate bytes, or prove that a supplied name matches
an IRI suffix. Registration and identity selection remain explicit authored
facts; no registry service or alternate parser is introduced.

Media identity does not replace `lang:profileEncoding` or
`lang:renderingSerialization`. Those grounding concepts retain their owners
and meanings. No speculative equivalence or grounding-owner change is made.

Both existing schema.org projection patterns carry the source media IRI
unchanged into URL-valued `schema:encodingFormat`. The first binding
coarsens the media relation; the DataDownload binding additionally reuses the
corpus member as its output node. Their explicit loss notes cover omitted
MediaType/name/policy metadata and the existing distribution typing loss.
Neither binding dereferences a registry resource or formats a MIME string.
The native consumer boundary has its own admission and lexical requirements;
these source declarations alone do not establish an executable round trip.

Existing ordinary scenes, authored fixture assertions and their current
conformance assertions use the resource-valued model directly. No parallel
carrier string property or compatibility path is retained.

## Source artifact coverage

The documentary source-file-type role reuses `mediaType`; it introduces no
parallel carrier string property, class or identity scheme. The existing Entity
domain admits the actual source InformationObject reached through a documentary
object's `sourceLocus`, its EvidenceSpan and that span's `locusSource`. The
[owning guide](../docs.md#source-artifact-file-type) makes this route explicit
(Principles 4, 7, 11 and 17).

An independently supplied source format is asserted on the source artifact.
A converted held artifact and an original source retain their respective supplied
media identities. The route supplies no format for the derived block, span or
abstract work. Optional locus/source relations and the existing at-most-one
media identity remain unchanged. Missing identity remains absent; a filename,
href, extension or extractor code supplies no artifact, revision or default.
`mediaTypeName` remains optional and parameter-free on the MediaType value,
with its existing literal preservation rules.

Raw extension text, full headers and extractor codes are source-record evidence,
kept separately from format identity. Existing RecordMetadata facets and
structured carriers admit actually supplied run-time keys, literal terms,
structure, repeated keys and explicit null under their existing contracts.
They authorize no invented key or inferred mapping. Preserve actual source
association and attribution where supplied; do not invent either. A raw code
alone establishes no MIME identity. `originalMediaTypeText` continues to mean
reported pre-reencoding history on the stored MediaObject only.

This settles vocabulary coverage without choosing an unknown private encoding.
Any local-field mapping or migration must establish its actual artifact and
value meanings independently. No parser, normalization, parameter extraction,
registry lookup, importer, byte verification or migration acceptance follows from
these source declarations. No numeric, region, figure or EPUB model is changed.
