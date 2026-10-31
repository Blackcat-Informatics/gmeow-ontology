<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Record metadata design

The shared abstraction is metadata about a record and its content occurrences.
Domain membership is unnecessary for recording time, modification, order or
creation-context exposure. Only the facet-specific predicates imply
`LabelledFacet`. This keeps the metadata useful across record types without
turning every record into a note, document or observation.

## Canonical obligations

Principles 4 and 17 place the vocabulary and its obligations in `module.ttl`.
The slice consumes the grounding kernel’s `logic:` vocabulary. OWL, SHACL and
ShEx remain generated projections; this slice authors none of those surfaces.

| Surface | Canonical obligation |
| --- | --- |
| Recording and modification literals | Anonymous data-range union of four standard XSD temporal datatypes; explicit property-global closed-world range reading. |
| Period references | Range union of the existing `TimeInterval` and `NamedPeriod` classes. |
| Ordinal | Standard `xsd:nonNegativeInteger` range with explicit property-global closed-world reading. |
| Facet label | Universal `xsd:string` restriction, class-scoped closed-world required path and canonical functionality assertion. |
| Facet payload | Literal restrictions, separate functionality assertions and a realized integrity formula excluding simultaneous text and value. |
| Concept interpretation | Nonfunctional relation to existing `Concept`; no preferred interpretation. |
| Context identity | Realized guarded-existence formula requiring `contentDigest` on every referenced context artifact. |

The required label uses a universal restriction plus explicit closure rather
than an existential that could create a witness. Its functionality record sets
at most one label. The payload exclusion formula applies to subjects of
`facetText`, so a label-only facet remains permitted. Functionality of each
payload channel combines with exclusion to bound their total to one.

The context identity formula applies to objects of `contextIncluded`.
The release and projection properties specialize that relation through canonical
`logic:subPropertyOf` axioms. The input role distinguishes the referenced bytes
without inventing a new artifact class or claiming that every compiled artifact
is a published manifestation. Multiple artifacts and multiple digest algorithms
remain representable. Digest lexical validation belongs to the existing digest
contract, not a provider invented by this slice.

The temporal range is anonymous: literals retain their actual standard datatype.
A new named datatype would imply a different literal type and lose that contract.
Coarse values carry no inferred period-start timestamp or timezone. A calendar
year literal and a named historical period remain distinct representations.
The model does not impose an ordering comparison between coarse, potentially
incomparable creation and modification values.

## Bridges follow meaning

`noteModifiedAt` and `dateModified` specialize `updatedAt`: both date the last
modification of their subject resource. Their owners retain the existing
narrower domains and timestamp ranges. The email-owned `participantOrdinal`
specializes `entryOrdinal` in the email module, preserving the extension-to-core
dependency direction.

No unconditional recording bridge is made from `assertedAt` (assertion witness),
`atTime` (occurrence), `sentAt` (dispatch), `dateCreated` (creative-work creation),
`ingestedAt` (recording imported claims in an import activity) or `noteCreatedAt`
(note creation rather than an independently established transaction clock).
The recording time of the import activity’s own record is a separate fact.

`segmentIndex` and `hopOrdinal` are one-based; `positionOrdinal` depends on a
frame. They do not specialize the zero-based ordinal. An importer must interpret
and, where appropriate, convert its source sequence convention explicitly.
Neither a recording value nor an ordinal is declared globally functional: the
model does not select a privileged clock, sequence or standpoint.

Context inclusion has no subproperty bridge to use, derivation or grounding.
Exposure is the entire assertion. A consumer may record stronger evidence
separately without obtaining it from mere co-presence in a context.

## Content fidelity

A label is a literal, including any producer-supplied relative-path notation.
`xsd:string` preserves its whitespace; `xsd:token` would normalize it. Position
names prevent key spelling from determining vocabulary or identity. No reversible
object reconstruction is promised from ambiguous path labels alone. A consumer
requiring the original serialization retains that artifact separately.

Typed scalars keep their source datatype and lexical form. Text keeps source
language and direction when known. No language, missing value or digest is
fabricated. Label-only facets preserve the existence of the key without inventing
a universal null datatype. These choices apply Principle 4 to imported content
and Principles 9 and 10 to reviewed concept interpretations.

## Intake fidelity and source ownership

Reception metadata describes a holder-specific occurrence rather than a shared
proposition. The open channel vocabulary and a guarded per-record uniqueness
formula preserve that scope without a globally functional property identifying
different channels. Missing metadata has no inferred default.

Rejected field occurrences reuse the facet identity and payload contracts.
Only the supplied field and raw representation become required on
`MalformedValue`; unrelated records acquire no mandatory fields. Universal
restrictions, typed local maxima and class-scoped `ClosedWorldClosure` entries
provide these requirements. Existing subproperty inference carries their
meaning into the generic facet path. No source path syntax, repair strategy or
contract interpreter is added.

Verbatim producer references use strings rather than token-normalized values.
Human wording retains its supplied language and direction, including plain
strings where language was not stated. Attribution and uncertainty remain on
the existing provenance and statement layers. Description subproperties record
stated reasons without turning those reasons into an execution receipt.

The suppression law follows one reason reifier to the already defined lowered
statement components. Its false boolean value and displayable predicate concern
that suppression statement alone. It relies on completed statement-metadata
lowering and does not broaden that mechanism's documented supported fragment.

Held-record terms remain beside the existing span owner in AI, which already
depends on record metadata. Tags and note purpose wording stay with their
respective owners; encoded byte length stays with the source artifact owner.
These directions avoid an AI/record-metadata dependency cycle. Every new
obligation is authored in canonical logic; no shape surface is authored
(Principles 4, 7, 9, 10 and 17).
